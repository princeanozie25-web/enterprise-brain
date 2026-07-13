//! Ask Brain governance harness A-1..A-9. FULLY OFFLINE: MockGenerator,
//! FileEmbeddings (full 600-doc corpus — closing the M2b review gap),
//! MockJudge; the only sockets any test may open are loopback listeners in
//! A-9's constructor checks.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::embed::FileEmbeddings;
use retrieval::index::{build_index, tokenize};
use retrieval::judge::{MockBehavior as JudgeBehavior, MockJudge};
use retrieval::vector::{build_vectors, snippet_of};
use serde_json::{json, Value};
use service::answer::{ask, validate_citations, AskOptions, AskTrace, CitationFault};
use service::cache::AnswerCache;
use service::generate::{Generator, MockBehavior as GenBehavior, MockGenerator};
use service::{app, loopback_listener, AppState};
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Shared, build-once world
// ---------------------------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    // Unique per invocation: Windows scanners (Search indexer / Defender) can
    // hold a just-deleted path in delete-pending state, so re-creating the
    // SAME path races them into Os error 5 "Access is denied". A fresh suffix
    // never re-opens a dying path; prior runs' dirs are swept best-effort (a
    // locked leftover is skipped now and reaped on a later run).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let base = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    let prefix = format!("{name}-");
    if let Ok(entries) = base.read_dir() {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let dir = base.join(format!(
        "{prefix}{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

struct World {
    fixtures_dir: PathBuf,
    artifacts_dir: PathBuf,
    idx_dir: PathBuf,
    embeddings: Arc<FileEmbeddings>,
    allowlists: BTreeMap<String, BTreeSet<String>>,
    principal_ids: Vec<String>,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();

        // Real M1 allowlists over the FULL corpus, via the frozen compiler.
        let artifacts_dir = scratch("a_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");
        let allowlists: BTreeMap<String, BTreeSet<String>> = set
            .artifacts
            .iter()
            .map(|a| {
                (
                    a.principal_id.clone(),
                    a.entries.iter().map(|e| e.document_id.clone()).collect(),
                )
            })
            .collect();
        let principal_ids: Vec<String> = allowlists.keys().cloned().collect();
        assert_eq!(principal_ids.len(), 124);

        // Hybrid index over ALL 600 documents with committed embeddings.
        let embeddings = Arc::new(
            FileEmbeddings::load(&[
                common::docs_embeddings_path().as_path(),
                common::query_embeddings_path().as_path(),
            ])
            .expect("load committed embeddings"),
        );
        let idx_dir = scratch("a_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");
        build_vectors(
            &fixtures_dir,
            &idx_dir,
            embeddings.as_ref(),
            std::time::Duration::from_millis(5000),
            240,
        )
        .expect("build full-corpus vectors");

        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
            embeddings,
            allowlists,
            principal_ids,
        }
    })
}

/// A fresh service state over the shared world.
fn state_with(
    generator: Option<Arc<dyn Generator>>,
    judge: Option<Arc<MockJudge>>,
    cache: Option<Arc<AnswerCache>>,
) -> AppState {
    let world = world();
    let mut state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
        .with_embedder(world.embeddings.clone())
        .with_cache(cache);
    if let Some(generator) = generator {
        state = state.with_generator(generator);
    }
    if let Some(judge) = judge {
        state = state.with_judge(judge);
    }
    state
}

fn ask_ok(
    state: &AppState,
    principal: &str,
    query: &str,
    options: &AskOptions,
) -> (Value, AskTrace, Vec<u8>) {
    let (bytes, trace) = ask(state, principal, query, options).expect("ask succeeds");
    let value: Value = serde_json::from_slice(&bytes).expect("envelope parses");
    (value, trace, bytes)
}

fn result_ids(envelope: &Value) -> Vec<&str> {
    envelope["results"]
        .as_array()
        .expect("results array")
        .iter()
        .map(|r| r["document_id"].as_str().expect("document_id"))
        .collect()
}

fn corpus_docs() -> &'static BTreeMap<String, (String, String)> {
    static DOCS: OnceLock<BTreeMap<String, (String, String)>> = OnceLock::new();
    DOCS.get_or_init(|| {
        let value: Value = serde_json::from_slice(
            &fs::read(world().fixtures_dir.join("documents.json")).expect("read documents"),
        )
        .expect("parse documents");
        value["documents"]
            .as_array()
            .expect("documents array")
            .iter()
            .map(|d| {
                (
                    d["id"].as_str().expect("id").to_string(),
                    (
                        d["title"].as_str().expect("title").to_string(),
                        d["body"].as_str().expect("body").to_string(),
                    ),
                )
            })
            .collect()
    })
}

fn vocabulary() -> &'static Vec<String> {
    static VOCAB: OnceLock<Vec<String>> = OnceLock::new();
    VOCAB.get_or_init(|| {
        let mut tokens: BTreeSet<String> = BTreeSet::new();
        for (title, body) in corpus_docs().values() {
            tokens.extend(tokenize(title));
            tokens.extend(tokenize(body));
        }
        tokens.into_iter().filter(|t| t.len() >= 3).collect()
    })
}

/// Hits BOTH mosaic template families (the internal "operating summary"
/// notices and the confidential "aggregate financial position" notices), so
/// tagged pairs co-surface in one result list.
const FAMILY_MIX_QUERY: &str =
    "operating summary headcount staff aggregate financial position payroll commitment annum";

/// The tagged pairs whose BOTH members appear in `ids`.
fn co_present_pairs<'a>(pairs: &'a [(String, String)], ids: &[&str]) -> Vec<&'a (String, String)> {
    pairs
        .iter()
        .filter(|(a, b)| ids.contains(&a.as_str()) && ids.contains(&b.as_str()))
        .collect()
}

/// Replays the serve-time mosaic rule over a result list: for each tagged
/// pair with both members present, the lower-ranked member is removed.
fn replay_mosaic_bound(pairs: &[(String, String)], ids: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut surviving: Vec<&str> = ids.to_vec();
    let mut removed = Vec::new();
    for (a, b) in pairs {
        let position_a = surviving.iter().position(|id| id == a);
        let position_b = surviving.iter().position(|id| id == b);
        if let (Some(i), Some(j)) = (position_a, position_b) {
            removed.push(surviving.remove(i.max(j)).to_string());
        }
    }
    (surviving.into_iter().map(str::to_string).collect(), removed)
}

// ---------------------------------------------------------------------------
// A-1 GENERATION SEAL
// ---------------------------------------------------------------------------

#[test]
fn a1_generator_sees_only_the_sealed_context() {
    let world = world();
    let generator = Arc::new(MockGenerator::new(GenBehavior::CiteFirst));
    let state = state_with(Some(generator.clone()), None, None);
    let vocab = vocabulary();
    let mut rng = common::Lcg::new(0xA1_5EED);

    let mut principals: BTreeSet<&str> = BTreeSet::new();
    while principals.len() < 25 {
        principals.insert(rng.pick(&world.principal_ids).as_str());
    }
    let queries: Vec<String> = (0..20)
        .map(|_| {
            let n = 1 + (rng.next() as usize) % 4;
            (0..n)
                .map(|_| rng.pick(vocab).clone())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();

    let mut asks = 0usize;
    let mut captures = 0usize;
    let mut ids_checked = 0usize;
    let mut violations = 0usize;
    for principal in &principals {
        let allowlist = &world.allowlists[*principal];
        for query in &queries {
            let (_, trace, _) = ask_ok(&state, principal, query, &AskOptions::default());
            asks += 1;
            let mut captured = generator.captured.lock().expect("captured");
            let calls: Vec<_> = captured.drain(..).collect();
            drop(captured);
            if trace.sealed.is_empty() {
                assert!(calls.is_empty(), "no generation without context");
                continue;
            }
            assert_eq!(calls.len(), 1, "exactly one generator call per ask");
            captures += 1;
            let (seen_query, seen_context) = &calls[0];
            assert_eq!(seen_query, query, "the generator sees the ask query only");
            assert_eq!(
                seen_context, &trace.sealed,
                "the generator saw exactly the instrumented sealed context"
            );
            for doc in seen_context {
                ids_checked += 1;
                if !allowlist.contains(&doc.doc_id) {
                    violations += 1;
                }
                let (title, body) = &corpus_docs()[&doc.doc_id];
                assert_eq!(&doc.title, title);
                assert_eq!(doc.snippet, snippet_of(body, 480));
                assert!(doc.snippet.chars().count() <= 480);
            }
            assert!(seen_context.len() <= 6, "context is at most 6 documents");
        }
    }
    println!(
        "A-1 summary: asks={asks} (25 principals x 20 queries) generator_calls={captures} \
         context_ids_checked={ids_checked} violations={violations}"
    );
    assert_eq!(asks, 500);
    assert_eq!(violations, 0, "generation seal is zero tolerance");
}

// ---------------------------------------------------------------------------
// A-2 CITATION VALIDATION
// ---------------------------------------------------------------------------

#[test]
fn a2_citation_validation_fails_closed() {
    // Pure rule first.
    let sealed: BTreeSet<&str> = ["d0001", "d0002"].into_iter().collect();
    assert_eq!(
        validate_citations("Grounded [d0001].", &sealed),
        Ok(vec!["d0001".to_string()])
    );
    assert_eq!(
        validate_citations("Grounded [d0001] and [d0002] and [d0001].", &sealed),
        Ok(vec!["d0001".to_string(), "d0002".to_string()])
    );
    assert_eq!(
        validate_citations("Claim [d9999].", &sealed),
        Err(CitationFault::OutOfContext)
    );
    assert_eq!(
        // EVERY bracketed segment is treated as a citation — fail closed.
        validate_citations("Claim [d0001], aside [sic].", &sealed),
        Err(CitationFault::OutOfContext)
    );
    assert_eq!(
        validate_citations("Confident uncited claim.", &sealed),
        Err(CitationFault::Uncited)
    );

    // Through the pipeline: a foreign citation is now a PER-CLAIM grounding
    // refusal (K1) — its lone claim dies at the gate, so the answer is
    // omitted, the drop is DISCLOSED, and it is not a format fault.
    let state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::ForeignCitation))),
        None,
        None,
    );
    let (envelope, trace, bytes) = ask_ok(
        &state,
        "p060",
        "payroll salary review",
        &AskOptions::default(),
    );
    assert!(envelope.get("answer").is_none(), "answer refused entirely");
    assert_eq!(envelope["generation_applied"], Value::Bool(false));
    assert_eq!(envelope["grounding_applied"], Value::Bool(true));
    assert_eq!(envelope["grounding"]["admitted"], json!(0));
    assert_eq!(envelope["grounding"]["refused"], json!(1));
    assert!(
        !trace.generation_fault,
        "a grounding refusal is disclosed, not a format fault"
    );
    assert!(
        !envelope["results"].as_array().expect("results").is_empty(),
        "retrieval-only response still serves results"
    );
    assert!(
        !String::from_utf8(bytes).expect("utf8").contains("d9999"),
        "the foreign id never reaches the envelope"
    );

    // Free prose with no claim blocks: a format fault (K1 strict parse).
    let state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::Uncited))),
        None,
        None,
    );
    let (envelope, trace, _) = ask_ok(
        &state,
        "p060",
        "payroll salary review",
        &AskOptions::default(),
    );
    assert!(envelope.get("answer").is_none());
    assert!(trace.generation_fault);
    assert_eq!(
        envelope["grounding_applied"],
        Value::Bool(false),
        "the gate never ran on an unparseable draft"
    );

    // Valid grounded claims pass through.
    let state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        None,
        None,
    );
    let (envelope, trace, _) = ask_ok(
        &state,
        "p060",
        "payroll salary review",
        &AskOptions::default(),
    );
    assert_eq!(envelope["generation_applied"], Value::Bool(true));
    assert_eq!(envelope["grounding_applied"], Value::Bool(true));
    assert!(!trace.generation_fault);
    let citations: Vec<&str> = envelope["answer"]["citations"]
        .as_array()
        .expect("citations")
        .iter()
        .map(|c| c.as_str().expect("citation"))
        .collect();
    let mut sealed_ids: Vec<&str> = trace.sealed.iter().map(|d| d.doc_id.as_str()).collect();
    sealed_ids.sort_unstable();
    assert_eq!(
        citations, sealed_ids,
        "CiteEach cites the whole sealed context (citations are deduped + sorted)"
    );
    assert_eq!(
        envelope["answer"]["claims"]
            .as_array()
            .expect("claims")
            .len(),
        trace.sealed.len(),
        "one admitted claim per sealed doc"
    );

    // Generator outage: degrade, no fault.
    let state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::Fail))),
        None,
        None,
    );
    let (envelope, trace, _) = ask_ok(
        &state,
        "p060",
        "payroll salary review",
        &AskOptions::default(),
    );
    assert!(envelope.get("answer").is_none());
    assert_eq!(envelope["generation_applied"], Value::Bool(false));
    assert!(
        !trace.generation_fault,
        "an outage is degradation, not a fault"
    );
}

// ---------------------------------------------------------------------------
// A-3 MOSAIC BOUND
// ---------------------------------------------------------------------------

#[test]
fn a3_mosaic_bound_removes_the_lower_ranked_member_from_context_only() {
    let state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        None,
        None,
    );
    assert!(
        state
            .mosaic_pairs
            .contains(&("d0193".to_string(), "d0194".to_string())),
        "the pairs are tagged in the compiled artifacts"
    );

    // Both families surface together -> tagged pairs co-present in results.
    let (envelope, trace, _) = ask_ok(&state, "p060", FAMILY_MIX_QUERY, &AskOptions::default());
    let ids = result_ids(&envelope);
    let co_present = co_present_pairs(&state.mosaic_pairs, &ids);
    assert!(
        !co_present.is_empty(),
        "the family-mix query co-surfaces at least one tagged pair: {ids:?}"
    );
    assert_eq!(envelope["aggregation_bounded"], Value::Bool(true));

    // The serve-time rule replayed exactly: per pair, the LOWER-ranked
    // member leaves; the sealed context is the top survivors.
    let (surviving, expected_removed) = replay_mosaic_bound(&state.mosaic_pairs, &ids);
    assert_eq!(trace.mosaic_removed, expected_removed);
    let sealed_ids: Vec<&str> = trace.sealed.iter().map(|d| d.doc_id.as_str()).collect();
    let expected_sealed: Vec<&str> = surviving.iter().take(6).map(String::as_str).collect();
    assert_eq!(sealed_ids, expected_sealed);
    for removed in &trace.mosaic_removed {
        assert!(
            !sealed_ids.contains(&removed.as_str()),
            "a removed member is absent from the sealed context"
        );
        assert!(
            ids.contains(&removed.as_str()),
            "the removed member still appears in plain retrieval results"
        );
        for (a, b) in &state.mosaic_pairs {
            if removed == a || removed == b {
                let other = if removed == a { b } else { a };
                assert!(
                    !trace.mosaic_removed.contains(other),
                    "only the lower-ranked member of a pair is removed"
                );
            }
        }
    }
    let answer = envelope.get("answer").expect("generation applied");
    for citation in answer["citations"].as_array().expect("citations") {
        assert!(
            !trace
                .mosaic_removed
                .contains(&citation.as_str().expect("citation").to_string()),
            "the answer cannot cite what the bound removed"
        );
    }

    // A query that surfaces only one family: no pair co-present, no bound.
    let (envelope, trace, _) = ask_ok(
        &state,
        "p060",
        "payroll salary review",
        &AskOptions::default(),
    );
    let ids = result_ids(&envelope);
    assert!(
        co_present_pairs(&state.mosaic_pairs, &ids).is_empty(),
        "the single-family query surfaces no complete pair: {ids:?}"
    );
    assert_eq!(envelope["aggregation_bounded"], Value::Bool(false));
    assert!(trace.mosaic_removed.is_empty());
}

// ---------------------------------------------------------------------------
// A-4 CACHE SCOPING
// ---------------------------------------------------------------------------

#[test]
fn a4_cache_is_scope_isolated_and_snapshot_pinned() {
    let world = world();
    let shared_cache = Arc::new(AnswerCache::new());
    let generator: Arc<dyn Generator> = Arc::new(MockGenerator::new(GenBehavior::CiteEach));
    let state = state_with(Some(generator.clone()), None, Some(shared_cache.clone()));

    // Same query, two principals -> two distinct entries, no cross-service.
    let options = AskOptions::default();
    let (envelope_a, _, bytes_a) = ask_ok(&state, "p060", "payroll salary review", &options);
    assert_eq!(shared_cache.len(), 1);
    let (envelope_b, _, bytes_b) = ask_ok(&state, "p061", "payroll salary review", &options);
    assert_eq!(shared_cache.len(), 2, "two principals, two entries");
    assert_ne!(bytes_a, bytes_b);
    assert_ne!(envelope_a["query_hash"], envelope_b["query_hash"]);

    // Cache hit serves the identical bytes; bypass recomputes them.
    let (_, trace_hit, bytes_hit) = ask_ok(&state, "p060", "payroll salary review", &options);
    assert!(trace_hit.cache_hit);
    assert_eq!(bytes_a, bytes_hit);
    let bypass = AskOptions {
        bypass_cache: true,
        ..AskOptions::default()
    };
    let (_, trace_bypass, bytes_bypass) = ask_ok(&state, "p060", "payroll salary review", &bypass);
    assert!(!trace_bypass.cache_hit, "--no-cache bypasses the cache");
    assert_eq!(bytes_a, bytes_bypass, "and still computes identical bytes");

    // Fixture byte-flip: a new snapshot_version makes old entries
    // unreachable by construction (the key embeds the snapshot).
    let flipped_fixtures = scratch("a4_flipped_fixtures");
    for name in ["company.json", "documents.json", "traps.json"] {
        fs::copy(world.fixtures_dir.join(name), flipped_fixtures.join(name)).expect("copy");
    }
    let documents_path = flipped_fixtures.join("documents.json");
    let text = fs::read_to_string(&documents_path).expect("read");
    assert!(text.contains("controlled"));
    fs::write(
        &documents_path,
        text.replacen("controlled", "cantrolled", 1),
    )
    .expect("flip");

    let flipped_artifacts = scratch("a4_flipped_artifacts");
    let snap = scope_compiler::snapshot::take(&flipped_fixtures).expect("snapshot");
    let m1_world = scope_compiler::load_world(&flipped_fixtures).expect("validate");
    let (set, unknown) =
        scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile flipped");
    assert!(unknown.is_empty());
    scope_compiler::compile::write_artifacts(&flipped_artifacts, &set).expect("write");

    let flipped_idx = scratch("a4_flipped_idx");
    build_index(&flipped_fixtures, &flipped_idx).expect("build flipped index");
    let flipped_state = AppState::build(&flipped_fixtures, &flipped_artifacts, &flipped_idx)
        .expect("build flipped state")
        .with_generator(generator)
        .with_cache(Some(shared_cache.clone()));

    let (envelope_c, trace_c, _) =
        ask_ok(&flipped_state, "p060", "payroll salary review", &options);
    assert!(
        !trace_c.cache_hit,
        "old entries are unreachable after the flip"
    );
    assert_eq!(
        shared_cache.len(),
        3,
        "the flipped world cached a new entry"
    );
    assert_ne!(
        envelope_a["snapshot_version"],
        envelope_c["snapshot_version"]
    );
}

// ---------------------------------------------------------------------------
// A-5 IDENTITY FAIL-CLOSED (HTTP)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a5_identity_fails_closed_and_unknown_is_indistinguishable() {
    let state = Arc::new(state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        None,
        None,
    ));
    let router = app(state);

    // Missing header -> 401.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ask")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query":"payroll"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(body["demo_identity_mode"], Value::Bool(true));

    // Unknown principal -> 200 with the empty envelope. Identity is now bound
    // from a server session; an unknown principal can still authenticate in the
    // demo, then sees the empty envelope (deny by default downstream).
    let ask_request = |authz: &str, query: &str| {
        Request::builder()
            .method("POST")
            .uri("/ask")
            .header("content-type", "application/json")
            .header("authorization", authz)
            .body(Body::from(
                serde_json::to_vec(&json!({ "query": query })).expect("body"),
            ))
            .expect("request")
    };
    let ghost_auth = common::bearer(&router, "p_ghost_404").await;
    let response = router
        .clone()
        .oneshot(ask_request(&ghost_auth, "payroll salary review"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let unknown: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(unknown["results"].as_array().expect("results").len(), 0);
    assert_eq!(
        unknown["scope_statement"]["groups"]
            .as_array()
            .expect("groups")
            .len(),
        0
    );
    assert_eq!(
        unknown["scope_statement"]["sites"]
            .as_array()
            .expect("sites")
            .len(),
        0
    );
    assert_eq!(unknown["scope_statement"]["band"], Value::Null);
    assert!(unknown.get("error").is_none(), "no error text at all");
    assert!(unknown.get("answer").is_none());

    // The unknown-principal envelope has EXACTLY the same key set as a known
    // principal whose query matches nothing: the response shape cannot
    // distinguish "unknown" from "ungranted".
    let void_auth = common::bearer(&router, "p_void").await;
    let response = router
        .clone()
        .oneshot(ask_request(&void_auth, "zzqqxx wwyyvv unmatched tokens"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let known_empty: Value = serde_json::from_slice(&bytes).expect("json");
    let keys = |v: &Value| -> Vec<String> {
        v.as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&unknown), keys(&known_empty));
    assert_eq!(known_empty["results"].as_array().expect("results").len(), 0);

    // /scope for an unknown principal: the empty statement, 200.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/scope")
                .header("authorization", common::bearer(&router, "p_ghost_404").await)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let scope: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        scope["scope_statement"]["groups"]
            .as_array()
            .expect("groups")
            .len(),
        0
    );

    // /scope without the header: 401.
    let response = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/scope")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// A-6 FULL-CORPUS HYBRID LEAK PASS
// ---------------------------------------------------------------------------

#[test]
fn a6_full_corpus_hybrid_service_pipeline_never_leaks() {
    let world = world();
    let generator = Arc::new(MockGenerator::new(GenBehavior::CiteEach));
    let judge = Arc::new(MockJudge::new(JudgeBehavior::Identity));
    let state = state_with(Some(generator), Some(judge), None);

    let options = AskOptions {
        hybrid: true,
        judge: true,
        bypass_cache: false,
        granted_context: None,
    };
    let mut asks = 0usize;
    let mut ids_checked = 0usize;
    let mut violations: Vec<String> = Vec::new();

    for principal in &world.principal_ids {
        let allowlist = &world.allowlists[principal];
        for query in common::QUERY_TEXTS {
            let (envelope, trace, _) = ask_ok(&state, principal, query, &options);
            asks += 1;
            assert_eq!(
                envelope["retrieval_mode"], "hybrid",
                "committed query vectors keep hybrid live"
            );

            let mut observe = |what: &str, id: &str| {
                ids_checked += 1;
                if !allowlist.contains(id) {
                    violations.push(format!("{what} leaked {id} for {principal} ({query:?})"));
                }
            };
            if let Some(retrieval_trace) = &trace.retrieval {
                for stage in &retrieval_trace.stages {
                    for id in &stage.doc_ids {
                        observe(&format!("stage {}", stage.stage), id);
                    }
                }
            }
            for doc in &trace.sealed {
                observe("sealed context", &doc.doc_id);
            }
            for id in result_ids(&envelope) {
                observe("results", id);
            }
            if let Some(answer) = envelope.get("answer") {
                for citation in answer["citations"].as_array().expect("citations") {
                    observe("citation", citation.as_str().expect("citation"));
                }
            }
        }
    }

    for violation in &violations {
        println!("{violation}");
    }
    println!(
        "A-6 summary: asks={asks} (124 principals x 12 queries, hybrid+judge+generation) \
         ids_checked={ids_checked} violations={}",
        violations.len()
    );
    assert_eq!(asks, 1488);
    assert!(asks >= 1000, "spec floor");
    assert_eq!(
        violations.len(),
        0,
        "full-pipeline leak pass is zero tolerance"
    );
}

// ---------------------------------------------------------------------------
// A-7 ENVELOPE RULES + REAL SCOPE STATEMENTS
// ---------------------------------------------------------------------------

const ENVELOPE_KEY_WHITELIST: [&str; 45] = [
    "active",
    "admitted",
    "aggregation_bounded",
    "answer",
    "approver_id",
    "band",
    "capability",
    "citations",
    "claims",
    "demo_identity_mode",
    "doc_id",
    "document_id",
    "effective_successor",
    "generation_applied",
    "grant_id",
    "grant_scope",
    "grant_status",
    "granted_context",
    "grounding",
    "grounding_applied",
    "groups",
    "id",
    "index_version",
    "initiative",
    "judge_applied",
    "locator",
    "name",
    "principal_id",
    "query_hash",
    "reasons_ref",
    "refused",
    "request_id",
    "results",
    "retrieval_mode",
    "scope_statement",
    "score_rank",
    "sensitivity",
    "sites",
    "snapshot_version",
    "strategy",
    "superseded",
    "target_kind",
    "text",
    "title",
    "workflow",
];

const FORBIDDEN_KEY_SUBSTRINGS: [&str; 18] = [
    "suppressed",
    "hidden",
    "filtered",
    "total",
    "count",
    "partition",
    "stats",
    "omitted",
    "redacted",
    "excluded",
    "denied",
    "hits",
    "embedded",
    "judged",
    "elided",
    "fault",
    "usage",
    "token",
];

fn collect_keys(value: &Value, keys: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                keys.insert(k.clone());
                collect_keys(v, keys);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_keys(item, keys);
            }
        }
        _ => {}
    }
}

#[test]
fn a7_envelope_keys_are_whitelisted_and_scope_matches_ground_truth() {
    let world = world();
    let judge = Arc::new(MockJudge::new(JudgeBehavior::Identity));
    let state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        Some(judge),
        None,
    );

    // A rich envelope (hybrid + judge + generation + mosaic pressure) and an
    // engineered heavy-suppression envelope (narrowest principal, broad
    // corpus vocabulary: hundreds of matching docs exist; the envelope must
    // say nothing about any of them).
    let rich_options = AskOptions {
        hybrid: true,
        judge: true,
        bypass_cache: false,
        granted_context: None,
    };
    let (rich, _, _) = ask_ok(&state, "p060", FAMILY_MIX_QUERY, &rich_options);
    assert_eq!(rich["aggregation_bounded"], Value::Bool(true));
    let (suppressed, _, _) = ask_ok(
        &state,
        "p_void",
        "procedure record review quality customer stock site warehouse",
        &AskOptions::default(),
    );

    for envelope in [&rich, &suppressed] {
        let mut keys = BTreeSet::new();
        collect_keys(envelope, &mut keys);
        for key in &keys {
            assert!(
                ENVELOPE_KEY_WHITELIST.contains(&key.as_str()),
                "envelope serialized an unexpected key {key:?}"
            );
            let lower = key.to_lowercase();
            for forbidden in FORBIDDEN_KEY_SUBSTRINGS {
                assert!(
                    !lower.contains(forbidden),
                    "envelope key {key:?} smells like a dark count"
                );
            }
        }
    }

    // Real scope statements: recompute the expectation from company.json and
    // compare for a deterministic sample of people and agents.
    let company: Value = serde_json::from_slice(
        &fs::read(world.fixtures_dir.join("company.json")).expect("read company"),
    )
    .expect("parse company");
    let mut expected_groups: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for group in company["groups"].as_array().expect("groups") {
        for member in group["member_ids"].as_array().expect("members") {
            expected_groups
                .entry(member.as_str().expect("member"))
                .or_default()
                .insert(group["id"].as_str().expect("group id"));
        }
    }

    let mut sampled = vec![
        "p001".to_string(),
        "p060".to_string(),
        "p_void".to_string(),
        "agent_finance_analyst".to_string(),
        "agent_ops_concierge".to_string(),
    ];
    let mut rng = common::Lcg::new(0xA7_5EED);
    for _ in 0..5 {
        sampled.push(rng.pick(&world.principal_ids).clone());
    }
    for principal in &sampled {
        let (envelope, _, _) = ask_ok(&state, principal, "quality", &AskOptions::default());
        let statement = &envelope["scope_statement"];
        if let Some(person) = company["people"]
            .as_array()
            .expect("people")
            .iter()
            .find(|p| p["id"] == principal.as_str())
        {
            let groups: Vec<&str> = statement["groups"]
                .as_array()
                .expect("groups")
                .iter()
                .map(|g| g.as_str().expect("group"))
                .collect();
            let expected: Vec<&str> = expected_groups
                .get(principal.as_str())
                .map(|set| set.iter().copied().collect())
                .unwrap_or_default();
            assert_eq!(groups, expected, "groups for {principal}");
            assert_eq!(
                statement["sites"],
                json!([person["site"].as_str().expect("site")]),
                "sites for {principal}"
            );
            assert_eq!(
                statement["band"], person["employment_band"],
                "band for {principal} (populated where the model defines it)"
            );
        } else {
            let agent = company["agents"]
                .as_array()
                .expect("agents")
                .iter()
                .find(|a| a["id"] == principal.as_str())
                .expect("sampled principal exists");
            let mut expected: Vec<&str> = agent["grant"]["groups"]
                .as_array()
                .expect("grant groups")
                .iter()
                .map(|g| g.as_str().expect("group"))
                .collect();
            expected.sort_unstable();
            let groups: Vec<&str> = statement["groups"]
                .as_array()
                .expect("groups")
                .iter()
                .map(|g| g.as_str().expect("group"))
                .collect();
            assert_eq!(groups, expected, "agent grant groups for {principal}");
        }
    }
}

// ---------------------------------------------------------------------------
// A-10 /doc ALLOWLIST + 404 INDISTINGUISHABILITY
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a10_doc_serves_allowlisted_cards_and_identical_404s() {
    let world = world();
    let router = app(Arc::new(state_with(None, None, None)));
    let docs = corpus_docs();

    // FC-A1: pre-authenticate every principal once; the sync request-builder
    // closure then attaches the session bearer (identity is no longer a header
    // the caller can assert). `None` -> no auth -> 401.
    let mut auth: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for principal in &world.principal_ids {
        auth.insert(principal.clone(), common::bearer(&router, principal).await);
    }
    auth.insert(
        "p_ghost_404".to_string(),
        common::bearer(&router, "p_ghost_404").await,
    );
    let get_doc = |principal: Option<&str>, id: &str| {
        let mut builder = Request::builder().method("GET").uri(format!("/doc/{id}"));
        if let Some(principal) = principal {
            builder = builder.header(
                "authorization",
                auth.get(principal).expect("principal pre-authenticated").clone(),
            );
        }
        builder.body(Body::empty()).expect("request")
    };
    let full_response = |response: axum::response::Response| async move {
        let status = response.status();
        let mut headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.to_string(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect();
        headers.sort();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        (status, headers, bytes.to_vec())
    };

    // Allowlist enforcement + snippet discipline across sampled principals.
    let mut rng = common::Lcg::new(0xA10_5EED);
    let mut checked_in = 0usize;
    let mut checked_out = 0usize;
    for _ in 0..10 {
        let principal = rng.pick(&world.principal_ids).clone();
        let allowlist = &world.allowlists[&principal];
        let in_scope = allowlist.iter().next().expect("everyone reads public docs");
        let response = router
            .clone()
            .oneshot(get_doc(Some(&principal), in_scope))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let card: Value = serde_json::from_slice(&bytes).expect("card parses");
        let (title, body, sensitivity) = {
            let (t, b) = (&docs[in_scope].0, &docs[in_scope].1);
            let s = card["sensitivity"].as_str().expect("sensitivity");
            (t, b, s.to_string())
        };
        assert_eq!(card["document_id"], in_scope.as_str());
        assert_eq!(card["title"], title.as_str());
        assert_eq!(card["snippet"], snippet_of(body, 480));
        assert!(card["snippet"].as_str().expect("snippet").chars().count() <= 480);
        assert!(!sensitivity.is_empty());
        assert!(
            card.get("body").is_none(),
            "full bodies never leave the service"
        );
        checked_in += 1;

        // An out-of-scope corpus doc and a nonexistent id: byte-identical 404s.
        let out_of_scope = docs
            .keys()
            .find(|id| !allowlist.contains(*id))
            .expect("nobody sees everything");
        let response_out = router
            .clone()
            .oneshot(get_doc(Some(&principal), out_of_scope))
            .await
            .expect("response");
        let response_missing = router
            .clone()
            .oneshot(get_doc(Some(&principal), "d9999_nope"))
            .await
            .expect("response");
        let out = full_response(response_out).await;
        let missing = full_response(response_missing).await;
        assert_eq!(out.0, StatusCode::NOT_FOUND);
        assert_eq!(out, missing, "ungranted and unknown must be byte-identical");
        checked_out += 1;
    }
    println!("A-10: {checked_in} in-scope cards verified, {checked_out} identical 404 pairs");

    // A superseded doc carries the marker and its in-allowlist successor.
    let reader = world
        .principal_ids
        .iter()
        .find(|p| world.allowlists[*p].contains("d0001"))
        .expect("someone reads the superseded SOP");
    let response = router
        .clone()
        .oneshot(get_doc(Some(reader), "d0001"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let card: Value = serde_json::from_slice(&bytes).expect("card parses");
    assert_eq!(card["superseded"], Value::Bool(true));
    assert_eq!(card["effective_successor"], "d0002");

    // Unknown principal: the same 404 as everything else. Missing header: 401.
    let response = router
        .clone()
        .oneshot(get_doc(Some("p_ghost_404"), "d0001"))
        .await
        .expect("response");
    let ghost = full_response(response).await;
    let response = router
        .clone()
        .oneshot(get_doc(Some(reader), "d9999_nope"))
        .await
        .expect("response");
    let missing = full_response(response).await;
    assert_eq!(ghost, missing, "unknown principal gets the identical 404");
    let response = router
        .clone()
        .oneshot(get_doc(None, "d0001"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// A-9 extension: CORS construction refusal + header behavior
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a9x_cors_allows_only_loopback_origins() {
    use service::cors::cors_layer;

    // Construction: the spec origins pass; anything non-loopback refuses.
    assert!(cors_layer(&["http://localhost:3000", "http://127.0.0.1:3000"]).is_ok());
    assert!(cors_layer(&["http://[::1]:3000"]).is_ok());
    for bad in [
        "http://192.168.1.5:3000",
        "http://10.0.0.1:3000",
        "http://evil.example:3000",
        "https://localhost:3000",
        "http://8.8.8.8:3000",
    ] {
        assert!(
            cors_layer(&[bad]).is_err(),
            "non-loopback origin {bad} must refuse at construction"
        );
    }
    assert!(
        cors_layer(&[]).is_err(),
        "an empty allowlist is a bug, not a wildcard"
    );

    // Behavior: allowed origins get the header; others get nothing.
    let router = app(Arc::new(state_with(None, None, None)));
    let healthz = |origin: Option<&str>, method: &str| {
        let mut builder = Request::builder().method(method).uri("/healthz");
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        if method == "OPTIONS" {
            builder = builder.header("access-control-request-method", "GET");
        }
        builder.body(Body::empty()).expect("request")
    };

    let response = router
        .clone()
        .oneshot(healthz(Some("http://localhost:3000"), "GET"))
        .await
        .expect("response");
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .map(|v| v.to_str().expect("ascii")),
        Some("http://localhost:3000")
    );

    let response = router
        .clone()
        .oneshot(healthz(Some("http://attacker.example"), "GET"))
        .await
        .expect("response");
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none(),
        "disallowed origins get no CORS headers"
    );

    let response = router
        .clone()
        .oneshot(healthz(Some("http://127.0.0.1:3000"), "OPTIONS"))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let allow_headers = response
        .headers()
        .get("access-control-allow-headers")
        .expect("preflight allows headers")
        .to_str()
        .expect("ascii");
    assert!(allow_headers.contains("authorization"));
}

// ---------------------------------------------------------------------------
// Demo profile: explicit, labeled, never implicit
// ---------------------------------------------------------------------------

#[test]
fn a_demo_profile_is_explicit_and_labeled() {
    let service_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let example = service::ServiceConfig::load(&service_dir.join("config.example.json"))
        .expect("example config parses");
    assert_eq!(example.judge_timeout_ms, Some(2000), "production default");

    let demo = service::ServiceConfig::load(&service_dir.join("config.demo.json"))
        .expect("demo config parses");
    assert_eq!(demo.judge_timeout_ms, Some(8000));
    let label = demo.profile.expect("the demo profile is labeled");
    assert!(label.contains("demo"), "the label says what it is");
    assert!(
        label.contains("2000ms remains the production default"),
        "the label states the production default plainly"
    );

    // Nothing selects the demo value implicitly: a config that says nothing
    // about the judge timeout gets the production default.
    let state = state_with(None, None, None);
    assert_eq!(state.judge_timeout_ms, 2000);
}

// ---------------------------------------------------------------------------
// A-8 DETERMINISM
// ---------------------------------------------------------------------------

#[test]
fn a8_same_ask_twice_is_byte_identical_with_and_without_cache() {
    let judge = Arc::new(MockJudge::new(JudgeBehavior::Identity));
    let cached_state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        Some(judge.clone()),
        Some(Arc::new(AnswerCache::new())),
    );
    let options = AskOptions {
        hybrid: true,
        judge: true,
        bypass_cache: false,
        granted_context: None,
    };

    let (_, trace_one, bytes_one) =
        ask_ok(&cached_state, "p060", "payroll salary review", &options);
    assert!(!trace_one.cache_hit);
    let (_, trace_two, bytes_two) =
        ask_ok(&cached_state, "p060", "payroll salary review", &options);
    assert!(trace_two.cache_hit, "second ask is served from the cache");
    assert_eq!(bytes_one, bytes_two);

    let uncached_state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        Some(judge),
        None,
    );
    let (_, _, bytes_three) = ask_ok(&uncached_state, "p060", "payroll salary review", &options);
    let (_, _, bytes_four) = ask_ok(&uncached_state, "p060", "payroll salary review", &options);
    assert_eq!(bytes_three, bytes_four, "no cache: recomputed identically");
    assert_eq!(bytes_one, bytes_three, "cache on/off serve the same bytes");
}

// ---------------------------------------------------------------------------
// A-9 BIND REFUSAL + SILENT HEALTHZ
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a9_non_loopback_bind_refused_and_healthz_reveals_nothing() {
    // Loopback binds construct (ephemeral ports; loopback sockets are the
    // one thing tests may open).
    assert!(loopback_listener("127.0.0.1:0").is_ok());
    assert!(loopback_listener("127.0.0.53:0").is_ok());
    assert!(loopback_listener("[::1]:0").is_ok());

    // Anything else is refused at construction.
    for bad in [
        "0.0.0.0:8787",
        "8.8.8.8:8787",
        "192.168.1.10:8787",
        "[2001:db8::1]:8787",
    ] {
        let err = loopback_listener(bad).expect_err("non-loopback bind must refuse");
        assert!(
            format!("{err:#}").contains("loopback"),
            "refusal names the loopback rule"
        );
    }

    // healthz: constant body, no identity, nothing about the world.
    let state = Arc::new(state_with(None, None, None));
    let response = app(state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    assert_eq!(&bytes[..], b"{\"status\":\"ok\"}\n");
}
