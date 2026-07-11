//! S1-7: the latency gates. Warm path: p99 < 100 ms over a 1,000-request
//! mix (800 document fetches, mixed allow/deny, + 200 retrieves) — the
//! release gate that keeps enforcement too fast to be worth disabling.
//! Cold path: a fresh HttpJwks with a 50 ms fetch delay under 32 concurrent
//! first requests must complete in < 2 s with no stall — the cache mutex
//! single-flights the fetch (one fetch total) and the blocking work runs on
//! `spawn_blocking`, so the async runtime never stalls.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_AUDIENCE, TEST_TENANT};
use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::agent_bridge::jwks::HttpJwks;
use service::agent_bridge::{AgentBridgeConfig, Bridge, RegisteredAgent, Registry, TokenValidator};
use service::{app, AppState};
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

/// (allow_docs, deny_docs) for the finance agent, from the raw oracle.
fn oracle_split() -> (Vec<String>, Vec<String>) {
    let path = common::repo_fixtures_dir().join("ground_truth.jsonl");
    let text = fs::read_to_string(path).expect("ground truth");
    let mut allows = Vec::new();
    let mut denies = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("row");
        if row["principal_id"] == "agent_finance_analyst" {
            let doc = row["resource_id"]
                .as_str()
                .expect("resource_id")
                .to_string();
            if row["decision"] == "ALLOW" {
                allows.push(doc);
            } else {
                denies.push(doc);
            }
        }
    }
    (allows, denies)
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let index = ((sorted.len() as f64) * p).ceil() as usize;
    sorted[index.saturating_sub(1).min(sorted.len() - 1)]
}

// F15: the warm gate. p99 < 100 ms over 800 document fetches (mixed
// allow/deny) + 200 retrieves, everything warm (JWKS from file, first
// request excluded by a warm-up).
#[tokio::test]
async fn warm_p99_stays_under_100ms() {
    let dir = scratch("v1-latency-warm");
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
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = base_state()
        .with_proposals(store)
        .with_agent_bridge(Arc::new(
            Bridge::from_config(&config).expect("bridge builds"),
        ));
    let router = app(Arc::new(state));

    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let (allows, denies) = oracle_split();
    let queries = [
        "temperature range storage procedure",
        "payroll salary review",
        "site stock value report",
        "supplier invoice payment terms",
        "quarterly budget summary",
        "customer account credit terms",
        "goods despatch picking note",
        "cold chain transit hours",
    ];

    // Warm-up: first requests pay one-off costs (key build, index mmap).
    for _ in 0..5 {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/documents/{}", allows[0]))
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let mut durations: Vec<Duration> = Vec::with_capacity(1_000);
    // 800 document fetches, alternating allow / deny.
    for i in 0..800usize {
        let doc = if i % 2 == 0 {
            &allows[(i / 2) % allows.len()]
        } else {
            &denies[(i / 2) % denies.len()]
        };
        let request = Request::builder()
            .method("GET")
            .uri(format!("/v1/documents/{doc}"))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request");
        let started = Instant::now();
        let response = router.clone().oneshot(request).await.expect("response");
        durations.push(started.elapsed());
        assert!(
            response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND,
            "unexpected status {}",
            response.status()
        );
    }
    // 200 retrieves.
    for i in 0..200usize {
        let body =
            serde_json::to_string(&json!({ "query": queries[i % queries.len()] })).expect("json");
        let request = Request::builder()
            .method("POST")
            .uri("/v1/retrieve")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .expect("request");
        let started = Instant::now();
        let response = router.clone().oneshot(request).await.expect("response");
        durations.push(started.elapsed());
        assert_eq!(response.status(), StatusCode::OK);
    }

    durations.sort();
    let p50 = percentile(&durations, 0.50);
    let p95 = percentile(&durations, 0.95);
    let p99 = percentile(&durations, 0.99);
    println!(
        "S1 warm latency over {} requests (800 documents mixed allow/deny + 200 retrieves): \
         p50 {:?}, p95 {:?}, p99 {:?}",
        durations.len(),
        p50,
        p95,
        p99
    );
    assert!(
        p99 < Duration::from_millis(100),
        "S1-7 release gate: warm p99 must stay under 100ms, measured {p99:?}"
    );
}

// F16: the cold-JWKS scenario. A fresh HttpJwks, a 50 ms injected fetch
// delay, 32 CONCURRENT first requests: all must complete in < 2 s, the
// runtime must not stall, and the cache mutex single-flights the fetch
// (exactly ONE fetch for all 32).
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn cold_jwks_32_concurrent_first_requests_complete_fast() {
    let dir = scratch("v1-latency-cold");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));

    let fetches = Arc::new(AtomicUsize::new(0));
    let counter = fetches.clone();
    let jwks_body = jwt::issuer().jwks_json.clone();
    let jwks = HttpJwks::with_fetcher(
        "https://tenant.example/discovery/v2.0/keys",
        Duration::from_secs(86_400),
        Box::new(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(50));
            Ok(jwks_body.clone())
        }),
    );
    let validator = TokenValidator::new(
        TEST_TENANT,
        TEST_AUDIENCE,
        &["RS256".to_string()],
        Box::new(jwks),
    )
    .expect("validator");
    let registry = Registry::from_entries(&[RegisteredAgent {
        tid: TEST_TENANT.to_string(),
        oid: FINANCE_OID.to_string(),
        principal: "agent_finance_analyst".to_string(),
    }])
    .expect("registry");
    let state = base_state()
        .with_proposals(store)
        .with_agent_bridge(Arc::new(Bridge::from_parts(validator, registry)));
    let router = app(Arc::new(state));

    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let started = Instant::now();
    let mut handles = Vec::with_capacity(32);
    for _ in 0..32 {
        let router = router.clone();
        let token = token.clone();
        handles.push(tokio::spawn(async move {
            let response = router
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/v1/whoami")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .body(Body::empty())
                        .expect("request"),
                )
                .await
                .expect("response");
            response.status()
        }));
    }
    for handle in handles {
        let status = handle.await.expect("task");
        assert_eq!(status, StatusCode::OK, "every cold first request completes");
    }
    let elapsed = started.elapsed();
    let fetch_count = fetches.load(Ordering::SeqCst);
    println!(
        "S1 cold JWKS: 32 concurrent first requests in {:?} with {} fetch(es) \
         (50ms injected delay; blocking fetch on spawn_blocking, single-flighted \
         by the cache mutex)",
        elapsed, fetch_count
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "32 concurrent cold requests must complete in < 2s, took {elapsed:?}"
    );
    assert_eq!(
        fetch_count, 1,
        "the cache mutex single-flights the cold fetch — one fetch serves all 32"
    );
}
