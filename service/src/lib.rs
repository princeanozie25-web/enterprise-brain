//! Enterprise Brain M3a — the Ask Brain answer service.
//!
//! The first long-running process in Enterprise Brain and the first place an
//! LLM generates text from retrieved context. The substrate (M1 compiler,
//! M2a/M2b retrieval) is conformance-proven and consumed read-only; this
//! crate adds a loopback-only service boundary and the answer layer without
//! weakening anything: deny by default, fail closed, no dark counts, no
//! out-of-scope ids — and the generator is untrusted by construction.
//!
//! Async lives in THIS crate only (the HTTP edge); the whole governed
//! pipeline (`answer::ask`) is synchronous, auditable code.

pub mod agent;
pub mod answer;
pub mod atlas;
pub mod cache;
pub mod cors;
pub mod diff;
pub mod export;
pub mod generate;
pub mod identity;
pub mod lens;
pub mod scope;
pub mod sidecar;

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use retrieval::embed::{EmbeddingSource, OllamaEmbeddings};
use retrieval::index::sha256_hex;
use retrieval::judge::{Judge, OllamaJudge};
use retrieval::local_llm::LocalLlmClient;
use retrieval::search::Engine;
use serde::Deserialize;

use crate::answer::{ask, AskError, AskOptions};
use crate::cache::AnswerCache;
use crate::generate::{Generator, OllamaGenerator};
use crate::identity::DemoPrincipal;
use crate::scope::IdentityModel;

/// The only address this service will ever bind.
pub const BIND_ADDR: &str = "127.0.0.1:8787";

// ---------------------------------------------------------------------------
// Startup state
// ---------------------------------------------------------------------------

/// Minimal strict mirror of the M1 index manifest — the root of trust for
/// everything the service loads at startup.
#[derive(Debug, Deserialize)]
struct M1IndexLite {
    #[allow(dead_code)]
    compiled_at: String,
    fixture_hashes: BTreeMap<String, String>,
    principals: Vec<M1RowLite>,
    snapshot_version: String,
    #[allow(dead_code)]
    totals: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct M1RowLite {
    artifact_file: String,
    artifact_sha256: String,
    #[allow(dead_code)]
    denied_count: u64,
    #[allow(dead_code)]
    entry_count: u64,
    #[allow(dead_code)]
    principal_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    unknown_principal: Option<bool>,
}

/// The corpus slice the answer layer needs (titles + bodies for sealed
/// context snippets, sensitivity for result/doc cards). Verified against the
/// M1-pinned hash before use.
#[derive(Debug, Deserialize)]
struct DocumentsLite {
    documents: Vec<DocLite>,
}

#[derive(Debug, Deserialize)]
struct DocLite {
    id: String,
    title: String,
    body: String,
    sensitivity: String,
}

/// What the service knows about one corpus document. Full bodies never leave
/// the process — only deterministic snippets do.
#[derive(Debug, Clone)]
pub struct DocMeta {
    pub title: String,
    pub body: String,
    pub sensitivity: String,
}

/// Artifact slice the service consumes: per-entry mosaic pass-through tags
/// (startup harvest) and the entry metadata `/doc` serves per request.
#[derive(Debug, Deserialize)]
pub(crate) struct ArtifactLite {
    pub(crate) entries: Vec<ArtifactEntryLite>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ArtifactEntryLite {
    pub(crate) document_id: String,
    #[serde(default)]
    pub(crate) superseded: Option<bool>,
    #[serde(default)]
    pub(crate) effective_successor: Option<String>,
    #[serde(default)]
    mosaic_tags: Option<Vec<MosaicTagLite>>,
}

#[derive(Debug, Deserialize)]
struct MosaicTagLite {
    doc_a: String,
    doc_b: String,
    #[allow(dead_code)]
    inferred_fact_class: String,
    #[allow(dead_code)]
    principal_id: String,
}

pub struct AppState {
    pub artifacts_dir: PathBuf,
    /// The fixtures directory (AP-2: /lens reads company.json per request,
    /// hash-verified against the M1 pin).
    pub fixtures_dir: PathBuf,
    pub engine: Arc<Engine>,
    pub identity: IdentityModel,
    /// doc id -> title/body/sensitivity, hash-verified at startup.
    pub docs: BTreeMap<String, DocMeta>,
    /// principal id -> (artifact file, artifact sha256) from the M1 index;
    /// `/doc` re-verifies the artifact bytes on every load.
    pub artifact_rows: BTreeMap<String, (String, String)>,
    /// Judge timeout; 2000ms unless a config EXPLICITLY says otherwise (the
    /// labeled demo profile). Nothing selects a non-default implicitly.
    pub judge_timeout_ms: u64,
    /// sha256 of company.json as pinned by the M1 compile (the agent
    /// registry re-verifies against it).
    pub company_sha256: String,
    /// AP-3: sha256 of brm.json, pinned by THIS service at startup after
    /// strict parse + referential closure against the M1-verified corpus
    /// (the frozen M1 manifest cannot pin it). Every /atlas request
    /// re-verifies against this pin. `None` = the world has no BRM and
    /// /atlas answers THE one 404.
    pub brm_sha256: Option<String>,
    pub snapshot_version: String,
    /// Globally harvested mosaic pairs (sorted, deduplicated). The bound is
    /// applied conservatively to ANY tagged pair co-present in one context.
    pub mosaic_pairs: Vec<(String, String)>,
    pub embedder: Option<Arc<dyn EmbeddingSource + Send + Sync>>,
    pub judge: Option<Arc<dyn Judge + Send + Sync>>,
    pub generator: Option<Arc<dyn Generator>>,
    pub cache: Option<Arc<AnswerCache>>,
    pub usage_out: Option<PathBuf>,
    /// M4: the standing-query registry; None = agent endpoints answer 404.
    pub agents: Option<agent::standing::AgentRegistry>,
    /// M4: the append-only proposal + audit store.
    pub proposals: Option<Arc<agent::proposals::ProposalStore>>,
    /// AP-5: the vendored print fonts (OFL TTFs decompressed from the
    /// console's own woff2 subsets). Compile-time crate path — a demo
    /// deployment choice, flagged in the AP-5 closeout.
    pub export_fonts_dir: PathBuf,
    /// AP-5: fixed-date mode for byte-identical test PDFs. `None` = the
    /// real clock — the export header is the ONE permitted dated line.
    pub export_fixed_date: Option<String>,
}

impl AppState {
    /// Loads and cross-verifies everything at startup, fail closed:
    /// the M1 index pins the fixture hashes; company.json and documents.json
    /// must match them; the retrieval index must be built from the same
    /// corpus; every artifact consulted for mosaic tags must match its
    /// recorded hash.
    pub fn build(fixtures_dir: &Path, artifacts_dir: &Path, idx_dir: &Path) -> Result<AppState> {
        let index_path = artifacts_dir.join("index.json");
        let index_bytes = std::fs::read(&index_path)
            .with_context(|| format!("cannot read M1 index {}", index_path.display()))?;
        let m1_index: M1IndexLite = serde_json::from_slice(&index_bytes)
            .with_context(|| format!("M1 index {} fails parse", index_path.display()))?;
        if m1_index.principals.is_empty() {
            bail!("M1 index lists no principals; refusing to serve an empty world");
        }

        let company_sha = m1_index
            .fixture_hashes
            .get("company.json")
            .context("M1 index records no company.json hash; refusing")?;
        let identity = IdentityModel::load(fixtures_dir, company_sha)?;

        let documents_path = fixtures_dir.join("documents.json");
        let document_bytes = std::fs::read(&documents_path)
            .with_context(|| format!("cannot read fixture {}", documents_path.display()))?;
        let documents_sha = m1_index
            .fixture_hashes
            .get("documents.json")
            .context("M1 index records no documents.json hash; refusing")?;
        if &sha256_hex(&document_bytes) != documents_sha {
            bail!("documents.json does not match the hash pinned by the M1 compile; refusing");
        }
        let parsed: DocumentsLite = serde_json::from_slice(&document_bytes)
            .with_context(|| format!("fixture {} fails parse", documents_path.display()))?;
        let docs: BTreeMap<String, DocMeta> = parsed
            .documents
            .into_iter()
            .map(|d| {
                (
                    d.id,
                    DocMeta {
                        title: d.title,
                        body: d.body,
                        sensitivity: d.sensitivity,
                    },
                )
            })
            .collect();

        let engine = Engine::open(idx_dir)?;
        if &engine.manifest.documents_sha256 != documents_sha {
            bail!("retrieval index was built from a different corpus; refusing");
        }

        // AP-3: brm.json joins the hash-verified input set here — validated
        // against the verified corpus, then byte-pinned for the life of the
        // process. Missing file = a world without an atlas (fail closed at
        // the route); present-but-wrong file = no service.
        let brm_sha256 = atlas::pin_brm(fixtures_dir, &docs)?;

        // Harvest the mosaic pairs from the compiled artifacts (verified
        // byte-for-byte against the M1 index) — the only non-test source of
        // the tags, exactly as M1 intended pass-through to be used. The same
        // sweep records each principal's (file, sha) row for `/doc`.
        let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
        let mut artifact_rows: BTreeMap<String, (String, String)> = BTreeMap::new();
        for row in &m1_index.principals {
            let artifact_path = artifacts_dir.join(&row.artifact_file);
            let artifact_bytes = std::fs::read(&artifact_path)
                .with_context(|| format!("cannot read artifact {}", artifact_path.display()))?;
            if sha256_hex(&artifact_bytes) != row.artifact_sha256 {
                bail!(
                    "artifact {} does not match the hash recorded in the M1 index; refusing",
                    artifact_path.display()
                );
            }
            let artifact: ArtifactLite = serde_json::from_slice(&artifact_bytes)
                .with_context(|| format!("artifact {} fails parse", artifact_path.display()))?;
            for entry in artifact.entries {
                for tag in entry.mosaic_tags.unwrap_or_default() {
                    let pair = if tag.doc_a <= tag.doc_b {
                        (tag.doc_a, tag.doc_b)
                    } else {
                        (tag.doc_b, tag.doc_a)
                    };
                    pairs.insert(pair);
                }
            }
            artifact_rows.insert(
                row.principal_id.clone(),
                (row.artifact_file.clone(), row.artifact_sha256.clone()),
            );
        }
        Ok(AppState {
            artifacts_dir: artifacts_dir.to_path_buf(),
            fixtures_dir: fixtures_dir.to_path_buf(),
            engine: Arc::new(engine),
            identity,
            docs,
            artifact_rows,
            judge_timeout_ms: 2000,
            company_sha256: company_sha.clone(),
            brm_sha256,
            snapshot_version: m1_index.snapshot_version,
            mosaic_pairs: pairs.into_iter().collect(),
            embedder: None,
            judge: None,
            generator: None,
            cache: Some(Arc::new(AnswerCache::new())),
            usage_out: None,
            agents: None,
            proposals: None,
            export_fonts_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts"),
            export_fixed_date: None,
        })
    }

    /// AP-5: fixed-date mode (tests only) — the export's Generated line and
    /// neutralized PDF metadata use this instead of the clock.
    pub fn with_export_fixed_date(mut self, date: Option<String>) -> AppState {
        self.export_fixed_date = date;
        self
    }

    pub fn with_embedder(mut self, embedder: Arc<dyn EmbeddingSource + Send + Sync>) -> AppState {
        self.embedder = Some(embedder);
        self
    }

    pub fn with_judge(mut self, judge: Arc<dyn Judge + Send + Sync>) -> AppState {
        self.judge = Some(judge);
        self
    }

    pub fn with_generator(mut self, generator: Arc<dyn Generator>) -> AppState {
        self.generator = Some(generator);
        self
    }

    pub fn with_cache(mut self, cache: Option<Arc<AnswerCache>>) -> AppState {
        self.cache = cache;
        self
    }

    pub fn with_usage_out(mut self, path: Option<PathBuf>) -> AppState {
        self.usage_out = path;
        self
    }

    pub fn with_agents(mut self, registry: agent::standing::AgentRegistry) -> AppState {
        self.agents = Some(registry);
        self
    }

    pub fn with_proposals(mut self, store: Arc<agent::proposals::ProposalStore>) -> AppState {
        self.proposals = Some(store);
        self
    }

    pub fn identity_count(&self) -> usize {
        self.identity.count()
    }
}

// ---------------------------------------------------------------------------
// Service configuration (model ids never default)
// ---------------------------------------------------------------------------

/// Service-side config: the retrieval models plus the generator. Model ids
/// NEVER default — a missing id refuses the capability rather than guessing.
/// `judge_timeout_ms` exists for the clearly-labeled demo profile (slow
/// hardware demonstrating the judge path at 8000ms); absent means the
/// production 2000ms, and no code path selects the demo value implicitly.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    /// Free-text label so a config file can say what it is ("production",
    /// "demo_profile — …"). Informational only.
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub embed_model: Option<String>,
    #[serde(default)]
    pub embed_dim: Option<u32>,
    #[serde(default)]
    pub judge_model: Option<String>,
    #[serde(default)]
    pub generate_model: Option<String>,
    #[serde(default)]
    pub judge_timeout_ms: Option<u64>,
}

impl ServiceConfig {
    pub fn load(path: &Path) -> Result<ServiceConfig> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("cannot read config {}", path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("config {} fails schema/parse", path.display()))
    }

    fn endpoint(&self) -> &str {
        self.endpoint
            .as_deref()
            .unwrap_or(retrieval::local_llm::DEFAULT_ENDPOINT)
    }

    /// Wires the configured local models onto the state. Each capability is
    /// enabled only when its model id is explicitly present.
    pub fn apply(&self, mut state: AppState) -> Result<AppState> {
        if let Some(judge_timeout_ms) = self.judge_timeout_ms {
            state.judge_timeout_ms = judge_timeout_ms;
        }
        if let Some(embed_model) = &self.embed_model {
            let dim = self
                .embed_dim
                .context("config has embed_model but no embed_dim; refusing to guess")?;
            let client = LocalLlmClient::new(self.endpoint())?;
            state = state.with_embedder(Arc::new(OllamaEmbeddings::new(client, embed_model, dim)));
        }
        if let Some(judge_model) = &self.judge_model {
            let client = LocalLlmClient::new(self.endpoint())?;
            state = state.with_judge(Arc::new(OllamaJudge::new(client, judge_model)));
        }
        if let Some(generate_model) = &self.generate_model {
            let client = LocalLlmClient::new(self.endpoint())?;
            state = state.with_generator(Arc::new(OllamaGenerator::new(client, generate_model)));
        }
        Ok(state)
    }
}

// ---------------------------------------------------------------------------
// Loopback-only listener
// ---------------------------------------------------------------------------

fn is_loopback(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.octets()[0] == 127,
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// Binds a listener, REFUSING any non-loopback address at construction.
/// The Ask Brain demo service has no business on a network interface.
pub fn loopback_listener(addr: &str) -> Result<TcpListener> {
    let socket_addr: SocketAddr = addr
        .parse()
        .with_context(|| format!("invalid bind address {addr:?}"))?;
    if !is_loopback(&socket_addr.ip()) {
        bail!("bind address {addr} is not loopback; the Ask Brain service binds 127.0.0.1 only");
    }
    let listener = TcpListener::bind(socket_addr).with_context(|| format!("cannot bind {addr}"))?;
    listener.set_nonblocking(true).context("set_nonblocking")?;
    Ok(listener)
}

// ---------------------------------------------------------------------------
// HTTP edge (the only async code in Enterprise Brain)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AskRequest {
    pub query: String,
    #[serde(default)]
    pub hybrid: bool,
    #[serde(default)]
    pub judge: bool,
}

pub fn app(state: Arc<AppState>) -> Router {
    let cors = cors::cors_layer(&cors::ALLOWED_ORIGINS)
        .expect("the compiled-in console origins are loopback by construction");
    Router::new()
        .route("/ask", post(handle_ask))
        .route("/doc/{id}", get(handle_doc))
        .route("/scope", get(handle_scope))
        .route("/healthz", get(handle_healthz))
        .route("/agent/{id}/run", post(handle_agent_run))
        .route("/lens/diff", get(handle_lens_diff))
        .route("/lens/{id}", get(handle_lens))
        .route("/atlas", get(handle_atlas))
        .route("/export", post(handle_export))
        .route("/proposals", get(handle_proposals_list))
        .route("/proposals/{id}/approve", post(handle_proposal_approve))
        .route("/proposals/{id}/reject", post(handle_proposal_reject))
        .layer(axum::middleware::from_fn(move |request, next| {
            cors::apply(cors.clone(), request, next)
        }))
        .with_state(state)
}

fn json_bytes_response(status: StatusCode, bytes: Vec<u8>) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], bytes).into_response()
}

async fn handle_ask(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(principal): DemoPrincipal,
    Json(request): Json<AskRequest>,
) -> Response {
    let options = AskOptions {
        hybrid: request.hybrid,
        judge: request.judge,
        bypass_cache: false,
    };
    let blocking_state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let outcome = ask(&blocking_state, &principal, &request.query, &options);
        if let (Ok((_, trace)), Some(usage_path)) = (&outcome, &blocking_state.usage_out) {
            // Metering failures must not fail the answer; they go to stderr.
            if let Err(err) = sidecar::append_all(usage_path, &trace.usage_events) {
                eprintln!("usage sidecar append failed: {err:#}");
            }
        }
        outcome
    })
    .await;

    match result {
        Ok(Ok((bytes, _trace))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Err(AskError::BadRequest(message))) => json_bytes_response(
            StatusCode::BAD_REQUEST,
            format!(
                "{{\"demo_identity_mode\":true,\"error\":{}}}\n",
                serde_json::to_string(&message).unwrap_or_else(|_| "\"bad request\"".into())
            )
            .into_bytes(),
        ),
        Ok(Err(AskError::Internal(err))) => {
            // Detail goes to the server log only; the body stays generic so
            // nothing internal (paths, ids) can leak through an error.
            eprintln!("ask failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
        Err(join_error) => {
            eprintln!("ask task failed: {join_error}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
    }
}

/// GET /doc/{id}: the scope-checked document card. Out-of-scope and
/// nonexistent ids return an IDENTICAL 404 — unknown is indistinguishable
/// from ungranted, by byte equality (A-10).
async fn handle_doc(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(principal): DemoPrincipal,
    axum::extract::Path(doc_id): axum::extract::Path<String>,
) -> Response {
    let blocking_state = state.clone();
    let result =
        tokio::task::spawn_blocking(move || answer::doc_card(&blocking_state, &principal, &doc_id))
            .await;
    match result {
        Ok(Ok(Some(card))) => match retrieval::index::canonical_json_bytes(&card) {
            Ok(bytes) => json_bytes_response(StatusCode::OK, bytes),
            Err(err) => {
                eprintln!("doc serialization failed: {err:#}");
                json_bytes_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
                )
            }
        },
        Ok(Ok(None)) => doc_not_found(),
        Ok(Err(err)) => {
            eprintln!("doc lookup failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
        Err(join_error) => {
            eprintln!("doc task failed: {join_error}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
    }
}

/// THE one 404 — a single constructor so out-of-scope and nonexistent are
/// byte-identical in status, body, and headers.
fn doc_not_found() -> Response {
    json_bytes_response(
        StatusCode::NOT_FOUND,
        b"{\"demo_identity_mode\":true,\"error\":\"not found\"}\n".to_vec(),
    )
}

/// GET /lens/{subject_id} — the Lens room body (AP-2). The actor is the
/// header principal; cross-lens views audit BEFORE the response renders;
/// unknown and malformed subjects share THE one 404.
async fn handle_lens(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
    axum::extract::Path(subject_id): axum::extract::Path<String>,
) -> Response {
    let blocking_state = state.clone();
    let result =
        tokio::task::spawn_blocking(move || lens::lens_view(&blocking_state, &actor, &subject_id))
            .await;
    match result {
        Ok(Ok(Some((bytes, _act_ordinal)))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Ok(None)) => doc_not_found(),
        Ok(Err(AskError::BadRequest(message))) => json_bytes_response(
            StatusCode::BAD_REQUEST,
            format!(
                "{{\"demo_identity_mode\":true,\"error\":{}}}\n",
                serde_json::to_string(&message).unwrap_or_else(|_| "\"bad request\"".into())
            )
            .into_bytes(),
        ),
        Ok(Err(AskError::Internal(err))) => {
            eprintln!("lens failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
        Err(join_error) => {
            eprintln!("lens task failed: {join_error}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
    }
}

/// GET /lens/diff?left=<id>&right=<id> — the lens diff (AP-4): two
/// principals side by side, set-exact against both compiled artifacts, one
/// audited act. The static segment outranks /lens/{id} in the router, so
/// "diff" is never a subject. Exactly the two parameters are accepted —
/// anything else is a 400 in OUR error shape (a string map deserializes
/// from any well-formed query, so axum's own rejection is unreachable in
/// practice).
async fn handle_lens_diff(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
    axum::extract::Query(params): axum::extract::Query<BTreeMap<String, String>>,
) -> Response {
    let bad_request = |message: &str| {
        json_bytes_response(
            StatusCode::BAD_REQUEST,
            format!(
                "{{\"demo_identity_mode\":true,\"error\":{}}}\n",
                serde_json::to_string(message).unwrap_or_else(|_| "\"bad request\"".into())
            )
            .into_bytes(),
        )
    };
    if params.keys().any(|k| k != "left" && k != "right") {
        return bad_request("only left and right are accepted");
    }
    let (Some(left), Some(right)) = (params.get("left"), params.get("right")) else {
        return bad_request("left and right are required");
    };
    let (left, right) = (left.clone(), right.clone());

    let blocking_state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        diff::diff_view(&blocking_state, &actor, &left, &right)
    })
    .await;
    match result {
        Ok(Ok(Some((bytes, _act_ordinal)))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Ok(None)) => doc_not_found(),
        Ok(Err(AskError::BadRequest(message))) => bad_request(&message),
        Ok(Err(AskError::Internal(err))) => {
            eprintln!("lens diff failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
        Err(join_error) => {
            eprintln!("lens diff task failed: {join_error}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
    }
}

/// GET /atlas — the capability surface (AP-3). STRUCTURE IS INTERNAL-GRADE;
/// EVIDENCE IS GOVERNED: the whole BRM renders for any principal with
/// standing, and each capability carries only the actor's own visible
/// documents. An actor with no standing receives the empty atlas; a world
/// with no brm.json answers THE one 404.
async fn handle_atlas(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
) -> Response {
    let blocking_state = state.clone();
    let result =
        tokio::task::spawn_blocking(move || atlas::atlas_view(&blocking_state, &actor)).await;
    match result {
        Ok(Ok(Some(bytes))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Ok(None)) => doc_not_found(),
        Ok(Err(AskError::BadRequest(message))) => json_bytes_response(
            StatusCode::BAD_REQUEST,
            format!(
                "{{\"demo_identity_mode\":true,\"error\":{}}}\n",
                serde_json::to_string(&message).unwrap_or_else(|_| "\"bad request\"".into())
            )
            .into_bytes(),
        ),
        Ok(Err(AskError::Internal(err))) => {
            eprintln!("atlas failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
        Err(join_error) => {
            eprintln!("atlas task failed: {join_error}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
    }
}

/// POST /export — Evidence Export (AP-5). THE SERVER DERIVES, NEVER
/// RECEIVES: the body is parsed strictly by hand (params only; any extra
/// field is a 400 in OUR shape, never axum's), the named act is performed
/// fresh through the same seams as the live views, and the PDF carries the
/// attestation of exactly what was derived.
async fn handle_export(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
    body: axum::body::Bytes,
) -> Response {
    let bad_request = |message: &str| {
        json_bytes_response(
            StatusCode::BAD_REQUEST,
            format!(
                "{{\"demo_identity_mode\":true,\"error\":{}}}\n",
                serde_json::to_string(message).unwrap_or_else(|_| "\"bad request\"".into())
            )
            .into_bytes(),
        )
    };
    let request: export::ExportRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(err) => {
            // Strict-parse refusal: the detail goes to the log; the body
            // names the law, not the offending field.
            eprintln!("export request refused: {err}");
            return bad_request("export request fails strict parse (params only)");
        }
    };
    let blocking_state = state.clone();
    let result =
        tokio::task::spawn_blocking(move || export::export_view(&blocking_state, &actor, &request))
            .await;
    match result {
        Ok(Ok(Some(bytes))) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/pdf")],
            bytes,
        )
            .into_response(),
        Ok(Ok(None)) => doc_not_found(),
        Ok(Err(AskError::BadRequest(message))) => bad_request(&message),
        Ok(Err(AskError::Internal(err))) => {
            eprintln!("export failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
        Err(join_error) => {
            eprintln!("export task failed: {join_error}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
    }
}

async fn handle_scope(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(principal): DemoPrincipal,
) -> Response {
    #[derive(serde::Serialize)]
    struct ScopeResponse {
        demo_identity_mode: bool,
        principal_id: String,
        scope_statement: retrieval::envelope::ScopeStatement,
    }
    let body = ScopeResponse {
        demo_identity_mode: true,
        principal_id: principal.clone(),
        scope_statement: state.identity.statement_for(&principal),
    };
    match retrieval::index::canonical_json_bytes(&body) {
        Ok(bytes) => json_bytes_response(StatusCode::OK, bytes),
        Err(err) => {
            eprintln!("scope serialization failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
    }
}

/// Liveness only. Reveals nothing about the corpus, principals, models, or
/// configuration — a constant body, no identity required.
async fn handle_healthz() -> Response {
    json_bytes_response(StatusCode::OK, b"{\"status\":\"ok\"}\n".to_vec())
}

// ---------------------------------------------------------------------------
// M4: agent run + proposals (owner-only, human-only, audit before effect)
// ---------------------------------------------------------------------------

type Reply = (StatusCode, Vec<u8>);

fn reply_error(status: StatusCode, label: &str) -> Reply {
    (
        status,
        format!(
            "{{\"demo_identity_mode\":true,\"error\":{}}}\n",
            serde_json::Value::from(label)
        )
        .into_bytes(),
    )
}

fn reply_internal() -> Reply {
    reply_error(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
}

fn reply_canonical(value: &impl serde::Serialize) -> Reply {
    match retrieval::index::canonical_json_bytes(value) {
        Ok(bytes) => (StatusCode::OK, bytes),
        Err(err) => {
            eprintln!("serialization failed: {err:#}");
            reply_internal()
        }
    }
}

async fn run_blocking(
    state: Arc<AppState>,
    task: impl FnOnce(&AppState) -> Reply + Send + 'static,
) -> Response {
    match tokio::task::spawn_blocking(move || task(&state)).await {
        Ok((status, bytes)) => json_bytes_response(status, bytes),
        Err(join_error) => {
            eprintln!("agent task failed: {join_error}");
            let (status, bytes) = reply_internal();
            json_bytes_response(status, bytes)
        }
    }
}

/// POST /agent/{id}/run — invocable ONLY by the agent's owner. Anyone else,
/// including the agent itself, is refused with an audit row. The run is an
/// explicit invocation: no scheduler, no daemon, no background anything.
async fn handle_agent_run(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> Response {
    run_blocking(state, move |state| {
        let (Some(registry), Some(store)) = (&state.agents, &state.proposals) else {
            return reply_error(StatusCode::NOT_FOUND, "not found");
        };
        let audit = |outcome: &str| store.audit("agent_run", &caller, &agent_id, outcome);
        let Some(entry) = registry.configured(&agent_id) else {
            if audit("refused_not_found").is_err() {
                return reply_internal();
            }
            return reply_error(StatusCode::NOT_FOUND, "not found");
        };
        if registry.is_agent_principal(&caller) {
            if audit("refused_agent_principal").is_err() {
                return reply_internal();
            }
            return reply_error(StatusCode::FORBIDDEN, "forbidden");
        }
        if caller != entry.owner_user_id {
            if audit("refused_not_owner").is_err() {
                return reply_internal();
            }
            return reply_error(StatusCode::FORBIDDEN, "forbidden");
        }
        // Audit BEFORE effect.
        if audit("allowed").is_err() {
            return reply_internal();
        }
        let outcome = match agent::context::execute_run(state, entry, store) {
            Ok(outcome) => outcome,
            Err(err) => {
                eprintln!("agent run failed: {err:#}");
                return reply_internal();
            }
        };
        if let Some(usage_path) = &state.usage_out {
            if let Err(err) = sidecar::append_all(usage_path, &outcome.usage_events) {
                eprintln!("usage sidecar append failed: {err:#}");
            }
        }
        #[derive(serde::Serialize)]
        struct RunResponse {
            agent_id: String,
            created_proposal_ids: Vec<String>,
            demo_identity_mode: bool,
        }
        reply_canonical(&RunResponse {
            agent_id: outcome.agent_id,
            created_proposal_ids: outcome
                .created
                .iter()
                .map(|p| p.proposal_id.clone())
                .collect(),
            demo_identity_mode: true,
        })
    })
    .await
}

/// GET /proposals — owner-scoped: a principal sees only proposals from
/// agents they own. Stale proposals render with their findings withheld.
async fn handle_proposals_list(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
) -> Response {
    run_blocking(state, move |state| {
        let Some(store) = &state.proposals else {
            return reply_error(StatusCode::NOT_FOUND, "not found");
        };
        let mut rendered = Vec::new();
        for proposal in store.owned_by(&caller) {
            match agent::proposals::render(&proposal, &state.snapshot_version) {
                Ok(value) => rendered.push(value),
                Err(err) => {
                    eprintln!("proposal render failed: {err:#}");
                    return reply_internal();
                }
            }
        }
        #[derive(serde::Serialize)]
        struct ProposalsResponse {
            demo_identity_mode: bool,
            proposals: Vec<serde_json::Value>,
        }
        reply_canonical(&ProposalsResponse {
            demo_identity_mode: true,
            proposals: rendered,
        })
    })
    .await
}

/// The shared approve/reject path. Authority matrix, fail closed, audit
/// before effect: agent principals are STRUCTURALLY refused; only the
/// owning human may decide; stale and already-decided proposals refuse.
/// Approval changes STATUS and nothing else.
fn decide_proposal(state: &AppState, caller: &str, proposal_id: &str, status: &str) -> Reply {
    use agent::proposals::{DecideError, STATUS_PENDING};

    let (Some(registry), Some(store)) = (&state.agents, &state.proposals) else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    let action = format!("proposal_{status}");
    let audit = |outcome: &str| store.audit(&action, caller, proposal_id, outcome);

    if registry.is_agent_principal(caller) {
        if audit("refused_agent_principal").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    }
    if !registry.is_human(caller) {
        // Neither a person nor an agent the fixtures know: deny by default.
        if audit("refused_unknown_principal").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    }
    let Some(proposal) = store.get(proposal_id) else {
        if audit("refused_not_found").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    if proposal.owner_user_id != caller {
        if audit("refused_not_owner").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    }
    if proposal.snapshot_version != state.snapshot_version {
        if audit("refused_stale").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "stale proposal: re-run to refresh");
    }
    if proposal.status != STATUS_PENDING {
        if audit("refused_already_decided").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "already decided");
    }
    // Audit BEFORE effect.
    if audit("allowed").is_err() {
        return reply_internal();
    }
    match store.decide(proposal_id, status, caller, &state.snapshot_version) {
        Ok(Ok(decided)) => match agent::proposals::render(&decided, &state.snapshot_version) {
            Ok(value) => reply_canonical(&value),
            Err(err) => {
                eprintln!("proposal render failed: {err:#}");
                reply_internal()
            }
        },
        Ok(Err(DecideError::NotFound)) => reply_error(StatusCode::NOT_FOUND, "not found"),
        Ok(Err(DecideError::Stale)) => {
            reply_error(StatusCode::CONFLICT, "stale proposal: re-run to refresh")
        }
        Ok(Err(DecideError::AlreadyDecided)) => {
            reply_error(StatusCode::CONFLICT, "already decided")
        }
        Err(err) => {
            eprintln!("decision failed: {err:#}");
            reply_internal()
        }
    }
}

async fn handle_proposal_approve(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(proposal_id): axum::extract::Path<String>,
) -> Response {
    run_blocking(state, move |state| {
        decide_proposal(
            state,
            &caller,
            &proposal_id,
            agent::proposals::STATUS_APPROVED,
        )
    })
    .await
}

async fn handle_proposal_reject(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(proposal_id): axum::extract::Path<String>,
) -> Response {
    run_blocking(state, move |state| {
        decide_proposal(
            state,
            &caller,
            &proposal_id,
            agent::proposals::STATUS_REJECTED,
        )
    })
    .await
}
