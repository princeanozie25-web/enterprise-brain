//! Atlas governance harness AT-1..AT-5 (AP-3). FULLY OFFLINE.
//!
//! THE RULING under test: STRUCTURE IS INTERNAL-GRADE; EVIDENCE IS
//! GOVERNED. The BRM hierarchy renders identically for every principal with
//! standing; the docs under each capability are exactly (mapped ∩ the
//! viewer's compiled allowlist); an actor with no standing receives the
//! empty atlas; brm.json is hash-verified at startup and on every request.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::index::build_index;
use serde_json::Value;
use service::{app, AppState};
use tower::ServiceExt;

fn scratch(name: &str) -> PathBuf {
    // Unique per invocation: Windows scanners (Search indexer / Defender) can
    // hold a just-deleted path in delete-pending state, so re-creating the
    // SAME path races them into Os error 5 "Access is denied". A fresh suffix
    // never re-opens a dying path; prior runs' dirs are swept best-effort (a
    // locked leftover is skipped now and reaped on a later run).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // The base lives in the SYSTEM temp dir, not target/tmp: the repo sits
    // under Documents\, which Windows Search indexes by default — its crawler
    // opens freshly written index segments mid-build and the write fails with
    // os error 5. AppData\Local\Temp is outside the default index scope.
    let base = std::env::temp_dir().join("enterprise-brain-test-scratch");
    std::fs::create_dir_all(&base).expect("scratch base");
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
    /// principal -> compiled allowlist (the M1 ground truth for scoping).
    allowlists: BTreeMap<String, BTreeSet<String>>,
    principal_ids: Vec<String>,
    /// The raw BRM fixture — the EXPECTED structure, parsed independently.
    brm: Value,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("at_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let mut allowlists = BTreeMap::new();
        for artifact in &set.artifacts {
            allowlists.insert(
                artifact.principal_id.clone(),
                artifact
                    .entries
                    .iter()
                    .map(|e| e.document_id.clone())
                    .collect::<BTreeSet<_>>(),
            );
        }
        let principal_ids: Vec<String> = allowlists.keys().cloned().collect();

        let idx_dir = scratch("at_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        let brm: Value = serde_json::from_slice(
            &fs::read(fixtures_dir.join("brm.json")).expect("read brm fixture"),
        )
        .expect("brm fixture parses");

        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
            allowlists,
            principal_ids,
            brm,
        }
    })
}

fn atlas_state() -> AppState {
    let world = world();
    AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
}

async fn get_path(router: &axum::Router, actor: &str, path: &str) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(path)
                .header("authorization", common::bearer(router, actor).await)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, bytes.to_vec())
}

async fn get_atlas(router: &axum::Router, actor: &str) -> (StatusCode, Vec<u8>) {
    get_path(router, actor, "/atlas").await
}

/// The broadest compiled allowlist anchors the "broad-scope principal".
fn broad_principal() -> &'static str {
    world()
        .allowlists
        .iter()
        .max_by_key(|(_, allowlist)| allowlist.len())
        .expect("at least one principal")
        .0
}

/// Fixture rows of one BRM level, keyed by id (BTreeMap = the sorted order
/// the service must emit).
fn fixture_by_id(level: &str) -> BTreeMap<&'static str, &'static Value> {
    world().brm[level]
        .as_array()
        .expect(level)
        .iter()
        .map(|row| (row["id"].as_str().expect("id"), row))
        .collect()
}

fn sorted_id_list<'a>(row: &'a Value, key: &str) -> Vec<&'a str> {
    let mut ids: Vec<&str> = row[key]
        .as_array()
        .expect(key)
        .iter()
        .map(|v| v.as_str().expect("id"))
        .collect();
    ids.sort_unstable();
    ids
}

fn ids_of(rows: &[Value]) -> Vec<&str> {
    rows.iter()
        .map(|row| row["id"].as_str().expect("id"))
        .collect()
}

fn for_each_capability(body: &Value, mut f: impl FnMut(&Value)) {
    for strategy in body["strategies"].as_array().expect("strategies") {
        for initiative in strategy["initiatives"].as_array().expect("initiatives") {
            for workflow in initiative["workflows"].as_array().expect("workflows") {
                for capability in workflow["capabilities"].as_array().expect("capabilities") {
                    f(capability);
                }
            }
        }
    }
}

/// Hand-rolled doc-id sweep (no regex dependency): a doc id is `d` + exactly
/// four digits, bounded by non-alphanumerics — the boundary rule keeps hex
/// hashes (snapshot_version) from false-matching.
fn extract_doc_ids(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut found = BTreeSet::new();
    for i in 0..bytes.len() {
        if bytes[i] != b'd' {
            continue;
        }
        if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
            continue;
        }
        let digits = &bytes[i + 1..];
        if digits.len() < 4 || !digits[..4].iter().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if digits.len() > 4 && digits[4].is_ascii_digit() {
            continue;
        }
        found.insert(text[i..i + 5].to_string());
    }
    found
}

// ---------------------------------------------------------------------------
// AT-1 STRUCTURE FIDELITY
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at1_hierarchy_equals_brm_exactly_for_a_broad_principal() {
    let router = app(Arc::new(atlas_state()));
    let broad = broad_principal();
    let (status, bytes) = get_atlas(&router, broad).await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&bytes).expect("atlas parses");
    assert_eq!(body["actor_id"], Value::String(broad.to_string()));

    let fx_strategies = fixture_by_id("strategies");
    let fx_initiatives = fixture_by_id("initiatives");
    let fx_workflows = fixture_by_id("workflows");
    let fx_capabilities = fixture_by_id("capabilities");

    let strategies = body["strategies"].as_array().expect("strategies");
    assert_eq!(
        ids_of(strategies),
        fx_strategies.keys().copied().collect::<Vec<_>>(),
        "strategy ids, sorted"
    );
    let (mut n_initiatives, mut n_workflows, mut n_capabilities) = (0usize, 0usize, 0usize);
    for strategy in strategies {
        let fx = fx_strategies[strategy["id"].as_str().expect("id")];
        assert_eq!(strategy["name"], fx["name"]);
        let initiatives = strategy["initiatives"].as_array().expect("initiatives");
        assert_eq!(ids_of(initiatives), sorted_id_list(fx, "initiative_ids"));
        for initiative in initiatives {
            n_initiatives += 1;
            let fx = fx_initiatives[initiative["id"].as_str().expect("id")];
            assert_eq!(initiative["name"], fx["name"]);
            let workflows = initiative["workflows"].as_array().expect("workflows");
            assert_eq!(ids_of(workflows), sorted_id_list(fx, "workflow_ids"));
            for workflow in workflows {
                n_workflows += 1;
                let fx = fx_workflows[workflow["id"].as_str().expect("id")];
                assert_eq!(workflow["name"], fx["name"]);
                let capabilities = workflow["capabilities"].as_array().expect("capabilities");
                assert_eq!(ids_of(capabilities), sorted_id_list(fx, "capability_ids"));
                for capability in capabilities {
                    n_capabilities += 1;
                    let fx = fx_capabilities[capability["id"].as_str().expect("id")];
                    assert_eq!(capability["name"], fx["name"]);
                }
            }
        }
    }
    assert_eq!(
        (strategies.len(), n_initiatives, n_workflows, n_capabilities),
        (6, 18, 40, 90),
        "the whole 6/18/40/90 hierarchy renders"
    );
    println!("AT-1 summary: actor={broad} hierarchy=6/18/40/90 ids+names+order all equal brm.json");
}

// ---------------------------------------------------------------------------
// AT-2 EVIDENCE SCOPING (property over 15 principals incl. agents)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at2_docs_equal_mapped_intersect_allowlist_and_nothing_else_leaks() {
    let world = world();
    let router = app(Arc::new(atlas_state()));

    let mut sampled: Vec<String> = world
        .principal_ids
        .iter()
        .filter(|p| p.starts_with("agent_"))
        .cloned()
        .collect();
    assert_eq!(sampled.len(), 4, "all four agents sampled");
    let mut rng = common::Lcg::new(0xA7_2026);
    while sampled.len() < 15 {
        let pick = rng.pick(&world.principal_ids).clone();
        if !sampled.contains(&pick) {
            sampled.push(pick);
        }
    }

    let fx_capabilities = fixture_by_id("capabilities");
    let mut docs_checked = 0usize;
    let mut empty_capabilities_seen = 0usize;
    for actor in &sampled {
        let allowlist = &world.allowlists[actor];
        let (status, bytes) = get_atlas(&router, actor).await;
        assert_eq!(status, StatusCode::OK);
        let body: Value = serde_json::from_slice(&bytes).expect("atlas parses");

        let mut capabilities_seen = 0usize;
        for_each_capability(&body, |capability| {
            capabilities_seen += 1;
            let cap_id = capability["id"].as_str().expect("id");
            let mut expected: Vec<&str> = fx_capabilities[cap_id]["document_ids"]
                .as_array()
                .expect("document_ids")
                .iter()
                .map(|v| v.as_str().expect("doc id"))
                .filter(|d| allowlist.contains(*d))
                .collect();
            expected.sort_unstable();
            let got: Vec<&str> = capability["docs"]
                .as_array()
                .expect("docs")
                .iter()
                .map(|d| d["document_id"].as_str().expect("document_id"))
                .collect();
            assert_eq!(got, expected, "docs for {cap_id} of {actor}");
            docs_checked += got.len();
            if got.is_empty() {
                empty_capabilities_seen += 1;
            }
        });
        assert_eq!(capabilities_seen, 90, "full structure for {actor}");

        // Zero out-of-scope ids ANYWHERE in the body — including successor
        // fields (R-13) and anything else that serializes.
        let text = String::from_utf8(bytes).expect("utf8");
        for doc_id in extract_doc_ids(&text) {
            assert!(
                allowlist.contains(&doc_id),
                "{doc_id} serialized for {actor} but is not in their allowlist"
            );
        }
    }
    assert!(
        empty_capabilities_seen > 0,
        "the empty-evidence state was exercised"
    );
    println!(
        "AT-2 summary: principals=15 (incl 4 agents) capabilities=90 each \
         visible_docs_checked={docs_checked} empty_capabilities_seen={empty_capabilities_seen} \
         out_of_scope_ids=0"
    );
}

// ---------------------------------------------------------------------------
// AT-3 FORBIDDEN SHAPE
// ---------------------------------------------------------------------------

fn assert_no_forbidden_keys(value: &Value, path: &str) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let lower = key.to_lowercase();
                for forbidden in ["count", "total", "hidden", "coverage"] {
                    assert!(
                        !lower.contains(forbidden),
                        "forbidden key {key:?} (matches {forbidden:?}) at {path}"
                    );
                }
                assert_no_forbidden_keys(child, &format!("{path}.{key}"));
            }
        }
        Value::Array(rows) => {
            for (index, row) in rows.iter().enumerate() {
                assert_no_forbidden_keys(row, &format!("{path}[{index}]"));
            }
        }
        _ => {}
    }
}

/// Empties every docs array and nulls the actor identity: what remains is
/// STRUCTURE, which must be identical for every standing viewer.
fn strip_evidence(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(docs) = map.get_mut("docs") {
                *docs = Value::Array(Vec::new());
            }
            if let Some(actor) = map.get_mut("actor_id") {
                *actor = Value::Null;
            }
            for (_, child) in map.iter_mut() {
                strip_evidence(child);
            }
        }
        Value::Array(rows) => {
            for row in rows.iter_mut() {
                strip_evidence(row);
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn at3_no_count_shaped_fields_and_scarcity_only_shortens_docs_arrays() {
    let router = app(Arc::new(atlas_state()));
    let broad = broad_principal();
    // The engineered almost-nothing actor exists in the fixtures: p_void
    // holds public docs only, and NO capability maps a public document.
    let (status_broad, bytes_broad) = get_atlas(&router, broad).await;
    let (status_void, bytes_void) = get_atlas(&router, "p_void").await;
    assert_eq!(status_broad, StatusCode::OK);
    assert_eq!(status_void, StatusCode::OK);

    let mut body_broad: Value = serde_json::from_slice(&bytes_broad).expect("parses");
    let mut body_void: Value = serde_json::from_slice(&bytes_void).expect("parses");
    assert_no_forbidden_keys(&body_broad, "$");
    assert_no_forbidden_keys(&body_void, "$");

    // The broad body carries evidence; the scarce body carries almost none.
    let count_docs = |body: &Value| {
        let mut docs = 0usize;
        for_each_capability(body, |capability| {
            docs += capability["docs"].as_array().expect("docs").len();
        });
        docs
    };
    let broad_docs = count_docs(&body_broad);
    let void_docs = count_docs(&body_void);
    assert!(broad_docs > 0, "broad actor sees evidence");
    assert!(
        void_docs < broad_docs,
        "the scarce actor sees strictly less ({void_docs} < {broad_docs})"
    );

    // Strip the docs arrays and the actor id: the remainders must be
    // BYTE-IDENTICAL. Scarcity has exactly one expression — shorter docs
    // arrays — and no second channel.
    strip_evidence(&mut body_broad);
    strip_evidence(&mut body_void);
    assert_eq!(
        body_broad, body_void,
        "structure must not vary with the viewer's scope"
    );
    println!(
        "AT-3 summary: forbidden_keys=0 broad_docs={broad_docs} scarce_docs={void_docs} \
         stripped_bodies_identical=true"
    );
}

// ---------------------------------------------------------------------------
// AT-4 EMPTY-ALLOWLIST RULE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at4_ghost_gets_the_empty_atlas_and_the_minimal_actor_gets_structure() {
    let world = world();
    let router = app(Arc::new(atlas_state()));

    // The ghost: an id M1 never compiled (M2a precedent — a principal M1
    // refused gets NOTHING, structure included).
    let (status, bytes) = get_atlas(&router, "p_ghost_at4").await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&bytes).expect("parses");
    let snapshot = body["snapshot_version"].as_str().expect("snapshot");
    assert_eq!(
        body,
        serde_json::json!({
            "actor_id": "p_ghost_at4",
            "demo_identity_mode": true,
            "snapshot_version": snapshot,
            "strategies": [],
        }),
        "the empty atlas carries identity, snapshot, and NOTHING else"
    );

    // The real minimal actor: p_void's compiled allowlist is public-only
    // (non-empty), so the FULL structure renders; and because no capability
    // maps a public document, every evidence area is the honest empty.
    let void_allowlist = &world.allowlists["p_void"];
    assert!(!void_allowlist.is_empty(), "p_void has standing");
    let (status, bytes) = get_atlas(&router, "p_void").await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&bytes).expect("parses");
    let fx_capabilities = fixture_by_id("capabilities");
    let mut capabilities_seen = 0usize;
    for_each_capability(&body, |capability| {
        capabilities_seen += 1;
        let cap_id = capability["id"].as_str().expect("id");
        let visible_mapped: Vec<&str> = fx_capabilities[cap_id]["document_ids"]
            .as_array()
            .expect("document_ids")
            .iter()
            .map(|v| v.as_str().expect("doc id"))
            .filter(|d| void_allowlist.contains(*d))
            .collect();
        let docs = capability["docs"].as_array().expect("docs");
        assert_eq!(
            docs.is_empty(),
            visible_mapped.is_empty(),
            "docs arrays render exactly where the viewer's docs realize {cap_id}"
        );
    });
    assert_eq!(
        capabilities_seen, 90,
        "structure visibility is pegged to standing, not scope"
    );
    println!(
        "AT-4 summary: ghost=empty-atlas p_void=full-structure capabilities={capabilities_seen} \
         (allowlist={} public-only; no capability maps a public doc, so every evidence \
         area is the honest empty)",
        void_allowlist.len()
    );
}

// ---------------------------------------------------------------------------
// AT-5 SNAPSHOT DISCIPLINE
// ---------------------------------------------------------------------------

fn copy_fixtures(dest: &Path, include_brm: bool) {
    let world = world();
    for name in ["company.json", "documents.json"] {
        fs::copy(world.fixtures_dir.join(name), dest.join(name)).expect("copy fixture");
    }
    if include_brm {
        fs::copy(world.fixtures_dir.join("brm.json"), dest.join("brm.json")).expect("copy brm");
    }
}

const INTERNAL_ERROR_BODY: &[u8] = b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n";

#[tokio::test]
async fn at5_brm_byte_flip_refuses_at_startup_and_at_refresh() {
    let world = world();

    // REFRESH: a flip AFTER startup — even one confined to a display name,
    // which no structural check could see — refuses on the next request,
    // because the request re-reads the file and re-verifies the pinned hash.
    let fixtures_flip = scratch("at5_fixtures_refresh");
    copy_fixtures(&fixtures_flip, true);
    let state = AppState::build(&fixtures_flip, &world.artifacts_dir, &world.idx_dir)
        .expect("valid copy builds");
    let router = app(Arc::new(state));
    let (status, _) = get_atlas(&router, "p060").await;
    assert_eq!(status, StatusCode::OK, "sanity: the untouched copy serves");

    let brm_path = fixtures_flip.join("brm.json");
    let flipped = fs::read_to_string(&brm_path).expect("read brm").replacen(
        "Cold Storage Monitoring 01",
        "Cold Storage Xonitoring 01",
        1,
    );
    fs::write(&brm_path, flipped).expect("write flipped brm");
    let (status, bytes) = get_atlas(&router, "p060").await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "refresh refuses the flip"
    );
    assert_eq!(bytes, INTERNAL_ERROR_BODY, "the refusal explains nothing");

    // STARTUP: a flip that corrupts the verified content — a document id the
    // corpus does not carry — refuses to build at all.
    let fixtures_corrupt = scratch("at5_fixtures_startup");
    copy_fixtures(&fixtures_corrupt, true);
    let brm_path = fixtures_corrupt.join("brm.json");
    let corrupted =
        fs::read_to_string(&brm_path)
            .expect("read brm")
            .replacen("\"d0001\"", "\"d9999\"", 1);
    fs::write(&brm_path, corrupted).expect("write corrupted brm");
    let err = match AppState::build(&fixtures_corrupt, &world.artifacts_dir, &world.idx_dir) {
        Ok(_) => panic!("startup must refuse the corrupt brm"),
        Err(err) => format!("{err:#}"),
    };
    assert!(
        err.contains("brm.json"),
        "the refusal names the file: {err}"
    );

    // And an unknown field refuses the strict parse the same way.
    let fixtures_extra = scratch("at5_fixtures_extra");
    copy_fixtures(&fixtures_extra, true);
    let brm_path = fixtures_extra.join("brm.json");
    let extended = fs::read_to_string(&brm_path).expect("read brm").replacen(
        "\"capabilities\":",
        "\"capability_total\": 90, \"capabilities\":",
        1,
    );
    fs::write(&brm_path, extended).expect("write extended brm");
    assert!(
        AppState::build(&fixtures_extra, &world.artifacts_dir, &world.idx_dir).is_err(),
        "unknown fields refuse — the count-shaped field can never even parse"
    );
    println!("AT-5a summary: refresh_flip=500 startup_corruption=refused unknown_field=refused");
}

#[tokio::test]
async fn at5_stale_artifacts_refuse_and_a_world_without_brm_serves_the_one_404() {
    let world = world();

    // STALE ARTIFACT: the actor's artifact drifts after startup -> 500 on
    // THEIR request; everyone else's standing still verifies and serves.
    let artifacts_copy = scratch("at5_artifacts");
    for entry in fs::read_dir(&world.artifacts_dir).expect("read artifacts dir") {
        let entry = entry.expect("dir entry");
        fs::copy(entry.path(), artifacts_copy.join(entry.file_name())).expect("copy artifact");
    }
    let state = AppState::build(&world.fixtures_dir, &artifacts_copy, &world.idx_dir)
        .expect("valid artifact copy builds");
    let router = app(Arc::new(state));
    let (status, _) = get_atlas(&router, "p060").await;
    assert_eq!(status, StatusCode::OK, "sanity: the untouched copy serves");

    let artifact_path = artifacts_copy.join("p060.json");
    let mut bytes = fs::read(&artifact_path).expect("read artifact");
    bytes.push(b' ');
    fs::write(&artifact_path, bytes).expect("write stale artifact");
    let (status, bytes) = get_atlas(&router, "p060").await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "stale artifact refuses"
    );
    assert_eq!(bytes, INTERNAL_ERROR_BODY);
    let (status, _) = get_atlas(&router, "p061").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "an untouched principal still serves"
    );

    // ABSENT BRM: a world without brm.json builds (the atlas is an absent
    // capability, the M4 precedent) and /atlas answers THE one 404 —
    // byte-identical to every other not-found in the service.
    let fixtures_bare = scratch("at5_fixtures_bare");
    copy_fixtures(&fixtures_bare, false);
    let state = AppState::build(&fixtures_bare, &world.artifacts_dir, &world.idx_dir)
        .expect("a world without a BRM still builds");
    let router = app(Arc::new(state));
    let atlas_404 = get_atlas(&router, "p060").await;
    let lens_404 = get_path(&router, "p060", "/lens/p_ghost_404").await;
    assert_eq!(atlas_404.0, StatusCode::NOT_FOUND);
    assert_eq!(
        atlas_404, lens_404,
        "absent atlas and unknown subject share THE one 404"
    );
    println!("AT-5b summary: stale_artifact=500(self-only) absent_brm=identical-404");
}
