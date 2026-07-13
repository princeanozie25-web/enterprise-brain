//! Access-request ledger governance. This slice records requests and review
//! decisions; grant creation is covered by governance_access_grants. Request
//! decisions themselves never mutate compiled artifacts.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::index::{build_index, sha256_hex};
use serde::Deserialize;
use serde_json::{json, Value};
use service::access_requests::{AccessRequestStore, AuditEvent};
use service::{app, AppState};
use tower::ServiceExt;

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

struct World {
    artifacts_dir: PathBuf,
    fixtures_dir: PathBuf,
    idx_dir: PathBuf,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("access_request_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let idx_dir = scratch("access_request_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        World {
            artifacts_dir,
            fixtures_dir,
            idx_dir,
        }
    })
}

fn ar_state(store_dir: &Path) -> (AppState, Arc<AccessRequestStore>) {
    let world = world();
    let store = Arc::new(AccessRequestStore::open(store_dir).expect("open access request store"));
    let state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
        .with_people()
        .expect("load + verify people")
        .with_access_requests(store.clone());
    (state, store)
}

#[derive(Debug, Deserialize)]
struct PeopleFile {
    people: Vec<Person>,
}

#[derive(Debug, Deserialize)]
struct Person {
    id: String,
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
struct Project {
    capability_id: String,
}

#[derive(Debug, Deserialize)]
struct CompanyFile {
    people: Vec<CompanyPerson>,
}

#[derive(Debug, Deserialize)]
struct CompanyPerson {
    id: String,
    #[serde(default)]
    manager_id: Option<String>,
}

fn request_fixture() -> (String, String, String, String) {
    let fixtures = common::repo_fixtures_dir();
    let people: PeopleFile =
        serde_json::from_slice(&fs::read(fixtures.join("people.json")).expect("people"))
            .expect("people parse");
    let company: CompanyFile =
        serde_json::from_slice(&fs::read(fixtures.join("company.json")).expect("company"))
            .expect("company parse");
    let managers: BTreeMap<String, String> = company
        .people
        .iter()
        .filter_map(|person| {
            person
                .manager_id
                .as_ref()
                .map(|manager| (person.id.clone(), manager.clone()))
        })
        .collect();
    let (requester, capability) = people
        .people
        .iter()
        .find_map(|person| {
            managers
                .contains_key(&person.id)
                .then(|| {
                    person
                        .projects
                        .first()
                        .map(|project| (person.id.clone(), project.capability_id.clone()))
                })
                .flatten()
        })
        .expect("a managed person with a project");
    let approver = managers[&requester].clone();
    let non_approver = company
        .people
        .iter()
        .map(|person| person.id.clone())
        .find(|id| id != &requester && id != &approver)
        .expect("third human");
    (requester, approver, non_approver, capability)
}

fn read_audit(store_dir: &Path) -> Vec<AuditEvent> {
    let text =
        fs::read_to_string(store_dir.join("access_requests_audit.jsonl")).unwrap_or_default();
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("audit row parses"))
        .collect()
}

fn hash_tree(root: &Path) -> BTreeMap<String, String> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, sha256_hex(&fs::read(&path).expect("read file")));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

async fn post_json(
    router: &axum::Router,
    principal: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("authorization", common::bearer(router, principal).await)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).expect("json body")))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec();
    let value = serde_json::from_slice(&bytes).expect("json");
    (status, value, bytes)
}

async fn post_empty(router: &axum::Router, principal: &str, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("authorization", common::bearer(router, principal).await)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, serde_json::from_slice(&bytes).expect("json"))
}

async fn get(router: &axum::Router, principal: &str, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("authorization", common::bearer(router, principal).await)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, serde_json::from_slice(&bytes).expect("json"))
}

#[tokio::test]
async fn access_request_create_is_audited_and_derives_approver() {
    let store_dir = scratch("access_request_create_store");
    let (requester, approver, _non_approver, capability) = request_fixture();
    let (state, _store) = ar_state(&store_dir);
    let router = app(Arc::new(state));

    let (status, body, _) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "project", "capability_id": capability },
            "justification": "Need to review my assigned capability context."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request_id = body["request"]["request_id"].as_str().unwrap();
    assert!(request_id.starts_with("ar_"));
    assert_eq!(body["request"]["requester_id"], requester);
    assert_eq!(body["request"]["approver_id"], approver);
    assert_eq!(body["request"]["status"], "pending");

    let audit = read_audit(&store_dir);
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].action, "access_request_create");
    assert_eq!(audit[0].actor_principal, requester);
    assert_eq!(audit[0].outcome, "allowed");

    let (status, mine) = get(&router, &requester, "/access-requests").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(mine["requests"].as_array().unwrap().len(), 1);
    let (status, inbox) = get(&router, &approver, "/access-requests/inbox").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(inbox["requests"][0]["request_id"], request_id);
}

#[tokio::test]
async fn access_request_decisions_are_authorized_audited_and_do_not_mutate_compiled_access() {
    let store_dir = scratch("access_request_decision_store");
    let (requester, approver, non_approver, capability) = request_fixture();
    let world = world();
    let before_artifacts = hash_tree(&world.artifacts_dir);
    let (state, store) = ar_state(&store_dir);
    let router = app(Arc::new(state));

    let (status, body, _) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "capability", "capability_id": capability },
            "justification": "Need the capability status for assigned work."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request_id = body["request"]["request_id"].as_str().unwrap().to_string();

    let (status, _) = post_empty(
        &router,
        &non_approver,
        &format!("/access-requests/{request_id}/approve"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        store.get(&request_id).expect("request").status,
        "pending",
        "non-approver refusal has no effect"
    );

    let before_request = store.get(&request_id).expect("request");
    let (status, approved) = post_json(
        &router,
        &approver,
        &format!("/access-requests/{request_id}/approve"),
        json!({ "reason_code": "manager_approved" }),
    )
    .await
    .pipe(|(s, v, _)| (s, v));
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["request"]["status"], "approved");
    assert_eq!(approved["request"]["decision"]["actor_principal"], approver);
    assert_eq!(approved["request"]["decision"]["outcome"], "approved");
    let after_request = store.get(&request_id).expect("request");
    assert_eq!(before_request.requester_id, after_request.requester_id);
    assert_eq!(before_request.target, after_request.target);
    assert_eq!(before_request.justification, after_request.justification);
    assert_eq!(before_request.approver_id, after_request.approver_id);

    drop(router);
    assert_eq!(before_artifacts, hash_tree(&world.artifacts_dir));

    let audit = read_audit(&store_dir);
    assert!(audit
        .iter()
        .any(|row| row.action == "access_request_approve"
            && row.actor_principal == non_approver
            && row.outcome == "refused_not_approver"));
    assert!(audit
        .iter()
        .any(|row| row.action == "access_request_approve"
            && row.actor_principal == approver
            && row.outcome == "allowed"));
}

#[tokio::test]
async fn access_request_refuses_client_approvers_and_hidden_document_targets() {
    let store_dir = scratch("access_request_refusal_store");
    let (requester, _approver, _non_approver, capability) = request_fixture();
    let (state, _store) = ar_state(&store_dir);
    let router = app(Arc::new(state));

    let (status, _body, _bytes) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "project", "capability_id": capability },
            "justification": "Need this for assigned work.",
            "approver_id": "p999"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _body, bytes) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "document", "document_id": "d0091" },
            "justification": "Need this hidden document."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        !String::from_utf8_lossy(&bytes).contains("d0091"),
        "strict-parse refusal does not echo a document id"
    );

    let (status, body, bytes) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "project", "capability_id": "missing_capability" },
            "justification": "Need this capability for assigned work."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");
    assert!(
        !String::from_utf8_lossy(&bytes).contains("d0091"),
        "unknown target refusal exposes no denied document id"
    );

    let audit = read_audit(&store_dir);
    let outcomes: BTreeSet<&str> = audit.iter().map(|row| row.outcome.as_str()).collect();
    assert!(outcomes.contains("refused_strict_parse"));
    assert!(outcomes.contains("refused_unknown_target"));
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}
