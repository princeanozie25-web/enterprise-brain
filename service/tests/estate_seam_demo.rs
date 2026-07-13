//! S3 — THE DONE-CRITERIA DEMO, as a named test. The architectural sentence
//! made concrete: PERMISSIONS DO NOT LIVE WITH THE DOCUMENT. One confidential
//! document in EACH source. Agent A (authorized for confidential, spanning
//! both sources) fetches both and receives full content. Agent B
//! (internal-only) is denied both — THE byte-identical 404. Both contrasts
//! are ledgered. The same object, two verdicts, decided entirely by the
//! access model — never by the bytes.
//!
//! This test's stdout is the demo seed; it is written to read aloud.

mod common;

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

async fn get_doc(router: &axum::Router, doc: &str, bearer: &str) -> (StatusCode, Vec<u8>) {
    let response = router
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
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, bytes.to_vec())
}

/// A confidential doc id in each source.
fn confidential_in_each_source() -> (String, String) {
    // Source 2: the first finance-restricted (confidential) object.
    let access = fs::read_to_string(
        common::repo_fixtures_dir()
            .join("estate")
            .join("s3-access.json"),
    )
    .expect("s3-access");
    let parsed: Value = serde_json::from_str(&access).expect("parse");
    let s3 = parsed["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["sensitivity"] == "confidential")
        .and_then(|o| o["doc_id"].as_str())
        .expect("a confidential s3 object")
        .to_string();
    // Source 1: the first confidential primary doc.
    let docs = fs::read_to_string(common::repo_fixtures_dir().join("documents.json"))
        .expect("documents.json");
    let parsed: Value = serde_json::from_str(&docs).expect("parse");
    let primary = parsed["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["sensitivity"] == "confidential")
        .and_then(|d| d["id"].as_str())
        .expect("a confidential primary doc")
        .to_string();
    (primary, s3)
}

#[tokio::test]
async fn estate_seam_demo() {
    let dir = scratch("estate-seam-demo");
    let jwks_path = dir.join("jwks.json");
    fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
    let config: AgentBridgeConfig = serde_json::from_value(json!({
        "enabled": true,
        "tenant_id": TEST_TENANT,
        "audience": TEST_AUDIENCE,
        "jwks": { "file": jwks_path },
        "agents": [
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
    .with_estate_from(&common::repo_fixtures_dir().join("estate"))
    .expect("estate loads")
    .with_proposals(store)
    .with_agent_bridge(Arc::new(
        Bridge::from_config(&config).expect("bridge builds"),
    ));
    let router = app(Arc::new(state));

    let (primary_conf, s3_conf) = confidential_in_each_source();
    let token_a = TokenSpec::autonomous(AGENT_A.1).sign();
    let token_b = TokenSpec::autonomous(AGENT_B.1).sign();

    println!("\n=== ESTATE SEAM DEMO — permissions do not live with the document ===");
    println!("Source 1 (primary corpus)  confidential doc: {primary_conf}");
    println!("Source 2 (S3-shaped store)  confidential doc: {s3_conf}");
    println!("Agent A = {} (authorized up to confidential)", AGENT_A.0);
    println!("Agent B = {} (internal only)\n", AGENT_B.0);

    // Agent A: 200 with full content on BOTH sources.
    let mut a_404: Option<Vec<u8>> = None;
    for (label, doc) in [("source 1", &primary_conf), ("source 2", &s3_conf)] {
        let (status, bytes) = get_doc(&router, doc, &token_a).await;
        assert_eq!(status, StatusCode::OK, "agent A must read {doc}");
        let payload: Value = serde_json::from_slice(&bytes).expect("json");
        let content_len = payload["content"].as_str().map(str::len).unwrap_or(0);
        assert!(content_len > 0, "agent A gets full content on {doc}");
        println!(
            "  A -> {label} {doc}: 200, {} bytes of content, source={}",
            content_len, payload["metadata"]["source"]
        );
    }

    // Agent B: THE byte-identical 404 on BOTH sources — same objects, denied.
    for (label, doc) in [("source 1", &primary_conf), ("source 2", &s3_conf)] {
        let (status, bytes) = get_doc(&router, doc, &token_b).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "agent B must be denied {doc}"
        );
        match &a_404 {
            None => a_404 = Some(bytes.clone()),
            Some(reference) => assert_eq!(&bytes, reference, "every 404 is byte-identical"),
        }
        let payload: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["error"], "not found");
        println!("  B -> {label} {doc}: 404 (denied — indistinguishable from nonexistent)");
    }

    // Both contrasts are ledgered: A's two allows (payload full, with
    // source), B's two denies (not_found, with source).
    let ledger = fs::read_to_string(dir.join("state").join("audit.jsonl")).expect("ledger");
    let rows: Vec<Value> = ledger
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    let a_allows: Vec<&Value> = rows
        .iter()
        .filter(|r| {
            r["actor_principal"] == AGENT_A.0
                && r["outcome"] == "authorized"
                && r["payload"] == "full"
        })
        .collect();
    let b_denies: Vec<&Value> = rows
        .iter()
        .filter(|r| r["actor_principal"] == AGENT_B.0 && r["outcome"] == "not_found")
        .collect();
    assert_eq!(a_allows.len(), 2, "agent A's two allows are ledgered");
    assert_eq!(b_denies.len(), 2, "agent B's two denies are ledgered");
    // Each contrast names its source.
    let a_sources: std::collections::BTreeSet<&str> = a_allows
        .iter()
        .filter_map(|r| r["source"].as_str())
        .collect();
    let b_sources: std::collections::BTreeSet<&str> = b_denies
        .iter()
        .filter_map(|r| r["source"].as_str())
        .collect();
    assert_eq!(
        a_sources,
        ["primary", "s3"].into_iter().collect(),
        "A's allows span both sources in the ledger"
    );
    assert_eq!(
        b_sources,
        ["primary", "s3"].into_iter().collect(),
        "B's denies span both sources in the ledger"
    );

    println!("\nLedger: A has 2 authorized (payload=full) rows across sources {a_sources:?};");
    println!("        B has 2 not_found rows across sources {b_sources:?}.");
    println!("The document did not decide. The access model did. ===\n");
}
