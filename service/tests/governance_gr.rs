//! AR-2 Org Graph governance harness GR-1..GR-4. FULLY OFFLINE: reads the
//! committed M1 artifacts + retrieval index directly (matching the frozen
//! corpus), loads the humanization layer, drives the axum router in-memory.
//!
//! GR-1 STRUCTURE: the graph's people set == the /people roster the actor may
//!      see (internal-grade consistency); departments + reporting edges match
//!      company.json.
//! GR-2 ANCHORS: ring="anchor" iff the AR-1 seniority is Leadership —
//!      deterministic, property-tested.
//! GR-3 NO HOLDINGS LEAK: the /graph payload carries no document id, no
//!      per-person count, no sensitivity — structure only.
//! GR-4 SELF + 404: is_self is set for exactly the actor; an unknown actor
//!      gets the one 404.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use service::{app, AppState};
use tower::ServiceExt;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("service crate sits in the repo root")
        .to_path_buf()
}

fn gr_state() -> AppState {
    AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("load + verify people.json")
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

fn collect_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(s) => out.push(s.as_str()),
        Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
        _ => {}
    }
}

fn collect_keys<'a>(value: &'a Value, out: &mut BTreeSet<&'a str>) {
    match value {
        Value::Array(items) => items.iter().for_each(|v| collect_keys(v, out)),
        Value::Object(map) => {
            for (k, v) in map {
                out.insert(k.as_str());
                collect_keys(v, out);
            }
        }
        _ => {}
    }
}

fn is_doc_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 5 && b[0] == b'd' && b[1..].iter().all(|c| c.is_ascii_digit())
}

fn company() -> Value {
    serde_json::from_slice(&fs::read(common::repo_fixtures_dir().join("company.json")).unwrap())
        .unwrap()
}

fn people_seniority() -> BTreeMap<String, String> {
    let v: Value =
        serde_json::from_slice(&fs::read(common::repo_fixtures_dir().join("people.json")).unwrap())
            .unwrap();
    v["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap().to_string(),
                p["seniority"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// GR-1 STRUCTURE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gr1_people_set_matches_roster_and_edges_match_company() {
    let router = app(Arc::new(gr_state()));

    let (gs, gb) = get(&router, "/graph", "p060").await;
    assert_eq!(gs, StatusCode::OK);
    let graph: Value = serde_json::from_slice(&gb).expect("graph parses");
    let (ps, pb) = get(&router, "/people", "p060").await;
    assert_eq!(ps, StatusCode::OK);
    let people: Value = serde_json::from_slice(&pb).expect("people parses");

    // Internal-grade consistency: the graph's people set == the /people roster.
    let graph_ids: BTreeSet<&str> = graph["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    let roster_ids: BTreeSet<&str> = people["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(graph_ids, roster_ids, "graph people == /people roster");
    assert_eq!(graph_ids.len(), 120, "all 120 humans in the graph");

    // Departments match company.json (the 8 declared, in order).
    let company = company();
    let expected_depts: Vec<&str> = company["departments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_str().unwrap())
        .collect();
    let graph_depts: Vec<&str> = graph["departments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["label"].as_str().unwrap())
        .collect();
    assert_eq!(graph_depts, expected_depts, "department hubs match company.json");

    // Reporting edges match company.json manager_id, and every person with a
    // manager has exactly one reports_to edge.
    let manager_of: BTreeMap<&str, &str> = company["people"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| {
            p["manager_id"]
                .as_str()
                .map(|m| (p["id"].as_str().unwrap(), m))
        })
        .collect();
    let mut reports_edges: BTreeMap<&str, &str> = BTreeMap::new();
    for e in graph["edges"].as_array().unwrap() {
        if e["kind"] == "reports_to" {
            let from = e["from"].as_str().unwrap();
            assert!(reports_edges.insert(from, e["to"].as_str().unwrap()).is_none(), "one reports_to per person");
        }
    }
    assert_eq!(reports_edges, manager_of, "reports_to edges == company manager_id");
    println!(
        "GR-1: graph people == roster (120); {} departments; {} reporting edges match company.json",
        graph_depts.len(),
        reports_edges.len()
    );
}

// ---------------------------------------------------------------------------
// GR-2 ANCHORS
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gr2_anchors_are_exactly_the_leadership_tier() {
    let router = app(Arc::new(gr_state()));
    let (status, bytes) = get(&router, "/graph", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let graph: Value = serde_json::from_slice(&bytes).unwrap();
    let seniority = people_seniority();

    let mut anchors = 0usize;
    for p in graph["people"].as_array().unwrap() {
        let id = p["id"].as_str().unwrap();
        let ring = p["ring"].as_str().unwrap();
        let is_leadership = seniority[id] == "Leadership";
        assert_eq!(
            ring == "anchor",
            is_leadership,
            "{id}: ring={ring} but seniority={}",
            seniority[id]
        );
        if ring == "anchor" {
            anchors += 1;
        }
    }
    // ~12 sector leads (the org's Leadership), derived deterministically.
    assert!(
        (8..=16).contains(&anchors),
        "anchor count {anchors} is the ~12 leadership band"
    );
    println!("GR-2: {anchors} anchors == Leadership tier (deterministic, ~12)");
}

// ---------------------------------------------------------------------------
// GR-3 NO HOLDINGS LEAK
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gr3_graph_carries_no_holding_or_document_id() {
    let router = app(Arc::new(gr_state()));
    let (status, bytes) = get(&router, "/graph", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let graph: Value = serde_json::from_slice(&bytes).unwrap();

    let mut strings = Vec::new();
    collect_strings(&graph, &mut strings);
    assert!(
        !strings.iter().any(|s| is_doc_id(s)),
        "the graph carries no document id"
    );

    // The shape is structurally incapable of expressing holdings/counts.
    let mut keys = BTreeSet::new();
    collect_keys(&graph, &mut keys);
    for forbidden in ["sensitivity", "document_id", "documents", "holdings", "count", "docs"] {
        assert!(!keys.contains(forbidden), "no {forbidden:?} field in /graph");
    }
    println!("GR-3: /graph has zero document ids and no holding/count fields ({} keys)", keys.len());
}

// ---------------------------------------------------------------------------
// GR-4 SELF + 404
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gr4_is_self_marks_only_the_actor_and_unknown_is_404() {
    let router = app(Arc::new(gr_state()));

    for actor in ["p060", "p001", "p_void"] {
        let (status, bytes) = get(&router, "/graph", actor).await;
        assert_eq!(status, StatusCode::OK, "{actor} ok");
        let graph: Value = serde_json::from_slice(&bytes).unwrap();
        let selves: Vec<&str> = graph["people"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["is_self"] == true)
            .map(|p| p["id"].as_str().unwrap())
            .collect();
        assert_eq!(selves, vec![actor], "exactly the actor is is_self");
    }

    // Unknown principal: the one 404.
    let (status, _) = get(&router, "/graph", "p_ghost_404").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown actor -> 404");
    println!("GR-4: is_self marks only the actor; unknown actor -> the one 404");
}
