//! S4 Part 4 — THE DONE-CRITERIA TEST. A denied access attempt surfaces as a
//! structured alert within 5 seconds carrying: agent identity, resource
//! attempted, decision basis, timestamp. Proven in the two-source estate:
//! `agent_estate_internal` attempts the confidential doc in EACH source →
//! both denies alert; agent A's allowed fetches alert ZERO times; a garbage
//! token (auth-ladder deny) alerts ZERO times; ledger rows and alerts
//! reconcile 1:1 by ordinal. Plus: alerting is OFF the request path (a
//! black-hole webhook does not touch request latency).

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_AUDIENCE, TEST_TENANT};
use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::agent_bridge::{AgentBridgeConfig, Bridge};
use service::alerts::{AlertDispatcher, UreqWebhook, WebhookSender};
use service::clock::{Clock, FixedClock};
use service::{app, AppState};
use tower::ServiceExt;

const START_MS: u64 = 1_783_762_923_500; // 2026-07-11T09:42:03.500Z
const FROZEN_TS: &str = "2026-07-11T09:42:03.500Z";

const AGENT_A: (&str, &str) = (
    "agent_estate_confidential",
    "bbbb0001-5c1e-4a2b-9d3e-000000000b01",
);
const AGENT_B: (&str, &str) = (
    "agent_estate_internal",
    "bbbb0002-5c1e-4a2b-9d3e-000000000b02",
);

/// A recording webhook — the "test receiver". Captures every delivered body.
struct RecordingWebhook(Arc<Mutex<Vec<Value>>>);
impl WebhookSender for RecordingWebhook {
    fn send(&self, _url: &str, body: &[u8]) -> anyhow::Result<()> {
        self.0.lock().unwrap().push(serde_json::from_slice(body)?);
        Ok(())
    }
}

/// A black-hole webhook — hangs long past any request measurement, proving
/// the request path never waits on the sink.
struct BlackHoleWebhook;
impl WebhookSender for BlackHoleWebhook {
    fn send(&self, _url: &str, _body: &[u8]) -> anyhow::Result<()> {
        std::thread::sleep(Duration::from_secs(30));
        Ok(())
    }
}

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
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Build a world with the estate, a CHAINED ledger, and an alert dispatcher
/// (file sink + the given webhook). Returns (router, alerts_path,
/// ledger_path).
fn alerting_world(dir: &Path, webhook: Arc<dyn WebhookSender>) -> (axum::Router, PathBuf, PathBuf) {
    let jwks_path = dir.join("jwks.json");
    std::fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
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

    let state_dir = dir.join("state");
    // Frozen clock: the ledger row's ts and the alert's ts coincide exactly.
    let ledger_clock: Arc<dyn Clock> = Arc::new(FixedClock::frozen(START_MS));
    let store = Arc::new(ProposalStore::open_chained(&state_dir, ledger_clock).expect("store"));
    let alerts_path = dir.join("alerts.jsonl");
    let alert_clock: Arc<dyn Clock> = Arc::new(FixedClock::frozen(START_MS));
    let dispatcher = Arc::new(AlertDispatcher::new(
        alerts_path.clone(),
        Some("http://webhook.test/receive".to_string()),
        webhook,
        alert_clock,
    ));

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
    ))
    .with_alerts(dispatcher);
    (
        app(Arc::new(state)),
        alerts_path,
        state_dir.join("audit.jsonl"),
    )
}

async fn get_doc(router: &axum::Router, doc: &str, bearer: &str) -> StatusCode {
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

fn confidential_in_each_source() -> (String, String) {
    let access = std::fs::read_to_string(estate_dir().join("s3-access.json")).expect("s3-access");
    let parsed: Value = serde_json::from_str(&access).expect("parse");
    let s3 = parsed["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["sensitivity"] == "confidential")
        .and_then(|o| o["doc_id"].as_str())
        .expect("confidential s3")
        .to_string();
    let docs = std::fs::read_to_string(common::repo_fixtures_dir().join("documents.json"))
        .expect("documents.json");
    let parsed: Value = serde_json::from_str(&docs).expect("parse");
    let primary = parsed["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["sensitivity"] == "confidential")
        .and_then(|d| d["id"].as_str())
        .expect("confidential primary")
        .to_string();
    (primary, s3)
}

fn read_jsonl(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect()
}

/// Poll the alert file sink until `want` alerts land or the 5 s gate
/// expires; returns (alerts, elapsed).
fn await_alerts(path: &Path, want: usize) -> (Vec<Value>, Duration) {
    let started = Instant::now();
    loop {
        let alerts = read_jsonl(path);
        if alerts.len() >= want {
            return (alerts, started.elapsed());
        }
        if started.elapsed() > Duration::from_secs(5) {
            return (alerts, started.elapsed());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[tokio::test]
async fn estate_deny_alert() {
    let dir = scratch("estate-deny-alert");
    let received = Arc::new(Mutex::new(Vec::new()));
    let (router, alerts_path, ledger_path) =
        alerting_world(&dir, Arc::new(RecordingWebhook(received.clone())));

    let (primary_conf, s3_conf) = confidential_in_each_source();
    let token_b = TokenSpec::autonomous(AGENT_B.1).sign();

    // Agent B (internal) attempts the confidential doc in EACH source.
    for doc in [&primary_conf, &s3_conf] {
        assert_eq!(get_doc(&router, doc, &token_b).await, StatusCode::NOT_FOUND);
    }

    // Within 5 seconds: two alerts in the file sink, measured for real.
    let (alerts, elapsed) = await_alerts(&alerts_path, 2);
    println!(
        "S4 estate_deny_alert: {} alerts in the file sink after {:?} (gate < 5s)",
        alerts.len(),
        elapsed
    );
    assert_eq!(alerts.len(), 2, "both denies alerted");
    assert!(elapsed < Duration::from_secs(5), "within the 5s gate");

    // Every done-criteria field present on each alert.
    let mut sources = std::collections::BTreeSet::new();
    for alert in &alerts {
        assert_eq!(alert["principal_id"], AGENT_B.0, "agent identity");
        assert!(alert["resource"].as_str().is_some(), "resource attempted");
        let basis = alert["decision_basis"].as_str().expect("decision basis");
        assert!(
            basis.contains("not in compiled scope"),
            "decision basis framing: {basis}"
        );
        assert_eq!(
            alert["ts"], FROZEN_TS,
            "timestamp present (and clock-injected)"
        );
        assert!(
            alert["ledger_ordinal"].is_u64(),
            "traceable to a ledger row"
        );
        assert_eq!(alert["claims"]["oid"], AGENT_B.1, "claims carried");
        sources.insert(alert["source"].as_str().unwrap().to_string());
    }
    assert_eq!(
        sources,
        ["primary", "s3"].into_iter().map(String::from).collect(),
        "the two denies span both sources"
    );

    // The webhook receiver got the same two alerts (best-effort, but here
    // the recorder never fails).
    let (webhook_alerts, _) = {
        // Give the webhook task a beat if it lags the file sink.
        let started = Instant::now();
        loop {
            let n = received.lock().unwrap().len();
            if n >= 2 || started.elapsed() > Duration::from_secs(5) {
                break (received.lock().unwrap().clone(), started.elapsed());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    assert_eq!(
        webhook_alerts.len(),
        2,
        "the webhook receiver got both alerts"
    );

    // Reconcile 1:1 with the ledger deny rows by ordinal.
    let ledger = read_jsonl(&ledger_path);
    let deny_ordinals: std::collections::BTreeSet<u64> = ledger
        .iter()
        .filter(|r| r["action"] == "v1_document" && r["outcome"] == "not_found")
        .map(|r| r["ordinal"].as_u64().unwrap())
        .collect();
    let alert_ordinals: std::collections::BTreeSet<u64> = alerts
        .iter()
        .map(|a| a["ledger_ordinal"].as_u64().unwrap())
        .collect();
    assert_eq!(
        deny_ordinals, alert_ordinals,
        "alerts reconcile 1:1 with ledger denies by ordinal"
    );
}

#[tokio::test]
async fn allowed_fetches_and_auth_denies_do_not_alert() {
    let dir = scratch("estate-no-alert");
    let received = Arc::new(Mutex::new(Vec::new()));
    let (router, alerts_path, _ledger) =
        alerting_world(&dir, Arc::new(RecordingWebhook(received.clone())));

    let (primary_conf, s3_conf) = confidential_in_each_source();
    let token_a = TokenSpec::autonomous(AGENT_A.1).sign();

    // Agent A is authorized for confidential in both sources → allows.
    assert_eq!(
        get_doc(&router, &primary_conf, &token_a).await,
        StatusCode::OK
    );
    assert_eq!(get_doc(&router, &s3_conf, &token_a).await, StatusCode::OK);

    // An auth-ladder deny (garbage token) → 401, NOT a policy deny.
    assert_eq!(
        get_doc(&router, &s3_conf, "ga.rb.age").await,
        StatusCode::UNAUTHORIZED
    );

    // Give any (erroneous) alert task time to land, then assert ZERO.
    std::thread::sleep(Duration::from_millis(300));
    let alerts = read_jsonl(&alerts_path);
    assert!(
        alerts.is_empty(),
        "allows and auth denies never alert: {alerts:?}"
    );
    assert!(
        received.lock().unwrap().is_empty(),
        "no webhook deliveries either"
    );
}

#[tokio::test]
async fn alerting_is_off_the_request_path() {
    // A black-hole webhook that hangs 30 s. If alerting were on the request
    // path, the deny would block on it; it must not.
    let dir = scratch("estate-alert-offpath");
    let (router, alerts_path, _ledger) = alerting_world(&dir, Arc::new(BlackHoleWebhook));

    let (_primary, s3_conf) = confidential_in_each_source();
    let token_b = TokenSpec::autonomous(AGENT_B.1).sign();

    let started = Instant::now();
    let status = get_doc(&router, &s3_conf, &token_b).await;
    let request_elapsed = started.elapsed();
    assert_eq!(status, StatusCode::NOT_FOUND, "the deny is still served");
    println!(
        "S4 off-path: request completed in {:?} while the webhook black-holes for 30s",
        request_elapsed
    );
    assert!(
        request_elapsed < Duration::from_secs(1),
        "the request must not wait on the sink, took {request_elapsed:?}"
    );

    // The durable file sink still lands (the fsync is quick; the webhook is
    // what hangs, and only in its own task).
    let (alerts, _) = await_alerts(&alerts_path, 1);
    assert_eq!(
        alerts.len(),
        1,
        "the deny still alerts to the durable file sink"
    );
}

#[tokio::test]
async fn alert_emission_latency_under_5s_over_100_denies() {
    let dir = scratch("estate-alert-latency");
    let received = Arc::new(Mutex::new(Vec::new()));
    let (router, alerts_path, _ledger) = alerting_world(&dir, Arc::new(RecordingWebhook(received)));
    let (_primary, s3_conf) = confidential_in_each_source();
    let token_b = TokenSpec::autonomous(AGENT_B.1).sign();

    let started = Instant::now();
    for _ in 0..100 {
        assert_eq!(
            get_doc(&router, &s3_conf, &token_b).await,
            StatusCode::NOT_FOUND
        );
    }
    let (alerts, elapsed) = await_alerts(&alerts_path, 100);
    println!(
        "S4 alert latency: 100 denies -> {} alerts durable after {:?} (gate < 5s)",
        alerts.len(),
        elapsed
    );
    assert_eq!(alerts.len(), 100);
    assert!(
        elapsed < Duration::from_secs(5),
        "100 alerts must all land within the 5s gate, took {elapsed:?}"
    );
    let _ = started;
}

/// The production webhook type exists and is constructible (compile-level
/// assurance that the real sink path is wired, without making a network
/// call in tests).
#[test]
fn production_webhook_type_exists() {
    let _sender: Arc<dyn WebhookSender> = Arc::new(UreqWebhook);
}

// Config: a valid `alerting` section wires the dispatcher; a malformed one
// fails startup LOUDLY, naming the field.
#[test]
fn alerting_config_wires_or_fails_loud() {
    let dir = scratch("estate-alert-config");
    // Valid: enabled + alerts_path parses through the real ServiceConfig.
    let valid = dir.join("valid.json");
    std::fs::write(
        &valid,
        json!({ "alerting": { "enabled": true, "alerts_path": "alerts.jsonl" } }).to_string(),
    )
    .expect("write");
    let config = service::ServiceConfig::load(&valid).expect("valid alerting parses");
    assert!(config.alerting.is_some());

    // Malformed: enabled but no alerts_path — a loud, field-named failure.
    let bad = dir.join("bad.json");
    std::fs::write(
        &bad,
        json!({ "alerting": { "enabled": true, "webhook_url": "http://x" } }).to_string(),
    )
    .expect("write");
    let err = service::ServiceConfig::load(&bad).expect_err("malformed alerting must fail");
    let message = format!("{err:#}");
    assert!(
        message.contains("alerts_path"),
        "the failure names the missing field: {message}"
    );
}

// The warm-path latency gate with alerting ENABLED — the 1,000-request mix,
// agent B doing mixed allow/deny fetches (every deny fires an alert OFF the
// request path). p99 < 100ms, and the delta vs the 34.5ms S3 baseline is
// ~0 because alerting never touches the request.
#[tokio::test]
async fn warm_p99_with_alerting_enabled_stays_under_100ms() {
    let dir = scratch("estate-alert-warm");
    let received = Arc::new(Mutex::new(Vec::new()));
    let (router, _alerts_path, _ledger) =
        alerting_world(&dir, Arc::new(RecordingWebhook(received)));

    let token = TokenSpec::autonomous(AGENT_B.1).sign();
    // Agent B: public/internal s3 objects allow; confidential deny (alerts).
    let s3_ids: Vec<String> = {
        let access = std::fs::read_to_string(estate_dir().join("s3-access.json")).expect("s3");
        let parsed: Value = serde_json::from_str(&access).expect("parse");
        parsed["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["doc_id"].as_str().unwrap().to_string())
            .collect()
    };

    // Warm-up.
    for _ in 0..5 {
        let _ = get_doc(&router, &s3_ids[0], &token).await;
    }
    let mut durations: Vec<Duration> = Vec::with_capacity(1_000);
    for i in 0..1_000usize {
        let doc = &s3_ids[i % s3_ids.len()];
        let started = Instant::now();
        let status = get_doc(&router, doc, &token).await;
        durations.push(started.elapsed());
        assert!(status == StatusCode::OK || status == StatusCode::NOT_FOUND);
    }
    durations.sort();
    let p99 = durations[(durations.len() as f64 * 0.99).ceil() as usize - 1];
    let p50 = durations[durations.len() / 2];
    println!(
        "S4 warm latency (alerting ENABLED, 1000 mixed allow/deny fetches): \
         p50 {:?}, p99 {:?} (S3 baseline p99 34.5ms; alerting is off the path)",
        p50, p99
    );
    assert!(
        p99 < Duration::from_millis(100),
        "warm p99 with alerting must stay under 100ms, measured {p99:?}"
    );
}
