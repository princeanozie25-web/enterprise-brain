//! S3 Part 5: the grown estate stays inside the gates. Warm p99 < 100 ms
//! over a 1,000-request mix (estate-agent document fetches across both
//! sources + estate retrieves); full estate load + hash verification
//! < 5 s. Reported against the S2b baseline (p99 4.19 ms).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_AUDIENCE, TEST_TENANT};
use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::agent_bridge::{AgentBridgeConfig, Bridge};
use service::estate::EstateModel;
use service::{app, AppState};
use tower::ServiceExt;

const AGENT_A: (&str, &str) = (
    "agent_estate_confidential",
    "bbbb0001-5c1e-4a2b-9d3e-000000000b01",
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
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let index = ((sorted.len() as f64) * p).ceil() as usize;
    sorted[index.saturating_sub(1).min(sorted.len() - 1)]
}

// Startup: the full estate load + content-hash verification is well under
// the 5 s budget.
#[test]
fn estate_load_stays_under_the_startup_budget() {
    let started = Instant::now();
    let model = EstateModel::load(&estate_dir()).expect("estate loads");
    let elapsed = started.elapsed();
    assert_eq!(model.object_count(), 150);
    println!(
        "S3 startup: full estate load + hash verification in {:?} (budget < 5s)",
        elapsed
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "estate load must stay under 5s, took {elapsed:?}"
    );
}

#[tokio::test]
async fn warm_p99_on_the_grown_estate_stays_under_100ms() {
    let dir = scratch("estate-latency");
    let jwks_path = dir.join("jwks.json");
    fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
    let config: AgentBridgeConfig = serde_json::from_value(json!({
        "enabled": true,
        "tenant_id": TEST_TENANT,
        "audience": TEST_AUDIENCE,
        "jwks": { "file": jwks_path },
        "agents": [{ "tid": TEST_TENANT, "oid": AGENT_A.1, "principal": AGENT_A.0 }],
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
    let router = app(Arc::new(state));

    let token = TokenSpec::autonomous(AGENT_A.1).sign();
    // A mix of authorized ids across BOTH sources.
    let s3_ids: Vec<String> = {
        let access = fs::read_to_string(estate_dir().join("s3-access.json")).expect("s3-access");
        let parsed: Value = serde_json::from_str(&access).expect("parse");
        parsed["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["doc_id"].as_str().unwrap().to_string())
            .collect()
    };
    let queries = [
        "supplier audit",
        "cold chain transit",
        "invoice matching",
        "site notice",
    ];

    async fn fetch(router: &axum::Router, uri: &str, token: &str) -> StatusCode {
        router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response")
            .status()
    }

    // Warm-up.
    for _ in 0..5 {
        let _ = fetch(&router, &format!("/v1/documents/{}", s3_ids[0]), &token).await;
    }

    let mut durations: Vec<Duration> = Vec::with_capacity(1_000);
    // 800 document fetches, alternating source (both authorized).
    for i in 0..800usize {
        let uri = if i % 2 == 0 {
            format!("/v1/documents/{}", s3_ids[(i / 2) % s3_ids.len()])
        } else {
            format!("/v1/documents/d{:04}", 134 + (i / 2) % 200) // finance-range primary
        };
        let started = Instant::now();
        let status = fetch(&router, &uri, &token).await;
        durations.push(started.elapsed());
        assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    }
    // 200 estate retrieves (source-spanning).
    for i in 0..200usize {
        let body = json!({ "query": queries[i % queries.len()] }).to_string();
        let started = Instant::now();
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/retrieve")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");
        durations.push(started.elapsed());
        assert_eq!(response.status(), StatusCode::OK);
    }

    durations.sort();
    let (p50, p95, p99) = (
        percentile(&durations, 0.50),
        percentile(&durations, 0.95),
        percentile(&durations, 0.99),
    );
    println!(
        "S3 warm latency over {} requests (800 estate documents both sources + 200 estate retrieves): \
         p50 {:?}, p95 {:?}, p99 {:?} (S2b baseline p99 4.19ms)",
        durations.len(),
        p50,
        p95,
        p99
    );
    assert!(
        p99 < Duration::from_millis(100),
        "S3 warm p99 must stay under 100ms, measured {p99:?}"
    );
}
