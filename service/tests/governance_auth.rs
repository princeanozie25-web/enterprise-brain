//! FC-A1 (AUTH-1): identity is bound from a SERVER-MINTED SESSION, never the
//! `x-demo-principal` header. Proven over the live router:
//!   S1 spoof — the header grants nothing without a session;
//!   S2       — expired / forged (fixation) / revoked sessions all 401;
//!   resolve  — a session yields the correct principal (bearer AND cookie);
//!   cross-id — a header cannot switch identity away from the session;
//!   surface  — only /healthz + /auth/login are public; demo login works.
//! Decision logic is untouched: this is the authentication boundary only.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use service::{app, AppState};
use tower::ServiceExt;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("service crate sits in the repo root")
        .to_path_buf()
}

fn auth_state() -> AppState {
    AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("load + verify people.json")
}

async fn send(router: &axum::Router, request: Request<Body>) -> (StatusCode, Vec<u8>) {
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, bytes.to_vec())
}

fn get(uri: &str) -> axum::http::request::Builder {
    Request::builder().method("GET").uri(uri)
}

// FC-A1.1: the public surface is exactly /healthz + /auth/login; every other
// route demands a valid session.
#[tokio::test]
async fn public_surface_is_only_healthz_and_login() {
    let router = app(Arc::new(auth_state()));

    let (s, _) = send(&router, get("/healthz").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK, "/healthz is public");

    // /auth/login is reachable with no session and mints one.
    let token = common::login_as(&router, "p060").await;
    assert!(!token.is_empty(), "login mints a session token");

    for uri in ["/me/scope", "/scope", "/graph", "/people", "/atlas"] {
        let (s, _) = send(&router, get(uri).body(Body::empty()).unwrap()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{uri} requires a session");
    }
    println!("FC-A1.1: only /healthz + /auth/login are public; protected routes 401 with no session");
}

// FC-A1.2 (S1 spoof): the retired header grants nothing without a session.
#[tokio::test]
async fn s1_x_demo_principal_header_without_session_is_401() {
    let router = app(Arc::new(auth_state()));
    for uri in ["/me/scope", "/scope", "/graph"] {
        let (s, _) = send(
            &router,
            get(uri)
                .header("x-demo-principal", "p060")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(
            s,
            StatusCode::UNAUTHORIZED,
            "x-demo-principal asserts no identity for {uri}"
        );
    }
    println!("FC-A1.2 (S1): x-demo-principal without a session -> 401 everywhere");
}

// FC-A1.3: a valid session resolves the correct principal (bearer AND cookie);
// different principals resolve to different scopes.
#[tokio::test]
async fn session_resolves_the_correct_principal_via_bearer_and_cookie() {
    let router = app(Arc::new(auth_state()));

    let t60 = common::login_as(&router, "p060").await;
    let t88 = common::login_as(&router, "p088").await;

    let (s60, b60) = send(
        &router,
        get("/scope")
            .header("authorization", format!("Bearer {t60}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    let (s88, b88) = send(
        &router,
        get("/scope")
            .header("authorization", format!("Bearer {t88}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s60, StatusCode::OK);
    assert_eq!(s88, StatusCode::OK);
    assert_ne!(
        b60, b88,
        "p060 (Finance) and p088 (HR) resolve to different scopes"
    );

    // Cookie path resolves identically (the cookie value is the session token).
    let (sc, bc) = send(
        &router,
        get("/scope")
            .header("cookie", format!("eb_session={t60}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(sc, StatusCode::OK);
    assert_eq!(
        bc, b60,
        "the session cookie resolves the same principal as the bearer"
    );
    println!("FC-A1.3: bearer + cookie both resolve the right principal; p060 != p088");
}

// FC-A1.4 (S2): expired, forged (fixation), and revoked sessions all 401.
#[tokio::test]
async fn s2_expired_forged_and_revoked_sessions_are_401() {
    let state = Arc::new(auth_state());
    let router = app(state.clone());

    // Expired: minted in 1970 (deterministic), rejected at the HTTP edge.
    let expired = state.sessions.mint_with_expiry("p060", 0, 1);
    let (s, _) = send(
        &router,
        get("/me/scope")
            .header("authorization", format!("Bearer {}", expired.token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "expired session -> 401");

    // Fixation: a client-chosen token the server never minted -> 401.
    let (s, _) = send(
        &router,
        get("/me/scope")
            .header("authorization", "Bearer client-picked-session-id")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "a client-supplied session id is not honoured"
    );

    // Revocation: a live session works, logout revokes it, then it's 401.
    let live = common::login_as(&router, "p060").await;
    let (ok, _) = send(
        &router,
        get("/me/scope")
            .header("authorization", format!("Bearer {live}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(ok, StatusCode::OK, "the live session works first");

    let (lo, _) = send(
        &router,
        Request::builder()
            .method("POST")
            .uri("/auth/logout")
            .header("authorization", format!("Bearer {live}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(lo, StatusCode::OK, "logout succeeds");

    let (after, _) = send(
        &router,
        get("/me/scope")
            .header("authorization", format!("Bearer {live}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(after, StatusCode::UNAUTHORIZED, "revoked session -> 401");
    println!("FC-A1.4 (S2): expired + forged + revoked sessions all 401");
}

// FC-A1.5: logged in as p088, there is no header (or other) path to act as
// p060. The session stands; the header is inert.
#[tokio::test]
async fn cross_identity_header_cannot_override_the_session() {
    let router = app(Arc::new(auth_state()));
    let t88 = common::login_as(&router, "p088").await;
    let t60 = common::login_as(&router, "p060").await;

    let (_, base88) = send(
        &router,
        get("/scope")
            .header("authorization", format!("Bearer {t88}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    // Same p088 session, but ALSO asserting p060 via the retired header.
    let (s, spoofed) = send(
        &router,
        get("/scope")
            .header("authorization", format!("Bearer {t88}"))
            .header("x-demo-principal", "p060")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        spoofed, base88,
        "the header cannot switch identity; the p088 session stands"
    );

    let (_, p60) = send(
        &router,
        get("/scope")
            .header("authorization", format!("Bearer {t60}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_ne!(spoofed, p60, "a p088 session never yields p060's scope");
    println!("FC-A1.5: a p088 session + x-demo-principal:p060 still resolves p088");
}

// FC-A1.6 (demo): under demo_identity_mode you can log in as each principal and
// /ask runs as EXACTLY that principal (resolved from the session).
#[tokio::test]
async fn demo_login_as_each_principal_and_ask_runs_as_that_principal() {
    let router = app(Arc::new(auth_state()));
    for principal in ["p060", "p088", "p_void"] {
        let token = common::login_as(&router, principal).await;
        let (s, body) = send(
            &router,
            Request::builder()
                .method("POST")
                .uri("/ask")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query":"payroll salary review"}"#))
                .unwrap(),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{principal} can ask");
        let v: Value = serde_json::from_slice(&body).expect("envelope");
        assert_eq!(v["demo_identity_mode"], Value::Bool(true));
        assert_eq!(
            v["principal_id"], principal,
            "the answer runs as the logged-in principal, resolved from the session"
        );
    }
    println!("FC-A1.6 (demo): login-as each principal works; /ask runs as the session principal");
}
