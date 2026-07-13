//! S3 conformance ACROSS THE SEAM: the full estate matrix, (124 + 2)
//! principals × (600 + 150) documents = 94,500 pairs, checked against an
//! oracle that reads BOTH raw ground truths independently. 0 false-allow /
//! 0 false-deny. Plus the token-path matrix through `/v1`: 6 registered
//! agents × 750 documents = 4,500 pairs, HTTP end-to-end, with full-body
//! content-leak assertions (bytes only on oracle-allows, both sources).
//!
//! The methodology is unchanged and sacred: the oracle (the committed
//! ground-truth files, produced independently by `synth/estate.py` and the
//! primary generator) is the truth; the engine is the system under test; a
//! single false-allow blocks release. What grew is the WORLD, once, here.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_AUDIENCE, TEST_TENANT};
use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::agent_bridge::{AgentBridgeConfig, Bridge};
use service::{app, v1_document_authorized, AppState};
use tower::ServiceExt;

/// The six registered agents: four legacy (source 1 only) + two estate
/// (both sources). Legacy oids match the existing conformance fixtures.
const LEGACY_AGENTS: [(&str, &str); 4] = [
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
const ESTATE_AGENTS: [(&str, &str); 2] = [
    (
        "agent_estate_confidential",
        "bbbb0001-5c1e-4a2b-9d3e-000000000b01",
    ),
    (
        "agent_estate_internal",
        "bbbb0002-5c1e-4a2b-9d3e-000000000b02",
    ),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("service crate sits in the repo root")
        .to_path_buf()
}

fn estate_dir() -> PathBuf {
    common::repo_fixtures_dir().join("estate")
}

fn scratch(name: &str) -> PathBuf {
    // Unique per invocation: Windows scanners (Search indexer / Defender) can
    // hold a just-deleted path in delete-pending state, so re-creating the
    // SAME path races them into Os error 5 "Access is denied". A fresh suffix
    // never re-opens a dying path.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // The base lives in the SYSTEM temp dir, not target/tmp: the repo sits
    // under Documents\, which Windows Search indexes by default — its crawler
    // opens freshly written index segments mid-build and the write fails with
    // os error 5. AppData\Local\Temp is outside the default index scope.
    let base = std::env::temp_dir().join("enterprise-brain-test-scratch");
    std::fs::create_dir_all(&base).expect("scratch base");
    // CREATE-ONLY: the unique pid+seq suffix already guarantees no collision,
    // so this helper never deletes a sibling. Reaping by shared name-prefix
    // raced a concurrently running test in the same binary that used the same
    // name and deleted its live dir (failed estate_probes on Linux CI). Stale
    // dirs from old runs are harmless; the OS temp cleaner reaps them.
    let dir = base.join(format!(
        "{name}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn estate_state(dir: &Path) -> AppState {
    let jwks_path = dir.join("jwks.json");
    fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
    let mut agents: Vec<Value> = LEGACY_AGENTS
        .iter()
        .chain(ESTATE_AGENTS.iter())
        .map(|(agent, oid)| json!({ "tid": TEST_TENANT, "oid": oid, "principal": agent }))
        .collect();
    // A ghost across the seam: registered at the bridge, unknown to every
    // authority model — must be all-deny over the whole estate.
    agents.push(json!({
        "tid": TEST_TENANT, "oid": "cccc9999-5c1e-4a2b-9d3e-000000000c99",
        "principal": "agent_estate_ghost"
    }));
    let config: AgentBridgeConfig = serde_json::from_value(json!({
        "enabled": true,
        "tenant_id": TEST_TENANT,
        "audience": TEST_AUDIENCE,
        "jwks": { "file": jwks_path },
        "agents": agents,
    }))
    .expect("bridge config parses");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("people layer")
    .with_estate_from(&estate_dir())
    .expect("estate loads")
    .with_proposals(store)
    .with_agent_bridge(Arc::new(
        Bridge::from_config(&config).expect("bridge builds"),
    ))
}

/// The composed oracle: (principal, resource) -> expected ALLOW, read
/// INDEPENDENTLY from both committed ground truths. Primary×existing comes
/// from the primary `ground_truth.jsonl`; everything estate-relevant from
/// `estate/estate_ground_truth.jsonl`.
fn composed_oracle() -> BTreeMap<(String, String), bool> {
    let mut oracle = BTreeMap::new();
    let primary = fs::read_to_string(common::repo_fixtures_dir().join("ground_truth.jsonl"))
        .expect("primary ground truth");
    for line in primary.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("row");
        oracle.insert(
            (
                row["principal_id"].as_str().unwrap().to_string(),
                row["resource_id"].as_str().unwrap().to_string(),
            ),
            row["decision"] == "ALLOW",
        );
    }
    let estate = fs::read_to_string(estate_dir().join("estate_ground_truth.jsonl"))
        .expect("estate ground truth");
    for line in estate.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("row");
        oracle.insert(
            (
                row["principal_id"].as_str().unwrap().to_string(),
                row["resource_id"].as_str().unwrap().to_string(),
            ),
            row["decision"] == "ALLOW",
        );
    }
    oracle
}

fn all_doc_ids() -> Vec<String> {
    let mut ids = Vec::new();
    let docs = fs::read_to_string(common::repo_fixtures_dir().join("documents.json"))
        .expect("documents.json");
    let parsed: Value = serde_json::from_str(&docs).expect("parse");
    for d in parsed["documents"].as_array().unwrap() {
        ids.push(d["id"].as_str().unwrap().to_string());
    }
    let access = fs::read_to_string(estate_dir().join("s3-access.json")).expect("s3-access");
    let parsed: Value = serde_json::from_str(&access).expect("parse");
    for o in parsed["objects"].as_array().unwrap() {
        ids.push(o["doc_id"].as_str().unwrap().to_string());
    }
    ids.sort();
    ids
}

// THE 94,500 — the full estate matrix through the engine's composed
// authorization, checked against the independent oracle. 0/0.
#[test]
fn estate_document_matrix_is_zero_false_allow_zero_false_deny() {
    let dir = scratch("estate-conformance-doc");
    let state = estate_state(&dir);
    let oracle = composed_oracle();
    let doc_ids = all_doc_ids();

    // Principals: the 124 existing (from the primary oracle) + the 2 estate
    // agents. The ghost is a bridge-only extra, proven separately below.
    let mut principals: std::collections::BTreeSet<String> = oracle
        .keys()
        .map(|(p, _)| p.clone())
        .filter(|p| !p.starts_with("agent_estate_"))
        .collect();
    for (agent, _) in ESTATE_AGENTS {
        principals.insert(agent.to_string());
    }
    assert_eq!(principals.len(), 126, "124 existing + 2 estate agents");
    assert_eq!(doc_ids.len(), 750, "600 primary + 150 estate objects");

    let mut pairs = 0u64;
    let mut false_allows = Vec::new();
    let mut false_denies = Vec::new();
    let mut allow_total = 0u64;
    for principal in &principals {
        for doc_id in &doc_ids {
            let expected = *oracle
                .get(&(principal.clone(), doc_id.clone()))
                .unwrap_or_else(|| panic!("oracle has no row for ({principal}, {doc_id})"));
            let actual = v1_document_authorized(&state, principal, doc_id).expect("decision");
            pairs += 1;
            if actual {
                allow_total += 1;
            }
            match (actual, expected) {
                (true, false) => false_allows.push(format!("{principal} x {doc_id}")),
                (false, true) => false_denies.push(format!("{principal} x {doc_id}")),
                _ => {}
            }
        }
    }
    println!(
        "S3 estate matrix: pairs={pairs} allow_total={allow_total} false_allows={} false_denies={}",
        false_allows.len(),
        false_denies.len()
    );
    assert_eq!(pairs, 94_500, "126 principals x 750 documents");
    assert!(
        false_allows.is_empty(),
        "FALSE ALLOWS across the seam: {false_allows:?}"
    );
    assert!(
        false_denies.is_empty(),
        "FALSE DENIES across the seam: {false_denies:?}"
    );

    // The ghost across the seam: registered at the bridge, unknown to every
    // authority model — all-deny over the whole estate (both sources).
    let mut ghost_denies = 0u64;
    for doc_id in &doc_ids {
        assert!(
            !v1_document_authorized(&state, "agent_estate_ghost", doc_id).expect("decision"),
            "the ghost must be denied {doc_id}"
        );
        ghost_denies += 1;
    }
    println!("S3 estate ghost: {ghost_denies}/750 denied across both sources");
    assert_eq!(ghost_denies, 750);
}

// THE 4,500 token-path matrix through /v1: 6 registered agents × 750 docs,
// HTTP end-to-end, decision checked against the oracle, with full-body
// content-leak assertions on BOTH sources.
#[tokio::test]
async fn estate_token_path_matrix_through_v1_is_zero_zero() {
    let dir = scratch("estate-conformance-token");
    let router = app(Arc::new(estate_state(&dir)));
    let oracle = composed_oracle();
    let doc_ids = all_doc_ids();
    let bodies = all_bodies();

    let mut pairs = 0u64;
    let mut false_allows = Vec::new();
    let mut false_denies = Vec::new();
    let mut content_faults = Vec::new();
    let mut served_allows = 0u64;
    let mut the_404: Option<Vec<u8>> = None;

    for (agent, oid) in LEGACY_AGENTS.iter().chain(ESTATE_AGENTS.iter()) {
        let token = TokenSpec::autonomous(oid).sign();
        for doc_id in &doc_ids {
            let (status, bytes) = get_doc(&router, doc_id, &token).await;
            let served = match status {
                StatusCode::OK => true,
                StatusCode::NOT_FOUND => false,
                other => panic!("{agent} x {doc_id}: unexpected status {other}"),
            };
            // Legacy agents over s3 objects have no oracle row (the oracle
            // enumerates existing PRINCIPALS x s3, and the 4 legacy AGENTS
            // are among neither the 124 nor the estate agents) — they must
            // be denied by the seam default; assert directly.
            let expected = oracle
                .get(&(agent.to_string(), doc_id.clone()))
                .copied()
                .unwrap_or(false);
            pairs += 1;
            if served {
                served_allows += 1;
                let payload: Value = serde_json::from_slice(&bytes).expect("json");
                if payload["content"].as_str() != Some(bodies[doc_id].as_str()) {
                    content_faults.push(format!("{agent} x {doc_id}: content mismatch"));
                }
            } else {
                match &the_404 {
                    None => the_404 = Some(bytes),
                    Some(reference) if &bytes != reference => {
                        content_faults.push(format!("{agent} x {doc_id}: 404 not byte-identical"))
                    }
                    _ => {}
                }
            }
            match (served, expected) {
                (true, false) => false_allows.push(format!("{agent} x {doc_id}")),
                (false, true) => false_denies.push(format!("{agent} x {doc_id}")),
                _ => {}
            }
        }
    }
    println!(
        "S3 token-path: pairs={pairs} served_allows={served_allows} false_allows={} false_denies={} content_faults={}",
        false_allows.len(),
        false_denies.len(),
        content_faults.len()
    );
    assert_eq!(pairs, 4_500, "6 agents x 750 documents");
    assert!(false_allows.is_empty(), "FALSE ALLOWS: {false_allows:?}");
    assert!(false_denies.is_empty(), "FALSE DENIES: {false_denies:?}");
    assert!(
        content_faults.is_empty(),
        "content faults: {content_faults:?}"
    );
}

fn all_bodies() -> BTreeMap<String, String> {
    let mut bodies = BTreeMap::new();
    let docs = fs::read_to_string(common::repo_fixtures_dir().join("documents.json"))
        .expect("documents.json");
    let parsed: Value = serde_json::from_str(&docs).expect("parse");
    for d in parsed["documents"].as_array().unwrap() {
        bodies.insert(
            d["id"].as_str().unwrap().to_string(),
            d["body"].as_str().unwrap().to_string(),
        );
    }
    // Estate object bodies come from the store on disk.
    let access = fs::read_to_string(estate_dir().join("s3-access.json")).expect("s3-access");
    let parsed: Value = serde_json::from_str(&access).expect("parse");
    for o in parsed["objects"].as_array().unwrap() {
        let doc_id = o["doc_id"].as_str().unwrap();
        let rel = doc_id.strip_prefix("s3/").unwrap();
        let body = fs::read_to_string(estate_dir().join("s3-store").join(rel))
            .unwrap_or_else(|_| panic!("estate object {doc_id} on disk"));
        bodies.insert(doc_id.to_string(), body);
    }
    bodies
}

async fn get_doc(router: &axum::Router, doc_id: &str, bearer: &str) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/documents/{doc_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
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
