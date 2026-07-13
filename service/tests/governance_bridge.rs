//! S1 governance over the live router: the bridge's ONE surface is `/v1`.
//! Console routes are session-only — a machine credential on the human door
//! is refused generically and never consulted (S1-1); the bridge resolves a
//! registered agent into the EXISTING principal seam on `/v1` and every
//! token-path decision is ledgered before its effect (EB-6/EB-7); an
//! unledgerable surface does not serve (EB-4 × EB-6); the wire carries no
//! deny reasons anywhere (S1-6); the raw token is never at rest.
//!
//! (S0 history: these properties were first proven through the console
//! `/doc` route; the S1 surface split migrated every JWT-authenticated
//! assertion to `/v1` — the migration table lives in the S1 closeout.)

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_APP_ID, TEST_AUDIENCE, TEST_PARENT_APP, TEST_TENANT};
use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::agent_bridge::{AgentBridgeConfig, Bridge, DenyReason};
use service::{app, AppState};
use tower::ServiceExt;

const FINANCE_OID: &str = "dddd4444-0000-4000-8000-0000000000d4";
const PROBE_OID: &str = "eeee5555-0000-4000-8000-0000000000e5";

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
    let base = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
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

/// The bridge config the enabled worlds share: fixture JWKS on disk, the
/// finance agent registered, plus a probe registration whose principal the
/// identity model does not know (it must fail CLOSED downstream).
fn bridge_for(dir: &Path) -> Arc<Bridge> {
    let jwks_path = dir.join("jwks.json");
    fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
    let config: AgentBridgeConfig = serde_json::from_value(json!({
        "enabled": true,
        "tenant_id": TEST_TENANT,
        "audience": TEST_AUDIENCE,
        "jwks": { "file": jwks_path },
        "agents": [
            { "tid": TEST_TENANT, "oid": FINANCE_OID, "principal": "agent_finance_analyst" },
            { "tid": TEST_TENANT, "oid": PROBE_OID, "principal": "agent_probe_no_such_principal" }
        ]
    }))
    .expect("bridge config parses");
    Arc::new(Bridge::from_config(&config).expect("bridge builds"))
}

/// First (ALLOW, DENY) doc ids for a principal, straight from the raw
/// ground-truth oracle — never from the service's own projection.
fn oracle_docs_for(principal: &str) -> (String, String) {
    let path = common::repo_fixtures_dir().join("ground_truth.jsonl");
    let text = fs::read_to_string(path).expect("ground truth");
    let mut allow = None;
    let mut deny = None;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).expect("row");
        if row["principal_id"] == principal {
            match row["decision"].as_str() {
                Some("ALLOW") if allow.is_none() => {
                    allow = row["resource_id"].as_str().map(str::to_string)
                }
                Some("DENY") if deny.is_none() => {
                    deny = row["resource_id"].as_str().map(str::to_string)
                }
                _ => {}
            }
        }
        if allow.is_some() && deny.is_some() {
            break;
        }
    }
    (allow.expect("an allowed doc"), deny.expect("a denied doc"))
}

async fn get_with_bearer(router: &axum::Router, uri: &str, bearer: &str) -> (StatusCode, Value) {
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
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn audit_rows(dir: &Path) -> Vec<Value> {
    let text = fs::read_to_string(dir.join("state").join("audit.jsonl")).expect("ledger");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("audit row"))
        .collect()
}

// S1-1 (console side): the human surface is session-only. A machine
// credential — valid OR garbage, bridge present OR absent — is refused
// generically and NEVER consulted; with a valid session cookie alongside,
// the session authenticates and the machine credential plays no part.
#[tokio::test]
async fn console_is_session_only_and_never_consults_the_bridge() {
    // A ledger is present and the bridge is ENABLED — proving the console
    // refusal is a surface rule, not a disabled-bridge side effect.
    let dir = scratch("bridge-console-split");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let router = app(Arc::new(
        base_state()
            .with_proposals(store)
            .with_agent_bridge(bridge_for(&dir)),
    ));

    // A perfectly valid agent token on the console: generic 401 — and the
    // ledger records the class violation, NOT a ladder outcome (the token
    // was never validated, its claims never parsed).
    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let (status, body) = get_with_bearer(&router, "/doc/d0001", &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "authentication required");
    let rows = audit_rows(&dir);
    let class_denies: Vec<&Value> = rows
        .iter()
        .filter(|r| r["outcome"] == "jwt_on_console_surface")
        .collect();
    assert_eq!(
        class_denies.len(),
        1,
        "the knock on the human door is ledgered"
    );
    assert_eq!(class_denies[0]["action"], "agent_token");
    assert_eq!(class_denies[0]["actor_principal"], "unresolved");
    assert!(
        class_denies[0].get("token_oid").is_none(),
        "the refused credential was never parsed — no claims in the row"
    );

    // A dotless garbage bearer stays on the SESSION path — unchanged 401.
    let (status, body) = get_with_bearer(&router, "/doc/d0001", "not-a-session").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "authentication required");

    // A valid session COOKIE + a JWT bearer: SESSION SEMANTICS APPLY — the
    // request succeeds via the cookie and the bridge is never consulted
    // (no new agent_token row appears).
    let session = common::login_as(&router, "p060").await;
    let rows_before = audit_rows(&dir).len();
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/scope")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::COOKIE, format!("eb_session={session}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the session cookie authenticates; the machine credential plays no part"
    );
    assert_eq!(
        audit_rows(&dir).len(),
        rows_before,
        "the bridge was never consulted — no token-path row was written"
    );

    // The same pairing with a GARBAGE dotted bearer: still session
    // semantics (the dotted credential is not consulted, valid or not).
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/scope")
                .header(header::AUTHORIZATION, "Bearer ga.rb.age")
                .header(header::COOKIE, format!("eb_session={session}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);

    // A JWT in the session COOKIE slot is just an unknown session — 401.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/doc/d0001")
                .header(header::COOKIE, format!("eb_session={token}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // And the opaque session bearer still authenticates as always.
    let (status, _) = get_with_bearer(&router, "/doc/d0001", &session).await;
    assert_ne!(status, StatusCode::UNAUTHORIZED);
}

// The enabled world, on ITS surface: a registered agent resolves on /v1,
// scope decides exactly as the oracle says, and every decision lands in
// the ledger BEFORE its effect — claims-attributed, never the token.
#[tokio::test]
async fn enabled_bridge_resolves_scopes_and_audits_every_decision_on_v1() {
    let dir = scratch("bridge-governance-enabled");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = base_state()
        .with_proposals(store.clone())
        .with_agent_bridge(bridge_for(&dir));
    let router = app(Arc::new(state));

    let (allowed_doc, denied_doc) = oracle_docs_for("agent_finance_analyst");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();

    // Row 11 (the untouched engine) through the machine surface:
    // oracle-ALLOW serves; oracle-DENY gets THE one 404.
    let (status, body) =
        get_with_bearer(&router, &format!("/v1/documents/{allowed_doc}"), &token).await;
    assert_eq!(status, StatusCode::OK, "oracle-allowed doc serves: {body}");
    let (status, body) =
        get_with_bearer(&router, &format!("/v1/documents/{denied_doc}"), &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");

    // A deny-path decision for the ledger: an expired token — the wire
    // says only the generic 401; the reason is ledger-only.
    let expired = TokenSpec::autonomous(FINANCE_OID)
        .with("exp", json!(jwt::now_unix() - 300))
        .sign();
    let (status, body) =
        get_with_bearer(&router, &format!("/v1/documents/{allowed_doc}"), &expired).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "authentication required");

    // The ledger: allow AND deny rows with claim attribution.
    let rows = audit_rows(&dir);
    let authorized: Vec<&Value> = rows
        .iter()
        .filter(|r| r["action"] == "v1_document" && r["outcome"] == "authorized")
        .collect();
    assert_eq!(authorized.len(), 1, "the served document was recorded");
    let first = authorized[0];
    assert_eq!(first["actor_principal"], "agent_finance_analyst");
    assert_eq!(first["target"], format!("GET /v1/documents/{allowed_doc}"));
    assert_eq!(first["token_oid"], FINANCE_OID);
    assert_eq!(first["token_azp"], TEST_APP_ID);
    assert_eq!(
        first["token_parent_azp"], TEST_PARENT_APP,
        "the parent app id is logged (and ONLY logged)"
    );
    assert_eq!(first["token_aud"], TEST_AUDIENCE);
    assert!(first["token_uti"].as_str().is_some());
    let not_found: Vec<&Value> = rows
        .iter()
        .filter(|r| r["action"] == "v1_document" && r["outcome"] == "not_found")
        .collect();
    assert_eq!(not_found.len(), 1, "the scope deny is a recorded signal");
    assert_eq!(not_found[0]["actor_principal"], "agent_finance_analyst");
    let expired_rows: Vec<&Value> = rows
        .iter()
        .filter(|r| r["action"] == "v1_document" && r["outcome"] == "token_expired")
        .collect();
    assert_eq!(expired_rows.len(), 1, "the auth deny is a recorded signal");
    assert_eq!(expired_rows[0]["actor_principal"], "unresolved");

    // The raw token (and its signature) is NEVER at rest in the ledger.
    let audit_text = fs::read_to_string(dir.join("state").join("audit.jsonl")).expect("ledger");
    assert!(
        !audit_text.contains(&token) && !audit_text.contains(&expired),
        "raw tokens must never be logged"
    );
    let signature = token.rsplit('.').next().expect("jws has a signature");
    assert!(
        !audit_text.contains(signature),
        "token signatures must never be logged"
    );
}

// EB-4 × EB-6 on the machine surface: no ledger, no /v1 — allows AND denies
// alike answer the generic 401 (the surface cannot meet its audit
// obligation, so it does not serve).
#[tokio::test]
async fn v1_without_a_ledger_does_not_serve() {
    let dir = scratch("bridge-governance-noledger");
    // Bridge wired, NO proposals store.
    let state = base_state().with_agent_bridge(bridge_for(&dir));
    let router = app(Arc::new(state));

    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    for uri in ["/v1/documents/d0001", "/v1/whoami"] {
        let (status, body) = get_with_bearer(&router, uri, &token).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
        assert_eq!(
            body["error"], "authentication required",
            "{uri}: an unledgerable surface does not serve, and does not say why"
        );
    }
}

// S0-2 downstream, on /v1: a registration pointing at a principal the
// identity model does not know compiles to the EMPTY scope — every
// document is THE 404, every retrieve is the empty 200. No default
// principal, no anonymous scope, no public fallback.
#[tokio::test]
async fn registration_to_an_unknown_principal_fails_closed_on_v1() {
    let dir = scratch("bridge-governance-probe");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = base_state()
        .with_proposals(store)
        .with_agent_bridge(bridge_for(&dir));
    let router = app(Arc::new(state));

    let token = TokenSpec::autonomous(PROBE_OID).sign();
    let (allowed_for_finance, _) = oracle_docs_for("agent_finance_analyst");
    // Even a doc other agents can read: empty statement, THE 404.
    let (status, body) = get_with_bearer(
        &router,
        &format!("/v1/documents/{allowed_for_finance}"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");

    // And retrieval for the ghost is the EMPTY 200 — indistinguishable in
    // shape from a principal granted nothing.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/retrieve")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"query":"temperature range storage"}"#))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(body["principal"], "agent_probe_no_such_principal");
    assert_eq!(body["candidates"].as_array().map(Vec::len), Some(0));
}

// THE STANDING WIRE LAW (S1-6, carried from the S0 merge condition): deny
// reasons never reach the wire — now proven on the bridge's ONE surface.
// Every ladder deny, the class denies, and the operational denies produce
// the ONE generic 401, byte-identical across reasons, with no reason
// string in any response body or header.
#[tokio::test]
async fn wire_deny_is_generic() {
    let dir = scratch("bridge-governance-wire");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let enabled = app(Arc::new(
        base_state()
            .with_proposals(store)
            .with_agent_bridge(bridge_for(&dir)),
    ));
    let disabled_dir = scratch("bridge-governance-wire-disabled");
    let disabled_store = Arc::new(ProposalStore::open(&disabled_dir.join("state")).expect("store"));
    let disabled = app(Arc::new(base_state().with_proposals(disabled_store)));
    let no_ledger_dir = scratch("bridge-governance-wire-noledger");
    let no_ledger = app(Arc::new(
        base_state().with_agent_bridge(bridge_for(&no_ledger_dir)),
    ));

    async fn raw_get(
        router: &axum::Router,
        uri: &str,
        bearer: Option<&str>,
    ) -> (StatusCode, Vec<(String, String)>, Vec<u8>) {
        let mut builder = Request::builder().method("GET").uri(uri);
        if let Some(bearer) = bearer {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
        }
        let response = router
            .clone()
            .oneshot(builder.body(Body::empty()).expect("request"))
            .await
            .expect("response");
        let status = response.status();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.to_string(),
                    String::from_utf8_lossy(value.as_bytes()).to_string(),
                )
            })
            .collect();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        (status, headers, bytes.to_vec())
    }

    let now = jwt::now_unix();
    let uri = "/v1/documents/d0001";
    // One credential per deny class, labelled by the reason the LEDGER
    // records — the wire must not distinguish any of them.
    let deny_set: Vec<(&str, &axum::Router, Option<String>)> = vec![
        ("credential_missing", &enabled, None),
        ("session_credential_on_v1", &enabled, Some("a".repeat(64))),
        ("token_malformed", &enabled, Some("ga.rb.age".to_string())),
        (
            "algorithm_rejected/none",
            &enabled,
            Some(TokenSpec::autonomous(FINANCE_OID).alg_none()),
        ),
        (
            "algorithm_rejected/hs256",
            &enabled,
            Some(TokenSpec::autonomous(FINANCE_OID).sign_hs256()),
        ),
        (
            "signature_invalid",
            &enabled,
            Some(jwt::tamper_signature(
                &TokenSpec::autonomous(FINANCE_OID).sign(),
            )),
        ),
        (
            "issuer_mismatch",
            &enabled,
            Some(
                TokenSpec::autonomous(FINANCE_OID)
                    .with("ver", json!("1.0"))
                    .sign(),
            ),
        ),
        (
            "audience_mismatch",
            &enabled,
            Some(
                TokenSpec::autonomous(FINANCE_OID)
                    .with("aud", json!("api://someone-else"))
                    .sign(),
            ),
        ),
        (
            "token_expired",
            &enabled,
            Some(
                TokenSpec::autonomous(FINANCE_OID)
                    .with("exp", json!(now - 300))
                    .sign(),
            ),
        ),
        (
            "token_not_yet_valid",
            &enabled,
            Some(
                TokenSpec::autonomous(FINANCE_OID)
                    .with("nbf", json!(now + 300))
                    .sign(),
            ),
        ),
        (
            "tenant_mismatch",
            &enabled,
            Some(
                TokenSpec::autonomous(FINANCE_OID)
                    .with("tid", json!("9e2b5c14-77aa-4f01-8c3d-2b9d01f7a6e4"))
                    .sign(),
            ),
        ),
        (
            "unsupported_token_type_delegated",
            &enabled,
            Some(
                TokenSpec::autonomous(FINANCE_OID)
                    .with("idtyp", json!("user"))
                    .without("xms_sub_fct")
                    .sign(),
            ),
        ),
        (
            "unsupported_token_type_agent_user",
            &enabled,
            Some(
                TokenSpec::autonomous(FINANCE_OID)
                    .with("idtyp", json!("user"))
                    .with("xms_sub_fct", json!("13"))
                    .sign(),
            ),
        ),
        (
            "agent_facets_missing",
            &enabled,
            Some(
                TokenSpec::autonomous(FINANCE_OID)
                    .without("xms_sub_fct")
                    .without("xms_act_fct")
                    .sign(),
            ),
        ),
        // S0b taxonomy: idtyp ABSENT + facets absent is the SAME reason
        // (evidence-insufficient, not provably delegated) — and the same
        // mute wire.
        (
            "agent_facets_missing (idtyp absent)",
            &enabled,
            Some(
                TokenSpec::autonomous(FINANCE_OID)
                    .without("idtyp")
                    .without("xms_sub_fct")
                    .without("xms_act_fct")
                    .sign(),
            ),
        ),
        (
            "agent_not_registered",
            &enabled,
            Some(TokenSpec::autonomous("ffff9999-0000-4000-8000-0000000000f9").sign()),
        ),
        (
            "bridge_disabled",
            &disabled,
            Some(TokenSpec::autonomous(FINANCE_OID).sign()),
        ),
        (
            "bridge_unavailable",
            &no_ledger,
            Some(TokenSpec::autonomous(FINANCE_OID).sign()),
        ),
    ];

    // Every reason string that exists ONLY in the ledger — the ladder enum
    // plus the S1 surface/validation reasons. None may appear on the wire.
    let mut forbidden: Vec<String> = DenyReason::ALL
        .iter()
        .map(|reason| reason.as_str().to_string())
        .collect();
    for extra in [
        "jwt_on_console_surface",
        "session_credential_on_v1",
        "credential_missing",
        "unknown_route",
        "payload_oversize",
        "query_out_of_range",
        "top_k_out_of_range",
    ] {
        forbidden.push(extra.to_string());
    }

    let mut bodies: Vec<(String, Vec<u8>)> = Vec::new();
    for (label, router, credential) in &deny_set {
        let (status, headers, body) = raw_get(router, uri, credential.as_deref()).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{label} must 401");
        let body_text = String::from_utf8_lossy(&body).to_string();
        for reason in &forbidden {
            assert!(
                !body_text.contains(reason.as_str()),
                "{label}: reason {reason} leaked into the response body"
            );
            for (name, value) in &headers {
                assert!(
                    !value.contains(reason.as_str()),
                    "{label}: reason {reason} leaked into header {name}"
                );
            }
        }
        bodies.push((label.to_string(), body));
    }

    // EVERY deny body is byte-identical — signature_invalid vs
    // agent_not_registered explicitly, and the whole set.
    let sig = &bodies
        .iter()
        .find(|(label, _)| label == "signature_invalid")
        .expect("sig case ran")
        .1;
    let unreg = &bodies
        .iter()
        .find(|(label, _)| label == "agent_not_registered")
        .expect("unreg case ran")
        .1;
    assert_eq!(
        sig, unreg,
        "signature_invalid and agent_not_registered must be byte-identical on the wire"
    );
    for (label, body) in &bodies {
        assert_eq!(body, sig, "{label}: every deny shares ONE generic body");
    }
}

// The committed example config parses through the REAL ServiceConfig and
// stays disabled — the documented shape can never drift from the schema,
// and its default posture is OFF.
#[test]
fn example_bridge_config_parses_and_is_disabled() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("config.agent-bridge.example.json");
    let config = service::ServiceConfig::load(&path).expect("example config parses");
    let bridge = config
        .agent_bridge
        .expect("example documents the agent_bridge section");
    assert!(!bridge.enabled, "the example ships disabled (S0-4)");
    assert_eq!(bridge.allowed_algs, vec!["RS256"]);
}
