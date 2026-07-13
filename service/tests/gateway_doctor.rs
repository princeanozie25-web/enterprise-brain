//! S5a: `gateway doctor` — one test per check in both states (healthy
//! fixture world all-✓; each induced fault produces its named ✗ and exit 1);
//! a broken-chain ledger caught; a ghost registration caught; --json pinned.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::clock::{Clock, FixedClock};
use service::doctor::{run, DoctorInputs};

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

fn base_inputs(config: Option<PathBuf>, state_dir: Option<PathBuf>) -> DoctorInputs {
    DoctorInputs {
        fixtures: common::repo_fixtures_dir(),
        artifacts: repo_root().join("compiler").join("artifacts"),
        idx: repo_root().join("retrieval").join("idx"),
        config,
        state_dir,
    }
}

fn check<'a>(report: &'a service::doctor::DoctorReport, name: &str) -> &'a service::doctor::Check {
    report
        .checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check named {name}: {:?}", report.checks))
}

// Healthy: no config, real fixtures + estate on disk -> all ✓ (config default,
// corpus loads, estate hashes verify + index builds).
#[test]
fn healthy_world_is_all_green() {
    let report = run(&base_inputs(None, None));
    assert!(report.all_ok(), "healthy world: {}", report.to_human());
    assert!(check(&report, "config").ok);
    assert!(check(&report, "estate").ok);
    let estate = &check(&report, "estate").detail;
    assert!(estate.contains("verify against the pinned hash") && estate.contains("index builds"));
}

// A fully-wired healthy config (bridge + ledger + alerting + estate) -> all ✓.
#[test]
fn fully_configured_healthy_world_is_all_green() {
    let dir = scratch("doctor-healthy-config");
    let jwks = dir.join("jwks.json");
    std::fs::write(&jwks, &common::jwt::issuer().jwks_json).expect("jwks");
    let ledger_dir = dir.join("ledger");
    std::fs::create_dir_all(&ledger_dir).unwrap();
    let config = json!({
        "agent_bridge": {
            "enabled": true,
            "tenant_id": common::jwt::TEST_TENANT,
            "audience": common::jwt::TEST_AUDIENCE,
            "jwks": { "file": jwks },
            "agents": [
                { "tid": common::jwt::TEST_TENANT,
                  "oid": "aaaa0003-5c1e-4a2b-9d3e-000000000a03",
                  "principal": "agent_finance_analyst" },
                { "tid": common::jwt::TEST_TENANT,
                  "oid": "bbbb0001-5c1e-4a2b-9d3e-000000000b01",
                  "principal": "agent_estate_confidential" }
            ]
        },
        "ledger": { "dir": ledger_dir.to_string_lossy().replace('\\', "/") },
        "alerting": { "enabled": true, "alerts_path": dir.join("alerts.jsonl").to_string_lossy().replace('\\', "/"),
                      "webhook_url": "https://alerts.example/hook" }
    });
    let config_path = dir.join("config.json");
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();

    let report = run(&base_inputs(Some(config_path), None));
    assert!(
        report.all_ok(),
        "fully-configured healthy world: {}",
        report.to_human()
    );
    // The bridge principals (a legacy agent + an estate agent) both resolve.
    assert!(check(&report, "bridge.registry").ok);
    assert!(check(&report, "bridge.jwks").ok);
    assert!(check(&report, "alerting.sink").ok);
    assert!(check(&report, "alerting.webhook_url").ok);
}

// Fault: malformed config (alerting enabled, no alerts_path) -> ✗ naming
// the field, and the whole run is not-ok.
#[test]
fn malformed_config_is_a_named_failure() {
    let dir = scratch("doctor-bad-config");
    let config_path = dir.join("bad.json");
    std::fs::write(
        &config_path,
        json!({ "alerting": { "enabled": true, "webhook_url": "https://x" } }).to_string(),
    )
    .unwrap();
    let report = run(&base_inputs(Some(config_path), None));
    assert!(!report.all_ok());
    let cfg = check(&report, "config");
    assert!(
        !cfg.ok && cfg.detail.contains("alerts_path"),
        "names the field: {}",
        cfg.detail
    );
}

// Fault: a ghost registration (a principal the identity model does not know)
// -> ✗ naming the ghost principal.
#[test]
fn ghost_registration_is_caught_and_named() {
    let dir = scratch("doctor-ghost");
    let jwks = dir.join("jwks.json");
    std::fs::write(&jwks, &common::jwt::issuer().jwks_json).expect("jwks");
    let config = json!({
        "agent_bridge": {
            "enabled": true,
            "tenant_id": common::jwt::TEST_TENANT,
            "audience": common::jwt::TEST_AUDIENCE,
            "jwks": { "file": jwks },
            "agents": [
                { "tid": common::jwt::TEST_TENANT, "oid": "1111",
                  "principal": "agent_does_not_exist" }
            ]
        }
    });
    let config_path = dir.join("config.json");
    std::fs::write(&config_path, config.to_string()).unwrap();
    let report = run(&base_inputs(Some(config_path), None));
    assert!(!report.all_ok());
    let reg = check(&report, "bridge.registry");
    assert!(!reg.ok, "ghost must fail");
    assert!(
        reg.detail.contains("agent_does_not_exist") && reg.detail.contains("ghost"),
        "names the ghost + the class: {}",
        reg.detail
    );
}

// Fault: a missing JWKS file -> ✗ naming the path.
#[test]
fn missing_jwks_file_is_caught() {
    let dir = scratch("doctor-jwks");
    let config = json!({
        "agent_bridge": {
            "enabled": true,
            "tenant_id": common::jwt::TEST_TENANT,
            "audience": common::jwt::TEST_AUDIENCE,
            "jwks": { "file": dir.join("nope.json").to_string_lossy().replace('\\', "/") },
            "agents": [{ "tid": common::jwt::TEST_TENANT, "oid": "1",
                         "principal": "agent_finance_analyst" }]
        }
    });
    let config_path = dir.join("config.json");
    std::fs::write(&config_path, config.to_string()).unwrap();
    let report = run(&base_inputs(Some(config_path), None));
    let jwks = check(&report, "bridge.jwks");
    assert!(!jwks.ok && jwks.detail.contains("not found"));
}

// Fault: a broken-chain ledger under the config's ledger dir -> ✗ with the
// breaking ordinal.
#[test]
fn broken_ledger_chain_is_caught() {
    let dir = scratch("doctor-broken-ledger");
    let ledger_dir = dir.join("ledger");
    // Write a chained ledger, then tamper it.
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(1_783_762_923_500, 1000));
    let store = ProposalStore::open_chained(&ledger_dir, clock).expect("store");
    for i in 0..4 {
        store
            .audit(
                "v1_document",
                "agent_a",
                &format!("GET /v1/documents/d{i:04}"),
                "not_found",
            )
            .expect("row");
    }
    drop(store);
    let audit = ledger_dir.join("audit.jsonl");
    let text = std::fs::read_to_string(&audit).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines[1] = lines[1].replace("agent_a", "agent_x");
    std::fs::write(&audit, lines.join("\n") + "\n").unwrap();

    let config = json!({ "ledger": { "dir": ledger_dir.to_string_lossy().replace('\\', "/") } });
    let config_path = dir.join("config.json");
    std::fs::write(&config_path, config.to_string()).unwrap();
    let report = run(&base_inputs(Some(config_path), None));
    let ledger = check(&report, "ledger");
    assert!(
        !ledger.ok && ledger.detail.contains("BREAKS at ordinal 2"),
        "{}",
        ledger.detail
    );
}

// The workflow store check catches a tampered wf_proposals.jsonl.
#[test]
fn broken_workflow_chain_is_caught() {
    let dir = scratch("doctor-broken-wf");
    let state_dir = dir.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    // A minimally chained workflow ledger line, then tamper it.
    let wf = state_dir.join("wf_proposals.jsonl");
    let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    // Two chained rows: row 1's prev = sha256(row 0 bytes). Forge row 0 so
    // the recomputed hash no longer matches row 1's prev.
    let row0 = format!(
        "{{\"event\":\"created\",\"prev\":\"{empty}\",\"ts\":\"2026-07-11T00:00:00.000Z\"}}\n"
    );
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(row0.as_bytes());
    let tip = h
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let row1 = format!(
        "{{\"event\":\"materialized\",\"prev\":\"{tip}\",\"ts\":\"2026-07-11T00:00:01.000Z\"}}\n"
    );
    // Tamper row0 AFTER computing tip, so the chain is broken.
    let tampered0 = row0.replace("created", "decided");
    std::fs::write(&wf, format!("{tampered0}{row1}")).unwrap();

    let report = run(&base_inputs(None, Some(state_dir)));
    let wf_check = check(&report, "workflow_store");
    assert!(
        !wf_check.ok && wf_check.detail.contains("BREAKS"),
        "{}",
        wf_check.detail
    );
}

// --json: schema pinned — {ok: bool, checks: [{name, ok, detail}]}.
#[test]
fn json_output_schema_is_pinned() {
    let report = run(&base_inputs(None, None));
    let value: Value = serde_json::from_str(&report.to_json()).expect("valid json");
    assert!(value["ok"].is_boolean());
    let checks = value["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty());
    for c in checks {
        assert!(c["name"].is_string());
        assert!(c["ok"].is_boolean());
        assert!(c["detail"].is_string());
        // Never a secret: the doctor's own probe never emits key material,
        // and no field carries a token.
        assert!(!c["detail"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("bearer "));
    }
}
