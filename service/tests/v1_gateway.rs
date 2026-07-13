//! S1 gateway discipline over the live router: the surface split in both
//! directions (S1-1), 404 parity (S1-3), default-deny unknown routes with
//! auth-before-routing (S1-4), the browser-CORS exclusion (S1-5), the
//! request limits, the whoami non-enumeration rule, and the one-row-per-
//! request ledger with pre-S1 byte-identity.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use common::jwt::{self, TokenSpec, TEST_AUDIENCE, TEST_TENANT};
use serde_json::{json, Value};
use service::agent::proposals::ProposalStore;
use service::agent_bridge::{AgentBridgeConfig, Bridge};
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

fn bridge_for(dir: &Path) -> Arc<Bridge> {
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
    Arc::new(Bridge::from_config(&config).expect("bridge builds"))
}

/// One enabled world per test binary run: bridge + ledger over the real
/// fixtures.
fn enabled_world(name: &str) -> (axum::Router, PathBuf) {
    let dir = scratch(name);
    let store = Arc::new(ProposalStore::open(&dir.join("state")).expect("store"));
    let state = base_state()
        .with_proposals(store)
        .with_agent_bridge(bridge_for(&dir));
    (app(Arc::new(state)), dir)
}

struct Reply {
    status: StatusCode,
    headers: Vec<(String, String)>,
    bytes: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or(Value::Null)
    }
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

async fn send(router: &axum::Router, request: Request<Body>) -> Reply {
    let response = router.clone().oneshot(request).await.expect("response");
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
        .expect("body")
        .to_vec();
    Reply {
        status,
        headers,
        bytes,
    }
}

fn get(uri: &str) -> axum::http::request::Builder {
    Request::builder().method("GET").uri(uri)
}

fn retrieve_request(body: &str, bearer: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/v1/retrieve")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(bearer) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
    }
    builder.body(Body::from(body.to_string())).expect("request")
}

fn audit_rows(dir: &Path) -> Vec<Value> {
    // The ledger file is created lazily on the first append; before any
    // row exists it is simply empty.
    let text = fs::read_to_string(dir.join("state").join("audit.jsonl")).unwrap_or_default();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("audit row"))
        .collect()
}

// A1: SESSIONS ON /v1 — cookie form and bearer form — are refused
// generically on every endpoint. The machine surface takes machine
// credentials only.
#[tokio::test]
async fn sessions_are_refused_on_every_v1_endpoint() {
    let (router, _dir) = enabled_world("v1-split-sessions");
    let session = common::login_as(&router, "p060").await;

    // Bearer form.
    for (method, uri, body) in [
        ("GET", "/v1/whoami", None),
        ("GET", "/v1/documents/d0001", None),
        ("POST", "/v1/retrieve", Some(r#"{"query":"anything"}"#)),
    ] {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {session}"));
        if body.is_some() {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
        }
        let request = builder
            .body(
                body.map(|b| Body::from(b.to_string()))
                    .unwrap_or(Body::empty()),
            )
            .expect("request");
        let reply = send(&router, request).await;
        assert_eq!(
            reply.status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} (bearer)"
        );
        assert_eq!(reply.json()["error"], "authentication required");
    }

    // Cookie form (no bearer at all): equally refused — cookies are never
    // read on /v1.
    for uri in ["/v1/whoami", "/v1/documents/d0001"] {
        let reply = send(
            &router,
            get(uri)
                .header(header::COOKIE, format!("eb_session={session}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{uri} (cookie)");
        assert_eq!(reply.json()["error"], "authentication required");
    }
}

// A2: AGENT JWTs ON CONSOLE ROUTES are refused generically, bridge enabled
// or not — the credential is never consulted on the human surface.
#[tokio::test]
async fn agent_jwts_are_refused_on_console_routes() {
    let (router, _dir) = enabled_world("v1-split-console");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    for uri in ["/me/scope", "/doc/d0001", "/graph", "/atlas", "/lane"] {
        let reply = send(
            &router,
            get(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{uri}");
        assert_eq!(reply.json()["error"], "authentication required");
    }
    // POST /ask too (the fenced answer surface stays session-only).
    let reply = send(
        &router,
        Request::builder()
            .method("POST")
            .uri("/ask")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"query":"anything"}"#))
            .expect("request"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "/ask");
    assert_eq!(reply.json()["error"], "authentication required");
}

// A3: the pairing rule on /v1 — a valid session cookie rides along with
// the JWT and is IGNORED; JWT semantics apply. (The console half of A3
// lives in governance_bridge::console_is_session_only....)
#[tokio::test]
async fn on_v1_the_jwt_decides_and_the_cookie_is_ignored() {
    let (router, dir) = enabled_world("v1-split-pairing");
    let session = common::login_as(&router, "p060").await;
    let token = TokenSpec::autonomous(FINANCE_OID).sign();

    // Valid JWT + valid cookie: JWT semantics -> whoami answers the AGENT.
    let reply = send(
        &router,
        get("/v1/whoami")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::COOKIE, format!("eb_session={session}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json()["principal_id"],
        "agent_finance_analyst",
        "the JWT decided; the session cookie played no part"
    );

    // Garbage JWT + valid cookie: JWT semantics -> deny (the cookie cannot
    // rescue a machine credential), ledgered with the ladder reason.
    let reply = send(
        &router,
        get("/v1/whoami")
            .header(header::AUTHORIZATION, "Bearer ga.rb.age")
            .header(header::COOKIE, format!("eb_session={session}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(reply.json()["error"], "authentication required");
    let rows = audit_rows(&dir);
    assert!(
        rows.iter()
            .any(|r| r["action"] == "v1_whoami" && r["outcome"] == "token_malformed"),
        "the /v1 deny is ledgered with its ladder reason"
    );
}

// D9: 404 parity — a document that does not exist and a document outside
// the caller's scope are byte-identical on the wire.
#[tokio::test]
async fn unauthorized_and_nonexistent_documents_are_byte_identical() {
    let (router, _dir) = enabled_world("v1-parity");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();

    // d0001 is outside finance's scope (oracle: finance allows start at
    // d0134); d9999 does not exist at all.
    let out_of_scope = send(
        &router,
        get("/v1/documents/d0001")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    let nonexistent = send(
        &router,
        get("/v1/documents/d9999")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(out_of_scope.status, StatusCode::NOT_FOUND);
    assert_eq!(nonexistent.status, StatusCode::NOT_FOUND);
    assert_eq!(
        out_of_scope.bytes, nonexistent.bytes,
        "unauthorized and nonexistent must be indistinguishable (S1-3)"
    );
}

// D10: unknown /v1 routes — auth FIRST, routing second. A valid token
// learns 404; an invalid or absent one learns only 401.
#[tokio::test]
async fn unknown_v1_routes_default_deny_with_auth_before_routing() {
    let (router, dir) = enabled_world("v1-unknown");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();

    let reply = send(
        &router,
        get("/v1/xyz")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND, "valid token -> 404");
    assert_eq!(reply.json()["error"], "not found");

    let reply = send(
        &router,
        get("/v1/xyz")
            .header(header::AUTHORIZATION, "Bearer ga.rb.age")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(
        reply.status,
        StatusCode::UNAUTHORIZED,
        "garbage token -> 401 (an unauthenticated probe cannot map the namespace)"
    );

    let reply = send(
        &router,
        get("/v1/xyz").body(Body::empty()).expect("request"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "no auth -> 401");

    // Wrong METHOD on a real route is an unknown route too (strict-method).
    let reply = send(
        &router,
        Request::builder()
            .method("POST")
            .uri("/v1/whoami")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(
        reply.status,
        StatusCode::NOT_FOUND,
        "POST /v1/whoami -> 404"
    );

    let rows = audit_rows(&dir);
    assert!(
        rows.iter()
            .any(|r| r["action"] == "v1_unknown_route" && r["outcome"] == "unknown_route"),
        "unknown-route probes are ledgered"
    );
}

// D11: the request limits, each violation generic on the wire.
#[tokio::test]
async fn retrieve_limits_are_hard() {
    let (router, dir) = enabled_world("v1-limits");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();

    // 2,049-char query -> 400.
    let long_query = "q".repeat(2_049);
    let body = serde_json::to_string(&json!({ "query": long_query })).expect("json");
    let reply = send(&router, retrieve_request(&body, Some(&token))).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "2,049-char query");
    assert_eq!(reply.json()["error"], "bad request");

    // A 2,048-char query is INSIDE the limit.
    let max_query = "q".repeat(2_048);
    let body = serde_json::to_string(&json!({ "query": max_query })).expect("json");
    let reply = send(&router, retrieve_request(&body, Some(&token))).await;
    assert_eq!(reply.status, StatusCode::OK, "2,048-char query is legal");

    // 17 KB body -> 413.
    let oversize = serde_json::to_string(&json!({ "query": "x".repeat(17_000) })).expect("json");
    assert!(oversize.len() > 16_384);
    let reply = send(&router, retrieve_request(&oversize, Some(&token))).await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE, "17KB body");
    assert_eq!(reply.json()["error"], "payload too large");

    // top_k 0 and 51 -> 400.
    for top_k in [0u64, 51] {
        let body =
            serde_json::to_string(&json!({ "query": "stock", "top_k": top_k })).expect("json");
        let reply = send(&router, retrieve_request(&body, Some(&token))).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "top_k {top_k}");
        assert_eq!(reply.json()["error"], "bad request");
    }
    // top_k 1 and 50 are legal.
    for top_k in [1u64, 50] {
        let body =
            serde_json::to_string(&json!({ "query": "stock", "top_k": top_k })).expect("json");
        let reply = send(&router, retrieve_request(&body, Some(&token))).await;
        assert_eq!(reply.status, StatusCode::OK, "top_k {top_k} is legal");
    }

    // Missing query / non-JSON -> 400.
    let reply = send(&router, retrieve_request(r#"{"top_k":5}"#, Some(&token))).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "missing query");
    let reply = send(&router, retrieve_request("not json", Some(&token))).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "non-JSON body");
    // Empty query -> 400.
    let reply = send(&router, retrieve_request(r#"{"query":""}"#, Some(&token))).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST, "empty query");

    // The violations are ledgered with their reasons — and the oversize
    // and out-of-range rows exist.
    let rows = audit_rows(&dir);
    for outcome in [
        "query_out_of_range",
        "payload_oversize",
        "top_k_out_of_range",
        "bad_request",
    ] {
        assert!(
            rows.iter().any(|r| r["outcome"] == outcome),
            "limit violation {outcome} is ledgered"
        );
    }
    // The capped ledger copy: the out-of-range query is stored, capped to
    // exactly 2,048 chars.
    let capped = rows
        .iter()
        .find(|r| r["outcome"] == "query_out_of_range")
        .expect("capped row");
    assert_eq!(
        capped["query"].as_str().map(|q| q.chars().count()),
        Some(2_048),
        "the ledger stores the query verbatim up to the cap"
    );
}

// The /v1/retrieve candidate CONTRACT pin (pre-SDK): each candidate
// carries exactly {doc_id, title, snippet, rank}; `rank` is the 1-based
// fused rank in ascending wire order (1 = best) — never a similarity
// score, never descending.
#[tokio::test]
async fn retrieve_candidates_pin_the_rank_contract() {
    let (router, _dir) = enabled_world("v1-rank-pin");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let reply = send(
        &router,
        retrieve_request(
            r#"{"query":"site stock value report","top_k":5}"#,
            Some(&token),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    let body = reply.json();
    let candidates = body["candidates"].as_array().expect("candidates");
    assert!(!candidates.is_empty(), "the fixture query has hits");
    for (index, candidate) in candidates.iter().enumerate() {
        let object = candidate.as_object().expect("candidate object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["doc_id", "rank", "snippet", "title"],
            "the candidate contract is exactly these four fields"
        );
        assert_eq!(
            candidate["rank"].as_u64(),
            Some(index as u64 + 1),
            "rank is 1-based and ascending in wire order"
        );
    }
    assert!(
        !body.to_string().contains("\"score\""),
        "the retired `score` spelling never reappears on the wire"
    );
}

// D13: whoami is a handshake, not an enumeration surface — principal id
// (plus display name), and NOTHING scope-shaped.
#[tokio::test]
async fn whoami_carries_no_scope_information() {
    let (router, _dir) = enabled_world("v1-whoami");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let reply = send(
        &router,
        get("/v1/whoami")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    let body = reply.json();
    assert_eq!(body["principal_id"], "agent_finance_analyst");
    let object = body.as_object().expect("whoami is an object");
    for key in object.keys() {
        assert!(
            key == "principal_id" || key == "display_name",
            "whoami must not carry {key} — no scope-shaped fields"
        );
    }
    // Belt and braces: the scope-shaped names must be absent.
    for forbidden in [
        "allowlist",
        "scope",
        "documents",
        "doc_count",
        "department",
        "groups",
        "sites",
        "band",
    ] {
        assert!(
            object.get(forbidden).is_none(),
            "whoami must not enumerate {forbidden}"
        );
    }
}

// S1-5: /v1 is not a browser surface — no CORS header is ever stamped, and
// a preflight-shaped OPTIONS gets the auth ladder, not a 204.
#[tokio::test]
async fn v1_is_absent_from_browser_cors() {
    let (router, _dir) = enabled_world("v1-cors");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();

    // An allowed console origin gets NO CORS headers on /v1 responses.
    let reply = send(
        &router,
        get("/v1/whoami")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::ORIGIN, "http://localhost:3000")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert!(
        reply.header("access-control-allow-origin").is_none(),
        "no ACAO on /v1, whatever the origin"
    );

    // A preflight-shaped OPTIONS on /v1: the auth ladder answers (401),
    // never a 204 with CORS grants.
    let reply = send(
        &router,
        Request::builder()
            .method(Method::OPTIONS)
            .uri("/v1/retrieve")
            .header(header::ORIGIN, "http://localhost:3000")
            .header("access-control-request-method", "POST")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(
        reply.status,
        StatusCode::UNAUTHORIZED,
        "no preflight service on the machine surface"
    );
    assert!(reply.header("access-control-allow-origin").is_none());

    // The console keeps its CORS behaviour untouched.
    let reply = send(
        &router,
        Request::builder()
            .method(Method::OPTIONS)
            .uri("/ask")
            .header(header::ORIGIN, "http://localhost:3000")
            .header("access-control-request-method", "POST")
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    assert_eq!(
        reply.status,
        StatusCode::NO_CONTENT,
        "console preflights still answer"
    );
    assert_eq!(
        reply.header("access-control-allow-origin"),
        Some("http://localhost:3000")
    );
}

// E14: one ledger row per /v1 request across a scripted mixed sequence,
// with the right actions, decisions, attribution, capped query, and
// candidate ids — and pre-S1 writer output stays byte-identical.
#[tokio::test]
async fn one_ledger_row_per_v1_request_with_full_attribution() {
    let (router, dir) = enabled_world("v1-audit-seq");
    let token = TokenSpec::autonomous(FINANCE_OID).sign();
    let rows_at = |dir: &Path| audit_rows(dir).len();
    let before = rows_at(&dir);

    // 1. whoami (allow)
    send(
        &router,
        get("/v1/whoami")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    // 2. document deny (out of scope)
    send(
        &router,
        get("/v1/documents/d0001")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    // 3. retrieve (allow)
    send(
        &router,
        retrieve_request(
            r#"{"query":"site stock value report","top_k":5}"#,
            Some(&token),
        ),
    )
    .await;
    // 4. unknown route (deny)
    send(
        &router,
        get("/v1/nope")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;
    // 5. oversize (deny)
    let oversize = serde_json::to_string(&json!({ "query": "x".repeat(17_000) })).expect("json");
    send(&router, retrieve_request(&oversize, Some(&token))).await;
    // 6. auth deny (expired)
    let expired = TokenSpec::autonomous(FINANCE_OID)
        .with("exp", json!(jwt::now_unix() - 300))
        .sign();
    send(
        &router,
        get("/v1/whoami")
            .header(header::AUTHORIZATION, format!("Bearer {expired}"))
            .body(Body::empty())
            .expect("request"),
    )
    .await;

    let rows = audit_rows(&dir);
    assert_eq!(
        rows.len() - before,
        6,
        "exactly ONE ledger row per /v1 request"
    );
    let tail = &rows[before..];
    assert_eq!(tail[0]["action"], "v1_whoami");
    assert_eq!(tail[0]["outcome"], "authorized");
    assert_eq!(tail[0]["token_oid"], FINANCE_OID);
    assert_eq!(tail[1]["action"], "v1_document");
    assert_eq!(tail[1]["outcome"], "not_found");
    assert_eq!(tail[2]["action"], "v1_retrieve");
    assert_eq!(tail[2]["outcome"], "authorized");
    assert_eq!(tail[2]["query"], "site stock value report");
    let candidates = tail[2]["candidates"].as_array().expect("candidate ids");
    assert!(
        !candidates.is_empty(),
        "the fixture query has in-scope hits"
    );
    assert!(candidates.len() <= 5, "top_k bounds the recorded ids");
    assert_eq!(tail[3]["action"], "v1_unknown_route");
    assert_eq!(tail[3]["outcome"], "unknown_route");
    assert_eq!(tail[4]["action"], "v1_retrieve");
    assert_eq!(tail[4]["outcome"], "payload_oversize");
    assert_eq!(tail[5]["action"], "v1_whoami");
    assert_eq!(tail[5]["outcome"], "token_expired");
    assert_eq!(tail[5]["actor_principal"], "unresolved");

    // Pre-S1 writers stay byte-identical: a legacy `audit()` row carries
    // NONE of the S0/S1 optional fields in its serialized form.
    let store = ProposalStore::open(&dir.join("legacy")).expect("legacy store");
    store
        .audit("lens_view", "p060", "p060", "allowed")
        .expect("legacy row");
    let legacy = fs::read_to_string(dir.join("legacy").join("audit.jsonl")).expect("ledger");
    assert_eq!(
        legacy.trim(),
        r#"{"action":"lens_view","actor_principal":"p060","ordinal":0,"outcome":"allowed","target":"p060"}"#,
        "pre-S1 rows serialize exactly as they always did"
    );
}
