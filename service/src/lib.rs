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

pub mod access_grants;
pub mod access_requests;
pub mod agent;
pub mod agent_bridge;
pub mod alerts;
pub mod answer;
pub mod atlas;
pub mod bootstrap;
pub mod cache;
pub mod clock;
pub mod cors;
pub mod doctor;
pub mod diff;
pub mod estate;
pub mod export;
pub mod generate;
pub mod graph;
pub mod grounding;
pub mod humanize;
pub mod identity;
pub mod lane;
pub mod lens;
pub mod node_summary;
pub mod proposals;
pub mod ratelimit;
pub mod role_scope;
pub mod routes;
pub mod scope;
pub mod session;
pub mod sidecar;
pub mod visibility;
pub mod workflow;

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
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
use crate::session::SessionPrincipal;

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
    /// AP-6: the lane's derivation rule matches capabilities to people by
    /// their realizing documents' departments.
    department: String,
}

/// What the service knows about one corpus document. Full bodies never leave
/// the process — only deterministic snippets do.
#[derive(Debug, Clone)]
pub struct DocMeta {
    pub title: String,
    pub body: String,
    pub sensitivity: String,
    pub department: String,
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
    /// Access request ledger: append-only request/decision records.
    pub access_requests: Option<Arc<access_requests::AccessRequestStore>>,
    /// Access grant ledger: append-only read entitlements derived from
    /// approved access requests. Grants never mutate compiled artifacts.
    pub access_grants: Option<Arc<access_grants::AccessGrantStore>>,
    /// AP-5: the vendored print fonts (OFL TTFs decompressed from the
    /// console's own woff2 subsets). Compile-time crate path — a demo
    /// deployment choice, flagged in the AP-5 closeout.
    pub export_fonts_dir: PathBuf,
    /// AP-5: fixed-date mode for byte-identical test PDFs. `None` = the
    /// real clock — the export header is the ONE permitted dated line.
    pub export_fixed_date: Option<String>,
    /// AP-6: the lane's graph view, converted from the SAME validated BRM
    /// the atlas pinned. `None` = a world without a BRM has no lane.
    pub lane_graph: Option<lane::LaneGraph>,
    /// AP-6: the derived assignments, computed once at startup from
    /// verified inputs only (humans with non-empty lanes).
    pub lane_seeds: BTreeMap<String, Vec<lane::BoxSeed>>,
    /// AP-6: the append-only box store (status changes + accepted boxes);
    /// None = transitions and accepts refuse, the lane stays readable.
    pub lane_boxes: Option<Arc<lane::BoxStore>>,
    /// AR-1: the humanization layer (fixtures/people.json), DISPLAY ONLY,
    /// loaded and proved against the live skeleton by `with_people`. `None` =
    /// a world without a humanization layer — surfaces fall back to the
    /// frozen company.json names and carry no human cards. It never sources
    /// an authorization fact.
    pub people: Option<Arc<humanize::PeopleLayer>>,
    /// FC-A1: server-minted session store. Identity is bound from a validated
    /// session, never from a caller-set header. In-memory per process; tokens
    /// are held only as sha256 hashes.
    pub sessions: Arc<session::SessionStore>,
    /// AUTH-3 (FC-A3): when true, cross-principal view-as (/lens/{other}, /diff)
    /// is free (still audited before render); when false (real deployment), it is
    /// admin-only. Defaults to true (the demo). Aperture charter §6.3.
    pub demo_identity_mode: bool,
    /// AUTH-4 (D1): fixed-window rate limiter guarding `/auth/login` — cheap-reject
    /// login floods before any session-minting work.
    pub login_rate: Arc<ratelimit::RateLimiter>,
    /// S0: the Entra agent-token bridge. `None` IS the disabled state
    /// (S0-4, the default): a JWT-shaped bearer credential is denied
    /// `bridge_disabled` and nothing else changes behaviour. Built only
    /// from an explicitly-enabled `agent_bridge` config section.
    pub agent_bridge: Option<Arc<agent_bridge::Bridge>>,
    /// S3: the multi-source estate — the second source + its authority
    /// model. `None` = a single-source world (the primary corpus alone),
    /// exactly the pre-S3 behaviour. Loaded from `fixtures/estate` when
    /// present; the estate agents and `s3/...` ids exist only when it does.
    pub estate: Option<Arc<estate::EstateModel>>,
    /// S5a: the estate retrieval index, built once at INGEST over both
    /// sources (primary docs + second-source objects). `None` = no estate.
    /// Request-time estate retrieval reads this index — it never
    /// re-tokenizes a corpus body. Authority stays inside candidate
    /// construction (EB-5); this index has zero authority over decisions.
    pub estate_index: Option<Arc<estate::EstateIndex>>,
    /// S4: the policy-deny alert dispatcher. `None` = alerting off (the
    /// default; zero behaviour change). Alerts derive from ledger deny
    /// rows and dispatch OFF the request path — never a second decision
    /// point, never a source of latency.
    pub alerts: Option<Arc<alerts::AlertDispatcher>>,
    /// SHOWCASE-III: the append-only grounded-workflow proposal store (the first
    /// mutation path). `None` = a build without `--state-dir`: proposal routes 503.
    pub wf_proposals: Option<Arc<proposals::WorkflowProposalStore>>,
    /// SHOWCASE-III: per-principal generation limiter (≤3 proposals / 60s → 429).
    pub generation_rate: Arc<ratelimit::PrincipalRateLimiter>,
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
                        department: d.department,
                    },
                )
            })
            .collect();

        let engine = Engine::open(idx_dir)?;
        if &engine.manifest.documents_sha256 != documents_sha {
            bail!("retrieval index was built from a different corpus; refusing");
        }

        // Harvest the mosaic pairs from the compiled artifacts (verified
        // byte-for-byte against the M1 index) — the only non-test source of
        // the tags, exactly as M1 intended pass-through to be used. The same
        // sweep records each principal's (file, sha) row for `/doc`.
        let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
        let mut artifact_rows: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut lane_entries: BTreeMap<String, Vec<lane::LaneEntryFacts>> = BTreeMap::new();
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
            // AP-6: the lane derives from the same verified sweep.
            lane_entries.insert(
                row.principal_id.clone(),
                artifact
                    .entries
                    .iter()
                    .map(|entry| lane::LaneEntryFacts {
                        document_id: entry.document_id.clone(),
                        superseded: entry.superseded,
                        effective_successor: entry.effective_successor.clone(),
                    })
                    .collect(),
            );
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

        // AP-3: brm.json joins the hash-verified input set here — validated
        // against the verified corpus, then byte-pinned for the life of the
        // process. Missing file = a world without an atlas (fail closed at
        // the route); present-but-wrong file = no service.
        // AP-6: the lane derives its assignments from the SAME validated
        // graph and the SAME verified artifact sweep, deterministically,
        // at startup.
        let pinned_brm = atlas::pin_brm(fixtures_dir, &docs)?;
        let (brm_sha256, lane_graph, lane_seeds) = match pinned_brm {
            Some((sha, brm)) => {
                let graph = lane::LaneGraph::from_brm(&brm);
                let seeds = lane::derive_lanes(
                    fixtures_dir,
                    company_sha,
                    &docs,
                    &graph,
                    &lane_entries,
                    &m1_index.snapshot_version,
                )?;
                (Some(sha), Some(graph), seeds)
            }
            None => (None, None, BTreeMap::new()),
        };

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
            access_requests: None,
            access_grants: None,
            export_fonts_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts"),
            export_fixed_date: None,
            lane_graph,
            lane_seeds,
            lane_boxes: None,
            people: None,
            sessions: Arc::new(session::SessionStore::new()),
            demo_identity_mode: true,
            login_rate: Arc::new(ratelimit::RateLimiter::default_login()),
            agent_bridge: None,
            estate: None,
            estate_index: None,
            alerts: None,
            wf_proposals: None,
            generation_rate: Arc::new(ratelimit::PrincipalRateLimiter::default_proposals()),
        })
    }

    /// S4: attach the policy-deny alert dispatcher (config- or test-wired).
    pub fn with_alerts(mut self, dispatcher: Arc<alerts::AlertDispatcher>) -> AppState {
        self.alerts = Some(dispatcher);
        self
    }

    /// S3: load the multi-source estate from `fixtures/estate` (the second
    /// source + `s3-access.json` authority + pinned content hash). Absent
    /// the directory, the world stays single-source (pre-S3 behaviour).
    pub fn with_estate_from(mut self, estate_dir: &Path) -> Result<AppState> {
        if estate_dir.join("s3-access.json").exists() {
            let model = estate::EstateModel::load(estate_dir)?;
            self.estate_index = Some(Arc::new(self.build_estate_index(&model)));
            self.estate = Some(Arc::new(model));
        }
        Ok(self)
    }

    /// S3: attach an already-built estate model (tests). S5a: the retrieval
    /// index is (re)built from the current corpus + the model's objects.
    pub fn with_estate(mut self, model: Arc<estate::EstateModel>) -> AppState {
        self.estate_index = Some(Arc::new(self.build_estate_index(&model)));
        self.estate = Some(model);
        self
    }

    /// S5a: build the estate retrieval index over BOTH sources — the primary
    /// corpus (`self.docs`, source `primary`) and the second-source objects
    /// (source `s3`). Tokenization happens once here, at ingest.
    pub(crate) fn build_estate_index(&self, model: &estate::EstateModel) -> estate::EstateIndex {
        let primary = self.docs.iter().map(|(id, meta)| {
            (
                id.clone(),
                meta.title.clone(),
                meta.body.clone(),
                meta.sensitivity.clone(),
                "primary".to_string(),
            )
        });
        let s3 = model.objects().map(|object| {
            (
                object.doc_id.clone(),
                object.title.clone(),
                object.body.clone(),
                object.sensitivity.clone(),
                "s3".to_string(),
            )
        });
        estate::EstateIndex::build(primary.chain(s3))
    }

    /// AUTH-4 (D1): set the `/auth/login` rate limit (max attempts per window
    /// seconds). Tests dial this down to exercise the 429 branch.
    pub fn with_login_rate(mut self, max: u32, window_secs: u64) -> AppState {
        self.login_rate = Arc::new(ratelimit::RateLimiter::new(max, window_secs));
        self
    }

    /// S0: enable the Entra agent-token bridge. The ONLY way the bridge
    /// exists at runtime — absent this call it is structurally off (S0-4).
    pub fn with_agent_bridge(mut self, bridge: Arc<agent_bridge::Bridge>) -> AppState {
        self.agent_bridge = Some(bridge);
        self
    }

    /// AUTH-4 (D1): set the per-principal concurrent-session quota. Replaces the
    /// (still-empty) session store, so it must be called before any login.
    pub fn with_session_quota(mut self, quota: usize) -> AppState {
        self.sessions = Arc::new(session::SessionStore::with_quota(quota));
        self
    }

    /// AUTH-3: toggle demo-identity mode (default on). Off = real deployment,
    /// where cross-principal view-as is admin-only. Used by the FC-A3 tests to
    /// exercise the real-mode (non-admin -> 404) branch.
    pub fn with_demo_identity_mode(mut self, on: bool) -> AppState {
        self.demo_identity_mode = on;
        self
    }

    /// AP-6: wires the append-only box store (status changes + accepted
    /// boxes) — opened from the same state dir as the audit store.
    pub fn with_lane_boxes(mut self, store: Arc<lane::BoxStore>) -> AppState {
        self.lane_boxes = Some(store);
        self
    }

    /// AR-1: loads and PROVES `fixtures/people.json` against the live
    /// skeleton — regenerate from the frozen inputs + this state's own lane
    /// seeds and require byte agreement (a stale or hand-edited layer fails
    /// closed). Absent file = `None`, the frozen names stand. Display only:
    /// the humanization layer is consulted for labels, never authorization.
    pub fn with_people(mut self) -> Result<AppState> {
        self.people =
            humanize::load_and_verify(&self.fixtures_dir, &self.company_sha256, &self.lane_seeds)?
                .map(Arc::new);
        Ok(self)
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

    /// SHOWCASE-III: wire the grounded-workflow proposal store (opened from
    /// `--state-dir`). Tests inject an in-temp-dir store here.
    pub fn with_wf_proposals(mut self, store: Arc<proposals::WorkflowProposalStore>) -> AppState {
        self.wf_proposals = Some(store);
        self
    }

    /// SHOWCASE-III: dial the per-principal proposal-generation rate (tests
    /// exercise the 429 branch).
    pub fn with_generation_rate(mut self, max: u32, window_secs: u64) -> AppState {
        self.generation_rate = Arc::new(ratelimit::PrincipalRateLimiter::new(max, window_secs));
        self
    }

    pub fn with_access_requests(
        mut self,
        store: Arc<access_requests::AccessRequestStore>,
    ) -> AppState {
        self.access_requests = Some(store);
        self
    }

    pub fn with_access_grants(mut self, store: Arc<access_grants::AccessGrantStore>) -> AppState {
        self.access_grants = Some(store);
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
    /// S0: the Entra agent-token bridge section. Absent — or present with
    /// `enabled: false` — wires NOTHING (default OFF; existing deployments
    /// are untouched until this is deliberately enabled).
    #[serde(default)]
    pub agent_bridge: Option<agent_bridge::AgentBridgeConfig>,
    /// S2b: the decision ledger, independently wired. Absent = no ledger
    /// from config (and, absent every other wiring, no `/v1` — the
    /// no-ledger ⇒ no-machine-surface invariant stands; only WHICH
    /// configuration brings the ledger to life changed). The `/v1` surface
    /// no longer depends on the M4 `--agents-config` flag to have a ledger.
    #[serde(default)]
    pub ledger: Option<LedgerConfig>,
    /// S3: the multi-source estate directory (holding `s3-access.json` +
    /// `s3-store/`). Absent = single-source. `None` keeps every existing
    /// deployment single-source until the estate is deliberately wired.
    #[serde(default)]
    pub estate_dir: Option<PathBuf>,
    /// S4: policy-deny alerting. Absent = alerting off (zero behaviour
    /// change). A malformed section fails startup LOUDLY naming the field.
    #[serde(default)]
    pub alerting: Option<AlertingConfig>,
    /// S5b bind amendment: the explicit listen address. ABSENT = the loopback
    /// default (`127.0.0.1:8787`) — the native invariant, byte-for-byte
    /// unchanged. PRESENT = the operator's explicit, config-recorded choice
    /// (the containerized demo binds `0.0.0.0:8787` INSIDE the container while
    /// the compose port mapping pins exposure to host-loopback). Config, not
    /// env magic: the deployment's exposure is readable in one file.
    #[serde(default)]
    pub bind: Option<String>,
}

/// S4: the `alerting` config section. `deny_unknown_fields` + the required
/// `alerts_path` mean a malformed section is a loud, field-named startup
/// failure (the explain-loudly-to-the-operator principle).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertingConfig {
    pub enabled: bool,
    /// The always-on durable file sink (append-only, fsync).
    pub alerts_path: PathBuf,
    /// Optional best-effort webhook (POST JSON, 3s timeout, 3 attempts).
    #[serde(default)]
    pub webhook_url: Option<String>,
}

/// S2b: where the decision ledger lives. Same store type, same file
/// format, same row schemas as ever — this is wiring, not schema.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerConfig {
    /// Directory holding the append-only ledger files.
    pub dir: PathBuf,
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
        // S0: the bridge exists only behind an EXPLICIT `enabled: true`.
        if let Some(bridge_config) = &self.agent_bridge {
            if bridge_config.enabled {
                let bridge = agent_bridge::Bridge::from_config(bridge_config)?;
                state = state.with_agent_bridge(Arc::new(bridge));
            }
        }
        // S2b: the decision ledger from config — the machine surface's
        // audit sink no longer rides the M4 --agents-config flag. S4: a
        // config-wired ledger is CHAINED (tamper-evident) and TIMESTAMPED
        // via the wall clock — an S4-era deployment gets tamper-evidence by
        // construction. The legacy `open()` writer (used by the exact-JSON
        // pin) stays byte-identical.
        if let Some(ledger) = &self.ledger {
            let clock: Arc<dyn crate::clock::Clock> = Arc::new(crate::clock::WallClock);
            let store = agent::proposals::ProposalStore::open_chained(&ledger.dir, clock)
                .with_context(|| format!("config.ledger.dir {}", ledger.dir.display()))?;
            state = state.with_proposals(Arc::new(store));
        }
        // S3: the multi-source estate, when the config names its directory.
        if let Some(estate_dir) = &self.estate_dir {
            state = state.with_estate_from(estate_dir)?;
        }
        // S4: policy-deny alerting, when the config enables it. Off the
        // request path; a wall clock stamps the alert `ts`, a `ureq` webhook
        // delivers best-effort.
        if let Some(alerting) = &self.alerting {
            if alerting.enabled {
                let clock: Arc<dyn crate::clock::Clock> = Arc::new(crate::clock::WallClock);
                let dispatcher = alerts::AlertDispatcher::new(
                    alerting.alerts_path.clone(),
                    alerting.webhook_url.clone(),
                    Arc::new(alerts::UreqWebhook),
                    clock,
                );
                state = state.with_alerts(Arc::new(dispatcher));
            }
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

/// S5b bind amendment: a listener at an EXPLICITLY CONFIGURED address.
/// [`loopback_listener`] remains the default path and its refusal stands
/// untouched; this one exists only for a config that NAMES its bind
/// (`ServiceConfig.bind`) — loopback-inside-a-container is fail-useless, so
/// the containerized gateway binds `0.0.0.0` and the intent of the invariant
/// (no external exposure by default) is enforced at the host boundary by the
/// compose mapping `127.0.0.1:8787:8787`. A non-loopback bind announces
/// itself loudly so no operator discovers it by accident.
pub fn configured_listener(addr: &str) -> Result<TcpListener> {
    let socket_addr: SocketAddr = addr
        .parse()
        .with_context(|| format!("invalid bind address {addr:?}"))?;
    if !is_loopback(&socket_addr.ip()) {
        eprintln!(
            "ask-brain: WARNING binding non-loopback {addr} (explicit `bind` config) — \
             exposure control now lives at the host boundary (compose host-loopback \
             port mapping / firewall); never expose this port to a network directly"
        );
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
    pub capability_id: Option<String>,
    #[serde(default)]
    pub grant_id: Option<String>,
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
        .route("/me/scope", get(handle_me_scope))
        .route("/healthz", get(handle_healthz))
        .route("/auth/login", post(handle_login))
        .route("/auth/logout", post(handle_logout))
        .route("/agent/{id}/run", post(handle_agent_run))
        .route("/lens/diff", get(handle_lens_diff))
        .route("/lens/{id}", get(handle_lens))
        .route("/atlas", get(handle_atlas))
        .route("/export", post(handle_export))
        .route("/lane", get(handle_lane))
        .route("/lane/box/{id}/status", post(handle_box_status))
        .route("/lane/inbox", get(handle_inbox))
        .route("/lane/inbox/{id}/accept", post(handle_inbox_accept))
        .route("/lane/inbox/{id}/dismiss", post(handle_inbox_dismiss))
        .route("/lane/rollup", get(handle_rollup))
        .route(
            "/workflow/project/{capability_id}",
            get(handle_project_workflow),
        )
        .route("/graph", get(handle_graph))
        .route("/node/{id}/summary", get(handle_node_summary))
        .route("/people", get(handle_people))
        .route("/access-requests", get(handle_access_requests_list))
        .route("/access-requests", post(handle_access_request_create))
        .route("/access-requests/inbox", get(handle_access_requests_inbox))
        .route("/access-grants", get(handle_access_grants_list))
        .route("/access-grants/{id}", get(handle_access_grant_get))
        .route(
            "/access-grants/{id}/revoke",
            post(handle_access_grant_revoke),
        )
        .route(
            "/access-requests/{id}/approve",
            post(handle_access_request_approve),
        )
        .route(
            "/access-requests/{id}/deny",
            post(handle_access_request_deny),
        )
        .route("/proposals", get(handle_proposals_list))
        .route("/proposals/{id}/approve", post(handle_proposal_approve))
        .route("/proposals/{id}/reject", post(handle_proposal_reject))
        // S1: the /v1 machine surface (agent tokens only; gated in
        // require_session BEFORE routing — see v1_gate).
        .route("/v1/retrieve", post(handle_v1_retrieve))
        // Catch-all `{*id}`: second-source ids are path-like
        // (`s3/<bucket>/<key>`), so the id spans multiple segments.
        .route("/v1/documents/{*id}", get(handle_v1_document))
        .route("/v1/whoami", get(handle_v1_whoami))
        // SHOWCASE-III: grounded workflow proposals (the first mutation
        // path — session-classed, so on the human surface by S1's split).
        .route("/workflow/proposals", get(handle_workflow_proposals_list))
        .route("/workflow/proposals", post(handle_workflow_proposal_create))
        .route("/workflow/proposals/{id}", get(handle_workflow_proposal_get))
        .route(
            "/workflow/proposals/{id}/approve",
            post(handle_workflow_proposal_approve),
        )
        .route(
            "/workflow/proposals/{id}/deny",
            post(handle_workflow_proposal_deny),
        )
        // Inner: every route except /healthz and /auth/login requires a valid
        // server session; the principal is resolved from it (FC-A1).
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_session,
        ))
        // Outer: CORS wraps everything so it answers preflights and stamps
        // headers onto all responses — including the 401 from require_session.
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
    let granted_context = match (request.grant_id, request.capability_id) {
        (Some(grant_id), Some(capability_id)) => Some(answer::GrantedContextRequest {
            capability_id,
            grant_id,
        }),
        (None, None) => None,
        _ => {
            return json_bytes_response(
                StatusCode::BAD_REQUEST,
                b"{\"demo_identity_mode\":true,\"error\":\"granted context requires grant_id and capability_id\"}\n".to_vec(),
            );
        }
    };
    let options = AskOptions {
        hybrid: request.hybrid,
        judge: request.judge,
        bypass_cache: granted_context.is_some(),
        granted_context,
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

// ---------------------------------------------------------------------------
// S1: the /v1 machine surface — retrieve / documents / whoami
// ---------------------------------------------------------------------------

/// `/v1/retrieve` query limit (characters) — also the ledger cap.
pub const V1_QUERY_MAX_CHARS: usize = 2_048;
/// `/v1/retrieve` request-body limit (bytes).
pub const V1_BODY_MAX_BYTES: usize = 16_384;
pub const V1_TOP_K_DEFAULT: usize = 8;
pub const V1_TOP_K_MAX: usize = 50;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct V1RetrieveRequest {
    query: String,
    #[serde(default)]
    top_k: Option<u64>,
}

fn v1_bad_request() -> Response {
    json_bytes_response(
        StatusCode::BAD_REQUEST,
        b"{\"demo_identity_mode\":true,\"error\":\"bad request\"}\n".to_vec(),
    )
}

fn v1_payload_too_large() -> Response {
    json_bytes_response(
        StatusCode::PAYLOAD_TOO_LARGE,
        b"{\"demo_identity_mode\":true,\"error\":\"payload too large\"}\n".to_vec(),
    )
}

fn v1_internal() -> Response {
    json_bytes_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
    )
}

/// The ledger copy of a retrieve query: verbatim up to the cap.
fn v1_capped_query(query: &str) -> String {
    query.chars().take(V1_QUERY_MAX_CHARS).collect()
}

/// GET /v1/whoami — the SDK handshake diagnostic: WHO resolved, and nothing
/// else. Deliberately no scope information (no allowlist, no counts, no
/// department): whoami is not an enumeration surface.
async fn handle_v1_whoami(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(principal): DemoPrincipal,
    axum::extract::Extension(context): axum::extract::Extension<V1TokenContext>,
) -> Response {
    let Some(store) = &state.proposals else {
        return identity::unauthorized();
    };
    if let Err(err) = store.audit_v1(
        "v1_whoami",
        &principal,
        "GET /v1/whoami",
        "authorized",
        &context.0,
        None,
        None,
        None,
        None,
    ) {
        eprintln!("v1 audit failed: {err:#}");
        return v1_internal();
    }
    let display = humanize::display_name_or(state.people.as_deref(), &principal, "");
    let body = if display.is_empty() {
        serde_json::json!({ "principal_id": principal })
    } else {
        serde_json::json!({ "principal_id": principal, "display_name": display })
    };
    match retrieval::index::canonical_json_bytes(&body) {
        Ok(bytes) => json_bytes_response(StatusCode::OK, bytes),
        Err(err) => {
            eprintln!("whoami serialization failed: {err:#}");
            v1_internal()
        }
    }
}

/// S2b: the `/v1` document response body cap — 2 MiB. A body past the cap
/// FAILS LOUD (generic 500 + ledger reason `body_exceeds_cap`): never
/// truncated (a partial document silently poisons downstream generation)
/// and never a 404 (which would lie about existence to an AUTHORIZED
/// principal). The fixture corpus tops out ~2.8 KB; a recon test pins the
/// margin.
pub const V1_CONTENT_MAX_BYTES: usize = 2 * 1024 * 1024;

/// GET /v1/documents/{id} — the machine-surface document fetch: the SAME
/// compiled-scope decision as the console `/doc` (S0's row 11, unmoved),
/// serving the FULL authorized body. The compiled allowlist is the
/// boundary; truncating an authorized read is not defense-in-depth
/// (S2b product ruling). The console's DocCard snippet law is a CONSOLE
/// law and stands there untouched — two surfaces, two consumers, two
/// laws. `content` comes from the hash-verified in-memory corpus (the
/// same bytes the M1 pin verified at startup), never an ad-hoc disk
/// read. Unauthorized and nonexistent share THE one 404, byte-identical
/// (S1-3).
async fn handle_v1_document(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(principal): DemoPrincipal,
    axum::extract::Extension(context): axum::extract::Extension<V1TokenContext>,
    axum::extract::Path(doc_id): axum::extract::Path<String>,
) -> Response {
    let Some(store) = &state.proposals else {
        return identity::unauthorized();
    };
    let target = format!("GET /v1/documents/{doc_id}");
    let blocking_state = state.clone();
    let blocking_principal = principal.clone();
    let blocking_doc = doc_id.clone();
    // Resolution runs off the async runtime: it may touch the compiled
    // artifacts (primary path). The estate paths are pure in-memory reads.
    let resolved = tokio::task::spawn_blocking(move || {
        resolve_v1_document(&blocking_state, &blocking_principal, &blocking_doc)
    })
    .await;
    let served = match resolved {
        Ok(Ok(served)) => served,
        Ok(Err(err)) => {
            eprintln!("v1 document lookup failed: {err:#}");
            return v1_internal();
        }
        Err(join_error) => {
            eprintln!("v1 document task failed: {join_error}");
            return v1_internal();
        }
    };
    let Some(served) = served else {
        // Out-of-scope OR nonexistent OR across-the-seam-denied — the
        // ledger records the deny; the wire cannot tell them apart (S1-3).
        let source = source_of(&doc_id);
        match store.audit_v1(
            "v1_document",
            &principal,
            &target,
            "not_found",
            &context.0,
            None,
            None,
            None,
            Some(source),
        ) {
            // S4: a validated principal denied a resource-level decision —
            // the policy-class deny the security team wants to see. The
            // alert is DERIVED FROM the ledger row (by its ordinal) and
            // dispatched OFF the request path.
            Ok(ordinal) => alert_policy_deny(
                &state,
                &principal,
                &context.0,
                &doc_id,
                source,
                "not in compiled scope (out-of-scope or nonexistent — indistinguishable)",
                ordinal,
            ),
            Err(err) => {
                eprintln!("v1 audit failed: {err:#}");
                return v1_internal();
            }
        }
        return doc_not_found();
    };
    if served.content.len() > V1_CONTENT_MAX_BYTES {
        // Fail LOUD: ledgered, generic 500, never truncated, never 404.
        match store.audit_v1(
            "v1_document",
            &principal,
            &target,
            "body_exceeds_cap",
            &context.0,
            None,
            None,
            None,
            Some(&served.source),
        ) {
            Ok(ordinal) => alert_policy_deny(
                &state,
                &principal,
                &context.0,
                &doc_id,
                &served.source,
                "body exceeds the response cap (not in compiled scope for delivery)",
                ordinal,
            ),
            Err(err) => eprintln!("v1 audit failed: {err:#}"),
        }
        return v1_internal();
    }
    let body = serde_json::json!({
        "doc_id": served.doc_id,
        "title": served.title,
        "snippet": served.snippet,
        "content": served.content,
        "metadata": served.metadata,
    });
    match retrieval::index::canonical_json_bytes(&body) {
        Ok(bytes) => {
            if let Err(err) = store.audit_v1(
                "v1_document",
                &principal,
                &target,
                "authorized",
                &context.0,
                None,
                None,
                Some("full"),
                Some(&served.source),
            ) {
                eprintln!("v1 audit failed: {err:#}");
                return v1_internal();
            }
            json_bytes_response(StatusCode::OK, bytes)
        }
        Err(err) => {
            eprintln!("v1 document serialization failed: {err:#}");
            v1_internal()
        }
    }
}

/// S4: derive a policy-deny alert from a just-written ledger deny row and
/// hand it to the dispatcher (which fires OFF the request path). A no-op
/// when alerting is disabled. Called ONLY from the resource-level deny
/// branches of `handle_v1_document` — a validated principal denied a
/// document — never for auth-ladder denies (those are fenced).
fn alert_policy_deny(
    state: &AppState,
    principal: &str,
    claims: &agent::proposals::TokenAuditFields,
    resource: &str,
    source: &str,
    reason: &str,
    ledger_ordinal: u64,
) {
    let Some(dispatcher) = &state.alerts else {
        return;
    };
    dispatcher.dispatch(alerts::AlertInput {
        principal_id: principal.to_string(),
        claims: alerts::AlertClaims {
            tid: claims.tid.clone(),
            oid: claims.oid.clone(),
            azp: claims.azp.clone(),
        },
        resource: resource.to_string(),
        source: source.to_string(),
        decision_basis: reason.to_string(),
        ledger_ordinal,
    });
}

/// S3 conformance seam: the composed `/v1` document authorization decision
/// for any (principal, doc_id) across the estate — `true` iff the principal
/// may read the document. This is EXACTLY what `handle_v1_document` gates on
/// (it wraps the same resolver), so the estate conformance can drive all
/// 94,500 (principal × document) pairs in-process, at the same authority the
/// wire enforces.
pub fn v1_document_authorized(
    state: &AppState,
    principal: &str,
    doc_id: &str,
) -> anyhow::Result<bool> {
    Ok(resolve_v1_document(state, principal, doc_id)?.is_some())
}

/// A resolved, authorized document ready to serve on `/v1`.
struct ServedDocument {
    doc_id: String,
    title: String,
    snippet: String,
    content: String,
    source: String,
    metadata: serde_json::Value,
}

/// Which source a doc id names — `s3/...` is the second source, everything
/// else is primary. (Ids carry the source; the candidate envelope is
/// unchanged.)
fn source_of(doc_id: &str) -> &'static str {
    if doc_id.starts_with("s3/") {
        "s3"
    } else {
        "primary"
    }
}

/// The composed authorization + resolution for `GET /v1/documents/{id}`,
/// across the seam. `None` is THE 404 (denied OR nonexistent — never
/// distinguished). Three cases:
///   * an `s3/...` id → the SECOND source, authorized by the estate tier
///     rule (estate agents only; everyone else denied — the fail-closed
///     seam default);
///   * a primary id, estate agent → the FIRST source, authorized by the
///     SAME tier rule over the primary doc's sensitivity;
///   * a primary id, any other principal → the existing compiled-allowlist
///     path (`doc_card`), completely unchanged.
fn resolve_v1_document(
    state: &AppState,
    principal: &str,
    doc_id: &str,
) -> anyhow::Result<Option<ServedDocument>> {
    // Second source.
    if doc_id.starts_with("s3/") {
        let Some(estate) = &state.estate else {
            return Ok(None);
        };
        let Some(object) = estate.object(doc_id) else {
            return Ok(None);
        };
        if !estate.can_read(principal, &object.sensitivity) {
            return Ok(None);
        }
        return Ok(Some(ServedDocument {
            doc_id: object.doc_id.clone(),
            title: object.title.clone(),
            snippet: retrieval::vector::snippet_of(&object.body, answer::CONTEXT_SNIPPET_CHARS),
            content: object.body.clone(),
            source: "s3".to_string(),
            metadata: serde_json::json!({
                "sensitivity": object.sensitivity,
                "source": "s3",
                "bucket": object.bucket,
            }),
        }));
    }

    // First source, estate agent: the SAME tier rule over the primary
    // doc's sensitivity (a distinct authority model from the compiled
    // allowlist — the estate agents are new principals with tier grants).
    if let Some(estate) = &state.estate {
        if estate.is_estate_agent(principal) {
            let Some(meta) = state.docs.get(doc_id) else {
                return Ok(None);
            };
            if !estate.can_read(principal, &meta.sensitivity) {
                return Ok(None);
            }
            return Ok(Some(ServedDocument {
                doc_id: doc_id.to_string(),
                title: meta.title.clone(),
                snippet: retrieval::vector::snippet_of(&meta.body, answer::CONTEXT_SNIPPET_CHARS),
                content: meta.body.clone(),
                source: "primary".to_string(),
                metadata: serde_json::json!({
                    "sensitivity": meta.sensitivity,
                    "source": "primary",
                }),
            }));
        }
    }

    // First source, existing principal: the compiled-allowlist path,
    // unchanged. `source: "primary"` is added to the metadata (every
    // document carries its source), nothing else moves.
    let Some(card) = answer::doc_card(state, principal, doc_id)? else {
        return Ok(None);
    };
    let Some(meta) = state.docs.get(doc_id) else {
        anyhow::bail!("{doc_id} authorized but absent from the corpus");
    };
    Ok(Some(ServedDocument {
        doc_id: card.document_id.clone(),
        title: card.title.clone(),
        snippet: card.snippet.clone(),
        content: meta.body.clone(),
        source: "primary".to_string(),
        metadata: v1_document_metadata(&card),
    }))
}

/// The `/v1` document metadata sub-object for a primary compiled-path
/// document: the card's non-core facts (sensitivity + supersession) plus
/// `source: "primary"` — exactly what the console card discloses beyond
/// id/title/snippet, and the source tag every document carries.
fn v1_document_metadata(card: &answer::DocCard) -> serde_json::Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "sensitivity".to_string(),
        serde_json::Value::String(card.sensitivity.clone()),
    );
    metadata.insert(
        "source".to_string(),
        serde_json::Value::String("primary".to_string()),
    );
    if let Some(superseded) = card.superseded {
        metadata.insert("superseded".to_string(), serde_json::Value::Bool(superseded));
    }
    if let Some(successor) = &card.effective_successor {
        metadata.insert(
            "effective_successor".to_string(),
            serde_json::Value::String(successor.clone()),
        );
    }
    serde_json::Value::Object(metadata)
}

/// POST /v1/retrieve — the core: governed retrieval scoped AT QUERY
/// CONSTRUCTION to the resolved principal's compiled allowlist (EB-5). An
/// out-of-scope document is never a candidate — not as an id, a title, a
/// snippet, or a rank entry. Empty results are a 200, not an error. The
/// `rank` field is the 1-based fused rank (1 = best) — named for what it
/// IS: under the name "score" every consumer would sort descending and
/// silently invert the ordering. Raw similarity scores are never
/// serialized (the M2a envelope invariant); `/v1` serializes rank only,
/// never raw similarity.
async fn handle_v1_retrieve(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(principal): DemoPrincipal,
    axum::extract::Extension(context): axum::extract::Extension<V1TokenContext>,
    request: Request,
) -> Response {
    let Some(store) = &state.proposals else {
        return identity::unauthorized();
    };
    let target = "POST /v1/retrieve";

    // Limits first, cheapest first. Violations are ledgered (the ledger is
    // the audit surface) and answered generically.
    let body = request.into_body();
    let bytes = match axum::body::to_bytes(body, V1_BODY_MAX_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => {
            audit_v1_deny(
                &state,
                "v1_retrieve",
                target,
                "payload_oversize",
                &context.0,
            );
            return v1_payload_too_large();
        }
    };
    let parsed: V1RetrieveRequest = match serde_json::from_slice(&bytes) {
        Ok(parsed) => parsed,
        Err(_) => {
            audit_v1_deny(&state, "v1_retrieve", target, "bad_request", &context.0);
            return v1_bad_request();
        }
    };
    let query_chars = parsed.query.chars().count();
    if query_chars == 0 || query_chars > V1_QUERY_MAX_CHARS {
        if let Err(err) = store.audit_v1(
            "v1_retrieve",
            &principal,
            target,
            "query_out_of_range",
            &context.0,
            Some(&v1_capped_query(&parsed.query)),
            None,
            None,
            None,
        ) {
            eprintln!("v1 audit failed: {err:#}");
        }
        return v1_bad_request();
    }
    let top_k = match parsed.top_k {
        None => V1_TOP_K_DEFAULT,
        Some(k) if (1..=V1_TOP_K_MAX as u64).contains(&k) => k as usize,
        Some(_) => {
            if let Err(err) = store.audit_v1(
                "v1_retrieve",
                &principal,
                target,
                "top_k_out_of_range",
                &context.0,
                Some(&v1_capped_query(&parsed.query)),
                None,
                None,
                None,
            ) {
                eprintln!("v1 audit failed: {err:#}");
            }
            return v1_bad_request();
        }
    };

    // Candidate resolution, composed across the seam:
    //   * an estate agent → source-spanning estate retrieval (both sources,
    //     authorize-first);
    //   * a known primary principal → the existing compiled retrieval;
    //   * anyone else → the empty result (deny by default, house rule).
    let is_estate_agent = state
        .estate
        .as_ref()
        .is_some_and(|e| e.is_estate_agent(&principal));
    let candidates: Vec<serde_json::Value> = if is_estate_agent {
        let blocking_state = state.clone();
        let blocking_principal = principal.clone();
        let blocking_query = parsed.query.clone();
        let searched = tokio::task::spawn_blocking(move || {
            let estate = blocking_state
                .estate
                .as_ref()
                .expect("estate agent implies an estate");
            let index = blocking_state
                .estate_index
                .as_ref()
                .expect("estate agent implies an estate index");
            // S5a: retrieve off the INGEST-built index — no corpus body is
            // re-tokenized here. Authority is a MUST clause on candidacy
            // (EB-5, inside construction): the tier predicate admits a
            // candidate, it never post-filters a wider result.
            index
                .retrieve(&blocking_query, top_k, |sensitivity| {
                    estate.can_read(&blocking_principal, sensitivity)
                })
                .iter()
                .enumerate()
                .map(|(rank, candidate)| {
                    serde_json::json!({
                        "doc_id": candidate.doc_id,
                        "title": candidate.title,
                        "snippet": retrieval::vector::snippet_of(
                            candidate.body,
                            answer::CONTEXT_SNIPPET_CHARS,
                        ),
                        "rank": rank as u32 + 1,
                    })
                })
                .collect::<Vec<_>>()
        })
        .await;
        match searched {
            Ok(candidates) => candidates,
            Err(join_error) => {
                eprintln!("v1 estate retrieve task failed: {join_error}");
                return v1_internal();
            }
        }
    } else if !state.identity.is_known(&principal) {
        Vec::new()
    } else {
        let blocking_state = state.clone();
        let blocking_principal = principal.clone();
        let blocking_query = parsed.query.clone();
        let searched = tokio::task::spawn_blocking(move || {
            let scope = retrieval::search::PrincipalScope::load(
                &blocking_state.artifacts_dir,
                &blocking_principal,
            )
            .context("loading the principal's compiled allowlist")?;
            let options = retrieval::search::SearchOptions {
                k: top_k,
                include_superseded: false,
                hybrid: None,
                judge: None,
            };
            let (envelope, _trace) = blocking_state
                .engine
                .search(&scope, &blocking_query, &options)
                .context("governed retrieval failed")?;
            let mut out = Vec::with_capacity(envelope.results.len());
            for result in &envelope.results {
                let meta = blocking_state
                    .docs
                    .get(&result.document_id)
                    .context("result id missing from the verified corpus")?;
                out.push(serde_json::json!({
                    "doc_id": result.document_id,
                    "title": meta.title,
                    "snippet": retrieval::vector::snippet_of(&meta.body, answer::CONTEXT_SNIPPET_CHARS),
                    "rank": result.score_rank,
                }));
            }
            anyhow::Ok(out)
        })
        .await;
        match searched {
            Ok(Ok(candidates)) => candidates,
            Ok(Err(err)) => {
                eprintln!("v1 retrieve failed: {err:#}");
                return v1_internal();
            }
            Err(join_error) => {
                eprintln!("v1 retrieve task failed: {join_error}");
                return v1_internal();
            }
        }
    };

    // The allow row — query verbatim (capped) + the candidate ids — lands
    // BEFORE the response reaches the wire.
    let candidate_ids: Vec<String> = candidates
        .iter()
        .filter_map(|c| c["doc_id"].as_str().map(str::to_string))
        .collect();
    if let Err(err) = store.audit_v1(
        "v1_retrieve",
        &principal,
        target,
        "authorized",
        &context.0,
        Some(&v1_capped_query(&parsed.query)),
        Some(&candidate_ids),
        None,
        None,
    ) {
        eprintln!("v1 audit failed: {err:#}");
        return v1_internal();
    }
    let body = serde_json::json!({ "principal": principal, "candidates": candidates });
    match retrieval::index::canonical_json_bytes(&body) {
        Ok(bytes) => json_bytes_response(StatusCode::OK, bytes),
        Err(err) => {
            eprintln!("v1 retrieve serialization failed: {err:#}");
            v1_internal()
        }
    }
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
    // AUTH-3 (FC-A3): the cross-principal gate now lives in diff::diff_view (the
    // source), so it also covers the export path; the handler just relays the
    // one-404 / 200 it returns.
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

fn lane_bad_request(message: &str) -> Response {
    json_bytes_response(
        StatusCode::BAD_REQUEST,
        format!(
            "{{\"demo_identity_mode\":true,\"error\":{}}}\n",
            serde_json::to_string(message).unwrap_or_else(|_| "\"bad request\"".into())
        )
        .into_bytes(),
    )
}

fn lane_conflict(message: &str) -> Response {
    json_bytes_response(
        StatusCode::CONFLICT,
        format!(
            "{{\"demo_identity_mode\":true,\"error\":{}}}\n",
            serde_json::to_string(message).unwrap_or_else(|_| "\"conflict\"".into())
        )
        .into_bytes(),
    )
}

fn lane_internal(err: anyhow::Error, what: &str) -> Response {
    eprintln!("{what} failed: {err:#}");
    json_bytes_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
    )
}

/// GET /lane — the v4a Workflow Surface, SELF-ONLY BY CONSTRUCTION
/// (invariant 2): no subject parameter exists; query strings are ignored
/// as everywhere in axum. Agents and unknowns get the same empty shape.
async fn handle_lane(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
) -> Response {
    let blocking_state = state.clone();
    let result =
        tokio::task::spawn_blocking(move || lane::lane_view(&blocking_state, &actor)).await;
    match result {
        Ok(Ok(Some(bytes))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Ok(None)) => doc_not_found(),
        Ok(Err(AskError::BadRequest(message))) => lane_bad_request(&message),
        Ok(Err(AskError::Internal(err))) => lane_internal(err, "lane"),
        Err(join_error) => lane_internal(anyhow::anyhow!("{join_error}"), "lane task"),
    }
}

/// POST /lane/box/{id}/status — a human act on the actor's own box,
/// audited before effect; illegal transitions and blocked boxes refuse
/// ROWLESS (AW-8).
async fn handle_box_status(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
    axum::extract::Path(box_id): axum::extract::Path<String>,
    body: axum::body::Bytes,
) -> Response {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StatusRequest {
        to: String,
    }
    let request: StatusRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(err) => {
            eprintln!("box status request refused: {err}");
            return lane_bad_request("status request fails strict parse");
        }
    };
    let blocking_state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        lane::status_change(&blocking_state, &actor, &box_id, &request.to)
    })
    .await;
    match result {
        Ok(Ok(lane::StatusOutcome::Applied(bytes))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Ok(lane::StatusOutcome::NotFound)) => doc_not_found(),
        Ok(Ok(lane::StatusOutcome::Refused(message))) => lane_conflict(&message),
        Ok(Err(AskError::BadRequest(message))) => lane_bad_request(&message),
        Ok(Err(AskError::Internal(err))) => lane_internal(err, "box status"),
        Err(join_error) => lane_internal(anyhow::anyhow!("{join_error}"), "box status task"),
    }
}

/// GET /lane/inbox — the actor's agents' pending proposals as candidate
/// previews (a read-only join over the M4 store).
async fn handle_inbox(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
) -> Response {
    let blocking_state = state.clone();
    let result =
        tokio::task::spawn_blocking(move || lane::inbox_view(&blocking_state, &actor)).await;
    match result {
        Ok(Ok(Some(bytes))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Ok(None)) => doc_not_found(),
        Ok(Err(AskError::BadRequest(message))) => lane_bad_request(&message),
        Ok(Err(AskError::Internal(err))) => lane_internal(err, "inbox"),
        Err(join_error) => lane_internal(anyhow::anyhow!("{join_error}"), "inbox task"),
    }
}

async fn handle_inbox_decision(
    state: Arc<AppState>,
    actor: String,
    proposal_id: String,
    accept: bool,
) -> Response {
    let result = tokio::task::spawn_blocking(move || {
        lane::inbox_decide(&state, &actor, &proposal_id, accept)
    })
    .await;
    match result {
        Ok(Ok(lane::InboxOutcome::Done(bytes))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Ok(lane::InboxOutcome::NotFound)) => doc_not_found(),
        Ok(Ok(lane::InboxOutcome::Forbidden)) => json_bytes_response(
            StatusCode::FORBIDDEN,
            b"{\"demo_identity_mode\":true,\"error\":\"forbidden\"}\n".to_vec(),
        ),
        Ok(Ok(lane::InboxOutcome::Conflict(message))) => lane_conflict(&message),
        Ok(Err(AskError::BadRequest(message))) => lane_bad_request(&message),
        Ok(Err(AskError::Internal(err))) => lane_internal(err, "inbox decision"),
        Err(join_error) => lane_internal(anyhow::anyhow!("{join_error}"), "inbox decision task"),
    }
}

/// POST /lane/inbox/{id}/accept — owner-only, human-only (M4's authority
/// pattern, audited refusals); accept materializes a candidate box and
/// approves the proposal through the existing M4 machinery.
async fn handle_inbox_accept(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
    axum::extract::Path(proposal_id): axum::extract::Path<String>,
) -> Response {
    handle_inbox_decision(state, actor, proposal_id, true).await
}

/// POST /lane/inbox/{id}/dismiss — the symmetric refusal: the proposal is
/// rejected through the same machinery; no box materializes.
async fn handle_inbox_dismiss(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
    axum::extract::Path(proposal_id): axum::extract::Path<String>,
) -> Response {
    handle_inbox_decision(state, actor, proposal_id, false).await
}

/// GET /lane/rollup — the structurally anonymous manager view (invariant
/// 3): status counts by capability at the N=5 floor, the fixed honesty
/// statement, nothing else.
async fn handle_rollup(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(_actor): DemoPrincipal,
) -> Response {
    let blocking_state = state.clone();
    let result = tokio::task::spawn_blocking(move || lane::rollup_view(&blocking_state)).await;
    match result {
        Ok(Ok(Some(bytes))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Ok(None)) => doc_not_found(),
        Ok(Err(AskError::BadRequest(message))) => lane_bad_request(&message),
        Ok(Err(AskError::Internal(err))) => lane_internal(err, "rollup"),
        Err(join_error) => lane_internal(anyhow::anyhow!("{join_error}"), "rollup task"),
    }
}

/// GET /workflow/project/{capability_id}: a read-only project execution
/// surface. It projects existing lane boxes, accepted agent boxes, and access
/// request rows; it never exposes evidence or creates task rows.
async fn handle_project_workflow(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
    axum::extract::Path(capability_id): axum::extract::Path<String>,
) -> Response {
    let blocking_state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        workflow::project_workflow_view(&blocking_state, &actor, &capability_id)
    })
    .await;
    match result {
        Ok(Ok(Some(bytes))) => json_bytes_response(StatusCode::OK, bytes),
        Ok(Ok(None)) => doc_not_found(),
        Ok(Err(AskError::BadRequest(message))) => lane_bad_request(&message),
        Ok(Err(AskError::Internal(err))) => lane_internal(err, "project workflow"),
        Err(join_error) => lane_internal(anyhow::anyhow!("{join_error}"), "project workflow task"),
    }
}

/// GET /graph — the Org Graph (AR-2): the org's shape as nodes + edges,
/// INTERNAL-GRADE and consistent with /people (no holding, no count, no
/// document id ever appears). An unknown actor or a world with no humanization
/// layer answers THE one 404 (the no-standing discipline).
async fn handle_graph(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
) -> Response {
    let blocking_state = state.clone();
    let result =
        tokio::task::spawn_blocking(move || graph::graph_view(&blocking_state, &actor)).await;
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
            eprintln!("graph failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
        Err(join_error) => {
            eprintln!("graph task failed: {join_error}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
    }
}

/// GET /node/{id}/summary — the org-graph inspector's data. Org core -> the
/// corpus cardinalities; a person/agent -> compiled scope + reason-grouped
/// access COUNTS (never the documents); a non-principal -> the one 404.
/// Metadata only: it can name no document (GR-7 enforces it).
async fn handle_node_summary(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(actor): DemoPrincipal,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Response {
    let blocking_state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        node_summary::node_summary(&blocking_state, &actor, &id)
    })
    .await;
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
            eprintln!("node summary failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
        Err(join_error) => {
            eprintln!("node summary task failed: {join_error}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
    }
}

/// GET /people — the org-structural directory (AR-1). Returns every humanized
/// principal's card: id, display name, title, department, avatar ref —
/// NEVER a holding or document id. INTERNAL-GRADE, the same tier the Atlas
/// BRM structure already publishes (names and titles, not private evidence),
/// and demo-open like every surface here. PRODUCTION SWAP POINT: in a real
/// deployment the roster is itself a permissioned resource (admin-classed, or
/// a self-plus-reports view) — gate THIS handler and nothing else moves. A
/// world without a humanization layer returns an empty roster.
async fn handle_people(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(_principal): DemoPrincipal,
) -> Response {
    #[derive(serde::Serialize)]
    struct PeopleResponse {
        demo_identity_mode: bool,
        people: Vec<humanize::PersonCard>,
    }
    let people: Vec<humanize::PersonCard> = state
        .people
        .as_ref()
        .map(|layer| {
            layer
                .roster()
                .map(humanize::PersonCard::from_record)
                .collect()
        })
        .unwrap_or_default();
    match retrieval::index::canonical_json_bytes(&PeopleResponse {
        demo_identity_mode: true,
        people,
    }) {
        Ok(bytes) => json_bytes_response(StatusCode::OK, bytes),
        Err(err) => {
            eprintln!("people serialization failed: {err:#}");
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

/// GET /me/scope - server-derived role/scope posture for the current demo
/// principal. It grants nothing and always reports enforcement as derived_only.
async fn handle_me_scope(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(principal): DemoPrincipal,
) -> Response {
    let blocking_state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        role_scope::role_scope_summary(&blocking_state, &principal)
    })
    .await;
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
            eprintln!("role scope failed: {err:#}");
            json_bytes_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n".to_vec(),
            )
        }
        Err(join_error) => {
            eprintln!("role scope task failed: {join_error}");
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
// FC-A1: authentication — server-minted sessions. Identity is bound from a
// validated session, NEVER a caller-asserted header.
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct LoginRequest {
    principal_id: String,
    /// The real-deployment path verifies this against the principal's stored
    /// hashed credential. The demo path (demo_identity_mode) accepts selection
    /// without one — the session is still server-minted, so identity stays
    /// unforgeable.
    #[serde(default)]
    #[allow(dead_code)]
    credential: Option<String>,
}

/// POST /auth/login — authenticate and mint a session. Public (no session
/// required). Demo path: any non-empty principal selection is accepted without
/// a credential, preserving "see as X"; downstream deny-by-default still
/// governs what the resulting principal can see. Sets an httpOnly, SameSite=Lax
/// session cookie AND returns the bearer token (the console is cross-origin and
/// uses the bearer; the cookie covers same-origin/direct use).
async fn handle_login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Response {
    // AUTH-4 (D1): cheap-reject login floods BEFORE any minting work. Every
    // attempt (including malformed ones) counts toward the window.
    if !state.login_rate.check() {
        return json_bytes_response(
            StatusCode::TOO_MANY_REQUESTS,
            b"{\"demo_identity_mode\":true,\"error\":\"too many login attempts\"}\n".to_vec(),
        );
    }
    let principal = req.principal_id.trim();
    if principal.is_empty() {
        return json_bytes_response(
            StatusCode::BAD_REQUEST,
            b"{\"demo_identity_mode\":true,\"error\":\"principal_id is required\"}\n".to_vec(),
        );
    }
    // AUTH-4 (D1): a principal may hold at most `quota` concurrent sessions;
    // excess is rejected rather than minted.
    let Some(minted) = state.sessions.try_mint(principal) else {
        return json_bytes_response(
            StatusCode::TOO_MANY_REQUESTS,
            b"{\"demo_identity_mode\":true,\"error\":\"session quota exceeded\"}\n".to_vec(),
        );
    };
    let body = format!(
        "{{\"demo_identity_mode\":true,\"principal_id\":{},\"session_token\":{},\"expires_at\":{}}}\n",
        serde_json::Value::from(minted.principal_id.as_str()),
        serde_json::Value::from(minted.token.as_str()),
        minted.expires_at
    );
    let cookie = format!(
        "{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}",
        session::SESSION_COOKIE,
        minted.token,
        session::SESSION_TTL_SECS
    );
    let mut response = json_bytes_response(StatusCode::OK, body.into_bytes());
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

/// POST /auth/logout — revoke the current session (protected: the middleware
/// already required a valid one). Clears the cookie.
async fn handle_logout(State(state): State<Arc<AppState>>, request: Request) -> Response {
    if let Some(token) = bearer_token(request.headers()).or_else(|| cookie_token(request.headers()))
    {
        state.sessions.revoke(&token);
    }
    let cleared = format!(
        "{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        session::SESSION_COOKIE
    );
    let mut response = json_bytes_response(
        StatusCode::OK,
        b"{\"demo_identity_mode\":true,\"status\":\"logged_out\"}\n".to_vec(),
    );
    if let Ok(value) = HeaderValue::from_str(&cleared) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response
}

/// AUTH-4 (M1) + FC-A1: the route-classification + session middleware. Every
/// request is classified by [`routes::classify`] for its (method, path):
///   * `Public` (`/healthz`, `/auth/login`) runs without a session;
///   * `SessionRequired` runs only behind a valid session — the resolved
///     principal is placed in the request extensions for `DemoPrincipal`;
///     no / expired / invalid / revoked session -> 401;
///   * UNCLASSIFIED (anything `classify` does not recognise, incl. unknown
///     paths and wrong methods) is DENIED, never served (fail-closed) — the
///     standing `governance_routes` test fails the build if a registered route
///     is left unclassified, so a new route cannot be silently exposed.
/// OPTIONS preflights carry no auth (the CORS layer answers them) and pass
/// through; HEAD shares its GET route's classification.
async fn require_session(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    // S1-1: the /v1 machine surface is gated BEFORE everything — before the
    // OPTIONS pass-through (no preflight leniency on a non-browser surface),
    // before HEAD folding (the machine surface is strict-method), before
    // route classification (auth first, routing second: an unauthenticated
    // probe cannot map the namespace).
    if cors::is_v1_path(request.uri().path()) {
        return v1_gate(state, request, next).await;
    }
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }
    let method = if request.method() == Method::HEAD {
        Method::GET
    } else {
        request.method().clone()
    };
    let path = request.uri().path().to_string();
    match routes::classify(&method, &path) {
        None => identity::route_denied(),
        Some(routes::RouteClass::Public) => next.run(request).await,
        // Defensive: /v1 routes never reach here (the prefix gate above
        // runs first), but a classification exists for AUTH-4's registry
        // and any future wiring drift still lands in the same gate.
        Some(routes::RouteClass::AgentTokenRequired(_scope)) => v1_gate(state, request, next).await,
        Some(routes::RouteClass::SessionRequired(_scope)) => {
            // S1-1 (console side): sessions ONLY. A JWT-shaped bearer
            // (dotted — opaque session tokens are 64 hex chars and can
            // never contain a dot) is NEVER consulted here: with a valid
            // session cookie the session authenticates the request and the
            // bridge is not involved; without one, the request is denied
            // generically. No credential class crosses surfaces, no
            // fallback in either direction.
            match console_session_principal(request.headers(), &state) {
                Some(principal) => {
                    request.extensions_mut().insert(SessionPrincipal(principal));
                    next.run(request).await
                }
                None => {
                    if bearer_token(request.headers())
                        .is_some_and(|credential| credential.contains('.'))
                    {
                        // A machine credential knocking on the human door —
                        // ledgered as a monitoring signal (EB-7), denied
                        // generically, never validated (its claims are not
                        // parsed, so nothing about it is trusted or echoed).
                        let target = format!("{} {}", request.method(), request.uri().path());
                        audit_token_deny_reason(
                            &state,
                            &target,
                            "jwt_on_console_surface",
                            &Default::default(),
                        );
                    }
                    identity::unauthorized()
                }
            }
        }
    }
}

/// S1: the claims attribution a resolved `/v1` request carries into its
/// handler, so the handler's ledger row is fully attributed. Inserted into
/// request extensions by [`v1_gate`] alongside the [`SessionPrincipal`].
#[derive(Clone)]
pub struct V1TokenContext(pub agent::proposals::TokenAuditFields);

/// The `/v1` audit action for a (method, path) — the ledger names the
/// surface even when the request never routes (auth denies, unknown
/// routes). Strict-method: HEAD is not folded into GET here.
fn v1_action_for(method: &Method, path: &str) -> &'static str {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match (method.as_str(), segments.as_slice()) {
        ("POST", ["v1", "retrieve"]) => "v1_retrieve",
        // The document id is path-like (may span segments — `s3/<bucket>/…`).
        ("GET", ["v1", "documents", rest @ ..]) if !rest.is_empty() => "v1_document",
        ("GET", ["v1", "whoami"]) => "v1_whoami",
        _ => "v1_unknown_route",
    }
}

/// S1: the `/v1` gate — the machine surface's authentication ladder, run
/// BEFORE routing (an unauthenticated caller cannot distinguish a real
/// `/v1` route from a fictional one). JWT-shaped bearers ONLY: cookies are
/// never read, session-shaped bearers are refused, and no deny reason
/// reaches the wire (S1-6 — the ledger holds the reason). A surface that
/// cannot ledger does not serve (EB-6: the whole namespace requires the
/// audit store).
async fn v1_gate(state: Arc<AppState>, mut request: Request, next: Next) -> Response {
    let target = format!("{} {}", request.method(), request.uri().path());
    let action = v1_action_for(request.method(), request.uri().path());

    // EB-6 precondition: no ledger, no surface. (Unledgerable itself —
    // stderr is the operational signal; the deny still stands.)
    if state.proposals.is_none() {
        eprintln!("v1 refused: no audit ledger wired (EB-6)");
        return identity::unauthorized();
    }

    // Credential class: exactly one acceptable shape — a dotted bearer.
    let token = match bearer_token(request.headers()) {
        None => {
            audit_v1_deny(
                &state,
                action,
                &target,
                "credential_missing",
                &Default::default(),
            );
            return identity::unauthorized();
        }
        Some(credential) if !credential.contains('.') => {
            // A session credential on the machine surface: refused, never
            // consulted against the session store (S1-1 — no crossing).
            audit_v1_deny(
                &state,
                action,
                &target,
                "session_credential_on_v1",
                &Default::default(),
            );
            return identity::unauthorized();
        }
        Some(jwt) => jwt,
    };

    let Some(bridge) = state.agent_bridge.clone() else {
        audit_v1_deny(
            &state,
            action,
            &target,
            agent_bridge::DenyReason::BridgeDisabled.as_str(),
            &Default::default(),
        );
        return identity::unauthorized();
    };
    let outcome = tokio::task::spawn_blocking(move || bridge.authenticate(&token)).await;
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(join_error) => {
            eprintln!("v1 token task failed: {join_error}");
            audit_v1_deny(
                &state,
                action,
                &target,
                agent_bridge::DenyReason::BridgeUnavailable.as_str(),
                &Default::default(),
            );
            return identity::unauthorized();
        }
    };
    match outcome {
        agent_bridge::BridgeOutcome::Denied { reason, claims } => {
            // Which ladder row denied is a ledger fact, not a wire fact.
            audit_v1_deny(
                &state,
                action,
                &target,
                reason.as_str(),
                &token_audit_fields(claims.as_deref()),
            );
            identity::unauthorized()
        }
        agent_bridge::BridgeOutcome::Resolved { principal, claims } => {
            let fields = token_audit_fields(Some(&claims));
            // Routing SECOND: only a resolved agent learns whether a /v1
            // route exists. Unknown -> the same 404 unknown paths get.
            if action == "v1_unknown_route" {
                if let Some(store) = &state.proposals {
                    if let Err(err) = store.audit_v1(
                        action,
                        &principal,
                        &target,
                        "unknown_route",
                        &fields,
                        None,
                        None,
                        None,
                        None,
                    ) {
                        eprintln!("v1 audit failed: {err:#}");
                        return identity::unauthorized();
                    }
                }
                return identity::route_denied();
            }
            // Known route: the handler owns the (allow / not-found) row —
            // it knows the decision and, for retrieve, the candidates.
            request.extensions_mut().insert(SessionPrincipal(principal));
            request.extensions_mut().insert(V1TokenContext(fields));
            next.run(request).await
        }
    }
}

/// `/v1` deny-side ledger write: the deny stands whether or not it could be
/// recorded; a write failure is an stderr signal.
fn audit_v1_deny(
    state: &AppState,
    action: &str,
    target: &str,
    reason: &str,
    fields: &agent::proposals::TokenAuditFields,
) {
    if let Some(store) = &state.proposals {
        if let Err(err) =
            store.audit_v1(action, "unresolved", target, reason, fields, None, None, None, None)
        {
            eprintln!("v1 audit failed: {err:#}");
        }
    }
}

/// Console-side ledger write for a refused machine credential — the reason
/// is a ledger-only string; the wire said only the generic 401.
fn audit_token_deny_reason(
    state: &AppState,
    target: &str,
    reason: &str,
    fields: &agent::proposals::TokenAuditFields,
) {
    if let Some(store) = &state.proposals {
        if let Err(err) = store.audit_agent_token("unresolved", target, reason, fields) {
            eprintln!("agent token audit failed: {err:#}");
        }
    }
}

/// Attribution claims for the audit row — ids only, never the raw token or
/// its signature (rows 1–3 deny before any claim is trusted: empty fields).
fn token_audit_fields(
    claims: Option<&agent_bridge::ClaimSet>,
) -> agent::proposals::TokenAuditFields {
    let Some(claims) = claims else {
        return Default::default();
    };
    agent::proposals::TokenAuditFields {
        tid: claims.tid.clone(),
        oid: claims.oid.clone(),
        azp: claims.azp.clone(),
        parent_azp: claims.parent_app_azp.clone(),
        aud: claims.aud.first().cloned(),
        uti: claims.uti.clone(),
    }
}

/// Resolve the principal from the request's SESSION credentials via the
/// authoritative store: a dotless bearer token or the session cookie.
/// Fail-closed: no token / unknown / expired -> None. A dotted (JWT-shaped)
/// bearer is NOT a session credential and is never consulted here — with a
/// valid cookie alongside, the cookie authenticates (S1-1: the machine
/// credential plays no part on the human surface).
fn console_session_principal(headers: &HeaderMap, state: &AppState) -> Option<String> {
    let token = bearer_token(headers)
        .filter(|credential| !credential.contains('.'))
        .or_else(|| cookie_token(headers))?;
    state.sessions.resolve(&token)
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))?
        .trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    let prefix = format!("{}=", session::SESSION_COOKIE);
    for part in cookies.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix(&prefix) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessRequestCreateBody {
    justification: String,
    target: access_requests::AccessTarget,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessRequestDecisionBody {
    #[serde(default)]
    reason_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccessCompany {
    people: Vec<AccessCompanyPerson>,
}

#[derive(Debug, Deserialize)]
struct AccessCompanyPerson {
    id: String,
    #[serde(default)]
    manager_id: Option<String>,
}

#[derive(serde::Serialize)]
struct AccessRequestsResponse {
    actor_id: String,
    demo_identity_mode: bool,
    requests: Vec<access_requests::AccessRequest>,
    snapshot_version: String,
}

#[derive(serde::Serialize)]
struct AccessGrantsResponse {
    actor_id: String,
    demo_identity_mode: bool,
    grants: Vec<access_grants::AccessGrant>,
    snapshot_version: String,
}

#[derive(serde::Serialize)]
struct AccessGrantResponse {
    demo_identity_mode: bool,
    grant: access_grants::AccessGrant,
    snapshot_version: String,
}

#[derive(serde::Serialize)]
struct AccessRequestMutationResponse {
    demo_identity_mode: bool,
    request: access_requests::AccessRequest,
    snapshot_version: String,
}

fn load_access_people(state: &AppState) -> Result<BTreeMap<String, Option<String>>> {
    let path = state.fixtures_dir.join("company.json");
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if sha256_hex(&bytes) != state.company_sha256 {
        bail!("company.json does not match the M1-pinned hash; refusing");
    }
    let company: AccessCompany = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} fails parse", path.display()))?;
    Ok(company
        .people
        .into_iter()
        .map(|person| (person.id, person.manager_id))
        .collect())
}

fn access_target_exists(state: &AppState, target: &access_requests::AccessTarget) -> bool {
    let Some(people) = state.people.as_deref() else {
        return false;
    };
    let capability_id = target.capability_id();
    !capability_id.trim().is_empty()
        && people.roster().any(|person| {
            person
                .projects
                .iter()
                .any(|project| project.capability_id == capability_id)
        })
}

fn clean_reason_code(reason_code: Option<String>) -> Result<Option<String>> {
    let Some(reason) = reason_code else {
        return Ok(None);
    };
    let trimmed = reason.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > 80
        || !trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.'))
    {
        bail!("reason_code fails validation");
    }
    Ok(Some(trimmed.to_string()))
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
fn create_access_request(state: &AppState, caller: &str, body: &[u8]) -> Reply {
    use access_requests::CreateOutcome;

    let Some(store) = &state.access_requests else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    let audit =
        |target: &str, outcome: &str| store.audit("access_request_create", caller, target, outcome);
    let request: AccessRequestCreateBody = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(err) => {
            eprintln!("access request create refused: {err}");
            if audit("access_request", "refused_strict_parse").is_err() {
                return reply_internal();
            }
            return reply_error(StatusCode::BAD_REQUEST, "request fails strict parse");
        }
    };
    let target_id = request.target.capability_id().to_string();
    let target_label = format!("{}:{target_id}", request.target.kind());
    let justification = request.justification.trim();
    if justification.len() < 8 || justification.len() > 1200 {
        if audit(&target_label, "refused_bad_justification").is_err() {
            return reply_internal();
        }
        return reply_error(
            StatusCode::BAD_REQUEST,
            "justification must be between 8 and 1200 characters",
        );
    }
    let people = match load_access_people(state) {
        Ok(people) => people,
        Err(err) => {
            eprintln!("access request people load failed: {err:#}");
            return reply_internal();
        }
    };
    let Some(manager) = people.get(caller) else {
        if audit(&target_label, "refused_unknown_principal").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    };
    let Some(approver_id) = manager.as_deref() else {
        if audit(&target_label, "refused_no_approver").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "no approver");
    };
    if !people.contains_key(approver_id) {
        if audit(&target_label, "refused_no_approver").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "no approver");
    }
    if !access_target_exists(state, &request.target) {
        if audit(&target_label, "refused_unknown_target").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::NOT_FOUND, "not found");
    }
    if audit(&target_label, "allowed").is_err() {
        return reply_internal();
    }
    match store.create(
        caller,
        request.target,
        justification,
        approver_id,
        &state.snapshot_version,
    ) {
        Ok(CreateOutcome::Created(request)) | Ok(CreateOutcome::Existing(request)) => {
            reply_canonical(&AccessRequestMutationResponse {
                demo_identity_mode: true,
                request: *request,
                snapshot_version: state.snapshot_version.clone(),
            })
        }
        Err(err) => {
            eprintln!("access request create failed: {err:#}");
            reply_internal()
        }
    }
}

fn list_access_requests(state: &AppState, caller: &str) -> Reply {
    let Some(store) = &state.access_requests else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    reply_canonical(&AccessRequestsResponse {
        actor_id: caller.to_string(),
        demo_identity_mode: true,
        requests: store.requested_by(caller),
        snapshot_version: state.snapshot_version.clone(),
    })
}

fn inbox_access_requests(state: &AppState, caller: &str) -> Reply {
    let Some(store) = &state.access_requests else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    let people = match load_access_people(state) {
        Ok(people) => people,
        Err(err) => {
            eprintln!("access request people load failed: {err:#}");
            return reply_internal();
        }
    };
    if !people.contains_key(caller) {
        return reply_canonical(&AccessRequestsResponse {
            actor_id: caller.to_string(),
            demo_identity_mode: true,
            requests: Vec::new(),
            snapshot_version: state.snapshot_version.clone(),
        });
    }
    reply_canonical(&AccessRequestsResponse {
        actor_id: caller.to_string(),
        demo_identity_mode: true,
        requests: store.inbox_for(caller),
        snapshot_version: state.snapshot_version.clone(),
    })
}

fn list_access_grants(state: &AppState, caller: &str) -> Reply {
    let Some(store) = &state.access_grants else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    let audit = |outcome: &str| store.audit("access_grant_list", caller, "access_grants", outcome);
    let people = match load_access_people(state) {
        Ok(people) => people,
        Err(err) => {
            eprintln!("access grant people load failed: {err:#}");
            return reply_internal();
        }
    };
    if !people.contains_key(caller) {
        if audit("refused_unknown_principal").is_err() {
            return reply_internal();
        }
        return reply_canonical(&AccessGrantsResponse {
            actor_id: caller.to_string(),
            demo_identity_mode: true,
            grants: Vec::new(),
            snapshot_version: state.snapshot_version.clone(),
        });
    }
    if audit("allowed").is_err() {
        return reply_internal();
    }
    reply_canonical(&AccessGrantsResponse {
        actor_id: caller.to_string(),
        demo_identity_mode: true,
        grants: store.visible_to(caller, &state.snapshot_version),
        snapshot_version: state.snapshot_version.clone(),
    })
}

fn get_access_grant(state: &AppState, caller: &str, grant_id: &str) -> Reply {
    let Some(store) = &state.access_grants else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    let audit = |outcome: &str| store.audit("access_grant_get", caller, grant_id, outcome);
    let people = match load_access_people(state) {
        Ok(people) => people,
        Err(err) => {
            eprintln!("access grant people load failed: {err:#}");
            return reply_internal();
        }
    };
    if !people.contains_key(caller) {
        if audit("refused_unknown_principal").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::NOT_FOUND, "not found");
    }
    let Some(grant) = store.get_effective(grant_id, &state.snapshot_version) else {
        if audit("refused_not_found").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    if grant.grantee_id != caller && grant.approver_id != caller {
        if audit("refused_not_party").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::NOT_FOUND, "not found");
    }
    if audit("allowed").is_err() {
        return reply_internal();
    }
    reply_canonical(&AccessGrantResponse {
        demo_identity_mode: true,
        grant,
        snapshot_version: state.snapshot_version.clone(),
    })
}

fn revoke_access_grant(state: &AppState, caller: &str, grant_id: &str, body: &[u8]) -> Reply {
    use access_grants::GrantRevokeError;

    let Some(store) = &state.access_grants else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    let audit = |outcome: &str| store.audit("access_grant_revoke", caller, grant_id, outcome);
    let decision_body: AccessRequestDecisionBody = if body.is_empty() {
        AccessRequestDecisionBody::default()
    } else {
        match serde_json::from_slice(body) {
            Ok(parsed) => parsed,
            Err(err) => {
                eprintln!("access grant revoke refused: {err}");
                if audit("refused_strict_parse").is_err() {
                    return reply_internal();
                }
                return reply_error(StatusCode::BAD_REQUEST, "revoke request fails strict parse");
            }
        }
    };
    let reason_code = match clean_reason_code(decision_body.reason_code) {
        Ok(reason) => reason,
        Err(err) => {
            eprintln!("access grant revoke reason refused: {err:#}");
            if audit("refused_bad_reason_code").is_err() {
                return reply_internal();
            }
            return reply_error(StatusCode::BAD_REQUEST, "reason_code fails validation");
        }
    };
    let people = match load_access_people(state) {
        Ok(people) => people,
        Err(err) => {
            eprintln!("access grant people load failed: {err:#}");
            return reply_internal();
        }
    };
    if !people.contains_key(caller) {
        if audit("refused_unknown_principal").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::NOT_FOUND, "not found");
    }
    let Some(grant) = store.get_effective(grant_id, &state.snapshot_version) else {
        if audit("refused_not_found").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    if grant.approver_id != caller {
        if grant.grantee_id == caller {
            if audit("refused_not_approver").is_err() {
                return reply_internal();
            }
            return reply_error(StatusCode::FORBIDDEN, "forbidden");
        }
        if audit("refused_not_party").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::NOT_FOUND, "not found");
    }
    if grant.snapshot_version != state.snapshot_version {
        if audit("refused_stale").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "stale grant");
    }
    if grant.status != access_grants::STATUS_ACTIVE {
        if audit("refused_inactive").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "inactive grant");
    }
    if audit("allowed").is_err() {
        return reply_internal();
    }
    match store.revoke(grant_id, caller, reason_code, &state.snapshot_version) {
        Ok(Ok(grant)) => reply_canonical(&AccessGrantResponse {
            demo_identity_mode: true,
            grant,
            snapshot_version: state.snapshot_version.clone(),
        }),
        Ok(Err(GrantRevokeError::NotFound)) => reply_error(StatusCode::NOT_FOUND, "not found"),
        Ok(Err(GrantRevokeError::AlreadyInactive)) => {
            reply_error(StatusCode::CONFLICT, "inactive grant")
        }
        Err(err) => {
            eprintln!("access grant revoke failed: {err:#}");
            reply_internal()
        }
    }
}

fn decide_access_request(
    state: &AppState,
    caller: &str,
    request_id: &str,
    status: &str,
    body: &[u8],
) -> Reply {
    use access_requests::{DecideError, STATUS_PENDING};

    let Some(store) = &state.access_requests else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    let action = if status == access_requests::STATUS_APPROVED {
        "access_request_approve"
    } else {
        "access_request_deny"
    };
    let audit = |outcome: &str| store.audit(action, caller, request_id, outcome);
    let decision_body: AccessRequestDecisionBody = if body.is_empty() {
        AccessRequestDecisionBody::default()
    } else {
        match serde_json::from_slice(body) {
            Ok(parsed) => parsed,
            Err(err) => {
                eprintln!("access request decision refused: {err}");
                if audit("refused_strict_parse").is_err() {
                    return reply_internal();
                }
                return reply_error(
                    StatusCode::BAD_REQUEST,
                    "decision request fails strict parse",
                );
            }
        }
    };
    let reason_code = match clean_reason_code(decision_body.reason_code) {
        Ok(reason) => reason,
        Err(err) => {
            eprintln!("access request decision reason refused: {err:#}");
            if audit("refused_bad_reason_code").is_err() {
                return reply_internal();
            }
            return reply_error(StatusCode::BAD_REQUEST, "reason_code fails validation");
        }
    };
    let people = match load_access_people(state) {
        Ok(people) => people,
        Err(err) => {
            eprintln!("access request people load failed: {err:#}");
            return reply_internal();
        }
    };
    if !people.contains_key(caller) {
        if audit("refused_unknown_principal").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    }
    let Some(request) = store.get(request_id) else {
        if audit("refused_not_found").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    if request.approver_id != caller {
        if audit("refused_not_approver").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    }
    if request.snapshot_version != state.snapshot_version {
        if audit("refused_stale").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "stale request");
    }
    if request.status != STATUS_PENDING {
        if audit("refused_already_decided").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "already decided");
    }
    if audit("allowed").is_err() {
        return reply_internal();
    }
    match store.decide(
        request_id,
        status,
        caller,
        reason_code,
        &state.snapshot_version,
    ) {
        Ok(Ok(request)) => {
            if status == access_requests::STATUS_APPROVED {
                if let Some(grant_store) = &state.access_grants {
                    let grant_target = format!(
                        "{}:{}",
                        request.target.kind(),
                        request.target.capability_id()
                    );
                    if grant_store
                        .audit("access_grant_create", caller, &grant_target, "allowed")
                        .is_err()
                    {
                        return reply_internal();
                    }
                    if let Err(err) = grant_store.create_from_approved_request(&request) {
                        eprintln!("access grant create failed: {err:#}");
                        return reply_internal();
                    }
                }
            }
            reply_canonical(&AccessRequestMutationResponse {
                demo_identity_mode: true,
                request,
                snapshot_version: state.snapshot_version.clone(),
            })
        }
        Ok(Err(DecideError::NotFound)) => reply_error(StatusCode::NOT_FOUND, "not found"),
        Ok(Err(DecideError::Stale)) => reply_error(StatusCode::CONFLICT, "stale request"),
        Ok(Err(DecideError::AlreadyDecided)) => {
            reply_error(StatusCode::CONFLICT, "already decided")
        }
        Err(err) => {
            eprintln!("access request decision failed: {err:#}");
            reply_internal()
        }
    }
}

async fn handle_access_request_create(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    body: axum::body::Bytes,
) -> Response {
    let body = body.to_vec();
    run_blocking(state, move |state| {
        create_access_request(state, &caller, &body)
    })
    .await
}

async fn handle_access_requests_list(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
) -> Response {
    run_blocking(state, move |state| list_access_requests(state, &caller)).await
}

async fn handle_access_requests_inbox(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
) -> Response {
    run_blocking(state, move |state| inbox_access_requests(state, &caller)).await
}

async fn handle_access_grants_list(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
) -> Response {
    run_blocking(state, move |state| list_access_grants(state, &caller)).await
}

async fn handle_access_grant_get(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(grant_id): axum::extract::Path<String>,
) -> Response {
    run_blocking(state, move |state| {
        get_access_grant(state, &caller, &grant_id)
    })
    .await
}

async fn handle_access_grant_revoke(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(grant_id): axum::extract::Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let body = body.to_vec();
    run_blocking(state, move |state| {
        revoke_access_grant(state, &caller, &grant_id, &body)
    })
    .await
}

async fn handle_access_request_approve(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(request_id): axum::extract::Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let body = body.to_vec();
    run_blocking(state, move |state| {
        decide_access_request(
            state,
            &caller,
            &request_id,
            access_requests::STATUS_APPROVED,
            &body,
        )
    })
    .await
}

async fn handle_access_request_deny(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(request_id): axum::extract::Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let body = body.to_vec();
    run_blocking(state, move |state| {
        decide_access_request(
            state,
            &caller,
            &request_id,
            access_requests::STATUS_DENIED,
            &body,
        )
    })
    .await
}

// ===========================================================================
// SHOWCASE-III: grounded workflow proposals — EB's first mutation path. A model
// PROPOSES (grounded, proposer-scoped); only the approver's decision (audit-
// before-effect) MATERIALIZES. Fail-closed throughout; existence-hiding 404s.
// ===========================================================================

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ProposalCreateBody {
    capability_id: String,
    title: String,
    goal: String,
}

#[derive(serde::Serialize)]
struct ProposalView {
    proposal_id: String,
    proposer_id: String,
    capability_id: String,
    approver_id: String,
    title: String,
    goal: String,
    /// S4 accountability line — the proposer owns the prose.
    drafted_from: String,
    boxes: Vec<proposals::BoxView>,
    grounding: proposals::GroundingCounts,
    status: String,
    created_ordinal: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    decided_by: Option<String>,
    materialized: bool,
    snapshot_version: String,
}

#[derive(serde::Serialize)]
struct ProposalEnvelope {
    demo_identity_mode: bool,
    proposal: ProposalView,
    snapshot_version: String,
}

#[derive(serde::Serialize)]
struct ProposalListResponse {
    actor_id: String,
    demo_identity_mode: bool,
    role: String,
    proposals: Vec<ProposalView>,
    snapshot_version: String,
}

#[derive(serde::Serialize)]
struct ProposalGenerateEmpty {
    demo_identity_mode: bool,
    generated: bool,
    reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    grounding: Option<proposals::GroundingCounts>,
    snapshot_version: String,
}

/// Build the per-VIEWER wire view of a stored proposal (S4 redaction applied).
fn proposal_view(
    state: &AppState,
    viewer: &str,
    p: &proposals::Proposal,
) -> Result<ProposalView, ()> {
    let boxes = proposals::redact_boxes_for(state, viewer, &p.boxes).map_err(|_| ())?;
    Ok(ProposalView {
        proposal_id: p.proposal_id.clone(),
        proposer_id: p.proposer_id.clone(),
        capability_id: p.capability_id.clone(),
        approver_id: p.approver_id.clone(),
        title: p.title.clone(),
        goal: p.goal.clone(),
        drafted_from: format!(
            "Drafted from {}'s authorized sources; proposed by {}",
            p.proposer_id, p.proposer_id
        ),
        boxes,
        grounding: p.grounding,
        status: p.status.clone(),
        created_ordinal: p.created_ordinal,
        decided_by: p.decided_by.clone(),
        materialized: p.materialized,
        snapshot_version: p.snapshot_version.clone(),
    })
}

fn create_workflow_proposal(state: &AppState, caller: &str, body: &[u8]) -> Reply {
    let Some(store) = &state.wf_proposals else {
        return reply_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "proposal store unavailable",
        );
    };
    if !state.identity.is_known(caller) {
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    }
    // Rate-limit BEFORE the expensive generation.
    if !state.generation_rate.check(caller) {
        return reply_error(StatusCode::TOO_MANY_REQUESTS, "too many proposals");
    }
    let request: ProposalCreateBody = match serde_json::from_slice(body) {
        Ok(parsed) => parsed,
        Err(_) => {
            return reply_error(
                StatusCode::BAD_REQUEST,
                "proposal request fails strict parse",
            );
        }
    };
    let title = request.title.trim();
    let goal = request.goal.trim();
    let capability_id = request.capability_id.trim();
    if title.is_empty()
        || title.chars().count() > 120
        || goal.is_empty()
        || goal.len() > 1200
        || capability_id.is_empty()
    {
        return reply_error(
            StatusCode::BAD_REQUEST,
            "title/goal/capability_id out of bounds",
        );
    }
    let Some(graph) = &state.lane_graph else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    if !graph.capabilities.iter().any(|c| c.id == capability_id) {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    }
    // Approver = the proposer's manager (the reused access-request resolution).
    let people = match load_access_people(state) {
        Ok(people) => people,
        Err(err) => {
            eprintln!("proposal people load failed: {err:#}");
            return reply_internal();
        }
    };
    let Some(manager) = people.get(caller) else {
        let _ = store.audit(
            "proposal_generate",
            caller,
            capability_id,
            "refused_unknown_principal",
        );
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    };
    let Some(approver_id) = manager.as_deref() else {
        let _ = store.audit(
            "proposal_generate",
            caller,
            capability_id,
            "refused_no_approver",
        );
        return reply_error(StatusCode::CONFLICT, "no accountable approver");
    };
    if !people.contains_key(approver_id) {
        let _ = store.audit(
            "proposal_generate",
            caller,
            capability_id,
            "refused_no_approver",
        );
        return reply_error(StatusCode::CONFLICT, "no accountable approver");
    }
    // Generate (proposer-scoped). Fault/zero-admitted write NOTHING.
    match proposals::generate_proposal(state, caller, capability_id, approver_id, title, goal) {
        Ok(proposals::GenerateOutcome::Fault) => {
            let _ = store.audit("proposal_generate", caller, capability_id, "fault");
            reply_canonical(&ProposalGenerateEmpty {
                demo_identity_mode: true,
                generated: false,
                reason: "could not draft a grounded plan".to_string(),
                grounding: None,
                snapshot_version: state.snapshot_version.clone(),
            })
        }
        Ok(proposals::GenerateOutcome::ZeroAdmitted { refused }) => {
            let _ = store.audit("proposal_generate", caller, capability_id, "zero_admitted");
            reply_canonical(&ProposalGenerateEmpty {
                demo_identity_mode: true,
                generated: false,
                reason: "no plan could be grounded in your sources".to_string(),
                grounding: Some(proposals::GroundingCounts {
                    admitted: 0,
                    refused,
                }),
                snapshot_version: state.snapshot_version.clone(),
            })
        }
        Ok(proposals::GenerateOutcome::Drafted(draft)) => {
            if store
                .audit("proposal_generate", caller, capability_id, "drafted")
                .is_err()
            {
                return reply_internal();
            }
            match store.create(draft) {
                Ok(proposal) => match proposal_view(state, caller, &proposal) {
                    Ok(view) => reply_canonical(&ProposalEnvelope {
                        demo_identity_mode: true,
                        proposal: view,
                        snapshot_version: state.snapshot_version.clone(),
                    }),
                    Err(()) => reply_internal(),
                },
                Err(err) => {
                    eprintln!("proposal create failed: {err:#}");
                    reply_internal()
                }
            }
        }
        Err(err) => {
            eprintln!("proposal generation failed: {err:#}");
            reply_internal()
        }
    }
}

fn list_workflow_proposals(state: &AppState, caller: &str, role: &str) -> Reply {
    let Some(store) = &state.wf_proposals else {
        return reply_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "proposal store unavailable",
        );
    };
    if !state.identity.is_known(caller) {
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    }
    let (role_label, stored) = if role == "approver" {
        ("approver", store.inbox_for(caller))
    } else {
        ("proposer", store.proposed_by(caller))
    };
    let mut views = Vec::new();
    for p in &stored {
        match proposal_view(state, caller, p) {
            Ok(view) => views.push(view),
            Err(()) => return reply_internal(),
        }
    }
    reply_canonical(&ProposalListResponse {
        actor_id: caller.to_string(),
        demo_identity_mode: true,
        role: role_label.to_string(),
        proposals: views,
        snapshot_version: state.snapshot_version.clone(),
    })
}

fn get_workflow_proposal(state: &AppState, caller: &str, proposal_id: &str) -> Reply {
    let Some(store) = &state.wf_proposals else {
        return reply_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "proposal store unavailable",
        );
    };
    let Some(proposal) = store.get(proposal_id) else {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    };
    // Existence-hiding: only the proposer or the approver may see it.
    if proposal.proposer_id != caller && proposal.approver_id != caller {
        return reply_error(StatusCode::NOT_FOUND, "not found");
    }
    match proposal_view(state, caller, &proposal) {
        Ok(view) => reply_canonical(&ProposalEnvelope {
            demo_identity_mode: true,
            proposal: view,
            snapshot_version: state.snapshot_version.clone(),
        }),
        Err(()) => reply_internal(),
    }
}

fn decide_workflow_proposal(
    state: &AppState,
    caller: &str,
    proposal_id: &str,
    status: &str,
) -> Reply {
    use proposals::{DecideError, STATUS_PENDING};
    let Some(store) = &state.wf_proposals else {
        return reply_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "proposal store unavailable",
        );
    };
    let action = if status == proposals::STATUS_APPROVED {
        "proposal_approve"
    } else {
        "proposal_deny"
    };
    let audit = |outcome: &str| store.audit(action, caller, proposal_id, outcome);
    let people = match load_access_people(state) {
        Ok(people) => people,
        Err(err) => {
            eprintln!("proposal people load failed: {err:#}");
            return reply_internal();
        }
    };
    if !people.contains_key(caller) {
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
    if proposal.approver_id != caller {
        if audit("refused_not_approver").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::FORBIDDEN, "forbidden");
    }
    if proposal.snapshot_version != state.snapshot_version {
        if audit("refused_stale").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "stale proposal");
    }
    if proposal.status != STATUS_PENDING {
        if audit("refused_already_decided").is_err() {
            return reply_internal();
        }
        return reply_error(StatusCode::CONFLICT, "already decided");
    }
    // AUDIT-BEFORE-EFFECT: the row is flushed before any state changes.
    if audit("allowed").is_err() {
        return reply_internal();
    }
    match store.decide(proposal_id, status, caller, &state.snapshot_version) {
        Ok(Ok(decided)) => {
            // THE ONE materialize() call site (INV1 / WF-G7): approve-only, after
            // the audit flush and the approved decision.
            if status == proposals::STATUS_APPROVED {
                if let Err(err) = store.materialize(proposal_id) {
                    eprintln!("proposal materialize failed: {err:#}");
                    return reply_internal();
                }
            }
            let latest = store.get(proposal_id).unwrap_or(decided);
            match proposal_view(state, caller, &latest) {
                Ok(view) => reply_canonical(&ProposalEnvelope {
                    demo_identity_mode: true,
                    proposal: view,
                    snapshot_version: state.snapshot_version.clone(),
                }),
                Err(()) => reply_internal(),
            }
        }
        Ok(Err(DecideError::NotFound)) => reply_error(StatusCode::NOT_FOUND, "not found"),
        Ok(Err(DecideError::Stale)) => reply_error(StatusCode::CONFLICT, "stale proposal"),
        Ok(Err(DecideError::AlreadyDecided)) => {
            reply_error(StatusCode::CONFLICT, "already decided")
        }
        Err(err) => {
            eprintln!("proposal decision failed: {err:#}");
            reply_internal()
        }
    }
}

async fn handle_workflow_proposal_create(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    body: axum::body::Bytes,
) -> Response {
    let body = body.to_vec();
    run_blocking(state, move |state| {
        create_workflow_proposal(state, &caller, &body)
    })
    .await
}

async fn handle_workflow_proposals_list(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let role = query.get("role").cloned().unwrap_or_default();
    run_blocking(state, move |state| {
        list_workflow_proposals(state, &caller, &role)
    })
    .await
}

async fn handle_workflow_proposal_get(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(proposal_id): axum::extract::Path<String>,
) -> Response {
    run_blocking(state, move |state| {
        get_workflow_proposal(state, &caller, &proposal_id)
    })
    .await
}

async fn handle_workflow_proposal_approve(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(proposal_id): axum::extract::Path<String>,
) -> Response {
    run_blocking(state, move |state| {
        decide_workflow_proposal(state, &caller, &proposal_id, proposals::STATUS_APPROVED)
    })
    .await
}

async fn handle_workflow_proposal_deny(
    State(state): State<Arc<AppState>>,
    DemoPrincipal(caller): DemoPrincipal,
    axum::extract::Path(proposal_id): axum::extract::Path<String>,
) -> Response {
    run_blocking(state, move |state| {
        decide_workflow_proposal(state, &caller, &proposal_id, proposals::STATUS_DENIED)
    })
    .await
}

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
