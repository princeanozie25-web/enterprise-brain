//! S3 Part 5: the grown estate stays inside the gates. Warm p99 < 100 ms
//! over a 1,000-request mix (estate-agent document fetches across both
//! sources + estate retrieves); full estate load + hash verification
//! < 5 s. Reported against the S2b baseline (p99 4.19 ms).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
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

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let index = ((sorted.len() as f64) * p).ceil() as usize;
    sorted[index.saturating_sub(1).min(sorted.len() - 1)]
}

/// These are BENCHMARKS with hard gates, not logic tests: the default test
/// harness runs a target's tests on parallel threads, so the timing tests
/// contend with each other for cores and flake under load (thrice observed,
/// always under concurrent heavy builds). Every test in this target takes
/// this lock so the target executes serially — the smallest mechanism the
/// harness supports (cargo has no per-target --test-threads knob, and a
/// workspace-wide RUST_TEST_THREADS=1 would serialize the whole suite).
/// Thresholds are untouched; the gate just gets a quiet core.
static QUIET_CORE: Mutex<()> = Mutex::new(());

fn quiet_core() -> std::sync::MutexGuard<'static, ()> {
    // A poisoned lock (an earlier benchmark panicked) must not cascade —
    // the serialization, not the payload, is what matters here.
    QUIET_CORE.lock().unwrap_or_else(PoisonError::into_inner)
}

// Startup: the full estate load + content-hash verification is well under
// the 5 s budget.
#[test]
fn estate_load_stays_under_the_startup_budget() {
    let _quiet = quiet_core();
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

// Holding the guard across await is THE mechanism (serialize the whole
// benchmark); each #[tokio::test] is a single-task current-thread runtime,
// so the deadlock this lint guards against cannot occur here.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn warm_p99_on_the_grown_estate_stays_under_100ms() {
    let _quiet = quiet_core();
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

// S5a: the RETRIEVE-HEAVY 1,000-mix — the exact shape that produced the
// S3 34.5ms p99 before the estate got an ingest-built index. Target < 10ms
// (hard gate stays < 100ms); this proves the O(corpus) cliff is gone (query
// time no longer re-tokenizes 750 bodies).
// Same single-task-runtime justification as the warm_p99 benchmark above.
#[allow(clippy::await_holding_lock)]
#[tokio::test]
async fn retrieve_heavy_warm_p99_after_indexing() {
    let _quiet = quiet_core();
    let dir = scratch("estate-latency-retrieve");
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
    let queries = [
        "supplier audit",
        "cold chain transit",
        "invoice matching",
        "site notice",
        "returns reconciliation",
        "budget variance",
    ];

    // Warm-up.
    for _ in 0..5 {
        let body = json!({ "query": queries[0] }).to_string();
        let _ = router
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
    }

    let mut durations: Vec<Duration> = Vec::with_capacity(1_000);
    for i in 0..1_000usize {
        let body = json!({ "query": queries[i % queries.len()], "top_k": 20 }).to_string();
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
        "S5a retrieve-heavy latency (1000 estate retrieves): p50 {:?}, p95 {:?}, p99 {:?} \
         (was 34.5ms before the ingest index; target < 10ms)",
        p50, p95, p99
    );
    assert!(
        p99 < Duration::from_millis(10),
        "S5a retrieve p99 target < 10ms, measured {p99:?}"
    );
}
