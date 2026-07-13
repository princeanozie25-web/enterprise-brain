//! Hybrid retrieval governance harness R-8..R-12 (R-13 extends R-3 in
//! `governance.rs`). FULLY OFFLINE: embeddings come from committed fixture
//! files, the judge is a mock, and the only `LocalLlmClient` constructions
//! here are loopback-refusal checks against literal IPs — no test opens a
//! socket.
//!
//! The corpus is a 40-document subset of the real fixtures (all five
//! sensitivity classes, supersede pairs, trap documents) with real M1
//! allowlists compiled over it for all 124 principals.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::bail;
use retrieval::embed::{EmbedOutcome, EmbeddingSource, FileEmbeddings};
use retrieval::envelope::Envelope;
use retrieval::index::build_index;
use retrieval::judge::{judge_eligible, Judge, MockBehavior, MockJudge};
use retrieval::local_llm::{append_usage_sidecar, LocalLlmClient};
use retrieval::search::{Engine, HybridParams, JudgeParams, PrincipalScope, SearchOptions, Trace};
use retrieval::vector::{build_vectors, snippet_of};
use serde_json::Value;

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

struct HybridSetup {
    subset_fixtures: PathBuf,
    artifacts_dir: PathBuf,
    engine: Engine,
    embeddings: FileEmbeddings,
    /// principal id -> compiled allowlist over the subset corpus.
    allowlists: BTreeMap<String, BTreeSet<String>>,
    /// principal id -> superseded doc ids within the allowlist.
    superseded: BTreeMap<String, BTreeSet<String>>,
    /// subset doc id -> (title, body).
    docs: BTreeMap<String, (String, String)>,
    principal_ids: Vec<String>,
}

fn setup() -> &'static HybridSetup {
    static SETUP: OnceLock<HybridSetup> = OnceLock::new();
    SETUP.get_or_init(|| {
        let subset_fixtures = scratch("hybrid_subset_fixtures");
        common::write_subset_fixtures(&subset_fixtures);

        // Real M1 allowlists over the subset corpus, via the frozen compiler.
        let artifacts_dir = scratch("hybrid_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&subset_fixtures).expect("snapshot");
        let world = scope_compiler::load_world(&subset_fixtures).expect("subset validates");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let mut allowlists: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut superseded: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for artifact in &set.artifacts {
            allowlists.insert(
                artifact.principal_id.clone(),
                artifact
                    .entries
                    .iter()
                    .map(|e| e.document_id.clone())
                    .collect(),
            );
            superseded.insert(
                artifact.principal_id.clone(),
                artifact
                    .entries
                    .iter()
                    .filter(|e| e.superseded == Some(true))
                    .map(|e| e.document_id.clone())
                    .collect(),
            );
        }
        let principal_ids: Vec<String> = allowlists.keys().cloned().collect();

        // Partitions + vectors from the committed fixture embeddings.
        let (docs_fixture, queries_fixture) = common::embedding_fixture_paths();
        let embeddings = FileEmbeddings::load(&[docs_fixture.as_path(), queries_fixture.as_path()])
            .expect("load committed embeddings");
        let idx_dir = scratch("hybrid_idx");
        build_index(&subset_fixtures, &idx_dir).expect("build subset index");
        build_vectors(
            &subset_fixtures,
            &idx_dir,
            &embeddings,
            Duration::from_millis(5000),
            240,
        )
        .expect("build subset vectors");
        let engine = Engine::open(&idx_dir).expect("open hybrid engine");
        assert!(engine.manifest.vectors.is_some(), "index carries vectors");

        let docs_value: Value = serde_json::from_slice(
            &fs::read(subset_fixtures.join("documents.json")).expect("read subset docs"),
        )
        .expect("parse subset docs");
        let docs: BTreeMap<String, (String, String)> = docs_value["documents"]
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
            .collect();

        HybridSetup {
            subset_fixtures,
            artifacts_dir,
            engine,
            embeddings,
            allowlists,
            superseded,
            docs,
            principal_ids,
        }
    })
}

fn scope_for(id: &str) -> PrincipalScope {
    PrincipalScope::load(&setup().artifacts_dir, id).expect("load principal scope")
}

fn judge_params(judge: &dyn Judge) -> JudgeParams<'_> {
    JudgeParams {
        judge,
        timeout: Duration::from_millis(2000),
        top_k: 12,
        min_candidates: 4,
        max_ratio: 1.3,
    }
}

fn hybrid_options<'a>(
    embedder: &'a dyn EmbeddingSource,
    judge: Option<JudgeParams<'a>>,
    k: usize,
    include_superseded: bool,
) -> SearchOptions<'a> {
    SearchOptions {
        k,
        include_superseded,
        hybrid: Some(HybridParams {
            embedder,
            query_embed_timeout: Duration::from_millis(1500),
        }),
        judge,
    }
}

fn hybrid_search(
    scope: &PrincipalScope,
    query: &str,
    judge: Option<&dyn Judge>,
    include_superseded: bool,
) -> (Envelope, Trace) {
    let setup = setup();
    let options = hybrid_options(
        &setup.embeddings,
        judge.map(judge_params),
        10,
        include_superseded,
    );
    setup
        .engine
        .search(scope, query, &options)
        .expect("hybrid search")
}

// ---------------------------------------------------------------------------
// Failure-injection embedders (offline doctrine branches)
// ---------------------------------------------------------------------------

/// Matches the manifest's model/dim (preflight passes) but every embed call
/// fails — the query-time outage branch.
struct FailingEmbeddings;

impl EmbeddingSource for FailingEmbeddings {
    fn model_id(&self) -> &str {
        common::FIXTURE_MODEL_ID
    }
    fn dim(&self) -> u32 {
        common::FIXTURE_DIM
    }
    fn embed_batch(&self, _texts: &[String], _timeout: Duration) -> anyhow::Result<EmbedOutcome> {
        bail!("simulated embedder outage")
    }
}

/// Reports a different model id — the stale-vectors refusal branch.
struct WrongModelEmbeddings;

impl EmbeddingSource for WrongModelEmbeddings {
    fn model_id(&self) -> &str {
        "some-other-model"
    }
    fn dim(&self) -> u32 {
        common::FIXTURE_DIM
    }
    fn embed_batch(&self, _texts: &[String], _timeout: Duration) -> anyhow::Result<EmbedOutcome> {
        bail!("never reached: preflight must refuse first")
    }
}

/// Reports a different dimension — same refusal branch, other half.
struct WrongDimEmbeddings;

impl EmbeddingSource for WrongDimEmbeddings {
    fn model_id(&self) -> &str {
        common::FIXTURE_MODEL_ID
    }
    fn dim(&self) -> u32 {
        common::FIXTURE_DIM + 1
    }
    fn embed_batch(&self, _texts: &[String], _timeout: Duration) -> anyhow::Result<EmbedOutcome> {
        bail!("never reached: preflight must refuse first")
    }
}

// ---------------------------------------------------------------------------
// The network carve-out: construction-level loopback refusal (no sockets)
// ---------------------------------------------------------------------------

#[test]
fn carve_out_refuses_everything_but_loopback_at_construction() {
    // Accepted: loopback literals. Construction parses and validates only —
    // it never connects.
    assert!(LocalLlmClient::new("http://127.0.0.1:11434").is_ok());
    assert!(
        LocalLlmClient::new("http://127.0.0.53:11434").is_ok(),
        "all of 127/8"
    );
    assert!(LocalLlmClient::new("http://[::1]:11434").is_ok());

    // Refused: non-loopback literal IPs, by construction.
    for bad in [
        "http://8.8.8.8:80",
        "http://93.184.216.34:11434",
        "http://[2001:db8::1]:11434",
        "http://0.0.0.0:11434",
        "http://10.0.0.5:11434",
    ] {
        let err = LocalLlmClient::new(bad).expect_err("non-loopback must refuse");
        assert!(
            format!("{err:#}").contains("loopback"),
            "refusal names the loopback rule: {err:#}"
        );
    }

    // Refused: any scheme that is not plain http (no TLS stack, no cloud).
    assert!(LocalLlmClient::new("https://127.0.0.1:11434").is_err());
    assert!(LocalLlmClient::new("ftp://127.0.0.1:11434").is_err());
    // Refused: paths, garbage.
    assert!(LocalLlmClient::new("http://127.0.0.1:11434/api").is_err());
    assert!(LocalLlmClient::new("not a url").is_err());
}

// ---------------------------------------------------------------------------
// R-8 VECTOR STAGE-LEAK
// ---------------------------------------------------------------------------

#[test]
fn r8_hybrid_stages_never_observe_an_out_of_scope_document() {
    let setup = setup();
    let judge = MockJudge::new(MockBehavior::Identity);

    let mut searches = 0usize;
    let mut ids_checked = 0usize;
    let mut violations: Vec<String> = Vec::new();

    for principal_id in &setup.principal_ids {
        let scope = scope_for(principal_id);
        let allowlist = &setup.allowlists[principal_id];
        let superseded = &setup.superseded[principal_id];
        for query in common::QUERY_TEXTS {
            for include_superseded in [false, true] {
                let (envelope, trace) =
                    hybrid_search(&scope, query, Some(&judge), include_superseded);
                searches += 1;
                assert_eq!(
                    envelope.retrieval_mode, "hybrid",
                    "committed query vectors must keep hybrid mode live"
                );
                for stage in &trace.stages {
                    for doc_id in &stage.doc_ids {
                        ids_checked += 1;
                        if !allowlist.contains(doc_id) {
                            violations.push(format!(
                                "stage {} leaked {doc_id} for {principal_id} ({query:?})",
                                stage.stage
                            ));
                        }
                        // The effective-version rule holds at every hybrid
                        // stage too: suppressed versions never reach scoring.
                        if !include_superseded && superseded.contains(doc_id) {
                            violations.push(format!(
                                "stage {} surfaced suppressed version {doc_id} for {principal_id}",
                                stage.stage
                            ));
                        }
                    }
                }
                for result in &envelope.results {
                    ids_checked += 1;
                    if !allowlist.contains(&result.document_id) {
                        violations.push(format!(
                            "envelope leaked {} for {principal_id}",
                            result.document_id
                        ));
                    }
                }
            }
        }
    }

    for violation in &violations {
        println!("{violation}");
    }
    println!(
        "R-8 summary: searches={searches} (124 principals x 12 queries x 2 modes) \
         stage_ids_checked={ids_checked} violations={}",
        violations.len()
    );
    assert_eq!(searches, 2976);
    assert!(searches >= 2000, "spec floor");
    assert_eq!(violations.len(), 0, "vector stage-leak is zero tolerance");
}

// ---------------------------------------------------------------------------
// R-9 JUDGE INPUT SEAL
// ---------------------------------------------------------------------------

#[test]
fn r9_judge_sees_only_allowed_snippets_and_foreign_ids_are_discarded() {
    let setup = setup();
    let scope = scope_for("p060");
    let allowlist = &setup.allowlists["p060"];

    // Capture exactly what the judge is shown.
    let judge = MockJudge::new(MockBehavior::Identity);
    let (_, trace) = hybrid_search(&scope, "payroll salary review", Some(&judge), false);
    assert_eq!(
        trace.judge_eligible,
        Some(true),
        "broad query must engage the judge"
    );

    let captured = judge.captured.lock().expect("captured");
    assert_eq!(captured.len(), 1, "exactly one judge call");
    let (given_query, candidates) = &captured[0];
    assert_eq!(
        given_query, "payroll salary review",
        "normalized query only"
    );
    let judge_input_stage = trace
        .stages
        .iter()
        .find(|s| s.stage == "judge_input")
        .expect("judge input instrumented");
    assert_eq!(
        candidates
            .iter()
            .map(|c| c.doc_id.clone())
            .collect::<Vec<_>>(),
        judge_input_stage.doc_ids,
        "the judge saw exactly the instrumented input"
    );
    for candidate in candidates.iter() {
        assert!(
            allowlist.contains(&candidate.doc_id),
            "judge candidate {} outside the allowlist",
            candidate.doc_id
        );
        let (title, body) = &setup.docs[&candidate.doc_id];
        assert_eq!(&candidate.title, title, "title is the doc's own");
        assert_eq!(
            candidate.snippet,
            snippet_of(body, 240),
            "snippet is the doc's own deterministic 240-char extract"
        );
    }
    let baseline_ids = judge_input_stage.doc_ids.clone();
    drop(captured);

    // A judge returning a foreign id: discarded, counted, envelope unaffected
    // beyond order.
    let mut fixed = vec!["d9999_foreign".to_string()];
    fixed.extend(baseline_ids.iter().rev().cloned());
    let foreign_judge = MockJudge::new(MockBehavior::Fixed(fixed));
    let (envelope, trace) =
        hybrid_search(&scope, "payroll salary review", Some(&foreign_judge), false);
    assert_eq!(
        trace.judge_faults, 1,
        "the foreign id is counted as a judge fault"
    );
    assert!(envelope.judge_applied, "valid ids still apply");
    let expected_top: Vec<&str> = baseline_ids
        .iter()
        .rev()
        .take(envelope.results.len())
        .map(String::as_str)
        .collect();
    let got: Vec<&str> = envelope
        .results
        .iter()
        .map(|r| r.document_id.as_str())
        .collect();
    assert_eq!(
        got, expected_top,
        "judge order applied minus the foreign id"
    );
    let bytes = String::from_utf8(envelope.to_canonical_bytes().expect("bytes")).expect("utf8");
    assert!(
        !bytes.contains("d9999_foreign"),
        "the foreign id never reaches the envelope"
    );
    for stage in &trace.stages {
        assert!(
            !stage.doc_ids.iter().any(|id| id == "d9999_foreign"),
            "the foreign id never reaches the trace (stage {})",
            stage.stage
        );
    }
}

// ---------------------------------------------------------------------------
// R-10 DEGRADATION DOCTRINE
// ---------------------------------------------------------------------------

#[test]
fn r10_embedder_failure_at_query_time_degrades_to_lexical_only() {
    let setup = setup();
    let scope = scope_for("p060");
    let failing = FailingEmbeddings;
    let options = hybrid_options(&failing, None, 10, false);
    let (degraded, trace) = setup
        .engine
        .search(&scope, "payroll salary review", &options)
        .expect("degradation is not an error");
    assert_eq!(degraded.retrieval_mode, "lexical_only");
    assert!(!degraded.judge_applied);
    assert_eq!(trace.hybrid_degraded, Some("query_embed_failed"));

    // Degraded hybrid is byte-identical to an honest lexical run.
    let (lexical, _) = setup
        .engine
        .search(
            &scope,
            "payroll salary review",
            &SearchOptions::lexical(10, false),
        )
        .expect("lexical search");
    assert_eq!(
        degraded.to_canonical_bytes().expect("bytes"),
        lexical.to_canonical_bytes().expect("bytes"),
        "degraded hybrid serves exactly the lexical envelope"
    );
}

#[test]
fn r10_embedder_failure_at_index_time_fails_the_build() {
    let setup = setup();
    let idx = scratch("r10_index_fail");
    build_index(&setup.subset_fixtures, &idx).expect("lexical build");
    let err = build_vectors(
        &setup.subset_fixtures,
        &idx,
        &FailingEmbeddings,
        Duration::from_millis(5000),
        240,
    )
    .expect_err("an index silently missing vectors is a lie");
    assert!(format!("{err:#}").contains("embedding"));
    // The manifest still has no vectors section: nothing pretends.
    let manifest = retrieval::index::load_manifest(&idx).expect("manifest intact");
    assert!(manifest.vectors.is_none());
}

#[test]
fn r10_judge_failure_leaves_the_fused_order_standing() {
    let scope = scope_for("p060");

    let (no_judge, _) = hybrid_search(&scope, "payroll salary review", None, false);
    let failing = MockJudge::new(MockBehavior::Fail);
    let (with_failing_judge, trace) =
        hybrid_search(&scope, "payroll salary review", Some(&failing), false);

    assert_eq!(trace.judge_eligible, Some(true), "the judge was due to run");
    assert!(trace.judge_failed, "the failure is instrumented");
    assert!(!with_failing_judge.judge_applied);
    assert_eq!(
        no_judge
            .results
            .iter()
            .map(|r| r.document_id.as_str())
            .collect::<Vec<_>>(),
        with_failing_judge
            .results
            .iter()
            .map(|r| r.document_id.as_str())
            .collect::<Vec<_>>(),
        "fused order stands"
    );
}

#[test]
fn r10_model_or_dim_mismatch_refuses_the_query() {
    let setup = setup();
    let scope = scope_for("p060");

    let options = hybrid_options(&WrongModelEmbeddings, None, 10, false);
    let err = setup
        .engine
        .search(&scope, "payroll salary review", &options)
        .expect_err("stale vectors must not rank");
    assert!(format!("{err:#}").contains("refusing"));

    let options = hybrid_options(&WrongDimEmbeddings, None, 10, false);
    let err = setup
        .engine
        .search(&scope, "payroll salary review", &options)
        .expect_err("dimension mismatch must refuse");
    assert!(format!("{err:#}").contains("dimension"));
}

// ---------------------------------------------------------------------------
// R-11 ELISION BOUNDARIES
// ---------------------------------------------------------------------------

#[test]
fn r11_elision_boundaries_on_both_sides_of_the_thresholds() {
    // Candidate-count boundary: 3 vs 4 (min_candidates = 4).
    let flat = |n: usize| -> Vec<f64> { vec![0.01; n] };
    assert!(!judge_eligible(&flat(3), 4, 1.3), "3 candidates: elided");
    assert!(judge_eligible(&flat(4), 4, 1.3), "4 equal candidates: runs");

    // Ratio boundary around 1.3 (strictly-below rule).
    let scores_at = |ratio: f64| -> Vec<f64> { vec![0.1 * ratio, 0.1, 0.05, 0.04] };
    assert!(
        !judge_eligible(&scores_at(1.3), 4, 1.3),
        "ratio exactly 1.3: elided (clear winner)"
    );
    assert!(
        !judge_eligible(&scores_at(1.31), 4, 1.3),
        "ratio above 1.3: elided"
    );
    assert!(
        judge_eligible(&scores_at(1.29), 4, 1.3),
        "ratio just below 1.3: runs"
    );
    assert!(
        judge_eligible(&scores_at(1.0), 4, 1.3),
        "tied top scores: runs"
    );
    // Degenerate guard: a zero second score can never satisfy the ratio.
    assert!(!judge_eligible(&[0.1, 0.0, 0.0, 0.0], 4, 1.3));

    // Integration: the wiring agrees with the pure rule on live searches —
    // eligibility recomputed from the instrumented source rankings must
    // match the engine's decision, and both sides of the boundary occur.
    let judge = MockJudge::new(MockBehavior::Identity);
    let mut seen_eligible = false;
    let mut seen_elided = false;
    for principal_id in ["p060", "p_void", "p001", "p017"] {
        let scope = scope_for(principal_id);
        for query in common::QUERY_TEXTS {
            let (envelope, trace) = hybrid_search(&scope, query, Some(&judge), false);
            let ranking_of = |name: &str| -> Vec<String> {
                trace
                    .stages
                    .iter()
                    .find(|s| s.stage == name)
                    .map(|s| s.doc_ids.clone())
                    .unwrap_or_default()
            };
            let mut scores: BTreeMap<String, f64> = BTreeMap::new();
            for ranking in [ranking_of("bm25_merged"), ranking_of("vector_merged")] {
                for (rank0, id) in ranking.iter().enumerate() {
                    *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (60.0 + (rank0 + 1) as f64);
                }
            }
            let fused = ranking_of("fused");
            let fused_scores: Vec<f64> = fused.iter().map(|id| scores[id]).collect();
            let expected = judge_eligible(&fused_scores, 4, 1.3);
            assert_eq!(
                trace.judge_eligible,
                Some(expected),
                "engine elision decision must equal the pure rule ({principal_id}, {query:?})"
            );
            assert_eq!(
                envelope.judge_applied, expected,
                "identity judge applies exactly when eligible"
            );
            seen_eligible |= expected;
            seen_elided |= !expected;
        }
    }
    assert!(seen_eligible, "battery exercises the eligible side");
    assert!(seen_elided, "battery exercises the elided side");
}

// ---------------------------------------------------------------------------
// R-12 DETERMINISM
// ---------------------------------------------------------------------------

#[test]
fn r12_hybrid_determinism_and_vectors_in_the_index_identity() {
    let setup = setup();
    let scope = scope_for("p060");

    // Identical hybrid query twice -> byte-identical envelopes.
    let (a, _) = hybrid_search(&scope, "payroll salary review", None, false);
    let (b, _) = hybrid_search(&scope, "payroll salary review", None, false);
    assert_eq!(
        a.to_canonical_bytes().expect("bytes"),
        b.to_canonical_bytes().expect("bytes")
    );

    // Judged variant is deterministic too (identity judge).
    let judge = MockJudge::new(MockBehavior::Identity);
    let (c, _) = hybrid_search(&scope, "payroll salary review", Some(&judge), false);
    let (d, _) = hybrid_search(&scope, "payroll salary review", Some(&judge), false);
    assert_eq!(
        c.to_canonical_bytes().expect("bytes"),
        d.to_canonical_bytes().expect("bytes")
    );

    // Rebuild from the same fixtures + embeddings -> identical index_version
    // and byte-identical envelopes from the rebuilt engine.
    let rebuilt = scratch("r12_rebuilt_idx");
    build_index(&setup.subset_fixtures, &rebuilt).expect("rebuild");
    let manifest = build_vectors(
        &setup.subset_fixtures,
        &rebuilt,
        &setup.embeddings,
        Duration::from_millis(5000),
        240,
    )
    .expect("rebuild vectors");
    assert_eq!(
        manifest.index_version, setup.engine.manifest.index_version,
        "same corpus + same vectors -> same index_version"
    );
    let vectors = manifest.vectors.as_ref().expect("vectors manifest");
    assert_eq!(vectors.files.len(), 5, "five hashed vector files");
    let rebuilt_engine = Engine::open(&rebuilt).expect("open rebuilt");
    let options = hybrid_options(&setup.embeddings, None, 10, false);
    let (e, _) = rebuilt_engine
        .search(&scope, "payroll salary review", &options)
        .expect("search rebuilt");
    assert_eq!(
        a.to_canonical_bytes().expect("bytes"),
        e.to_canonical_bytes().expect("bytes")
    );

    // The vector files are part of the index identity: a lexical-only build
    // hashes differently, and a tampered vector file refuses to load.
    let lexical_only = scratch("r12_lexical_idx");
    let lexical_manifest = build_index(&setup.subset_fixtures, &lexical_only).expect("lexical");
    assert_ne!(lexical_manifest.index_version, manifest.index_version);

    let tampered_path = rebuilt.join("vectors_internal.json");
    let mut bytes = fs::read(&tampered_path).expect("read vector file");
    let flip = bytes
        .iter()
        .position(|b| *b == b'7')
        .expect("a digit to flip");
    bytes[flip] = b'8';
    fs::write(&tampered_path, bytes).expect("tamper vector file");
    match Engine::open(&rebuilt) {
        Ok(_) => panic!("tampered vectors must refuse to load"),
        Err(err) => assert!(format!("{err:#}").contains("does not match the manifest hash")),
    }
}

// ---------------------------------------------------------------------------
// Usage sidecar: rows are numbers + model ids, never content
// ---------------------------------------------------------------------------

#[test]
fn usage_sidecar_rows_are_content_free_and_ordinal() {
    let scope = scope_for("p060");
    let judge = MockJudge::new(MockBehavior::Identity);
    let (_, trace) = hybrid_search(&scope, "payroll salary review", Some(&judge), false);

    // One row per call: the query embed and the judge.
    assert_eq!(trace.usage_events.len(), 2);
    assert!(trace.usage_events.iter().all(|e| e.cost_usd.is_none()));
    assert!(trace.usage_events.iter().all(|e| e.estimated));
    assert_eq!(trace.usage_events[0].model, common::FIXTURE_MODEL_ID);
    assert_eq!(trace.usage_events[1].model, "mock-judge");

    let sidecar = scratch("usage_sidecar").join("usage.jsonl");
    append_usage_sidecar(&sidecar, &trace.usage_events).expect("append");
    append_usage_sidecar(&sidecar, &trace.usage_events).expect("append again");
    let text = fs::read_to_string(&sidecar).expect("read sidecar");
    let rows: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).expect("row parses"))
        .collect();
    assert_eq!(rows.len(), 4);
    for (i, row) in rows.iter().enumerate() {
        let keys: Vec<&str> = row
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "cost_usd",
                "estimated",
                "input_tokens",
                "model",
                "output_tokens",
                "ts"
            ],
            "rows carry numbers and a model id, nothing else"
        );
        assert_eq!(
            row["ts"].as_u64(),
            Some(i as u64),
            "ts ordinals continue across appends"
        );
        assert_eq!(row["cost_usd"], Value::Null);
    }
    // No snippet, query, or document id ever lands in the sidecar.
    assert!(!text.contains("payroll"));
    assert!(!text.contains("d0"));
}
