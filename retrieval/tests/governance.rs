//! Retrieval governance harness R-1..R-7.
//!
//! This harness is the product: it judges any retriever behind the
//! `RankSource` seam without modification. It is the ONLY code in this crate
//! allowed to read `/fixtures/ground_truth.jsonl` (not needed here) and
//! `/fixtures/traps.json`. M1 artifacts are compiled fresh into a temp dir
//! via the frozen compiler crate (dev-dependency).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use retrieval::envelope::Envelope;
use retrieval::index::{build_index, sha256_hex, tokenize, Class};
use retrieval::search::{Engine, PrincipalScope, SearchOptions, Trace};
use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Shared, build-once setup
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("retrieval crate sits in the repo root")
        .join("fixtures")
}

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

struct Setup {
    artifacts_dir: PathBuf,
    engine: Engine,
    /// principal id -> full compiled allowlist (including superseded docs).
    allowlists: BTreeMap<String, BTreeSet<String>>,
    /// document id -> (title, body, sensitivity class).
    docs: BTreeMap<String, (String, String, Class)>,
    principal_ids: Vec<String>,
}

fn setup() -> &'static Setup {
    static SETUP: OnceLock<Setup> = OnceLock::new();
    SETUP.get_or_init(|| {
        let fixtures = fixtures_dir();

        // Compile M1 artifacts once, via the frozen compiler crate.
        let artifacts_dir = scratch("governance_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures).expect("snapshot fixtures");
        let world = scope_compiler::load_world(&fixtures).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&world, &snap, None).expect("compile M1");
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

        // Build the five partitions once.
        let idx_dir = scratch("governance_idx");
        build_index(&fixtures, &idx_dir).expect("build index");
        let engine = Engine::open(&idx_dir).expect("open engine");

        let docs = load_docs(&fixtures);

        Setup {
            artifacts_dir,
            engine,
            allowlists,
            docs,
            principal_ids,
        }
    })
}

#[derive(Debug, Deserialize)]
struct TestDocumentsFile {
    documents: Vec<TestDocRecord>,
}

#[derive(Debug, Deserialize)]
struct TestDocRecord {
    id: String,
    title: String,
    body: String,
    sensitivity: Class,
}

fn load_docs(fixtures: &Path) -> BTreeMap<String, (String, String, Class)> {
    let bytes = fs::read(fixtures.join("documents.json")).expect("read documents.json");
    let parsed: TestDocumentsFile = serde_json::from_slice(&bytes).expect("parse documents.json");
    parsed
        .documents
        .into_iter()
        .map(|d| (d.id, (d.title, d.body, d.sensitivity)))
        .collect()
}

/// Mirror of /fixtures/traps.json — readable by tests ONLY.
#[derive(Debug, Deserialize)]
struct Traps {
    effective_version: Vec<EffectiveVersionTrap>,
    mosaic: Vec<Value>,
    confused_deputy: Vec<ConfusedDeputyTrap>,
    manager_overreach: Vec<ManagerOverreachTrap>,
    cross_site: Vec<CrossSiteTrap>,
}

#[derive(Debug, Deserialize)]
struct EffectiveVersionTrap {
    current_id: String,
    superseded_id: String,
    #[allow(dead_code)]
    parameter_class: String,
}

#[derive(Debug, Deserialize)]
struct ConfusedDeputyTrap {
    agent_id: String,
    #[allow(dead_code)]
    owner_id: String,
    resource_id: String,
}

#[derive(Debug, Deserialize)]
struct ManagerOverreachTrap {
    manager_id: String,
    subject_id: String,
    resource_id: String,
}

#[derive(Debug, Deserialize)]
struct CrossSiteTrap {
    principal_id: String,
    resource_id: String,
    #[allow(dead_code)]
    required_site: String,
    #[allow(dead_code)]
    principal_site: String,
}

fn load_traps() -> Traps {
    let bytes = fs::read(fixtures_dir().join("traps.json")).expect("read traps.json");
    serde_json::from_slice(&bytes).expect("parse traps.json")
}

/// Deterministic LCG so the harness needs no rand dependency.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Lcg {
        Lcg(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() as usize) % items.len()]
    }
}

fn scope_for(id: &str) -> PrincipalScope {
    PrincipalScope::load(&setup().artifacts_dir, id).expect("load principal scope")
}

fn search(
    scope: &PrincipalScope,
    query: &str,
    k: usize,
    include_superseded: bool,
) -> (Envelope, Trace) {
    setup()
        .engine
        .search(scope, query, &SearchOptions::lexical(k, include_superseded))
        .expect("governed search")
}

fn result_ids(envelope: &Envelope) -> Vec<&str> {
    envelope
        .results
        .iter()
        .map(|r| r.document_id.as_str())
        .collect()
}

fn title_query(doc_id: &str) -> String {
    let (title, _, _) = &setup().docs[doc_id];
    title.clone()
}

/// Corpus vocabulary, sorted, for deterministic query generation.
fn vocabulary() -> &'static Vec<String> {
    static VOCAB: OnceLock<Vec<String>> = OnceLock::new();
    VOCAB.get_or_init(|| {
        let mut tokens: BTreeSet<String> = BTreeSet::new();
        for (title, body, _) in setup().docs.values() {
            tokens.extend(tokenize(title));
            tokens.extend(tokenize(body));
        }
        tokens.into_iter().filter(|t| t.len() >= 3).collect()
    })
}

// ---------------------------------------------------------------------------
// R-1 STAGE-LEAK PROPERTY
// ---------------------------------------------------------------------------

#[test]
fn r1_no_stage_ever_observes_an_out_of_scope_document() {
    let setup = setup();
    let vocab = vocabulary();
    let mut rng = Lcg::new(0x5EED_0001);

    // 30 sampled principals (deterministic), 200 generated queries.
    let mut principals: BTreeSet<&str> = BTreeSet::new();
    while principals.len() < 30 {
        principals.insert(rng.pick(&setup.principal_ids).as_str());
    }
    let queries: Vec<String> = (0..200)
        .map(|_| {
            let n = 1 + (rng.next() as usize) % 4;
            (0..n)
                .map(|_| rng.pick(vocab).clone())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();

    let mut searches = 0usize;
    let mut ids_checked = 0usize;
    let mut violations: Vec<String> = Vec::new();

    for &principal_id in &principals {
        let scope = scope_for(principal_id);
        let allowlist = &setup.allowlists[principal_id];
        for (qi, query) in queries.iter().enumerate() {
            let include_superseded = qi % 2 == 1;
            let (envelope, trace) = search(&scope, query, 10, include_superseded);
            searches += 1;
            for stage in &trace.stages {
                for doc_id in &stage.doc_ids {
                    ids_checked += 1;
                    if !allowlist.contains(doc_id) {
                        violations.push(format!(
                            "stage {} leaked {doc_id} for {principal_id} (query {qi})",
                            stage.stage
                        ));
                    }
                }
            }
            for r in &envelope.results {
                ids_checked += 1;
                if !allowlist.contains(&r.document_id) {
                    violations.push(format!(
                        "envelope leaked {} for {principal_id} (query {qi})",
                        r.document_id
                    ));
                }
            }
        }
    }

    for v in &violations {
        println!("{v}");
    }
    println!(
        "R-1 summary: searches={searches} (30 principals x 200 queries) \
         stage_ids_checked={ids_checked} violations={}",
        violations.len()
    );
    assert_eq!(searches, 6000);
    assert_eq!(violations.len(), 0, "stage-leak property is zero tolerance");
}

// ---------------------------------------------------------------------------
// R-2 TRAP BATTERY
// ---------------------------------------------------------------------------

#[test]
fn r2_trap_battery() {
    let setup = setup();
    let traps = load_traps();
    assert_eq!(traps.confused_deputy.len(), 15);
    assert_eq!(traps.manager_overreach.len(), 8);
    assert_eq!(traps.cross_site.len(), 6);
    assert_eq!(traps.mosaic.len(), 10);

    // Restricted/board terms return nothing restricted for unauthorized
    // principals, while a board member retrieves the very document.
    let restricted_ids: BTreeSet<&str> = setup
        .docs
        .iter()
        .filter(|(_, (_, _, class))| *class == Class::Restricted)
        .map(|(id, _)| id.as_str())
        .collect();
    let board_doc = *restricted_ids.iter().next().expect("restricted docs exist");
    let board_query = title_query(board_doc);
    let restricted_owned: BTreeSet<String> = restricted_ids.iter().map(|s| s.to_string()).collect();
    let unauthorized: Vec<&String> = setup
        .principal_ids
        .iter()
        .filter(|p| setup.allowlists[*p].is_disjoint(&restricted_owned))
        .take(5)
        .collect();
    assert!(unauthorized.len() >= 5, "need unauthorized principals");
    for principal_id in unauthorized {
        let scope = scope_for(principal_id);
        let (envelope, _) = search(&scope, &board_query, 50, false);
        for id in result_ids(&envelope) {
            assert!(
                !restricted_ids.contains(id),
                "restricted doc {id} surfaced for unauthorized {principal_id}"
            );
        }
    }
    let board_member = setup
        .principal_ids
        .iter()
        .find(|p| setup.allowlists[*p].contains(board_doc))
        .expect("someone is on the board");
    let scope = scope_for(board_member);
    let (envelope, _) = search(&scope, &board_query, 50, false);
    assert!(
        result_ids(&envelope).contains(&board_doc),
        "board member {board_member} should retrieve {board_doc} by its own title"
    );

    // Manager overreach: the HR record never surfaces for the manager, and
    // does for its subject.
    for trap in &traps.manager_overreach {
        let query = title_query(&trap.resource_id);
        let manager_scope = scope_for(&trap.manager_id);
        let (envelope, _) = search(&manager_scope, &query, 50, false);
        assert!(
            !result_ids(&envelope).contains(&trap.resource_id.as_str()),
            "manager {} must not surface {}",
            trap.manager_id,
            trap.resource_id
        );
        let subject_scope = scope_for(&trap.subject_id);
        let (envelope, _) = search(&subject_scope, &query, 50, false);
        assert!(
            result_ids(&envelope).contains(&trap.resource_id.as_str()),
            "subject {} should surface their own HR record {}",
            trap.subject_id,
            trap.resource_id
        );
    }

    // Cross-site: the document never surfaces for the wrong-site principal,
    // and does for somebody whose compiled allowlist holds it.
    for trap in &traps.cross_site {
        let query = title_query(&trap.resource_id);
        let scope = scope_for(&trap.principal_id);
        let (envelope, _) = search(&scope, &query, 50, false);
        assert!(
            !result_ids(&envelope).contains(&trap.resource_id.as_str()),
            "cross-site doc {} surfaced for {}",
            trap.resource_id,
            trap.principal_id
        );
        let allowed_principal = setup
            .principal_ids
            .iter()
            .find(|p| setup.allowlists[*p].contains(&trap.resource_id))
            .expect("someone on the right site reads it");
        let (envelope, _) = search(&scope_for(allowed_principal), &query, 50, false);
        assert!(
            result_ids(&envelope).contains(&trap.resource_id.as_str()),
            "{allowed_principal} should retrieve {} by its title",
            trap.resource_id
        );
    }

    // Confused deputy: agent queries return only intersection-scoped results
    // and never the trap resource.
    for trap in &traps.confused_deputy {
        let query = title_query(&trap.resource_id);
        let scope = scope_for(&trap.agent_id);
        let allowlist = &setup.allowlists[&trap.agent_id];
        let (envelope, trace) = search(&scope, &query, 50, false);
        assert!(
            !result_ids(&envelope).contains(&trap.resource_id.as_str()),
            "confused deputy: {} surfaced {}",
            trap.agent_id,
            trap.resource_id
        );
        for stage in &trace.stages {
            for doc_id in &stage.doc_ids {
                assert!(
                    allowlist.contains(doc_id),
                    "agent {} observed {doc_id} outside its intersection scope",
                    trap.agent_id
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// R-3 EFFECTIVE-VERSION
// ---------------------------------------------------------------------------

#[test]
fn r3_superseded_documents_never_served_by_default() {
    let setup = setup();
    let traps = load_traps();
    assert_eq!(traps.effective_version.len(), 12);

    for trap in &traps.effective_version {
        let query = title_query(&trap.superseded_id);
        let reader = setup
            .principal_ids
            .iter()
            .find(|p| {
                setup.allowlists[*p].contains(&trap.superseded_id)
                    && setup.allowlists[*p].contains(&trap.current_id)
            })
            .expect("someone reads both versions");
        let scope = scope_for(reader);

        // Default: the superseded version appears nowhere — not in results,
        // not at any instrumented stage, not in the envelope bytes. The
        // successor substitutes, ranked by its own match.
        let (envelope, trace) = search(&scope, &query, 50, false);
        assert!(
            !result_ids(&envelope).contains(&trap.superseded_id.as_str()),
            "superseded {} served by default",
            trap.superseded_id
        );
        for stage in &trace.stages {
            assert!(
                !stage.doc_ids.contains(&trap.superseded_id),
                "superseded {} observed at stage {} by default",
                trap.superseded_id,
                stage.stage
            );
        }
        let bytes = envelope.to_canonical_bytes().expect("serialize envelope");
        let text = String::from_utf8(bytes).expect("utf8");
        assert!(
            !text.contains(&trap.superseded_id),
            "envelope traces superseded id {}",
            trap.superseded_id
        );
        assert!(
            result_ids(&envelope).contains(&trap.current_id.as_str()),
            "successor {} should substitute for {}",
            trap.current_id,
            trap.superseded_id
        );

        // --include-superseded: the old version may appear, always marked.
        let (envelope, _) = search(&scope, &query, 50, true);
        let entry = envelope
            .results
            .iter()
            .find(|r| r.document_id == trap.superseded_id)
            .expect("strong title match should surface the superseded doc when included");
        assert_eq!(entry.superseded, Some(true));
        assert_eq!(
            entry.effective_successor.as_deref(),
            Some(trap.current_id.as_str())
        );
    }
}

#[test]
fn r3_successor_not_in_allowlist_serves_neither_and_leaves_no_trace() {
    // Synthetic corpus: the real fixtures never put a successor outside the
    // allowlist (each pair shares its ACL), so this serve-time case is
    // exercised with a purpose-built corpus + handcrafted M1-shaped
    // artifacts. Ids are unique strings that cannot collide with real docs.
    let fixtures = scratch("r3_synthetic_fixtures");
    let documents = json!({
        "documents": [
            { "id": "syn_old_v1", "title": "zebra quagga containment procedure",
              "body": "zebra quagga unique containment wording, version one",
              "sensitivity": "internal" },
            { "id": "syn_new_v2", "title": "zebra quagga containment procedure",
              "body": "zebra quagga unique containment wording, version two",
              "sensitivity": "internal" },
            { "id": "syn_filler", "title": "unrelated warehouse note",
              "body": "nothing about striped animals here",
              "sensitivity": "internal" }
        ]
    });
    let documents_bytes = serde_json::to_vec_pretty(&documents).expect("encode");
    fs::write(fixtures.join("documents.json"), &documents_bytes).expect("write");

    let idx = scratch("r3_synthetic_idx");
    build_index(&fixtures, &idx).expect("build synthetic index");
    let engine = Engine::open(&idx).expect("open synthetic index");

    // Allowlist: the superseded v1 (marked, successor named) and the filler.
    // The successor v2 is NOT in the allowlist.
    let artifact = json!({
        "compiled_at": "2026-01-05T00:00:00Z",
        "denied_count": 1,
        "entries": [
            { "document_id": "syn_old_v1", "reasons": ["REBAC:grp_test"],
              "superseded": true, "effective_successor": "syn_new_v2" },
            { "document_id": "syn_filler", "reasons": ["REBAC:grp_test"] }
        ],
        "principal_id": "tester",
        "snapshot_version": "synthetic-snapshot"
    });
    let artifacts_dir = scratch("r3_synthetic_artifacts");
    let artifact_bytes = serde_json::to_vec(&artifact).expect("encode");
    fs::write(artifacts_dir.join("tester.json"), &artifact_bytes).expect("write");
    let index_json = json!({
        "compiled_at": "2026-01-05T00:00:00Z",
        "fixture_hashes": { "documents.json": sha256_hex(&documents_bytes) },
        "principals": [
            { "artifact_file": "tester.json",
              "artifact_sha256": sha256_hex(&artifact_bytes),
              "denied_count": 1, "entry_count": 2, "principal_id": "tester" }
        ],
        "snapshot_version": "synthetic-snapshot",
        "totals": { "allow_entries": 2, "documents": 3, "principals": 1 }
    });
    fs::write(
        artifacts_dir.join("index.json"),
        serde_json::to_vec(&index_json).expect("encode"),
    )
    .expect("write");

    let scope = PrincipalScope::load(&artifacts_dir, "tester").expect("load synthetic scope");
    let (envelope, trace) = engine
        .search(
            &scope,
            "zebra quagga containment",
            &SearchOptions::lexical(10, false),
        )
        .expect("synthetic search");

    // Neither version is served, observed at any stage, or traceable in the
    // envelope bytes; the unrelated filler is unaffected.
    for forbidden in ["syn_old_v1", "syn_new_v2"] {
        assert!(!result_ids(&envelope).contains(&forbidden));
        for stage in &trace.stages {
            assert!(
                !stage.doc_ids.iter().any(|id| id == forbidden),
                "{forbidden} observed at stage {}",
                stage.stage
            );
        }
        let text = String::from_utf8(envelope.to_canonical_bytes().expect("bytes")).expect("utf8");
        assert!(
            !text.contains(forbidden),
            "envelope contains a trace of {forbidden}"
        );
    }
    assert!(result_ids(&envelope).is_empty() || result_ids(&envelope) == vec!["syn_filler"]);

    // R-13 (R-3x, M2b hardening): under --include-superseded the superseded
    // entry may itself be served, but its successor is NOT in the allowlist,
    // so the successor field is OMITTED — the id never leaves through the
    // metadata side door.
    let (envelope, _) = engine
        .search(
            &scope,
            "zebra quagga containment",
            &SearchOptions::lexical(10, true),
        )
        .expect("synthetic search, include-superseded");
    let entry = envelope
        .results
        .iter()
        .find(|r| r.document_id == "syn_old_v1")
        .expect("superseded doc surfaces when explicitly included");
    assert_eq!(entry.superseded, Some(true));
    assert_eq!(
        entry.effective_successor, None,
        "out-of-allowlist successor must be redacted"
    );
    let text = String::from_utf8(envelope.to_canonical_bytes().expect("bytes")).expect("utf8");
    assert!(
        !text.contains("syn_new_v2"),
        "envelope leaks the out-of-allowlist successor id"
    );
    assert!(
        !text.contains("effective_successor"),
        "redaction means the field is absent, not null"
    );
}

// ---------------------------------------------------------------------------
// R-4 NO-DARK-COUNTS
// ---------------------------------------------------------------------------

/// Every key the envelope type can ever serialize. Adding a field to the
/// envelope without updating this whitelist fails the harness — and the
/// whitelist will only ever accept fields that cannot carry dark counts.
const ENVELOPE_KEY_WHITELIST: [&str; 16] = [
    "band",
    "document_id",
    "effective_successor",
    "groups",
    "index_version",
    "judge_applied",
    "principal_id",
    "query_hash",
    "reasons_ref",
    "results",
    "retrieval_mode",
    "scope_statement",
    "score_rank",
    "sites",
    "snapshot_version",
    "superseded",
];

/// M2b extends the M2a list: no count of embedded/judged/elided documents,
/// no fault tallies, no usage/token numbers may ever appear in an envelope.
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
fn r4_envelope_cannot_represent_dark_counts() {
    let setup = setup();

    // Schema level: a forbidden field cannot even deserialize into the type.
    let smuggled = r#"{
        "index_version": "x", "principal_id": "x", "query_hash": "x",
        "results": [], "suppressed_count": 3,
        "scope_statement": { "band": null, "groups": [], "sites": [] },
        "snapshot_version": "x"
    }"#;
    assert!(
        serde_json::from_str::<Envelope>(smuggled).is_err(),
        "the envelope type must reject suppressed-count fields structurally"
    );

    // Serialization level, on a query engineered to suppress many candidates:
    // a principal with a narrow allowlist querying the corpus' most common
    // vocabulary. Hundreds of matching docs exist; the envelope must say
    // nothing about any of them.
    let narrowest = setup
        .principal_ids
        .iter()
        .min_by_key(|p| setup.allowlists[*p].len())
        .expect("principals exist");
    let scope = scope_for(narrowest);
    let (envelope, _) = search(
        &scope,
        "procedure record review customer quality warehouse site stock",
        10,
        false,
    );

    let value = serde_json::to_value(&envelope).expect("to value");
    let mut keys = BTreeSet::new();
    collect_keys(&value, &mut keys);
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

// ---------------------------------------------------------------------------
// R-5 PARTITION DISCIPLINE
// ---------------------------------------------------------------------------

#[test]
fn r5_zero_allowed_docs_in_a_class_never_opens_its_partition() {
    let setup = setup();
    // Tokens that hit documents in every sensitivity class, maximizing the
    // pressure to open partitions.
    let query = "procedure record customer payroll board review quality minutes";

    for principal_id in &setup.principal_ids {
        let scope = scope_for(principal_id);
        let allowlist = &setup.allowlists[principal_id];
        let (_, trace) = search(&scope, query, 50, true);
        let opened: BTreeSet<&str> = trace.opened_partitions.iter().map(String::as_str).collect();

        for class in Class::ALL {
            let allowed_in_class = setup.engine.manifest.partitions[class.as_str()]
                .doc_ids
                .iter()
                .any(|id| allowlist.contains(id));
            if !allowed_in_class {
                assert!(
                    !opened.contains(class.as_str()),
                    "{principal_id} opened {} with zero allowed docs in it",
                    class.as_str()
                );
            }
        }
        // The public partition opens for every (real) principal — M1 grants
        // everyone the public corpus, so discipline and short-circuit agree.
        assert!(
            opened.contains("public"),
            "{principal_id} should always open the public partition"
        );
    }
}

// ---------------------------------------------------------------------------
// R-6 DETERMINISM
// ---------------------------------------------------------------------------

#[test]
fn r6_identical_queries_and_rebuilt_indexes_are_identical() {
    let setup = setup();
    let scope = scope_for("p060");

    let (a, _) = search(&scope, "payroll salary review", 10, false);
    let (b, _) = search(&scope, "payroll salary review", 10, false);
    assert_eq!(
        a.to_canonical_bytes().expect("bytes"),
        b.to_canonical_bytes().expect("bytes"),
        "identical query twice must produce byte-identical envelopes"
    );

    // Rebuild the index from the same fixtures: identical index_version and
    // byte-identical envelopes from the rebuilt engine.
    let rebuilt_dir = scratch("r6_rebuilt_idx");
    let rebuilt_manifest = build_index(&fixtures_dir(), &rebuilt_dir).expect("rebuild");
    assert_eq!(
        rebuilt_manifest.index_version, setup.engine.manifest.index_version,
        "same fixtures must hash to the same index_version"
    );
    let rebuilt_engine = Engine::open(&rebuilt_dir).expect("open rebuilt");
    let (c, _) = rebuilt_engine
        .search(
            &scope,
            "payroll salary review",
            &SearchOptions::lexical(10, false),
        )
        .expect("search rebuilt");
    assert_eq!(
        a.to_canonical_bytes().expect("bytes"),
        c.to_canonical_bytes().expect("bytes"),
        "a rebuilt index must serve byte-identical envelopes"
    );
}

// ---------------------------------------------------------------------------
// R-7 PERFORMANCE
// ---------------------------------------------------------------------------

#[test]
fn r7_build_and_query_bounds() {
    let setup = setup();

    let build_dir = scratch("r7_build_idx");
    let started = Instant::now();
    build_index(&fixtures_dir(), &build_dir).expect("build");
    let build_elapsed = started.elapsed();

    // 60-query battery over six principals of varied breadth.
    let vocab = vocabulary();
    let mut rng = Lcg::new(0x5EED_0007);
    let principals: Vec<&String> = (0..6).map(|_| rng.pick(&setup.principal_ids)).collect();
    let scopes: Vec<PrincipalScope> = principals.iter().map(|p| scope_for(p)).collect();

    let mut durations: Vec<f64> = Vec::with_capacity(60);
    let mut judge_would_run = 0usize;
    for i in 0..60 {
        let n = 2 + (rng.next() as usize) % 3;
        let query = (0..n)
            .map(|_| rng.pick(vocab).clone())
            .collect::<Vec<_>>()
            .join(" ");
        let scope = &scopes[i % scopes.len()];
        let started = Instant::now();
        let (_, trace) = search(scope, &query, 10, false);
        durations.push(started.elapsed().as_secs_f64() * 1000.0);
        // M2b closeout stat: with a single rank source the RRF top1/top2
        // ratio is fixed at 62/61 (< 1.3), so judge eligibility over this
        // battery reduces to the >= 4 fused-candidates rule.
        let fused_len = trace
            .stages
            .iter()
            .find(|s| s.stage == "fused")
            .map(|s| s.doc_ids.len())
            .unwrap_or(0);
        if fused_len >= 4 {
            judge_would_run += 1;
        }
    }
    durations.sort_by(f64::total_cmp);
    let p50 = durations[durations.len() / 2];
    let max = durations[durations.len() - 1];

    println!(
        "R-7: index build {:.3}s (bound 5.0s); query battery n=60 p50={:.3}ms max={:.3}ms \
         (bound p50 < 25ms) [{} profile]; judge elision over battery: {} elided / {} would run",
        build_elapsed.as_secs_f64(),
        p50,
        max,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        60 - judge_would_run,
        judge_would_run
    );

    // Bounds are specified for the release build; debug runs report only.
    if !cfg!(debug_assertions) {
        assert!(
            build_elapsed.as_secs_f64() < 5.0,
            "index build bound exceeded: {:.3}s",
            build_elapsed.as_secs_f64()
        );
        assert!(p50 < 25.0, "query p50 bound exceeded: {p50:.3}ms");
    }
}
