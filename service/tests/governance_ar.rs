//! AR-1 governance harness — the humanization layer over the FROZEN skeleton.
//! FULLY OFFLINE. Reads the committed M1 artifacts + retrieval index directly
//! (they already match the frozen fixtures, so no scratch compile/index — and
//! no tantivy build race). The skeleton is M1's; this suite proves the human
//! layer decorates it without changing one authorization fact.
//!
//! AR-1  determinism: generate twice is byte-identical, and equals the
//!       committed fixtures/people.json.
//! AR-2  structural untouched: company/documents/traps match the M1 pins to
//!       the byte, and the humanization lives in a SEPARATE file.
//! AR-3  no leak: the new human fields (roster cards + masthead) carry no
//!       document id — a card is org-structural, never evidence — and the
//!       masthead's projects are inside the subject's own Lane derivation.
//! AR-4  the override: the generated display name replaces the frozen name on
//!       every surface (/lens, /lens/diff).
//! AR-5  additive + fail-soft: with no layer loaded, names fall back to the
//!       frozen company.json and no human fields appear (every prior suite is
//!       this path, so it stays green).

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::index::sha256_hex;
use serde_json::Value;
use service::agent::proposals::ProposalStore;
use service::{app, humanize, AppState};
use tower::ServiceExt;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("service crate sits in the repo root")
        .to_path_buf()
}

fn artifacts_dir() -> PathBuf {
    repo_root().join("compiler").join("artifacts")
}

fn idx_dir() -> PathBuf {
    repo_root().join("retrieval").join("idx")
}

/// Builds service state over the committed (frozen-matching) artifacts. The
/// humanization layer is loaded + proved iff `with_people` is true.
fn ar_state(with_people: bool, store_dir: Option<&Path>) -> AppState {
    let fixtures = common::repo_fixtures_dir();
    let mut state = AppState::build(&fixtures, &artifacts_dir(), &idx_dir())
        .expect("build service state over committed artifacts");
    if with_people {
        state = state.with_people().expect("load + verify people.json");
    }
    if let Some(dir) = store_dir {
        state = state.with_proposals(Arc::new(ProposalStore::open(dir).expect("audit store")));
    }
    state
}

async fn get(router: &axum::Router, uri: &str, actor: &str) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("x-demo-principal", actor)
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

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    for attempt in 0u64..50 {
        let _ = fs::remove_dir_all(&dir);
        if fs::create_dir_all(&dir).is_ok()
            && fs::read_dir(&dir)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false)
        {
            return dir;
        }
        std::thread::sleep(std::time::Duration::from_millis(20 * (attempt.min(5) + 1)));
    }
    panic!("scratch dir {name} could not be reset");
}

/// Every scalar string in a JSON value, recursively — for leak scanning.
fn collect_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(s) => out.push(s.as_str()),
        Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
        _ => {}
    }
}

/// True iff `s` has the shape of a corpus document id: `d` + exactly four
/// digits (the artifact id format). Capability ids ("cap30"), principal ids
/// ("p060"), and avatar refs ("faces/p001.jpg") never match.
fn is_doc_id(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 5 && bytes[0] == b'd' && bytes[1..].iter().all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// AR-1 DETERMINISM
// ---------------------------------------------------------------------------

#[test]
fn ar1_generation_is_deterministic_and_matches_the_committed_file() {
    let fixtures = common::repo_fixtures_dir();
    let state = ar_state(false, None);
    let inputs = humanize::read_person_inputs(&fixtures, &state.company_sha256).expect("inputs");

    let first = humanize::generate(&inputs, &state.lane_seeds);
    let second = humanize::generate(&inputs, &state.lane_seeds);
    assert_eq!(first, second, "two generations are identical");

    let committed = fs::read(fixtures.join("people.json")).expect("committed people.json");
    let regenerated = humanize::to_pretty_bytes(&first).expect("serialize");
    assert_eq!(
        regenerated, committed,
        "the committed people.json equals a fresh generation to the byte"
    );

    // Every human principal, no duplicate full names.
    assert_eq!(first.people.len(), 120, "all 120 human principals humanized");
    let names: BTreeSet<&str> = first.people.iter().map(|p| p.display_name.as_str()).collect();
    assert_eq!(names.len(), 120, "no duplicate full names");
    println!(
        "AR-1: 120 principals, {} unique names, byte-identical generation",
        names.len()
    );
}

// ---------------------------------------------------------------------------
// AR-2 STRUCTURAL UNTOUCHED — the skeleton is byte-identical to the M1 pin
// ---------------------------------------------------------------------------

#[test]
fn ar2_frozen_skeleton_is_byte_identical_and_humanization_is_separate() {
    let fixtures = common::repo_fixtures_dir();
    let index: Value = serde_json::from_slice(
        &fs::read(artifacts_dir().join("index.json")).expect("read M1 index"),
    )
    .expect("parse index");
    let pins = index["fixture_hashes"].as_object().expect("fixture_hashes");

    // The three M1-pinned fixtures must match their pins exactly — AR-1 did
    // not touch one byte of the governed inputs.
    for file in ["company.json", "documents.json", "traps.json"] {
        let bytes = fs::read(fixtures.join(file)).unwrap_or_else(|_| panic!("read {file}"));
        let pinned = pins[file].as_str().unwrap_or_else(|| panic!("pin for {file}"));
        assert_eq!(
            sha256_hex(&bytes),
            pinned,
            "{file} is byte-identical to the hash the M1 compile pinned"
        );
    }

    // The humanization lives in its OWN file; the governed skeleton carries
    // none of the display fields.
    assert!(fixtures.join("people.json").exists(), "people.json exists");
    let company = String::from_utf8(fs::read(fixtures.join("company.json")).expect("company"))
        .expect("utf8");
    for field in ["display_name", "avatar_ref", "\"bio\"", "personality_tag", "\"projects\""] {
        assert!(
            !company.contains(field),
            "company.json must not carry the humanization field {field}"
        );
    }
    println!("AR-2: company/documents/traps match M1 pins; humanization is a separate file");
}

// ---------------------------------------------------------------------------
// AR-3 NO LEAK — human fields are org-structural, never evidence
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ar3_human_fields_carry_no_document_id_and_projects_stay_in_scope() {
    let store = scratch("ar3_store");
    let state = ar_state(true, Some(&store));
    let lane_seeds = state.lane_seeds.clone();
    let router = app(Arc::new(state));

    // /people roster: 120 cards, each EXACTLY the org-structural fields, and
    // not a single document id anywhere in the body.
    let (status, bytes) = get(&router, "/people", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let people: Value = serde_json::from_slice(&bytes).expect("people parses");
    let roster = people["people"].as_array().expect("roster");
    assert_eq!(roster.len(), 120, "roster lists every humanized principal");
    let allowed_card_keys: BTreeSet<&str> =
        ["avatar_ref", "department_label", "display_name", "id", "title"].into();
    for card in roster {
        let keys: BTreeSet<&str> = card.as_object().expect("card").keys().map(String::as_str).collect();
        assert_eq!(keys, allowed_card_keys, "a roster card is org-structural only");
    }
    let mut roster_strings = Vec::new();
    collect_strings(&people, &mut roster_strings);
    assert!(
        !roster_strings.iter().any(|s| is_doc_id(s)),
        "the roster carries no document id"
    );

    // /lens self: the actor card + subject masthead carry no document id, and
    // the masthead's projects are a prefix of the subject's own Lane seeds
    // (so every project's evidence is inside the subject's holdings).
    let (status, bytes) = get(&router, "/lens/p060", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let lens: Value = serde_json::from_slice(&bytes).expect("lens parses");
    for field in ["actor", "subject_human"] {
        let mut strings = Vec::new();
        collect_strings(&lens[field], &mut strings);
        assert!(
            !strings.iter().any(|s| is_doc_id(s)),
            "lens.{field} carries no document id"
        );
    }
    let seed_caps: Vec<&str> = lane_seeds["p060"].iter().map(|s| s.capability.id.as_str()).collect();
    let project_caps: Vec<&str> = lens["subject_human"]["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .map(|p| p["capability_id"].as_str().expect("cap id"))
        .collect();
    assert!(!project_caps.is_empty(), "p060 has projects to check");
    assert!(project_caps.len() <= humanize::MAX_PROJECTS, "projects capped");
    assert_eq!(
        project_caps,
        seed_caps[..project_caps.len()],
        "projects are the top of the subject's own Lane derivation (no invented access)"
    );

    // /atlas and /lane: the actor card carries no document id either.
    for uri in ["/atlas", "/lane"] {
        let (status, bytes) = get(&router, uri, "p060").await;
        assert_eq!(status, StatusCode::OK, "{uri} ok");
        let body: Value = serde_json::from_slice(&bytes).expect("parses");
        let mut strings = Vec::new();
        collect_strings(&body["actor"], &mut strings);
        assert!(
            !strings.iter().any(|s| is_doc_id(s)),
            "{uri} actor card carries no document id"
        );
        assert_eq!(body["actor"]["id"], "p060", "{uri} actor card is the viewer");
    }
    println!("AR-3: roster + mastheads carry zero document ids; projects stay in Lane scope");
}

// ---------------------------------------------------------------------------
// AR-4 THE OVERRIDE — the generated name replaces the frozen name everywhere
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ar4_display_name_overrides_the_frozen_name_on_every_surface() {
    let store = scratch("ar4_store");
    let state = ar_state(true, Some(&store));
    let people = state.people.clone().expect("layer loaded");
    let router = app(Arc::new(state));

    let name = |id: &str| people.get(id).expect("record").display_name.clone();

    // The frozen company.json name (what the surface showed before AR-1).
    let company: Value =
        serde_json::from_slice(&fs::read(common::repo_fixtures_dir().join("company.json")).unwrap())
            .unwrap();
    let frozen = |id: &str| {
        company["people"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap()["name"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_ne!(name("p060"), frozen("p060"), "the name was regenerated");

    // /lens masthead shows the generated name, not the frozen one.
    let (_, bytes) = get(&router, "/lens/p060", "p060").await;
    let lens: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(lens["subject"]["name"], name("p060").as_str());
    assert_eq!(lens["subject_human"]["display_name"], name("p060").as_str());
    assert_eq!(lens["actor"]["display_name"], name("p060").as_str());

    // /lens/diff passports show the generated names on both sides.
    let (status, bytes) = get(&router, "/lens/diff?left=p016&right=p087", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let diff: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(diff["left"]["name"], name("p016").as_str());
    assert_eq!(diff["right"]["name"], name("p087").as_str());
    assert_eq!(diff["left_human"]["display_name"], name("p016").as_str());
    assert_eq!(diff["right_human"]["display_name"], name("p087").as_str());
    println!(
        "AR-4: p060 '{}' -> '{}' on lens + diff (was '{}')",
        frozen("p060"),
        name("p060"),
        frozen("p060")
    );
}

// ---------------------------------------------------------------------------
// AR-5 ADDITIVE + FAIL-SOFT — no layer => frozen names, no human fields
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ar5_without_the_layer_names_fall_back_and_no_human_fields_appear() {
    let state = ar_state(false, None); // no with_people
    assert!(state.people.is_none(), "no humanization layer loaded");
    let router = app(Arc::new(state));

    // /lens: the frozen company.json name stands; no actor/subject_human keys.
    let (status, bytes) = get(&router, "/lens/p060", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let lens: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        lens["subject"]["name"], "Kerensa Pellbrook",
        "without the layer, the frozen name stands (every prior suite's path)"
    );
    assert!(lens.get("actor").is_none(), "no actor card without the layer");
    assert!(
        lens.get("subject_human").is_none(),
        "no masthead without the layer"
    );

    // /people: an empty roster (no layer), still a clean 200.
    let (status, bytes) = get(&router, "/people", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let people: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(people["people"].as_array().unwrap().len(), 0);
    println!("AR-5: layer-absent path is byte-compatible with the pre-AR-1 surfaces");
}
