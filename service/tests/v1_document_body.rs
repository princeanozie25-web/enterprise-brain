//! S2b Part 1: full-body serving on GET /v1/documents/{id}.
//!
//! The compiled allowlist is the boundary: an authorized principal on the
//! machine surface receives the FULL document body (truncating an
//! authorized read is not defense-in-depth — the S2b product ruling).
//! The console's DocCard snippet law is a console law and stands there
//! untouched. Bodies come from the hash-verified in-memory corpus; a body
//! past the 2 MiB cap fails LOUD (generic 500 + `body_exceeds_cap` in the
//! ledger) — never truncated, never a 404 that would lie about existence
//! to an authorized principal.

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
use service::{app, AppState, V1_CONTENT_MAX_BYTES};
use tower::ServiceExt;

const FINANCE_OID: &str = "dddd4444-0000-4000-8000-0000000000d4";

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

fn base_state() -> AppState {
    AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("people layer")
}

fn bridge_for(dir: &Path) -> Arc<Bridge> {
    let jwks_path = dir.join("jwks.json");
    fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
    let config: AgentBridgeConfig = serde_json::from_value(json!({
        "enabled": true,
        "tenant_id": TEST_TENANT,
        "audience": TEST_AUDIENCE,
        "jwks": { "file": jwks_path },
        "agents": [
            { "tid": TEST_TENANT, "oid": FINANCE_OID, "principal": "agent_finance_analyst" }
        ]
    }))
    .expect("bridge config parses");
    Arc::new(Bridge::from_config(&config).expect("bridge builds"))
}

/// One oracle-allowed doc id for finance, from raw ground truth.
fn finance_allowed_doc() -> String {
    let text = fs::read_to_string(common::repo_fixtures_dir().join("ground_truth.jsonl"))
        .expect("ground truth");
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("row");
        if row["principal_id"] == "agent_finance_analyst" && row["decision"] == "ALLOW" {
            return row["resource_id"]
                .as_str()
                .expect("resource_id")
                .to_string();
        }
    }
    panic!("finance has allowed docs");
}

/// doc id -> full fixture body, from raw documents.json.
fn fixture_bodies() -> std::collections::BTreeMap<String, String> {
    let text = fs::read_to_string(common::repo_fixtures_dir().join("documents.json"))
        .expect("documents.json");
    let parsed: Value = serde_json::from_str(&text).expect("documents parse");
    parsed["documents"]
        .as_array()
        .expect("documents array")
        .iter()
        .map(|d| {
            (
                d["id"].as_str().expect("id").to_string(),
                d["body"].as_str().expect("body").to_string(),
            )
        })
        .collect()
}

async fn get_doc_raw(router: &axum::Router, doc: &str, bearer: &str) -> (StatusCode, Vec<u8>) {
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

// RECON PIN: the fixture corpus sits FAR below the cap — the cap must
// never fire on real fixtures (largest body ~2.8 KB vs 2 MiB; require
// two orders of magnitude of margin).
#[test]
fn fixture_bodies_sit_far_below_the_cap() {
    let largest = fixture_bodies()
        .values()
        .map(|body| body.len())
        .max()
        .expect("bodies exist");
    assert!(
        largest * 100 < V1_CONTENT_MAX_BYTES,
        "largest fixture body ({largest} B) must sit at least 100x under \
         the {V1_CONTENT_MAX_BYTES} B cap"
    );
}

// CONTRACT PIN: the response field set is exactly
// {content, doc_id, metadata, snippet, title}; content is the FULL fixture
// body byte-for-byte; snippet is the deterministic prefix and differs from
// content (every fixture body exceeds the 480-char snippet).
#[tokio::test]
async fn document_response_serves_the_full_body_with_the_pinned_shape() {
    let dir = scratch("v1-body-shape");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = base_state()
        .with_proposals(store)
        .with_agent_bridge(bridge_for(&dir));
    let router = app(Arc::new(state));

    let doc = finance_allowed_doc();
    let bodies = fixture_bodies();
    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let (status, bytes) = get_doc_raw(&router, &doc, &token).await;
    assert_eq!(status, StatusCode::OK);
    let payload: Value = serde_json::from_slice(&bytes).expect("json");
    let object = payload.as_object().expect("object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["content", "doc_id", "metadata", "snippet", "title"],
        "the S2b document contract is exactly these five fields"
    );
    assert_eq!(payload["doc_id"], doc.as_str());
    assert_eq!(
        payload["content"].as_str().expect("content"),
        bodies[&doc],
        "content is the full fixture body, byte-for-byte"
    );
    let snippet = payload["snippet"].as_str().expect("snippet");
    assert_ne!(
        snippet, bodies[&doc],
        "snippet and content are distinctly populated"
    );
    assert!(
        payload["metadata"]["sensitivity"].as_str().is_some(),
        "metadata carries the card's sensitivity"
    );

    // The ledger row records that the FULL payload was served.
    let ledger = fs::read_to_string(dir.join("state").join("audit.jsonl")).expect("ledger");
    assert!(
        ledger.contains("\"payload\":\"full\""),
        "the authorized serve is ledgered as payload: full"
    );
}

// CAP GUARD: an injected oversized body (the fixture corpus can never
// produce one — recon pin above) fails LOUD: generic 500, never a
// truncated 200, never a 404 — and the ledger says body_exceeds_cap.
#[tokio::test]
async fn oversized_body_fails_loud_and_is_ledgered() {
    let dir = scratch("v1-body-cap");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let doc = finance_allowed_doc();
    let mut state = base_state();
    // Inject: blow the body past the cap AFTER startup verification —
    // the decision path (compiled artifacts) is untouched; only the
    // serving payload is oversized.
    let meta = state.docs.get_mut(&doc).expect("doc exists");
    meta.body = "x".repeat(V1_CONTENT_MAX_BYTES + 1);
    let state = state
        .with_proposals(store)
        .with_agent_bridge(bridge_for(&dir));
    let router = app(Arc::new(state));

    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let (status, bytes) = get_doc_raw(&router, &doc, &token).await;
    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "past the cap the request FAILS — loud, generic"
    );
    let body_text = String::from_utf8_lossy(&bytes);
    assert!(
        !body_text.contains("body_exceeds_cap"),
        "the reason stays ledger-only"
    );
    assert!(
        bytes.len() < 1024,
        "never a truncated 200 smuggling partial content"
    );

    let ledger = fs::read_to_string(dir.join("state").join("audit.jsonl")).expect("ledger");
    assert!(
        ledger.contains("\"outcome\":\"body_exceeds_cap\""),
        "the cap refusal is a ledgered monitoring signal"
    );
}
