//! S5b: `bootstrap-dev` — the generated demo world is COMPLETE, its tokens
//! validate through the REAL ladder (not a test-only relaxation), no key
//! material is ever tracked (S5b-1), the doctor passes on it (the container
//! healthcheck), and — the acceptance proof — the six minted tokens drive the
//! real router: whoami ×6 and the tier seam (a confidential estate object one
//! agent reads and another cannot).
//!
//! S5c (verdict condition): the default path is NON-DESTRUCTIVE — a complete
//! world is an untouched no-op (byte-identical keys, ledger intact), a partial
//! world is an error naming the missing files, `--force` is the only
//! destructive path, and the compose one-shot runs WITHOUT --force (standing).

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::Value;
use service::agent_bridge::{Bridge, BridgeOutcome};
use service::bootstrap::{bootstrap_dev, BootstrapOutcome, BootstrapOutput};
use service::doctor::{run, DoctorInputs};
use service::{app, AppState, ServiceConfig};
use tower::ServiceExt;

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

/// The token minted for one principal in a generated world.
fn token_for<'a>(output: &'a BootstrapOutput, principal: &str) -> &'a str {
    output
        .tokens
        .iter()
        .find(|(p, _)| p == principal)
        .map(|(_, t)| t.as_str())
        .unwrap_or_else(|| panic!("no token for {principal}"))
}

/// Bootstrap and assert the run actually CREATED a world (not the S5c
/// untouched no-op) — what every "fresh world" test in this file means.
fn mint(out: &Path, force: bool, container: bool) -> BootstrapOutput {
    match bootstrap_dev(out, force, container).expect("bootstrap") {
        BootstrapOutcome::Created(output) => output,
        BootstrapOutcome::Untouched { out_dir } => panic!(
            "expected a fresh world, found an untouched one at {}",
            out_dir.display()
        ),
    }
}

/// Snapshot the bytes of every world file (plus the ledger) — the
/// byte-identity oracle for the non-destructive default.
fn world_bytes(out: &Path) -> Vec<(String, Vec<u8>)> {
    [
        "config.json",
        "jwks.json",
        "private_key.pem",
        "tokens.json",
        "ledger/audit.jsonl",
    ]
    .iter()
    .map(|name| {
        let bytes = std::fs::read(out.join(name))
            .unwrap_or_else(|e| panic!("world file {name} unreadable: {e}"));
        (name.to_string(), bytes)
    })
    .collect()
}

// -- 1. A fresh world is complete, and its tokens validate through the REAL
//       bridge ladder (the same TokenValidator + Registry production runs).
#[test]
fn fresh_world_is_complete_and_tokens_validate_through_the_real_ladder() {
    let out = scratch("bootstrap-fresh").join("dev-out");
    let output = mint(&out, false, false);

    assert!(output.config_path.exists(), "config.json");
    assert!(output.jwks_path.exists(), "jwks.json");
    assert!(output.private_key_path.exists(), "private_key.pem");
    assert!(output.tokens_path.exists(), "tokens.json");
    assert_eq!(output.tokens.len(), 6, "six demo agents");

    // The config the demo ships: bridge ENABLED, four registrations, and a
    // profile that LOUDLY says DEMO (JSON has no comments; the schema is
    // deny_unknown_fields, so the label lives in `profile`).
    let cfg =
        ServiceConfig::load(&output.config_path).expect("config loads through the real schema");
    assert!(
        cfg.profile.as_deref().unwrap_or_default().contains("DEMO"),
        "the generated config is labelled DEMO"
    );
    let bridge_cfg = cfg.agent_bridge.expect("bridge section present");
    assert!(bridge_cfg.enabled, "demo config enables the bridge");
    assert_eq!(bridge_cfg.agents.len(), 6);

    // The strong proof: build the REAL bridge from the generated config and
    // authenticate every minted token end-to-end. Each resolves to exactly
    // its principal — no test shim, the production ladder.
    let bridge = Bridge::from_config(&bridge_cfg).expect("bridge builds from the generated config");
    for (principal, token) in &output.tokens {
        match bridge.authenticate(token) {
            BridgeOutcome::Resolved { principal: got, .. } => {
                assert_eq!(&got, principal, "token resolves to its principal");
            }
            BridgeOutcome::Denied { reason, .. } => {
                panic!(
                    "{principal} denied through the real ladder: {}",
                    reason.as_str()
                )
            }
        }
    }

    // Shape sanity: a real private-key PEM, and compact JWS tokens.
    let pem = std::fs::read_to_string(&output.private_key_path).unwrap();
    assert!(pem.contains("PRIVATE KEY"), "a real PEM");
    for (_, token) in &output.tokens {
        assert_eq!(
            token.split('.').count(),
            3,
            "compact JWS: header.payload.signature"
        );
    }
}

// -- 2. S5c THE CONDITION: the default path is NON-DESTRUCTIVE. A second run
//       without --force on a COMPLETE world is a no-op that exits 0 — keys,
//       tokens and config stay BYTE-IDENTICAL and a live ledger is untouched.
//       The audit-wipes-evidence class (`compose up` re-running the one-shot
//       over a world in use) is structurally impossible, not flag-guarded.
#[test]
fn double_run_without_force_is_a_noop_world_byte_identical() {
    let out = scratch("bootstrap-noop").join("dev-out");
    mint(&out, false, false);

    // Live evidence: a ledger row the OLD semantics (--force in compose)
    // would have deleted on the next cycle.
    std::fs::write(
        out.join("ledger").join("audit.jsonl"),
        "{\"act\":\"evidence-of-use\"}\n",
    )
    .expect("seed the ledger");

    let before = world_bytes(&out);
    let again = bootstrap_dev(&out, false, false).expect("a complete world is exit 0, not an error");
    assert!(
        matches!(again, BootstrapOutcome::Untouched { .. }),
        "the second run reports the world untouched"
    );
    assert_eq!(
        before,
        world_bytes(&out),
        "byte-identical after the no-op: keys, tokens, config, AND the ledger"
    );
}

// -- 2b. A PARTIAL world (some files missing) is a NAMED error — never a
//        silent partial overwrite: the survivors keep their bytes and the
//        missing file is not quietly re-minted.
#[test]
fn partial_world_is_refused_naming_the_missing_files() {
    let out = scratch("bootstrap-partial").join("dev-out");
    mint(&out, false, false);
    std::fs::remove_file(out.join("tokens.json")).expect("drop tokens.json");
    let jwks_before = std::fs::read(out.join("jwks.json")).unwrap();

    // (No `expect_err`: BootstrapOutcome deliberately has no Debug impl —
    // its Created arm carries token material that must never hit a log.)
    let err = match bootstrap_dev(&out, false, false) {
        Err(err) => err,
        Ok(_) => panic!("a partial world must refuse"),
    };
    let msg = format!("{err:#}");
    assert!(msg.contains("tokens.json"), "names the missing file: {msg}");
    assert!(msg.contains("--force"), "points at the deliberate path: {msg}");

    assert_eq!(
        jwks_before,
        std::fs::read(out.join("jwks.json")).unwrap(),
        "the refused run overwrote nothing"
    );
    assert!(
        !out.join("tokens.json").exists(),
        "and did not quietly re-mint the missing file"
    );
}

// -- 3. --force — the ONLY destructive path — regenerates a clean,
//       still-valid world (fresh keys each time).
#[test]
fn force_regenerates_a_clean_world() {
    let out = scratch("bootstrap-force").join("dev-out");
    let first = mint(&out, false, false);
    let first_token = token_for(&first, "agent_finance_analyst").to_string();

    let second = mint(&out, true, false);
    assert_eq!(second.tokens.len(), 6);
    assert_ne!(
        first_token,
        token_for(&second, "agent_finance_analyst"),
        "--force mints a fresh key, so the signed token differs"
    );

    // The regenerated world still validates through the real ladder.
    let cfg = ServiceConfig::load(&second.config_path).unwrap();
    let bridge = Bridge::from_config(&cfg.agent_bridge.unwrap()).unwrap();
    for (principal, token) in &second.tokens {
        assert!(
            matches!(bridge.authenticate(token), BridgeOutcome::Resolved { principal: ref p, .. } if p == principal),
            "{principal} re-validates after --force"
        );
    }
}

// -- 4. S5b-1: no key material or token file is EVER tracked in git.
#[test]
fn no_key_material_is_tracked() {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(["ls-files"])
        .output()
        .expect("git ls-files runs");
    assert!(output.status.success(), "git ls-files failed");
    let tracked = String::from_utf8_lossy(&output.stdout);
    for line in tracked.lines() {
        let lower = line.to_ascii_lowercase();
        assert!(!lower.ends_with(".pem"), "a key file is tracked: {line}");
        assert!(
            !lower.ends_with("tokens.json"),
            "a token file is tracked: {line}"
        );
        assert!(
            !lower.contains("private_key"),
            "a private key is tracked: {line}"
        );
    }
}

// -- 5. The doctor passes on the generated config (all ✓ — the container
//       healthcheck), and an induced JWKS fault is a NAMED ✗.
#[test]
fn doctor_passes_on_generated_config_and_names_an_induced_fault() {
    let out = scratch("bootstrap-doctor").join("dev-out");
    let output = mint(&out, false, false);
    let inputs = DoctorInputs {
        fixtures: common::repo_fixtures_dir(),
        artifacts: repo_root().join("compiler").join("artifacts"),
        idx: repo_root().join("retrieval").join("idx"),
        config: Some(output.config_path.clone()),
        state_dir: None,
    };

    let healthy = run(&inputs);
    assert!(
        healthy.all_ok(),
        "doctor on the generated demo config:\n{}",
        healthy.to_human()
    );

    // Induced fault: point the JWKS at nothing. Doctor names bridge.jwks.
    let mut cfg: Value =
        serde_json::from_str(&std::fs::read_to_string(&output.config_path).unwrap()).unwrap();
    cfg["agent_bridge"]["jwks"]["file"] = Value::String(
        out.join("does-not-exist.json")
            .to_string_lossy()
            .replace('\\', "/"),
    );
    std::fs::write(
        &output.config_path,
        serde_json::to_string_pretty(&cfg).unwrap(),
    )
    .unwrap();

    let broken = run(&inputs);
    assert!(!broken.all_ok(), "a missing JWKS must fail the preflight");
    let jwks = broken
        .checks
        .iter()
        .find(|c| c.name == "bridge.jwks")
        .expect("bridge.jwks check");
    assert!(
        !jwks.ok && jwks.detail.to_lowercase().contains("not found"),
        "names the missing JWKS: {}",
        jwks.detail
    );
}

// -- 6. THE ACCEPTANCE PROOF: the six minted tokens drive the REAL router,
//       wired from the generated config exactly as production wires it.
//       whoami ×6 -> 200; the tier seam holds (confidential agent reads the
//       confidential object; the internal agent gets THE 404).
#[tokio::test]
async fn tokens_drive_the_real_router_whoami_and_the_seam() {
    let out = scratch("bootstrap-router").join("dev-out");
    let output = mint(&out, false, false);

    // Build the state production builds: fixtures + people + estate, then the
    // generated config APPLIED (bridge + ledger + alerting from the file).
    let fixtures = common::repo_fixtures_dir();
    let state = AppState::build(
        &fixtures,
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("people")
    .with_estate_from(&fixtures.join("estate"))
    .expect("estate");
    let cfg = ServiceConfig::load(&output.config_path).expect("config");
    let state = cfg.apply(state).expect("apply generated config");
    let router = app(Arc::new(state));

    // whoami ×6 -> 200, echoing the resolved principal.
    for (principal, token) in &output.tokens {
        let (status, body) = send(&router, "/v1/whoami", token).await;
        assert_eq!(status, StatusCode::OK, "{principal} whoami");
        assert!(
            body.contains(principal),
            "whoami echoes {principal}: {body}"
        );
    }

    // The seam: one confidential estate object, two agents.
    let doc = "/v1/documents/s3/finance-restricted/2026/q1/budget-variance-ashcombe.md";
    let (conf_status, conf_body) = send(
        &router,
        doc,
        token_for(&output, "agent_estate_confidential"),
    )
    .await;
    assert_eq!(conf_status, StatusCode::OK, "confidential agent reads it");
    assert!(
        conf_body.contains("\"source\":\"s3\""),
        "the estate object is tagged source s3: {conf_body}"
    );

    let (intl_status, _) = send(&router, doc, token_for(&output, "agent_estate_internal")).await;
    assert_eq!(
        intl_status,
        StatusCode::NOT_FOUND,
        "the internal agent gets THE 404 — the access model decided, not the document"
    );
}

// -- 7. S5b bind amendment: --container sets an explicit wide bind AND says
//       why it is safe; the native world never carries a `bind` key at all
//       (the loopback invariant untouched by omission).
#[test]
fn container_mode_binds_wide_and_says_why_native_stays_loopback() {
    let dir = scratch("bootstrap-bind");
    let native = mint(&dir.join("native"), false, false);
    let container = mint(&dir.join("container"), false, true);

    // Native: NO bind key — the default (loopback_listener) path.
    let native_cfg = ServiceConfig::load(&native.config_path).expect("native config");
    assert!(
        native_cfg.bind.is_none(),
        "the native demo config must not carry a bind key"
    );

    // Container: bind 0.0.0.0:8787, and the profile states the compose
    // host-loopback mapping that makes it safe.
    let container_cfg = ServiceConfig::load(&container.config_path).expect("container config");
    assert_eq!(
        container_cfg.bind.as_deref(),
        Some("0.0.0.0:8787"),
        "the container demo config binds wide, explicitly"
    );
    let profile = container_cfg.profile.unwrap_or_default();
    assert!(
        profile.contains("127.0.0.1:8787:8787"),
        "the generated file states WHY 0.0.0.0 is safe (the compose mapping): {profile}"
    );
}

// -- 8. STANDING (1.4): every published port in docker-compose.yml is
//       host-loopback. An unqualified mapping ("8787:8787") would expose the
//       gateway to the network and fails the build forever.
#[test]
fn compose_ports_are_host_loopback() {
    let compose = std::fs::read_to_string(repo_root().join("docker-compose.yml"))
        .expect("docker-compose.yml at the repo root");
    // Hand-rolled scan (no yaml dep): a `ports:` line opens a block; the
    // block's `- ` list items are the mappings; any other line closes it.
    let mut in_ports = false;
    let mut mappings = Vec::new();
    for line in compose.lines() {
        let trimmed = line.trim();
        if trimmed == "ports:" {
            in_ports = true;
            continue;
        }
        if in_ports {
            if let Some(item) = trimmed.strip_prefix("- ") {
                mappings.push(item.trim_matches(['"', '\'']).to_string());
                continue;
            }
            if !trimmed.starts_with('#') && !trimmed.is_empty() {
                in_ports = false;
            }
        }
    }
    assert!(
        !mappings.is_empty(),
        "the gateway publishes a host-loopback port (none found — did ports: move?)"
    );
    for mapping in &mappings {
        assert!(
            mapping.starts_with("127.0.0.1:"),
            "compose port mapping {mapping:?} is not host-loopback; \
             every published port must carry the 127.0.0.1: prefix"
        );
    }
}

// -- 9. STANDING (S5c): the compose bootstrap one-shot runs WITHOUT --force.
//       With the non-destructive default, this line is exactly what makes
//       repeated `docker compose up` / `compose run` cycles unable to rotate
//       keys or reset ledgers; a --force here would resurrect the footgun.
#[test]
fn compose_bootstrap_one_shot_is_not_forced() {
    let compose = std::fs::read_to_string(repo_root().join("docker-compose.yml"))
        .expect("docker-compose.yml at the repo root");
    let bootstrap_cmd = compose
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .find(|line| line.contains("bootstrap-dev"))
        .expect("the bootstrap one-shot command is in docker-compose.yml");
    assert!(
        !bootstrap_cmd.contains("--force"),
        "the compose bootstrap one-shot must NOT pass --force — the \
         non-destructive default is the point: {bootstrap_cmd}"
    );
}

/// GET `uri` with a Bearer token through the router; return (status, body).
async fn send(router: &axum::Router, uri: &str, bearer: &str) -> (StatusCode, String) {
    let response = router
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
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&bytes).to_string())
}
