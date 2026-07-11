//! S0 governance over the live router: the bridge is OFF by default and
//! structurally inert (S0-4); enabled, it resolves a registered agent into
//! the EXISTING principal seam and audits EVERY token-path decision before
//! its effect (EB-6/EB-7); the session and bearer paths never fall back to
//! each other in either direction (S0-5); an allow that cannot be recorded
//! is a deny (EB-4 × EB-6); the raw token is never at rest in the ledger.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_APP_ID, TEST_AUDIENCE, TEST_PARENT_APP, TEST_TENANT};
use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::agent_bridge::{AgentBridgeConfig, Bridge};
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

async fn get_doc(router: &axum::Router, doc: &str, bearer: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/doc/{doc}"))
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

// S0-4: the DEFAULT world has no bridge. A JWT-shaped bearer is denied
// `bridge_disabled`; opaque bearers and sessions behave exactly as before.
#[tokio::test]
async fn disabled_by_default_denies_jwt_bearers_and_nothing_else_changes() {
    let router = app(Arc::new(base_state()));

    // A perfectly valid agent token: denied bridge_disabled, not validated.
    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let (status, body) = get_doc(&router, "d0001", &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "bridge_disabled");

    // A dotless garbage bearer stays on the SESSION path — the same 401 it
    // always produced (no behaviour change for non-JWT credentials).
    let (status, body) = get_doc(&router, "d0001", "not-a-session").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "authentication required");

    // The session flow itself is untouched: login -> authorized read.
    let session = common::login_as(&router, "p060").await;
    let (status, _) = get_doc(&router, "d0001", &session).await;
    assert_ne!(
        status,
        StatusCode::UNAUTHORIZED,
        "session-authenticated requests are untouched by the bridge's existence"
    );
}

// The enabled world: a registered agent resolves into the EXISTING seam,
// scope decides exactly as the oracle says, and every decision lands in the
// ledger BEFORE its effect — claims only, never the token.
#[tokio::test]
async fn enabled_bridge_resolves_scopes_and_audits_every_decision() {
    let dir = scratch("bridge-governance-enabled");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = base_state()
        .with_proposals(store.clone())
        .with_agent_bridge(bridge_for(&dir));
    let router = app(Arc::new(state));

    let (allowed_doc, denied_doc) = oracle_docs_for("agent_finance_analyst");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();

    // Row 11 (the untouched engine): oracle-ALLOW doc serves; oracle-DENY
    // doc gets THE one 404, indistinguishable from nonexistence.
    let (status, body) = get_doc(&router, &allowed_doc, &token).await;
    assert_eq!(status, StatusCode::OK, "oracle-allowed doc serves: {body}");
    let (status, body) = get_doc(&router, &denied_doc, &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");

    // A deny-path decision for the ledger: an expired token.
    let expired = TokenSpec::autonomous(FINANCE_OID)
        .with("exp", json!(jwt::now_unix() - 300))
        .sign();
    let (status, body) = get_doc(&router, &allowed_doc, &expired).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "token_expired");

    // The ledger: allow AND deny rows, written with claim attribution.
    let audit_text = fs::read_to_string(dir.join("state").join("audit.jsonl")).expect("ledger");
    let rows: Vec<Value> = audit_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("audit row"))
        .collect();
    let authorized: Vec<&Value> = rows
        .iter()
        .filter(|r| r["action"] == "agent_token" && r["outcome"] == "authorized")
        .collect();
    assert_eq!(authorized.len(), 2, "both decided requests were recorded");
    let first = authorized[0];
    assert_eq!(first["actor_principal"], "agent_finance_analyst");
    assert_eq!(first["target"], format!("GET /doc/{allowed_doc}"));
    assert_eq!(first["token_oid"], FINANCE_OID);
    assert_eq!(first["token_azp"], TEST_APP_ID);
    assert_eq!(
        first["token_parent_azp"], TEST_PARENT_APP,
        "the parent app id is logged (and ONLY logged)"
    );
    assert_eq!(first["token_aud"], TEST_AUDIENCE);
    assert!(first["token_uti"].as_str().is_some());
    let denied: Vec<&Value> = rows
        .iter()
        .filter(|r| r["action"] == "agent_token" && r["outcome"] == "token_expired")
        .collect();
    assert_eq!(denied.len(), 1, "the deny is a recorded monitoring signal");
    assert_eq!(denied[0]["actor_principal"], "unresolved");

    // The raw token (and its signature) is NEVER at rest in the ledger.
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

// S0-5: the two authentication paths never fall back to each other.
#[tokio::test]
async fn no_fallback_between_bearer_and_session_in_either_direction() {
    let dir = scratch("bridge-governance-nofallback");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = base_state()
        .with_proposals(store)
        .with_agent_bridge(bridge_for(&dir));
    let router = app(Arc::new(state));

    // A VALID session cookie rides along with a garbage JWT-shaped bearer:
    // the bridge decides (and denies) — the session is never consulted.
    let session = common::login_as(&router, "p060").await;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/doc/d0001")
                .header(header::AUTHORIZATION, "Bearer ga.rb.age")
                .header(header::COOKIE, format!("eb_session={session}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        body["error"], "token_malformed",
        "a failed bridge credential NEVER falls back to session semantics"
    );

    // A JWT in the session COOKIE slot is just an unknown session: the
    // bridge is bearer-only, and a session credential never reaches it.
    let token = TokenSpec::autonomous(FINANCE_OID).sign();
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
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(
        body["error"], "authentication required",
        "a bearer credential is the ONLY door into the bridge"
    );

    // And the opaque session bearer still authenticates as always.
    let (status, _) = get_doc(&router, "d0001", &session).await;
    assert_ne!(status, StatusCode::UNAUTHORIZED);
}

// EB-4 × EB-6: an allow that cannot be recorded is a deny. A deny that
// cannot be recorded is still a deny.
#[tokio::test]
async fn enabled_bridge_without_a_ledger_refuses_allows() {
    let dir = scratch("bridge-governance-noledger");
    // Bridge wired, NO proposals store.
    let state = base_state().with_agent_bridge(bridge_for(&dir));
    let router = app(Arc::new(state));

    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let (status, body) = get_doc(&router, "d0001", &token).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        body["error"], "bridge_unavailable",
        "an unrecordable allow must not proceed"
    );

    // Denies stand even without the ledger.
    let expired = TokenSpec::autonomous(FINANCE_OID)
        .with("exp", json!(jwt::now_unix() - 300))
        .sign();
    let (status, body) = get_doc(&router, "d0001", &expired).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "token_expired");
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

// S0-2 downstream: a registration pointing at a principal the identity
// model does not know compiles to the EMPTY scope — every document is THE
// 404. No default principal, no anonymous scope, no public fallback.
#[tokio::test]
async fn registration_to_an_unknown_principal_fails_closed() {
    let dir = scratch("bridge-governance-probe");
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = base_state()
        .with_proposals(store)
        .with_agent_bridge(bridge_for(&dir));
    let router = app(Arc::new(state));

    let token = TokenSpec::autonomous(PROBE_OID).sign();
    let (allowed_for_finance, _) = oracle_docs_for("agent_finance_analyst");
    // Even a doc other agents can read: empty statement, THE 404.
    let (status, body) = get_doc(&router, &allowed_for_finance, &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");
}
