//! Role-scope posture governance.
//!
//! `/me/scope` is a read-only contract for UI labels and command pods. It is
//! deliberately descriptive: derived role posture must not become an
//! authorization grant, a Bursar surface, or a hidden-document side channel.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::index::build_index;
use serde::Deserialize;
use serde_json::{json, Value};
use service::access_requests::AccessRequestStore;
use service::{app, AppState};
use tower::ServiceExt;

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

struct World {
    artifacts_dir: PathBuf,
    fixtures_dir: PathBuf,
    idx_dir: PathBuf,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("role_scope_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let idx_dir = scratch("role_scope_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        World {
            artifacts_dir,
            fixtures_dir,
            idx_dir,
        }
    })
}

fn role_scope_state(store_dir: &Path) -> AppState {
    let world = world();
    let store = Arc::new(AccessRequestStore::open(store_dir).expect("open access request store"));
    AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
        .with_people()
        .expect("load + verify people")
        .with_access_requests(store)
}

#[derive(Debug, Deserialize)]
struct PeopleFile {
    people: Vec<Person>,
}

#[derive(Debug, Deserialize)]
struct Person {
    department_label: String,
    id: String,
    manages: Vec<String>,
    projects: Vec<Project>,
    title: String,
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

fn people_file() -> PeopleFile {
    serde_json::from_slice(
        &fs::read(common::repo_fixtures_dir().join("people.json")).expect("people"),
    )
    .expect("people parse")
}

fn employee_fixture() -> String {
    people_file()
        .people
        .into_iter()
        .find(|person| {
            let title = person.title.to_ascii_lowercase();
            person.manages.is_empty()
                && !person.department_label.eq("Executive")
                && !title.contains("head")
                && !title.contains("chief")
                && !title.contains("director")
        })
        .map(|person| person.id)
        .expect("ordinary employee")
}

fn manager_fixture() -> String {
    people_file()
        .people
        .into_iter()
        .find(|person| {
            !person.manages.is_empty() && person.title.to_ascii_lowercase().contains("head")
        })
        .map(|person| person.id)
        .expect("department head")
}

fn executive_fixture() -> String {
    people_file()
        .people
        .into_iter()
        .find(|person| {
            let title = person.title.to_ascii_lowercase();
            person.department_label == "Executive"
                || title.contains("chief")
                || title.contains("director")
                || title.contains("company secretary")
        })
        .map(|person| person.id)
        .expect("executive candidate")
}

fn request_fixture() -> (String, String, String) {
    let fixtures = common::repo_fixtures_dir();
    let people = people_file();
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
    (requester, approver, capability)
}

async fn get(router: &axum::Router, principal: &str, uri: &str) -> (StatusCode, Value, Vec<u8>) {
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
        .expect("body")
        .to_vec();
    let value = serde_json::from_slice(&bytes).expect("json");
    (status, value, bytes)
}

async fn post_json(
    router: &axum::Router,
    principal: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
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
        .expect("body");
    (status, serde_json::from_slice(&bytes).expect("json"))
}

fn assert_sensitive_surfaces_denied(body: &Value) {
    assert_eq!(body["admin_surface_allowed"], false);
    assert_eq!(body["bursar_surface_allowed"], false);
    assert_eq!(body["governance_surface_allowed"], false);
    assert_eq!(body["enforcement"], "derived_only");
    assert_ne!(body["derived_level"], "super_admin");
}

fn assert_no_hidden_document_side_channel(bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    assert!(!text.contains("document_id"));
    assert!(!text.contains("documents"));
    assert!(!text.contains("holdings"));
    assert!(!text.contains("visible_documents"));
    assert!(!text.contains("denied"));
    assert!(!text.contains("hidden"));
}

#[tokio::test]
async fn ordinary_employee_scope_is_derived_only_and_sanitized() {
    let actor = employee_fixture();
    let router = app(Arc::new(role_scope_state(&scratch(
        "role_scope_employee_store",
    ))));

    let (status, body, bytes) = get(&router, &actor, "/me/scope").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["actor_id"], actor);
    assert_eq!(body["derived_level"], "employee");
    assert_eq!(body["team_scope"]["has_team_scope"], false);
    assert_sensitive_surfaces_denied(&body);
    assert_no_hidden_document_side_channel(&bytes);
}

#[tokio::test]
async fn team_scope_does_not_grant_admin_or_governance_surfaces() {
    let actor = manager_fixture();
    let router = app(Arc::new(role_scope_state(&scratch(
        "role_scope_manager_store",
    ))));

    let (status, body, bytes) = get(&router, &actor, "/me/scope").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["actor_id"], actor);
    assert_eq!(body["derived_level"], "department_head");
    assert_eq!(body["team_scope"]["has_team_scope"], true);
    assert!(body["team_scope"]["direct_report_count"].as_u64().unwrap() > 0);
    assert_sensitive_surfaces_denied(&body);
    assert_no_hidden_document_side_channel(&bytes);
}

#[tokio::test]
async fn executive_signals_remain_candidates_not_super_admin() {
    let actor = executive_fixture();
    let router = app(Arc::new(role_scope_state(&scratch(
        "role_scope_executive_store",
    ))));

    let (status, body, bytes) = get(&router, &actor, "/me/scope").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["actor_id"], actor);
    assert_eq!(body["derived_level"], "executive_candidate");
    assert_eq!(body["confidence"], "medium");
    assert_sensitive_surfaces_denied(&body);
    assert_no_hidden_document_side_channel(&bytes);
}

#[tokio::test]
async fn approval_scope_counts_assigned_requests_without_granting_access() {
    let (requester, approver, capability) = request_fixture();
    let router = app(Arc::new(role_scope_state(&scratch(
        "role_scope_approval_store",
    ))));

    let (status, created) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "project", "capability_id": capability },
            "justification": "Need scoped context for assigned work."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["request"]["approver_id"], approver);

    let (status, body, bytes) = get(&router, &approver, "/me/scope").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["approval_scope"]["has_approval_scope"], true);
    assert_eq!(body["approval_scope"]["pending_count"], 1);
    assert_sensitive_surfaces_denied(&body);
    assert_no_hidden_document_side_channel(&bytes);
}

#[tokio::test]
async fn unknown_actor_gets_not_found_without_scope_details() {
    let router = app(Arc::new(role_scope_state(&scratch(
        "role_scope_unknown_store",
    ))));

    let (status, body, bytes) = get(&router, "p999", "/me/scope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");
    assert_no_hidden_document_side_channel(&bytes);
}
