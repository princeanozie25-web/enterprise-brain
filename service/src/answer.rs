//! The answer pipeline. THE ORDER IS LAW:
//! identity -> retrieval (M2b hybrid library path, degradation doctrine
//! inherited) -> mosaic bound -> sealed context -> generate -> per-claim
//! grounding (K1: verbatim anchoring, drop-with-disclosure) -> citation
//! validation -> envelope.
//!
//! Deny by default at every step: an unknown principal gets the empty
//! envelope (indistinguishable from a principal granted nothing); a
//! generator that cites outside its sealed context loses the WHOLE answer;
//! an uncited answer over a private corpus is an unauditable claim and is
//! refused the same way. The mosaic bound discloses that it fired
//! (`aggregation_bounded`) and never what it hid.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use anyhow::Context as _;
use retrieval::envelope::{normalize_query, query_hash, ScopeStatement};
use retrieval::index::{canonical_json_bytes, sha256_hex};
use retrieval::search::{HybridParams, JudgeParams, PrincipalScope, SearchOptions, Trace};
use retrieval::vector::snippet_of;
use serde::{Deserialize, Serialize};

use crate::cache::CacheKey;
use crate::generate::ContextDoc;
use crate::lane::LaneGraph;
use crate::sidecar::{generation_event, UsageEvent};
use crate::AppState;

/// NUMBERS (spec-fixed).
pub const CONTEXT_DOCS: usize = 6;
pub const CONTEXT_SNIPPET_CHARS: usize = 480;
pub const GENERATION_TIMEOUT_MS: u64 = 15_000;
/// Retrieval numbers unchanged from M2a/M2b. The judge timeout lives on
/// `AppState` (production 2000ms; the labeled demo profile may raise it,
/// never implicitly).
pub const ASK_TOP_K: usize = 10;
const QUERY_EMBED_TIMEOUT_MS: u64 = 1_500;
const JUDGE_TOP_K: usize = 12;
const JUDGE_MIN_CANDIDATES: usize = 4;
const JUDGE_MAX_RATIO: f64 = 1.3;

#[derive(Debug, Clone, Default)]
pub struct AskOptions {
    pub hybrid: bool,
    pub judge: bool,
    pub bypass_cache: bool,
    pub granted_context: Option<GrantedContextRequest>,
}

#[derive(Debug, Clone)]
pub struct GrantedContextRequest {
    pub capability_id: String,
    pub grant_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantedContextNode {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantedContextSummary {
    pub active: bool,
    pub approver_id: String,
    pub capability: GrantedContextNode,
    pub grant_id: String,
    pub grant_scope: String,
    pub grant_status: String,
    pub initiative: GrantedContextNode,
    pub request_id: String,
    pub strategy: GrantedContextNode,
    pub target_kind: String,
    pub workflow: GrantedContextNode,
}

/// One ADMITTED claim: a rendered sentence and the anchor that proves it —
/// `locator` = "doc_id@byte_offset", the real offset of the quote's first
/// verbatim match in the cited document's FULL body. Refused claims never
/// appear here (they are counted in [`GroundingCounts`], never rendered).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerClaim {
    pub doc_id: String,
    pub locator: String,
    pub text: String,
}

/// Drop-with-disclosure (K1 ruling R-C): how many of the generator's OWN
/// draft claims survived the grounding gate and how many were removed.
/// These numbers describe the model's draft output, never any document —
/// disclosing them is the honesty law, not a dark count.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundingCounts {
    pub admitted: u32,
    pub refused: u32,
}

/// The validated answer: text composed ONLY of admitted (verbatim-anchored)
/// claims, each followed by its [doc_id] citation; `citations` = the
/// deduped, sorted admitted doc ids; `claims` = the admitted claims sorted
/// by (doc_id, locator, text).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub citations: Vec<String>,
    pub claims: Vec<AnswerClaim>,
    pub text: String,
}

/// One /ask result, enriched with the title and sensitivity of the (already
/// authorized) document — copied from the same scope-checked source as /doc.
/// Field-for-field this is retrieval's ResultEntry plus those two fields;
/// nothing here can carry a count.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrichedResult {
    pub document_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_successor: Option<String>,
    pub reasons_ref: Vec<String>,
    pub score_rank: u32,
    pub sensitivity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
    pub title: String,
}

/// GET /doc/{id}: the scope-checked document card. Never the full body —
/// only the deterministic snippet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocCard {
    pub document_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_successor: Option<String>,
    pub sensitivity: String,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
    pub title: String,
}

/// The answer envelope. Canonical JSON (sorted keys, compact, trailing
/// newline); all M2a/M2b forbidden-field rules apply — no count, list, or
/// statistic of suppressed documents, ever. `aggregation_bounded` is
/// pattern-level disclosure of policy operation (explicit M3a ruling), not a
/// dark count: it says a rule fired, never what it hid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerEnvelope {
    pub aggregation_bounded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<Answer>,
    /// Always true: this identity layer is a stand-in for real OIDC and
    /// says so on every response.
    pub demo_identity_mode: bool,
    pub generation_applied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granted_context: Option<GrantedContextSummary>,
    /// Present whenever the grounding gate ran — INCLUDING the all-refused
    /// case where `answer` is omitted, so the drop is always disclosed
    /// (R-C). Counts the generator's own draft claims, nothing else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding: Option<GroundingCounts>,
    /// True whenever the grounding gate ran (same pattern as
    /// judge_applied/generation_applied — false says "did not run", never why).
    pub grounding_applied: bool,
    pub index_version: String,
    pub judge_applied: bool,
    pub principal_id: String,
    pub query_hash: String,
    pub results: Vec<EnrichedResult>,
    pub retrieval_mode: String,
    /// The REAL scope statement from the identity model (company.json).
    pub scope_statement: ScopeStatement,
    pub snapshot_version: String,
}

/// Per-ask instrumentation for the governance harness. In-memory only.
#[derive(Debug, Default)]
pub struct AskTrace {
    pub cache_hit: bool,
    pub retrieval: Option<Trace>,
    /// The sealed context exactly as handed to the generator.
    pub sealed: Vec<ContextDoc>,
    /// Mosaic-bound removals (in-scope ids; never serialized).
    pub mosaic_removed: Vec<String>,
    pub generation_fault: bool,
    /// K1 grounding instrumentation (G-6): parsed draft size and the
    /// admit/refuse split. In-memory only, like everything else here.
    pub draft_claims: u32,
    pub grounding_admitted: u32,
    pub grounding_refused: u32,
    pub usage_events: Vec<UsageEvent>,
}

/// Errors the HTTP layer maps to status codes. Internal detail never
/// reaches a response body.
#[derive(Debug)]
pub enum AskError {
    BadRequest(String),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for AskError {
    fn from(err: anyhow::Error) -> AskError {
        AskError::Internal(err)
    }
}

fn capability_context(
    graph: &LaneGraph,
    capability_id: &str,
) -> anyhow::Result<Option<(GrantedContextSummary, BTreeSet<String>)>> {
    let Some(capability) = graph.capabilities.iter().find(|c| c.id == capability_id) else {
        return Ok(None);
    };
    let workflow = graph
        .workflows
        .iter()
        .find(|w| w.id == capability.workflow_id)
        .context("capability references an unknown workflow")?;
    let initiative = graph
        .initiatives
        .iter()
        .find(|i| i.id == workflow.initiative_id)
        .context("workflow references an unknown initiative")?;
    let strategy = graph
        .strategies
        .iter()
        .find(|s| s.id == initiative.strategy_id)
        .context("initiative references an unknown strategy")?;

    let summary = GrantedContextSummary {
        active: true,
        approver_id: String::new(),
        capability: GrantedContextNode {
            id: capability.id.clone(),
            name: capability.name.clone(),
        },
        grant_id: String::new(),
        grant_scope: String::new(),
        grant_status: String::new(),
        initiative: GrantedContextNode {
            id: initiative.id.clone(),
            name: initiative.name.clone(),
        },
        request_id: String::new(),
        strategy: GrantedContextNode {
            id: strategy.id.clone(),
            name: strategy.name.clone(),
        },
        target_kind: String::new(),
        workflow: GrantedContextNode {
            id: workflow.id.clone(),
            name: workflow.name.clone(),
        },
    };
    Ok(Some((
        summary,
        capability.document_ids.iter().cloned().collect(),
    )))
}

fn resolve_granted_context(
    state: &AppState,
    principal_id: &str,
    request: &GrantedContextRequest,
) -> Result<(GrantedContextSummary, BTreeSet<String>), AskError> {
    let Some(store) = &state.access_grants else {
        return Err(AskError::BadRequest(
            "granted context is unavailable".to_string(),
        ));
    };
    let Some(grant) = store.get_effective(&request.grant_id, &state.snapshot_version) else {
        return Err(AskError::BadRequest(
            "granted context is unavailable".to_string(),
        ));
    };
    if grant.grantee_id != principal_id
        || grant.permission != crate::access_grants::PERMISSION_READ
        || grant.status != crate::access_grants::STATUS_ACTIVE
        || grant.snapshot_version != state.snapshot_version
        || grant.target.capability_id() != request.capability_id
    {
        return Err(AskError::BadRequest(
            "granted context is unavailable".to_string(),
        ));
    }
    let Some(graph) = &state.lane_graph else {
        return Err(AskError::BadRequest(
            "granted context is unavailable".to_string(),
        ));
    };
    let Some((mut summary, document_ids)) =
        capability_context(graph, &request.capability_id).map_err(AskError::Internal)?
    else {
        return Err(AskError::BadRequest(
            "granted context is unavailable".to_string(),
        ));
    };
    summary.approver_id = grant.approver_id;
    summary.grant_id = grant.grant_id;
    summary.grant_scope = format!("{}:{}", grant.target.kind(), grant.target.capability_id());
    summary.grant_status = grant.status;
    summary.request_id = grant.request_id;
    summary.target_kind = grant.target.kind().to_string();
    Ok((summary, document_ids))
}

/// The full ask pipeline. Synchronous on purpose — the async boundary stays
/// in the HTTP layer; everything governed is plain auditable code.
pub fn ask(
    state: &AppState,
    principal_id: &str,
    query: &str,
    options: &AskOptions,
) -> Result<(Vec<u8>, AskTrace), AskError> {
    let mut trace = AskTrace::default();
    let granted_context = match &options.granted_context {
        Some(request) => Some(resolve_granted_context(state, principal_id, request)?),
        None => None,
    };

    let normalized = normalize_query(query);
    let computed_query_hash = query_hash(
        &normalized,
        principal_id,
        &state.snapshot_version,
        &state.engine.manifest.index_version,
    );
    let cache_key = CacheKey {
        query_hash: computed_query_hash.clone(),
        hybrid: options.hybrid,
        judge: options.judge,
    };
    if !options.bypass_cache && granted_context.is_none() {
        if let Some(cache) = &state.cache {
            if let Some(bytes) = cache.get(&cache_key) {
                trace.cache_hit = true;
                return Ok((bytes, trace));
            }
        }
    }

    // Mode preflight mirrors the M2b CLI: requesting a mode the service has
    // no model for is a caller error, not a silent downgrade.
    if options.hybrid && state.embedder.is_none() {
        return Err(AskError::BadRequest(
            "hybrid asks require the service to be configured with an embedder".to_string(),
        ));
    }
    if options.judge && state.judge.is_none() {
        return Err(AskError::BadRequest(
            "judged asks require the service to be configured with a judge model".to_string(),
        ));
    }
    // K1 §4: a capability whose vector arm is absent is a 400 with a reason,
    // never a 500 out of the engine's preflight bail. (With no model
    // configured the checks above answer first; this closes the
    // config-wired, vectors-absent curl hole.)
    if (options.hybrid || options.judge) && state.engine.manifest.vectors.is_none() {
        return Err(AskError::BadRequest(
            "the index carries no vector arm in this build".to_string(),
        ));
    }

    // Unknown principal: the empty envelope. Deny by default; the shape is
    // identical to a known principal granted nothing.
    if !state.identity.is_known(principal_id) {
        let envelope = AnswerEnvelope {
            aggregation_bounded: false,
            answer: None,
            demo_identity_mode: true,
            generation_applied: false,
            granted_context: None,
            grounding: None,
            grounding_applied: false,
            index_version: state.engine.manifest.index_version.clone(),
            judge_applied: false,
            principal_id: principal_id.to_string(),
            query_hash: computed_query_hash,
            results: Vec::new(),
            retrieval_mode: "lexical_only".to_string(),
            scope_statement: crate::scope::empty_statement(),
            snapshot_version: state.snapshot_version.clone(),
        };
        let bytes = canonical_json_bytes(&envelope).map_err(AskError::Internal)?;
        // Not cached: unknown ids must not be able to churn the LRU.
        return Ok((bytes, trace));
    }

    // 2. Retrieval through the M2b library path.
    let scope = PrincipalScope::load(&state.artifacts_dir, principal_id)
        .context("loading the principal's compiled allowlist")
        .map_err(AskError::Internal)?;
    let search_options = SearchOptions {
        k: ASK_TOP_K,
        include_superseded: false,
        hybrid: state
            .embedder
            .as_deref()
            .filter(|_| options.hybrid)
            .map(|embedder| HybridParams {
                embedder,
                query_embed_timeout: Duration::from_millis(QUERY_EMBED_TIMEOUT_MS),
            }),
        judge: state
            .judge
            .as_deref()
            .filter(|_| options.judge)
            .map(|judge| JudgeParams {
                judge,
                timeout: Duration::from_millis(state.judge_timeout_ms),
                top_k: JUDGE_TOP_K,
                min_candidates: JUDGE_MIN_CANDIDATES,
                max_ratio: JUDGE_MAX_RATIO,
            }),
    };
    let (mut retrieval_envelope, retrieval_trace) = state
        .engine
        .search(&scope, query, &search_options)
        .context("governed retrieval failed")
        .map_err(AskError::Internal)?;
    trace
        .usage_events
        .extend(retrieval_trace.usage_events.iter().cloned());
    let granted_context_summary = if let Some((summary, document_ids)) = granted_context {
        retrieval_envelope
            .results
            .retain(|result| document_ids.contains(&result.document_id));
        for (index, result) in retrieval_envelope.results.iter_mut().enumerate() {
            result.score_rank = (index + 1) as u32;
        }
        Some(summary)
    } else {
        None
    };

    // 3. Mosaic bound: if both members of a tagged pair appear in this
    // principal's results, the LOWER-RANKED member leaves the generation
    // context. Results themselves are untouched — the bound governs
    // co-presence in one context, and the envelope discloses only that it
    // fired.
    let mut surviving: Vec<&str> = retrieval_envelope
        .results
        .iter()
        .map(|r| r.document_id.as_str())
        .collect();
    for (doc_a, doc_b) in &state.mosaic_pairs {
        let position_a = surviving.iter().position(|id| id == doc_a);
        let position_b = surviving.iter().position(|id| id == doc_b);
        if let (Some(a), Some(b)) = (position_a, position_b) {
            let lower = a.max(b);
            trace
                .mosaic_removed
                .push(surviving.remove(lower).to_string());
        }
    }
    let aggregation_bounded = !trace.mosaic_removed.is_empty();

    // 4. Sealed context: top surviving docs as (id, title, snippet<=480).
    let sealed: Vec<ContextDoc> = surviving
        .iter()
        .take(CONTEXT_DOCS)
        .map(|id| {
            let meta = state
                .docs
                .get(*id)
                .context("result id missing from the verified corpus")?;
            Ok(ContextDoc {
                doc_id: (*id).to_string(),
                title: meta.title.clone(),
                snippet: snippet_of(&meta.body, CONTEXT_SNIPPET_CHARS),
            })
        })
        .collect::<anyhow::Result<_>>()
        .map_err(AskError::Internal)?;
    trace.sealed = sealed.clone();

    // 5 + 6. Generate -> STRICT parse -> per-claim grounding (K1) ->
    // compose -> answer-level citation validation (the old gate, kept as a
    // second lock). Any failure on this side degrades to retrieval-only —
    // never to less governance. Refused claims are DROPPED WITH DISCLOSURE
    // (the `grounding` counts); the whole answer is refused only when zero
    // claims survive.
    let mut answer: Option<Answer> = None;
    let mut transient_failure = false;
    let mut grounding_applied = false;
    let mut grounding_counts: Option<GroundingCounts> = None;
    if let (Some(generator), false) = (&state.generator, sealed.is_empty()) {
        let outcome =
            generator.generate(query, &sealed, Duration::from_millis(GENERATION_TIMEOUT_MS));
        let estimate_basis: usize = query.len()
            + sealed
                .iter()
                .map(|d| d.doc_id.len() + d.title.len() + d.snippet.len())
                .sum::<usize>();
        let usage = outcome.as_ref().ok().and_then(|o| o.usage);
        trace.usage_events.push(generation_event(
            generator.model_id(),
            usage,
            estimate_basis,
        ));
        match outcome {
            Err(_) => {
                transient_failure = true;
            }
            Ok(outcome) => match crate::generate::parse_claims(&outcome.text) {
                Err(_) => {
                    // Format deviation: no salvage parsing (K1 §1).
                    trace.generation_fault = true;
                }
                Ok(draft) => {
                    trace.draft_claims = draft.len() as u32;
                    // The ONE lookup grounding receives: sealed ids -> FULL
                    // bodies. Built from the sealed context only, so no code
                    // path can anchor against any other document (G-4).
                    let sealed_bodies: BTreeMap<&str, &str> = sealed
                        .iter()
                        .map(|d| {
                            let meta = state
                                .docs
                                .get(&d.doc_id)
                                .expect("sealed ids originate from the verified corpus");
                            (d.doc_id.as_str(), meta.body.as_str())
                        })
                        .collect();
                    let verifier = crate::grounding::AnchorOnly;
                    let mut admitted: Vec<(crate::grounding::Claim, crate::grounding::Anchor)> =
                        Vec::new();
                    let mut refused: u32 = 0;
                    for c in draft {
                        let claim = crate::grounding::Claim {
                            text: c.text,
                            doc_id: c.doc_id,
                            quote: c.quote,
                        };
                        match crate::grounding::ground(claim, &sealed_bodies, &verifier) {
                            crate::grounding::Grounded::Admitted { claim, anchor } => {
                                admitted.push((claim, anchor));
                            }
                            crate::grounding::Grounded::Refused { .. } => refused += 1,
                        }
                    }
                    grounding_applied = true;
                    trace.grounding_admitted = admitted.len() as u32;
                    trace.grounding_refused = refused;
                    grounding_counts = Some(GroundingCounts {
                        admitted: admitted.len() as u32,
                        refused,
                    });
                    if !admitted.is_empty() {
                        let text = admitted
                            .iter()
                            .map(|(c, _)| format!("{} [{}]", c.text, c.doc_id))
                            .collect::<Vec<_>>()
                            .join(" ");
                        // Belt and braces: the pre-K1 answer-level gate still
                        // runs on the composed text.
                        let sealed_ids: BTreeSet<&str> =
                            sealed.iter().map(|d| d.doc_id.as_str()).collect();
                        match validate_citations(&text, &sealed_ids) {
                            Ok(_) => {
                                let mut citations: Vec<String> =
                                    admitted.iter().map(|(c, _)| c.doc_id.clone()).collect();
                                citations.sort();
                                citations.dedup();
                                let mut claims: Vec<AnswerClaim> = admitted
                                    .iter()
                                    .map(|(c, a)| AnswerClaim {
                                        doc_id: c.doc_id.clone(),
                                        locator: a.locator.clone(),
                                        text: c.text.clone(),
                                    })
                                    .collect();
                                claims.sort_by(|x, y| {
                                    x.doc_id
                                        .cmp(&y.doc_id)
                                        .then_with(|| x.locator.cmp(&y.locator))
                                        .then_with(|| x.text.cmp(&y.text))
                                });
                                answer = Some(Answer {
                                    citations,
                                    claims,
                                    text,
                                });
                            }
                            Err(_) => {
                                trace.generation_fault = true;
                            }
                        }
                    }
                }
            },
        }
    }
    let generation_applied = answer.is_some();

    // 7. Envelope, with results enriched by title + sensitivity from the
    // same scope-checked source /doc uses — already-authorized docs only.
    let results: Vec<EnrichedResult> = retrieval_envelope
        .results
        .iter()
        .map(|r| {
            let meta = state
                .docs
                .get(&r.document_id)
                .context("result id missing from the verified corpus")?;
            Ok(EnrichedResult {
                document_id: r.document_id.clone(),
                effective_successor: r.effective_successor.clone(),
                reasons_ref: r.reasons_ref.clone(),
                score_rank: r.score_rank,
                sensitivity: meta.sensitivity.clone(),
                superseded: r.superseded,
                title: meta.title.clone(),
            })
        })
        .collect::<anyhow::Result<_>>()
        .map_err(AskError::Internal)?;
    let envelope = AnswerEnvelope {
        aggregation_bounded,
        answer,
        demo_identity_mode: true,
        generation_applied,
        granted_context: granted_context_summary,
        grounding: grounding_counts,
        grounding_applied,
        index_version: retrieval_envelope.index_version.clone(),
        judge_applied: retrieval_envelope.judge_applied,
        principal_id: principal_id.to_string(),
        query_hash: computed_query_hash,
        results,
        retrieval_mode: retrieval_envelope.retrieval_mode.clone(),
        scope_statement: state.identity.statement_for(principal_id),
        snapshot_version: retrieval_envelope.snapshot_version.clone(),
    };
    let bytes = canonical_json_bytes(&envelope).map_err(AskError::Internal)?;

    // Cache only clean envelopes: a transiently degraded ask (embedder,
    // judge, or generator failure; citation fault) must not be pinned.
    let hybrid_degraded = options.hybrid && envelope.retrieval_mode != "hybrid";
    let cacheable = !options.bypass_cache
        && envelope.granted_context.is_none()
        && !trace.generation_fault
        && !transient_failure
        && !hybrid_degraded
        && !retrieval_trace.judge_failed;
    if cacheable {
        if let Some(cache) = &state.cache {
            cache.put(cache_key, bytes.clone());
        }
    }

    trace.retrieval = Some(retrieval_trace);
    Ok((bytes, trace))
}

/// Citation faults — both kinds refuse the whole answer.
#[derive(Debug, PartialEq, Eq)]
pub enum CitationFault {
    /// A bracketed citation referenced something outside the sealed context.
    OutOfContext,
    /// No citation at all: an unauditable claim over a private corpus.
    Uncited,
}

/// The /doc lookup: Some(card) IF AND ONLY IF the id is in the principal's
/// compiled allowlist (artifact re-verified byte-for-byte on every call).
/// None covers unknown principal, out-of-scope, and nonexistent identically —
/// the HTTP layer turns every None into THE one 404. The effective successor
/// is emitted only when it is itself in the allowlist (the R-13 rule).
pub fn doc_card(
    state: &AppState,
    principal_id: &str,
    doc_id: &str,
) -> anyhow::Result<Option<DocCard>> {
    let Some((artifact_file, artifact_sha)) = state.artifact_rows.get(principal_id) else {
        return Ok(None);
    };
    let artifact_path = state.artifacts_dir.join(artifact_file);
    let bytes = std::fs::read(&artifact_path)
        .with_context(|| format!("cannot read artifact {}", artifact_path.display()))?;
    if &sha256_hex(&bytes) != artifact_sha {
        anyhow::bail!(
            "artifact {} does not match the hash recorded in the M1 index; refusing",
            artifact_path.display()
        );
    }
    let artifact: crate::ArtifactLite = serde_json::from_slice(&bytes)
        .with_context(|| format!("artifact {} fails parse", artifact_path.display()))?;

    let Some(entry) = artifact.entries.iter().find(|e| e.document_id == doc_id) else {
        return Ok(None);
    };
    let Some(meta) = state.docs.get(doc_id) else {
        return Ok(None);
    };
    let allowlist: BTreeSet<&str> = artifact
        .entries
        .iter()
        .map(|e| e.document_id.as_str())
        .collect();
    let superseded = entry.superseded == Some(true);
    let effective_successor = if superseded {
        entry
            .effective_successor
            .as_ref()
            .filter(|s| allowlist.contains(s.as_str()))
            .cloned()
    } else {
        None
    };
    Ok(Some(DocCard {
        document_id: doc_id.to_string(),
        effective_successor,
        sensitivity: meta.sensitivity.clone(),
        snippet: snippet_of(&meta.body, CONTEXT_SNIPPET_CHARS),
        superseded: superseded.then_some(true),
        title: meta.title.clone(),
    }))
}

/// Fail-closed citation validation: EVERY bracketed segment in the answer is
/// treated as a citation and must exactly match a sealed-context doc id; at
/// least one is required. Returns the cited ids in order of first appearance.
pub fn validate_citations(
    text: &str,
    sealed_ids: &BTreeSet<&str>,
) -> Result<Vec<String>, CitationFault> {
    let mut citations: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            // An unclosed bracket cites nothing; the zero-citation rule
            // below still applies.
            break;
        };
        let candidate = &after[..close];
        if !sealed_ids.contains(candidate) {
            return Err(CitationFault::OutOfContext);
        }
        if !citations.iter().any(|c| c == candidate) {
            citations.push(candidate.to_string());
        }
        rest = &after[close + 1..];
    }
    if citations.is_empty() {
        return Err(CitationFault::Uncited);
    }
    Ok(citations)
}
