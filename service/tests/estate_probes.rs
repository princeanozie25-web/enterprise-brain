//! S3 candidacy across the seam (EB-5, estate-wide): every estate-agent
//! retrieve candidate is inside that agent's authorized union across BOTH
//! sources; no out-of-scope id or content leaks; retrieval SPANS sources
//! within scope; and the legacy agents — who hold nothing in the second
//! source — surface nothing from it.

mod common;

use std::collections::BTreeSet;
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

const LEGACY_FINANCE: (&str, &str) = (
    "agent_finance_analyst",
    "aaaa0003-5c1e-4a2b-9d3e-000000000a03",
);
const AGENT_A: (&str, &str) = (
    "agent_estate_confidential",
    "bbbb0001-5c1e-4a2b-9d3e-000000000b01",
);
const AGENT_B: (&str, &str) = (
    "agent_estate_internal",
    "bbbb0002-5c1e-4a2b-9d3e-000000000b02",
);

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

fn world() -> axum::Router {
    let dir = scratch("estate-probes");
    let jwks_path = dir.join("jwks.json");
    fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
    let config: AgentBridgeConfig = serde_json::from_value(json!({
        "enabled": true,
        "tenant_id": TEST_TENANT,
        "audience": TEST_AUDIENCE,
        "jwks": { "file": jwks_path },
        "agents": [
            { "tid": TEST_TENANT, "oid": LEGACY_FINANCE.1, "principal": LEGACY_FINANCE.0 },
            { "tid": TEST_TENANT, "oid": AGENT_A.1, "principal": AGENT_A.0 },
            { "tid": TEST_TENANT, "oid": AGENT_B.1, "principal": AGENT_B.0 },
        ],
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
    .with_estate_from(&estate_dir())
    .expect("estate loads")
    .with_proposals(store)
    .with_agent_bridge(Arc::new(
        Bridge::from_config(&config).expect("bridge builds"),
    ));
    app(Arc::new(state))
}

/// The estate ground truth: principal -> its ALLOW set (both sources).
fn authorized_sets() -> std::collections::BTreeMap<String, BTreeSet<String>> {
    let mut map: std::collections::BTreeMap<String, BTreeSet<String>> = Default::default();
    let text = fs::read_to_string(estate_dir().join("estate_ground_truth.jsonl"))
        .expect("estate ground truth");
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("row");
        if row["decision"] == "ALLOW" {
            map.entry(row["principal_id"].as_str().unwrap().to_string())
                .or_default()
                .insert(row["resource_id"].as_str().unwrap().to_string());
        }
    }
    map
}

/// Estate objects as (doc_id, sensitivity, body).
fn estate_objects() -> Vec<(String, String, String)> {
    let access = fs::read_to_string(estate_dir().join("s3-access.json")).expect("s3-access");
    let parsed: Value = serde_json::from_str(&access).expect("parse");
    parsed["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            let doc_id = o["doc_id"].as_str().unwrap().to_string();
            let rel = doc_id.strip_prefix("s3/").unwrap();
            let body =
                fs::read_to_string(estate_dir().join("s3-store").join(rel)).expect("object body");
            (doc_id, o["sensitivity"].as_str().unwrap().to_string(), body)
        })
        .collect()
}

async fn retrieve(router: &axum::Router, bearer: &str, query: &str) -> Vec<Value> {
    let body = json!({ "query": query, "top_k": 50 }).to_string();
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/retrieve")
                .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let payload: Value = serde_json::from_slice(&bytes).expect("json");
    payload["candidates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// A distinctive anchor token from a body: the first EST-* reference tag.
fn est_tag(body: &str) -> Option<String> {
    body.split_whitespace()
        .find(|w| w.starts_with("EST-"))
        .map(|w| {
            w.trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_string()
        })
}

#[tokio::test]
async fn estate_agents_retrieve_within_scope_across_the_seam() {
    let router = world();
    let authorized = authorized_sets();
    let objects = estate_objects();

    for (agent, oid) in [AGENT_A, AGENT_B] {
        let token = TokenSpec::autonomous(oid).sign();
        let allow_set = &authorized[agent];
        // 15 probes: distinctive EST tags from objects THIS agent may read.
        let readable: Vec<&(String, String, String)> = objects
            .iter()
            .filter(|(doc_id, _, _)| allow_set.contains(doc_id))
            .collect();
        let mut probes = 0;
        for (doc_id, _sens, body) in readable.iter().take(15) {
            let Some(anchor) = est_tag(body) else {
                continue;
            };
            let candidates = retrieve(&router, &token, &anchor).await;
            let ids: Vec<&str> = candidates
                .iter()
                .map(|c| c["doc_id"].as_str().unwrap())
                .collect();
            // (a) every candidate is inside the agent's authorized union.
            for id in &ids {
                assert!(
                    allow_set.contains(*id),
                    "{agent}: candidate {id} outside the authorized union (anchor {anchor})"
                );
            }
            // (b) the anchored object surfaces (it is distinctive + in scope).
            assert!(
                ids.contains(&doc_id.as_str()),
                "{agent}: anchor {anchor} must surface its object {doc_id}"
            );
            probes += 1;
        }
        assert!(
            probes >= 15,
            "{agent}: at least 15 probes ran (got {probes})"
        );
        println!("{agent}: {probes} in-scope probes, zero out-of-scope candidates");
    }
}

#[tokio::test]
async fn retrieval_spans_both_sources_within_scope() {
    let router = world();
    let token = TokenSpec::autonomous(AGENT_A.1).sign();
    // "supplier audit" appears in BOTH the primary corpus and the S3 store.
    let candidates = retrieve(&router, &token, "supplier audit").await;
    let sources: BTreeSet<&str> = candidates
        .iter()
        .map(|c| {
            if c["doc_id"].as_str().unwrap().starts_with("s3/") {
                "s3"
            } else {
                "primary"
            }
        })
        .collect();
    assert!(
        sources.contains("primary") && sources.contains("s3"),
        "agent A's retrieve for a shared term spans BOTH sources; saw {sources:?}"
    );
    // Ranks are 1..n ascending, intact across the merged sources.
    let ranks: Vec<u64> = candidates
        .iter()
        .map(|c| c["rank"].as_u64().unwrap())
        .collect();
    assert_eq!(ranks, (1..=ranks.len() as u64).collect::<Vec<_>>());
    println!(
        "agent A 'supplier audit': {} candidates spanning {sources:?}, ranks intact",
        candidates.len()
    );
}

#[tokio::test]
async fn internal_agent_never_sees_confidential_second_source() {
    let router = world();
    let token = TokenSpec::autonomous(AGENT_B.1).sign();
    let objects = estate_objects();
    // Probe agent B with anchors of CONFIDENTIAL s3 objects it must NOT see.
    let mut checked = 0;
    for (doc_id, sens, body) in &objects {
        if sens != "confidential" {
            continue;
        }
        let Some(anchor) = est_tag(body) else {
            continue;
        };
        let candidates = retrieve(&router, &token, &anchor).await;
        for c in &candidates {
            let id = c["doc_id"].as_str().unwrap();
            assert_ne!(
                id, doc_id,
                "agent B (internal) must not surface confidential {doc_id}"
            );
            assert!(
                !id.starts_with("s3/") || !is_confidential(&objects, id),
                "agent B surfaced a confidential s3 object {id}"
            );
        }
        checked += 1;
        if checked >= 10 {
            break;
        }
    }
    println!("agent B: {checked} confidential-anchor probes, none surfaced");
}

#[tokio::test]
async fn legacy_agent_surfaces_nothing_from_the_second_source() {
    let router = world();
    let token = TokenSpec::autonomous(LEGACY_FINANCE.1).sign();
    let objects = estate_objects();
    // Probe the legacy finance agent with second-source anchors — it holds
    // NOTHING in the estate; its retrieval is the primary path, which does
    // not even index the second source.
    let mut checked = 0;
    for (_doc_id, _sens, body) in objects.iter().take(20) {
        let Some(anchor) = est_tag(body) else {
            continue;
        };
        let candidates = retrieve(&router, &token, &anchor).await;
        for c in &candidates {
            assert!(
                !c["doc_id"].as_str().unwrap().starts_with("s3/"),
                "legacy agent must never surface a second-source object"
            );
        }
        checked += 1;
    }
    assert!(checked >= 10);
    println!("legacy finance agent: {checked} second-source anchors, zero s3 candidates");
}

fn is_confidential(objects: &[(String, String, String)], doc_id: &str) -> bool {
    objects
        .iter()
        .any(|(id, sens, _)| id == doc_id && sens == "confidential")
}
