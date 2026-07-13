//! S2b Part 2: the `/v1` decision ledger is wired from SERVICE CONFIG
//! (`ledger.dir`) with no dependency on the M4 `--agents-config` flag.
//! The invariant is unchanged — no ledger ⇒ no `/v1` — only WHICH
//! configuration brings the ledger to life moved. Same store type, same
//! file format, byte-identical rows.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_AUDIENCE, TEST_TENANT};
use serde_json::{json, Value};
use service::{app, AppState, ServiceConfig};
use tower::ServiceExt;

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

/// A ServiceConfig FILE (the same artifact `--config` takes), with the
/// bridge section and optionally the S2b ledger section — and NOTHING
/// from the M4 --agents-config path.
fn config_file(dir: &Path, with_ledger: bool) -> PathBuf {
    let jwks_path = dir.join("jwks.json");
    fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
    let mut config = json!({
        "profile": "S2b ledger-wiring test world",
        "agent_bridge": {
            "enabled": true,
            "tenant_id": TEST_TENANT,
            "audience": TEST_AUDIENCE,
            "jwks": { "file": jwks_path },
            "agents": AGENTS
                .iter()
                .map(|(agent, oid)| json!({
                    "tid": TEST_TENANT, "oid": oid, "principal": agent
                }))
                .collect::<Vec<_>>(),
        },
    });
    if with_ledger {
        config["ledger"] = json!({ "dir": dir.join("ledger").as_posix_lossy() });
    }
    let path = dir.join("service-config.json");
    fs::write(&path, serde_json::to_vec_pretty(&config).expect("json")).expect("write config");
    path
}

trait AsPosixLossy {
    fn as_posix_lossy(&self) -> String;
}

impl AsPosixLossy for PathBuf {
    fn as_posix_lossy(&self) -> String {
        self.to_string_lossy().replace('\\', "/")
    }
}

async fn get_with_bearer(router: &axum::Router, uri: &str, bearer: &str) -> StatusCode {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response")
        .status()
}

// Test 7: config with bridge + ledger, NO --agents-config anywhere:
// /v1 fully live — whoami for all four agents, spot allow/deny green,
// rows written to the config-named ledger dir.
#[tokio::test]
async fn config_ledger_brings_v1_to_life_without_agents_config() {
    let dir = scratch("v1-ledger-decoupled");
    let config_path = config_file(&dir, true);
    // The SAME loading path the binary uses: ServiceConfig::load + apply.
    // No with_proposals, no with_agents — nothing from the M4 path.
    let config = ServiceConfig::load(&config_path).expect("config loads");
    let state = config.apply(base_state()).expect("config applies");
    assert!(
        state.agents.is_none(),
        "no M4 registry was wired — the decoupling is real"
    );
    let router = app(Arc::new(state));

    for (agent, oid) in AGENTS {
        let token = TokenSpec::autonomous(oid).sign();
        let response = router
            .clone()
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
        assert_eq!(response.status(), StatusCode::OK, "{agent} whoami");
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(payload["principal_id"], agent);
    }

    // Spot allow/deny: finance's oracle-first allow serves; d0001 (outside
    // finance's scope) is THE 404.
    let finance = TokenSpec::autonomous(AGENTS[2].1).sign();
    let text = fs::read_to_string(common::repo_fixtures_dir().join("ground_truth.jsonl"))
        .expect("ground truth");
    let allowed = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("row"))
        .find(|row| row["principal_id"] == "agent_finance_analyst" && row["decision"] == "ALLOW")
        .and_then(|row| row["resource_id"].as_str().map(str::to_string))
        .expect("an allowed doc");
    assert_eq!(
        get_with_bearer(&router, &format!("/v1/documents/{allowed}"), &finance).await,
        StatusCode::OK
    );
    assert_eq!(
        get_with_bearer(&router, "/v1/documents/d0001", &finance).await,
        StatusCode::NOT_FOUND
    );

    // Rows landed in the CONFIG-named ledger dir.
    let ledger = fs::read_to_string(dir.join("ledger").join("audit.jsonl")).expect("ledger");
    assert!(ledger.contains("\"action\":\"v1_whoami\""));
    assert!(ledger.contains("\"action\":\"v1_document\""));
    assert!(ledger.contains("\"payload\":\"full\""));
}

// Test 8: same config WITHOUT the ledger section: every /v1 request is
// the generic 401 — the no-ledger ⇒ no-machine-surface invariant, now
// enforced through the new wiring.
#[tokio::test]
async fn config_without_ledger_keeps_v1_dark() {
    let dir = scratch("v1-ledger-absent");
    let config_path = config_file(&dir, false);
    let config = ServiceConfig::load(&config_path).expect("config loads");
    let state = config.apply(base_state()).expect("config applies");
    let router = app(Arc::new(state));

    let token = TokenSpec::autonomous(AGENTS[0].1).sign();
    for uri in ["/v1/whoami", "/v1/documents/d0001"] {
        let response = router
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
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        let payload: Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            payload["error"], "authentication required",
            "no ledger, no /v1 — and no explanation on the wire"
        );
    }
}
