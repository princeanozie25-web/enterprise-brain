//! Project workflow projection governance.
//!
//! The projection is read-only and item-backed: lane boxes, accepted agent
//! boxes, and access-request rows only. It must not expose evidence rows,
//! document ids, or fabricated task fields.

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
use service::lane::{AcceptedBox, BoxStore};
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
        let artifacts_dir = scratch("workflow_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let idx_dir = scratch("workflow_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        World {
            artifacts_dir,
            fixtures_dir,
            idx_dir,
        }
    })
}

fn workflow_state(
    access_dir: &Path,
    box_dir: &Path,
) -> (AppState, Arc<AccessRequestStore>, Arc<BoxStore>) {
    let world = world();
    let access = Arc::new(AccessRequestStore::open(access_dir).expect("open access store"));
    let boxes = Arc::new(BoxStore::open(box_dir).expect("open box store"));
    let state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
        .with_people()
        .expect("load + verify people")
        .with_access_requests(access.clone())
        .with_lane_boxes(boxes.clone());
    (state, access, boxes)
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

fn request_fixture() -> (String, String, String) {
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

#[tokio::test]
async fn project_workflow_projects_lane_items_without_document_ids() {
    let (actor, _approver, capability) = request_fixture();
    let (state, _access, _boxes) = workflow_state(
        &scratch("workflow_lane_access_store"),
        &scratch("workflow_lane_box_store"),
    );
    let router = app(Arc::new(state));

    let (status, body, bytes) =
        get(&router, &actor, &format!("/workflow/project/{capability}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["capability_id"], capability);
    assert_eq!(body["demo_identity_mode"], true);
    let items = body["items"].as_array().expect("items");
    assert!(items.iter().any(|item| item["kind"] == "lane_box"));
    assert!(items.iter().all(|item| item["capability_id"] == capability));

    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("document_id"));
    assert!(!text.contains("evidence"));
}

#[tokio::test]
async fn project_workflow_includes_access_requests_for_requester_and_approver() {
    let (requester, approver, capability) = request_fixture();
    let (state, _access, _boxes) = workflow_state(
        &scratch("workflow_access_request_store"),
        &scratch("workflow_access_box_store"),
    );
    let router = app(Arc::new(state));

    let (status, created) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "project", "capability_id": capability },
            "justification": "Need workflow context for my assigned project."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request_id = created["request"]["request_id"].as_str().unwrap();

    for actor in [&requester, &approver] {
        let (status, body, bytes) =
            get(&router, actor, &format!("/workflow/project/{capability}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["items"].as_array().unwrap().iter().any(|item| {
            item["kind"] == "access_request"
                && item["item_id"] == request_id
                && item["status"] == "pending"
                && item["requester_id"] == requester
                && item["approver_id"] == approver
        }));
        assert!(!String::from_utf8_lossy(&bytes).contains("document_id"));
    }
}

#[tokio::test]
async fn project_workflow_includes_accepted_agent_boxes() {
    let (actor, _approver, capability) = request_fixture();
    let (state, _access, boxes) = workflow_state(
        &scratch("workflow_accepted_access_store"),
        &scratch("workflow_accepted_box_store"),
    );
    let snapshot_version = state.snapshot_version.clone();
    boxes
        .record_accepted(AcceptedBox {
            agent_id: "agent_finance_analyst".to_string(),
            box_id: "accepted_agent_projection".to_string(),
            capability_id: capability.clone(),
            citations: Vec::new(),
            created_ordinal: 0,
            principal: actor.clone(),
            proposal_id: "proposal_projection".to_string(),
            snapshot_version,
            standing_query: "Review accepted agent proposal".to_string(),
        })
        .expect("accepted box");
    let router = app(Arc::new(state));

    let (status, body, bytes) =
        get(&router, &actor, &format!("/workflow/project/{capability}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"].as_array().unwrap().iter().any(|item| {
        item["kind"] == "accepted_agent_box"
            && item["item_id"] == "accepted_agent_projection"
            && item["agent_id"] == "agent_finance_analyst"
    }));
    assert!(!String::from_utf8_lossy(&bytes).contains("document_id"));
}

#[tokio::test]
async fn project_workflow_refuses_unknown_capabilities() {
    let (actor, _approver, _capability) = request_fixture();
    let (state, _access, _boxes) = workflow_state(
        &scratch("workflow_unknown_access_store"),
        &scratch("workflow_unknown_box_store"),
    );
    let router = app(Arc::new(state));

    let (status, body, bytes) = get(&router, &actor, "/workflow/project/not_a_capability").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");
    assert!(!String::from_utf8_lossy(&bytes).contains("document_id"));
}
