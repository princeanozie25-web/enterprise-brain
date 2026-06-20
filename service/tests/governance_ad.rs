//! Lens-diff governance harness AD-1..AD-5 (AP-4). FULLY OFFLINE.
//!
//! THE LAW under test: SET EXACTNESS — (left_only ∪ shared) == left's
//! compiled artifact and (right_only ∪ shared) == right's, exactly, for
//! every sampled pair; every difference attributed to the rule verbatim
//! from the owning artifact; ONE lens_diff audit row per act, written
//! before render; refusals (self-diff 400, unknown 404) leave no row.

// AUTH-2: the diff-correctness helpers/imports are retained for AUTH-3, when
// admin view-as re-enables /lens/diff and the full AD suite returns.
#![allow(dead_code, unused_imports)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::index::build_index;
use serde_json::Value;
use service::agent::proposals::{AuditEvent, ProposalStore};
use service::diff::build_diff_columns;
use service::lens::LensEntry;
use service::{app, AppState};
use tower::ServiceExt;

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

#[derive(Clone)]
struct EntryFacts {
    reasons: Vec<String>,
    superseded: Option<bool>,
    effective_successor: Option<String>,
}

struct World {
    fixtures_dir: PathBuf,
    artifacts_dir: PathBuf,
    idx_dir: PathBuf,
    /// principal -> doc -> the artifact entry facts (the ground truth).
    facts: BTreeMap<String, BTreeMap<String, EntryFacts>>,
    allowlists: BTreeMap<String, BTreeSet<String>>,
    principal_ids: Vec<String>,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("ad_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let mut facts = BTreeMap::new();
        let mut allowlists = BTreeMap::new();
        for artifact in &set.artifacts {
            let mut per_doc = BTreeMap::new();
            let mut allow = BTreeSet::new();
            for entry in &artifact.entries {
                allow.insert(entry.document_id.clone());
                per_doc.insert(
                    entry.document_id.clone(),
                    EntryFacts {
                        reasons: entry.reasons.clone(),
                        superseded: entry.superseded,
                        effective_successor: entry.effective_successor.clone(),
                    },
                );
            }
            facts.insert(artifact.principal_id.clone(), per_doc);
            allowlists.insert(artifact.principal_id.clone(), allow);
        }
        let principal_ids: Vec<String> = allowlists.keys().cloned().collect();

        let idx_dir = scratch("ad_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
            facts,
            allowlists,
            principal_ids,
        }
    })
}

fn diff_state(store_dir: Option<&Path>) -> AppState {
    let world = world();
    let state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state");
    match store_dir {
        Some(dir) => state.with_proposals(Arc::new(
            ProposalStore::open(dir).expect("open audit store"),
        )),
        None => state,
    }
}

async fn get_raw(router: &axum::Router, actor: &str, uri: &str) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
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

async fn get_diff(
    router: &axum::Router,
    actor: &str,
    left: &str,
    right: &str,
) -> (StatusCode, Vec<u8>) {
    get_raw(
        router,
        actor,
        &format!("/lens/diff?left={left}&right={right}"),
    )
    .await
}

/// 10 deterministic pairs: 4 human-human, 2 agent-agent, 4 mixed.
fn sample_pairs(world: &World) -> Vec<(String, String)> {
    let agents: Vec<String> = world
        .principal_ids
        .iter()
        .filter(|p| p.starts_with("agent_"))
        .cloned()
        .collect();
    assert_eq!(agents.len(), 4, "all four agents in play");
    let humans_pool: Vec<String> = world
        .principal_ids
        .iter()
        .filter(|p| !p.starts_with("agent_"))
        .cloned()
        .collect();
    let mut rng = common::Lcg::new(0xAD_2026);
    let mut humans: Vec<String> = Vec::new();
    while humans.len() < 8 {
        let pick = rng.pick(&humans_pool).clone();
        if !humans.contains(&pick) {
            humans.push(pick);
        }
    }
    vec![
        (humans[0].clone(), humans[1].clone()),
        (humans[2].clone(), humans[3].clone()),
        (humans[4].clone(), humans[5].clone()),
        (humans[6].clone(), humans[7].clone()),
        (agents[0].clone(), agents[1].clone()),
        (agents[2].clone(), agents[3].clone()),
        (agents[0].clone(), humans[0].clone()),
        (agents[1].clone(), humans[2].clone()),
        (agents[2].clone(), humans[4].clone()),
        (agents[3].clone(), humans[6].clone()),
    ]
}

/// Independent reimplementation of the priority law for the property tests.
fn class_of(reason: &str) -> u8 {
    if reason == "SUBJECT:self" {
        0
    } else if reason.starts_with("REBAC:") {
        1
    } else if reason.starts_with("ABAC:") {
        2
    } else if reason.starts_with("AGENT:") {
        3
    } else {
        4 // PUBLIC
    }
}

fn normalize(reason: &str) -> String {
    if reason == "PUBLIC:sensitivity" {
        "PUBLIC:all".to_string()
    } else {
        reason.to_string()
    }
}

fn primary_of(reasons: &[String]) -> String {
    let mut all: Vec<String> = reasons.iter().map(|r| normalize(r)).collect();
    all.sort_by_key(|r| (class_of(r), r.clone()));
    all.dedup();
    all[0].clone()
}

fn column_doc_ids(sections: &Value) -> Vec<String> {
    sections
        .as_array()
        .expect("sections")
        .iter()
        .flat_map(|s| {
            s["docs"]
                .as_array()
                .expect("docs")
                .iter()
                .map(|d| d["document_id"].as_str().expect("id").to_string())
        })
        .collect()
}

fn shared_doc_ids(shared: &Value) -> Vec<String> {
    shared
        .as_array()
        .expect("shared")
        .iter()
        .map(|r| r["doc"]["document_id"].as_str().expect("id").to_string())
        .collect()
}

/// The AT-suite doc-id sweep, reused: `d` + exactly four digits, bounded by
/// non-alphanumerics (hex hashes cannot false-match).
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
// AUTH-2 (FC-A2): a diff compares two principals' scopes — cross-principal
// viewing, the AUTH-3 boundary (admin view-as). In THIS slice /lens/diff is
// DENIED with the one 404; nothing is assembled, nothing is audited. The rich
// diff-correctness suite (set partition, single audit row, reasons-verbatim,
// schema-exactness) returns when AUTH-3 re-enables admin view-as.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ad_cross_principal_diff_is_denied_and_unaudited() {
    let store_dir = scratch("ad_denied_store");
    let router = app(Arc::new(diff_state(Some(&store_dir))));

    // Representative cross-principal pairs (human-human, mixed-with-caller,
    // agent): every one is denied with the one 404.
    for (actor, left, right) in [
        ("p060", "p033", "p085"),
        ("p016", "p016", "p087"),
        ("p061", "agent_finance_analyst", "p060"),
    ] {
        let (status, _) = get_diff(&router, actor, left, right).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "cross-principal diff {actor}: {left} vs {right} -> 404 (denied)"
        );
    }

    // A denied act is not an act: no lens_diff audit row is written.
    let audit = fs::read_to_string(store_dir.join("audit.jsonl")).unwrap_or_default();
    assert!(
        audit.lines().all(|line| !line.contains("lens_diff")),
        "a denied diff writes no lens_diff audit row"
    );
    println!("AD: /lens/diff denied (404) for cross-principal pairs + unaudited (AUTH-3 boundary)");
}

#[tokio::test]
async fn ad_diff_request_shape_is_still_validated() {
    let router = app(Arc::new(diff_state(None)));
    // The denial does not mask the public request contract: an unknown query
    // parameter is still a 400 (the shape check runs before the cross-principal
    // gate).
    let (status, _) = get_raw(&router, "p060", "/lens/diff?left=p060&bogus=x").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "unknown diff param -> 400");
}
