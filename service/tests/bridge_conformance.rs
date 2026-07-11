//! S1 conformance: the full fixture-agent × 600-document matrix THROUGH THE
//! /v1 MACHINE SURFACE, end-to-end — a real locally-signed Entra-shaped JWT
//! in, a live-router `GET /v1/documents/{id}` decision out — judged against
//! /fixtures/ground_truth.jsonl, the oracle computed INDEPENDENTLY from raw
//! company.json (never from the service's own projection). 4 × 600 = 2,400
//! decisions; a single false-allow or false-deny fails the build.
//!
//! Fixture reality (recon 2026-07-11): all four Bryremead agents carry
//! grants and NON-EMPTY compiled scopes (allows 60/124/168/60 of 600), and
//! no fixture principal is all-deny — so the matrix rows are all MIXED
//! (the strong evidence), and the all-deny pole is proven separately: a
//! registration mapped to a principal the identity model does not know,
//! which compiles to the empty statement and must deny all 600 (the
//! deny-by-default floor under every stale or wrong registration).

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_AUDIENCE, TEST_TENANT};
use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::agent_bridge::{AgentBridgeConfig, Bridge};
use service::{app, AppState};
use tower::ServiceExt;

/// The four Bryremead agents and their fixture Entra object ids.
const AGENTS: [(&str, &str); 4] = [
    ("agent_qa_drafter", "aaaa0001-5c1e-4a2b-9d3e-000000000a01"),
    (
        "agent_ops_concierge",
        "aaaa0002-5c1e-4a2b-9d3e-000000000a02",
    ),
    (
        "agent_finance_analyst",
        "aaaa0003-5c1e-4a2b-9d3e-000000000a03",
    ),
    ("agent_exec_brief", "aaaa0004-5c1e-4a2b-9d3e-000000000a04"),
];
/// The all-deny pole: registered at the bridge, unknown to the identity
/// model — the empty statement by construction.
const PROBE_OID: &str = "aaaa0005-5c1e-4a2b-9d3e-000000000a05";
const PROBE_PRINCIPAL: &str = "agent_probe_no_such_principal";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("service crate sits in the repo root")
        .to_path_buf()
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// The independent oracle: principal -> doc -> ALLOW/DENY, from the raw
/// fixture ground truth.
fn oracle() -> BTreeMap<String, BTreeMap<String, bool>> {
    let path = common::repo_fixtures_dir().join("ground_truth.jsonl");
    let text = fs::read_to_string(path).expect("ground truth");
    let wanted: BTreeSet<&str> = AGENTS.iter().map(|(agent, _)| *agent).collect();
    let mut map: BTreeMap<String, BTreeMap<String, bool>> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("ground truth row");
        let principal = row["principal_id"].as_str().expect("principal_id");
        if !wanted.contains(principal) {
            continue;
        }
        let doc = row["resource_id"].as_str().expect("resource_id");
        let allow = match row["decision"].as_str() {
            Some("ALLOW") => true,
            Some("DENY") => false,
            other => panic!("unknown oracle decision {other:?}"),
        };
        map.entry(principal.to_string())
            .or_default()
            .insert(doc.to_string(), allow);
    }
    map
}

async fn doc_status(router: &axum::Router, doc: &str, bearer: &str) -> StatusCode {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/documents/{doc}"))
                .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response")
        .status()
}

#[tokio::test]
async fn bridge_conformance_full_agent_matrix_is_zero_false_allow_zero_false_deny() {
    let oracle = oracle();
    // Every agent must have exactly the full 600-document row set, and the
    // four ALLOW sets must be pairwise DISTINCT non-empty scopes (mixed
    // rows are what make the matrix a proof, not an all-deny tautology).
    let mut allow_sets: Vec<(String, BTreeSet<String>)> = Vec::new();
    for (agent, _) in AGENTS {
        let rows = oracle
            .get(agent)
            .unwrap_or_else(|| panic!("oracle has no rows for {agent}"));
        assert_eq!(
            rows.len(),
            600,
            "{agent} must have the full document row set"
        );
        let allows: BTreeSet<String> = rows
            .iter()
            .filter(|(_, allow)| **allow)
            .map(|(doc, _)| doc.clone())
            .collect();
        assert!(
            !allows.is_empty(),
            "{agent} carries a non-empty compiled scope"
        );
        allow_sets.push((agent.to_string(), allows));
    }
    // The diversity floor the proof needs: AT LEAST TWO distinct non-empty
    // scopes among the registered agents. (Fixture fact: qa_drafter and
    // exec_brief compile to the IDENTICAL 60-doc set d0213–d0272 — their
    // groups reach the same documents — so the four agents hold THREE
    // distinct scopes, not four.)
    let distinct: BTreeSet<&BTreeSet<String>> =
        allow_sets.iter().map(|(_, allows)| allows).collect();
    assert!(
        distinct.len() >= 2,
        "at least two registered agents must hold distinct non-empty scopes; found {}",
        distinct.len()
    );
    println!(
        "S1 conformance scopes: {} distinct non-empty allow-sets across {} agents ({})",
        distinct.len(),
        allow_sets.len(),
        allow_sets
            .iter()
            .map(|(agent, allows)| format!("{agent}={}", allows.len()))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // The world: real fixtures, compiled artifacts, retrieval index; the
    // bridge enabled through the SAME config type production uses.
    let dir = scratch("bridge-conformance");
    let jwks_path = dir.join("jwks.json");
    fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
    let mut agents_config: Vec<Value> = AGENTS
        .iter()
        .map(|(agent, oid)| json!({ "tid": TEST_TENANT, "oid": oid, "principal": agent }))
        .collect();
    agents_config.push(json!({
        "tid": TEST_TENANT, "oid": PROBE_OID, "principal": PROBE_PRINCIPAL
    }));
    let config: AgentBridgeConfig = serde_json::from_value(json!({
        "enabled": true,
        "tenant_id": TEST_TENANT,
        "audience": TEST_AUDIENCE,
        "jwks": { "file": jwks_path },
        "agents": agents_config,
    }))
    .expect("bridge config parses");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("people layer")
    .with_proposals(store)
    .with_agent_bridge(Arc::new(
        Bridge::from_config(&config).expect("bridge builds"),
    ));
    let router = app(Arc::new(state));

    // The matrix: one real signed token per agent, every document, decision
    // compared against the oracle. ALLOW = 200; DENY = THE 404. Anything
    // else is a harness fault, not a conformance datum.
    let mut pairs = 0u32;
    let mut false_allows: Vec<String> = Vec::new();
    let mut false_denies: Vec<String> = Vec::new();
    let mut served_allow_total = 0u32;
    for (agent, oid) in AGENTS {
        let token = TokenSpec::autonomous(oid).sign();
        let rows = &oracle[agent];
        for (doc, expected_allow) in rows {
            let status = doc_status(&router, doc, &token).await;
            let served_allow = match status {
                StatusCode::OK => true,
                StatusCode::NOT_FOUND => false,
                other => panic!("{agent} x {doc}: unexpected status {other}"),
            };
            pairs += 1;
            if served_allow {
                served_allow_total += 1;
            }
            match (served_allow, *expected_allow) {
                (true, false) => false_allows.push(format!("{agent} x {doc}")),
                (false, true) => false_denies.push(format!("{agent} x {doc}")),
                _ => {}
            }
        }
    }
    println!(
        "S1 conformance summary: pairs={pairs} false_allows={} false_denies={} served_allow_total={served_allow_total}",
        false_allows.len(),
        false_denies.len(),
    );
    assert_eq!(pairs, 2_400, "4 agents x 600 documents");
    assert!(
        false_allows.is_empty(),
        "FALSE ALLOWS through the token path: {false_allows:?}"
    );
    assert!(
        false_denies.is_empty(),
        "FALSE DENIES through the token path: {false_denies:?}"
    );

    // The all-deny pole (supplementary, beyond the 2,400): the probe
    // registration resolves at the bridge but compiles to the EMPTY
    // statement — all 600 documents are THE 404.
    let probe_token = TokenSpec::autonomous(PROBE_OID).sign();
    let mut probe_denies = 0u32;
    for doc in oracle[AGENTS[0].0].keys() {
        let status = doc_status(&router, doc, &probe_token).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "the empty-scope registration must deny {doc}"
        );
        probe_denies += 1;
    }
    println!(
        "S1 conformance supplementary: empty-scope probe denied {probe_denies}/600 (all-deny pole)"
    );
    assert_eq!(probe_denies, 600);
}
