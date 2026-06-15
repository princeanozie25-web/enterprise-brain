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
// GR-5 NAMES: every node is a real, humanized person — no placeholder labels
// ---------------------------------------------------------------------------

fn people_display() -> BTreeMap<String, (String, String)> {
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
                (
                    p["display_name"].as_str().unwrap().to_string(),
                    p["title"].as_str().unwrap().to_string(),
                ),
            )
        })
        .collect()
}

#[tokio::test]
async fn gr5_every_person_carries_a_real_humanized_name_no_placeholder() {
    let router = app(Arc::new(gr_state()));
    let (status, bytes) = get(&router, "/graph", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let graph: Value = serde_json::from_slice(&bytes).unwrap();
    let expected = people_display();

    // The renderer must never have to invent a label: the payload carries a
    // real name + title for EVERY node. Guards against a regression to the
    // "anonymous team member" graph the rebuild replaced.
    let placeholders = [
        "", "member", "anchor", "team member", "teammate", "unknown", "person", "n/a", "tbd",
    ];

    let mut checked = 0usize;
    for p in graph["people"].as_array().unwrap() {
        let id = p["id"].as_str().unwrap();
        let name = p["display_name"].as_str().unwrap_or("");
        let title = p["title"].as_str().unwrap_or("");

        let trimmed = name.trim();
        assert!(!trimmed.is_empty(), "{id}: display_name is empty");
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        assert!(
            tokens.len() >= 2 && tokens.iter().all(|t| !t.is_empty()),
            "{id}: display_name {name:?} is not a real (multi-token) name"
        );
        assert!(
            !placeholders.contains(&trimmed.to_ascii_lowercase().as_str()),
            "{id}: display_name {name:?} is a placeholder"
        );
        assert_ne!(trimmed, id, "{id}: display_name must not be the principal id");
        assert!(!title.trim().is_empty(), "{id}: title is empty");

        // Baked at source: the graph's name + title equal the humanization
        // layer exactly (no fabrication in the endpoint).
        let (exp_name, exp_title) = expected.get(id).expect("graph id exists in people.json");
        assert_eq!(name, exp_name, "{id}: graph name == people.json display_name");
        assert_eq!(title, exp_title, "{id}: graph title == people.json title");
        checked += 1;
    }
    assert_eq!(checked, 120, "all 120 people carry a real name + title");
    println!(
        "GR-5: all {checked} people carry a real 2-token humanized name + title; zero placeholders"
    );
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

// ---------------------------------------------------------------------------
// GR-6 SOURCES: the 5 real systems ride the graph, and still nothing leaks
// ---------------------------------------------------------------------------

#[tokio::test]
async fn gr6_graph_carries_the_real_sources_and_still_no_leak() {
    let router = app(Arc::new(gr_state()));
    let (status, bytes) = get(&router, "/graph", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let graph: Value = serde_json::from_slice(&bytes).unwrap();

    // The five real systems of record (company.json sources[]), nothing else.
    let sources: BTreeSet<&str> = graph["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    let expected: BTreeSet<&str> = ["docstore", "wiki", "mail_lite", "hr_system", "quality_system"]
        .into_iter()
        .collect();
    assert_eq!(sources, expected, "graph sources == company.json sources");
    for s in graph["sources"].as_array().unwrap() {
        assert_eq!(s["kind"], "source", "a source declares its kind");
        assert!(!s["label"].as_str().unwrap().is_empty(), "a source is labelled");
    }

    // One system_of edge per source, to the org core — never to a person.
    let system_edges: Vec<&Value> = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "system_of")
        .collect();
    assert_eq!(system_edges.len(), 5, "one system_of edge per source");
    for e in system_edges {
        assert_eq!(e["to"], "org", "a source is a system of the org");
        assert!(expected.contains(e["from"].as_str().unwrap()), "edge from a real source");
    }

    // Adding sources introduced NO holding: still zero document ids, still no
    // forbidden key anywhere in the payload (GR-3's law, re-proven).
    let mut strings = Vec::new();
    collect_strings(&graph, &mut strings);
    assert!(!strings.iter().any(|s| is_doc_id(s)), "sources carry no document id");
    let mut keys = BTreeSet::new();
    collect_keys(&graph, &mut keys);
    for forbidden in ["sensitivity", "document_id", "documents", "holdings", "count", "docs"] {
        assert!(!keys.contains(forbidden), "no {forbidden:?} field after sources");
    }
    println!("GR-6: 5 real sources + 5 system_of edges; zero leak preserved");
}

// ---------------------------------------------------------------------------
// GR-7 NODE SUMMARY: the inspector is REAL, scope-respecting, metadata-only
// ---------------------------------------------------------------------------

fn index_entry_count(id: &str) -> usize {
    let v: Value = serde_json::from_slice(
        &fs::read(repo_root().join("compiler").join("artifacts").join("index.json")).unwrap(),
    )
    .unwrap();
    v["principals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["principal_id"] == id)
        .map(|p| p["entry_count"].as_u64().unwrap() as usize)
        .unwrap_or_else(|| panic!("{id} missing from index"))
}

fn assert_metadata_only(summary: &Value, who: &str) {
    let mut strings = Vec::new();
    collect_strings(summary, &mut strings);
    assert!(
        !strings.iter().any(|s| is_doc_id(s)),
        "{who}: node summary names no document id"
    );
    let mut keys = BTreeSet::new();
    collect_keys(summary, &mut keys);
    for forbidden in ["sensitivity", "document_id", "documents", "holdings", "count", "docs"] {
        assert!(!keys.contains(forbidden), "{who}: no {forbidden:?} field");
    }
}

#[tokio::test]
async fn gr7_node_summary_is_real_scoped_and_metadata_only() {
    let router = app(Arc::new(gr_state()));

    // ORG: the corpus cardinalities, every one matching the real fixtures.
    let (status, bytes) = get(&router, "/node/org/summary", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let org: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(org["kind"], "org");
    let stats = &org["stats"];
    assert_eq!(stats["people"], 120, "people");
    assert_eq!(stats["departments"], 8, "departments");
    assert_eq!(stats["document_total"], 600, "documents");
    assert_eq!(stats["workflows"], 40, "workflows");
    assert_eq!(stats["capabilities"], 90, "capabilities");
    assert_eq!(stats["agents"], 4, "agents");
    assert_eq!(stats["sources"], 5, "sources");
    assert_eq!(stats["groups"], 14, "groups");
    assert_eq!(stats["sites"], 2, "sites");
    assert_eq!(stats["permission_edges"], 16881, "compiled allow edges");
    assert_eq!(stats["principals"], 124, "principals");
    assert_eq!(stats["total_decisions"], 74400, "the conformance total");
    assert_metadata_only(&org, "org");

    // PERSON: scope + reason-grouped COUNTS that sum to the compiled allowlist.
    let (status, bytes) = get(&router, "/node/p060/summary", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let person: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(person["kind"], "human");
    assert_eq!(person["corpus_documents"], 600);
    let visible = person["visible_documents"].as_u64().unwrap() as usize;
    assert_eq!(visible, index_entry_count("p060"), "visible == compiled allowlist");
    let reason_sum: usize = person["access_by_reason"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["granted"].as_u64().unwrap() as usize)
        .sum();
    assert_eq!(reason_sum, visible, "every visible doc is grouped under exactly one reason");
    assert!(person["access_by_reason"].as_array().unwrap().iter().all(|g| {
        !g["sentence"].as_str().unwrap().is_empty()
    }), "each reason carries its human sentence");
    assert!(person["groups"].as_array().unwrap().len() > 0, "person carries scope groups");
    assert_metadata_only(&person, "p060");

    // AGENT: the M4 authority, stated from real fixtures.
    let (status, bytes) = get(&router, "/node/agent_finance_analyst/summary", "p061").await;
    assert_eq!(status, StatusCode::OK);
    let agent: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(agent["kind"], "agent");
    assert_eq!(agent["owner_user_id"], "p061");
    assert_eq!(
        agent["visible_documents"].as_u64().unwrap() as usize,
        index_entry_count("agent_finance_analyst")
    );
    let permitted: Vec<&str> = agent["permitted_actions"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    let blocked: Vec<&str> = agent["blocked_actions"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert!(permitted.iter().any(|a| a.contains("retrieve")), "agent may retrieve");
    assert!(permitted.iter().any(|a| a.contains("propose")), "agent may propose");
    assert!(blocked.iter().any(|a| a.contains("approve")), "agent cannot approve/reject");
    assert!(blocked.iter().any(|a| a.contains("mutate")), "agent cannot mutate");
    assert_metadata_only(&agent, "agent");

    // A person who owns an agent surfaces it (real ownership edge).
    let (_s, bytes) = get(&router, "/node/p061/summary", "p061").await;
    let owner: Value = serde_json::from_slice(&bytes).unwrap();
    let owned: Vec<&str> = owner["agents_owned"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap()).collect();
    assert!(owned.contains(&"agent_finance_analyst"), "p061 owns the finance agent");

    // NON-PRINCIPALS are summarised on the client, never here: a department, a
    // source, and an unknown id each get the one 404.
    for id in ["Finance", "docstore", "p_ghost_404"] {
        let (status, _) = get(&router, &format!("/node/{id}/summary"), "p060").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{id} -> the one 404");
    }
    println!("GR-7: node summary is real (counts == compiled), scope-respecting, metadata-only");
}
