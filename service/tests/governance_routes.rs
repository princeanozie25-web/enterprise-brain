//! AUTH-4 (FC-A4) — route-completeness default-deny (M1) + DoS hardening (D1).
//!
//! The capstone guarantee: every route the service exposes is EXPLICITLY
//! auth/scope-classified, an unclassified route is DENIED (not served), and a
//! standing test fails the build if any registered route lacks a classification
//! — so a future-added route cannot be silently exposed. Plus: identity is the
//! session alone (no header trust), and the auth endpoints are rate-limited with
//! a per-principal session quota.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::Value;
use service::routes::{classify, RouteClass, REGISTERED_ROUTES};
use service::{app, AppState};
use tower::ServiceExt;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("service crate sits in the repo root")
        .to_path_buf()
}

fn base_state() -> AppState {
    AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
}

/// Substitute every `{param}` segment with a concrete value, so a registered
/// pattern becomes a routable path.
fn concrete(pattern: &str) -> String {
    pattern
        .split('/')
        .map(|seg| if seg.starts_with('{') { "x" } else { seg })
        .collect::<Vec<_>>()
        .join("/")
}

async fn send(router: &axum::Router, method: &str, path: &str, auth: Option<&str>) -> StatusCode {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(a) = auth {
        builder = builder.header("authorization", a);
    }
    router
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

async fn login(router: &axum::Router, principal: &str) -> StatusCode {
    let body = serde_json::json!({ "principal_id": principal }).to_string();
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

// RC-1 (the standing guard): every route registered in `app()` has an explicit
// classification. This FAILS THE BUILD if a future route is added to the table
// without classifying it — the M1 completeness guarantee.
#[test]
fn rc1_every_registered_route_is_explicitly_classified() {
    let mut unclassified = vec![];
    for (method, pattern) in REGISTERED_ROUTES {
        let m = Method::from_bytes(method.as_bytes()).expect("valid method");
        if classify(&m, pattern).is_none() {
            unclassified.push(format!("{method} {pattern}"));
        }
    }
    assert!(
        unclassified.is_empty(),
        "every route must declare an explicit auth/scope classification; unclassified: {unclassified:?}"
    );
    println!(
        "RC-1: all {} registered routes are explicitly classified",
        REGISTERED_ROUTES.len()
    );
}

// RC-2 (prove the guard works, then it stays in place): a route that is NOT in
// the classification table resolves to None — which is exactly what the
// middleware fail-closes on. If such a route were ever served, this is the
// moment it would be denied instead of exposed.
#[test]
fn rc2_an_unclassified_route_resolves_to_none_and_is_denied() {
    assert_eq!(classify(&Method::GET, "/totally-new-route"), None);
    assert_eq!(classify(&Method::POST, "/admin/backdoor"), None);
    // Right path, wrong method is unclassified too (default-deny is per method).
    assert_eq!(classify(&Method::DELETE, "/doc/d0001"), None);
    println!("RC-2: unclassified routes (incl. wrong-method) -> None -> denied");
}

// RC-3: only `/healthz` and `/auth/login` are public; every other registered
// route is session-required. No accidental public surface.
#[test]
fn rc3_exactly_two_public_routes() {
    let public: Vec<String> = REGISTERED_ROUTES
        .iter()
        .filter(|(method, pattern)| {
            let m = Method::from_bytes(method.as_bytes()).unwrap();
            classify(&m, pattern) == Some(RouteClass::Public)
        })
        .map(|(method, pattern)| format!("{method} {pattern}"))
        .collect();
    assert_eq!(public, vec!["GET /healthz", "POST /auth/login"]);
    println!("RC-3: exactly /healthz and /auth/login are public");
}

// RC-4 (no bypass anywhere): every protected route returns 401 without a
// session; the two public routes do not. Runs the full registered set through
// the live middleware.
#[tokio::test]
async fn rc4_every_protected_route_is_401_without_a_session() {
    let router = app(Arc::new(base_state()));
    for (method, pattern) in REGISTERED_ROUTES {
        let path = concrete(pattern);
        let status = send(&router, method, &path, None).await;
        let m = Method::from_bytes(method.as_bytes()).unwrap();
        match classify(&m, pattern) {
            Some(RouteClass::Public) => assert_ne!(
                status,
                StatusCode::UNAUTHORIZED,
                "public route {method} {pattern} must not require a session"
            ),
            _ => assert_eq!(
                status,
                StatusCode::UNAUTHORIZED,
                "protected route {method} {pattern} must be 401 without a session"
            ),
        }
    }
    println!("RC-4: all protected routes 401 without a session; public routes pass");
}

// RC-5: an unknown / unclassified path is denied (not served) — the middleware's
// default-deny, with or without a session.
#[tokio::test]
async fn rc5_unknown_path_is_denied_not_served() {
    let router = app(Arc::new(base_state()));
    assert_eq!(
        send(&router, "GET", "/totally-new-route", None).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&router, "POST", "/admin/backdoor", None).await,
        StatusCode::NOT_FOUND
    );
    // Even authenticated, an unclassified path is not served.
    let auth = common::bearer(&router, "p060").await;
    assert_eq!(
        send(&router, "GET", "/totally-new-route", Some(&auth)).await,
        StatusCode::NOT_FOUND
    );
    println!("RC-5: unknown/unclassified paths -> 404 (denied), authed or not");
}

// RC-6 (no header trust): a caller-asserted identity header is ignored — without
// a real session the request is 401. Identity is the session alone (AUTH-1, and
// it holds across the AUTH-3 routes too).
#[tokio::test]
async fn rc6_identity_header_is_not_trusted() {
    let router = app(Arc::new(base_state()));
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/scope")
                .header("x-demo-principal", "p060") // the retired header
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "a caller-asserted identity header grants nothing — session is the only source"
    );
    println!("RC-6: x-demo-principal is ignored; no session -> 401");
}

// RC-7 (D1 rate limit): more than N /auth/login attempts in the window -> 429.
#[tokio::test]
async fn rc7_login_is_rate_limited() {
    let router = app(Arc::new(base_state().with_login_rate(5, 60)));
    // The first 5 attempts (distinct principals, so the quota never trips) pass.
    for i in 0..5 {
        assert_eq!(
            login(&router, &format!("p{i:03}")).await,
            StatusCode::OK,
            "attempt {i} within the window should succeed"
        );
    }
    // The 6th is cheap-rejected.
    let status = login(&router, "p099").await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "the (max+1)th login in the window is 429"
    );
    println!("RC-7: >5 logins/window -> 429 (cheap-reject)");
}

// RC-8 (D1 session quota): a principal cannot hold more than M concurrent
// sessions; the excess login is rejected. A different principal is unaffected.
#[tokio::test]
async fn rc8_per_principal_session_quota_is_enforced() {
    let router = app(Arc::new(base_state().with_session_quota(3)));
    for i in 0..3 {
        assert_eq!(
            login(&router, "p060").await,
            StatusCode::OK,
            "session {i} for p060 fits the quota"
        );
    }
    // The 4th concurrent session for p060 is rejected (429, with the quota error).
    let body = serde_json::json!({ "principal_id": "p060" }).to_string();
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["error"], "session quota exceeded");
    // A different principal still gets in (quota is per-principal).
    assert_eq!(login(&router, "p088").await, StatusCode::OK);
    println!("RC-8: >3 concurrent sessions for one principal -> 429; other principals unaffected");
}
