//! Access-grant governance. Grants are append-only read entitlements derived
//! from approved access requests. They do not mutate compiled artifacts,
//! retrieval indexes, or document allowlists, and they expose no raw document
//! identifiers.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::index::{build_index, sha256_hex};
use serde::Deserialize;
use serde_json::{json, Value};
use service::access_grants::{AccessGrantStore, AuditEvent};
use service::access_requests::{
    AccessDecision, AccessRequest, AccessRequestStore, AccessTarget, STATUS_APPROVED,
};
use service::{app, AppState};
use tower::ServiceExt;

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    for attempt in 0u64..50 {
        let _ = fs::remove_dir_all(&dir);
        if fs::create_dir_all(&dir).is_ok()
            && fs::read_dir(&dir)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false)
        {
            return dir;
        }
        std::thread::sleep(std::time::Duration::from_millis(20 * (attempt.min(5) + 1)));
    }
    panic!("scratch dir {name} could not be reset");
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
        let artifacts_dir = scratch("access_grant_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let idx_dir = scratch("access_grant_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        World {
            artifacts_dir,
            fixtures_dir,
            idx_dir,
        }
    })
}

fn access_grant_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn grant_state(store_dir: &Path) -> (AppState, Arc<AccessRequestStore>, Arc<AccessGrantStore>) {
    let world = world();
    let requests =
        Arc::new(AccessRequestStore::open(store_dir).expect("open access request store"));
    let grants = Arc::new(AccessGrantStore::open(store_dir).expect("open access grant store"));
    let state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
        .with_people()
        .expect("load + verify people")
        .with_access_requests(requests.clone())
        .with_access_grants(grants.clone());
    (state, requests, grants)
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

fn request_fixture() -> (String, String, String, String, String) {
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
    let all_capabilities: BTreeSet<String> = people
        .people
        .iter()
        .flat_map(|person| {
            person
                .projects
                .iter()
                .map(|project| project.capability_id.clone())
        })
        .collect();
    let (requester, own_capability, outside_capability) = people
        .people
        .iter()
        .find_map(|person| {
            let manager = managers.get(&person.id)?;
            let own: BTreeSet<&str> = person
                .projects
                .iter()
                .map(|project| project.capability_id.as_str())
                .collect();
            let own_capability = person.projects.first()?.capability_id.clone();
            let outside = all_capabilities
                .iter()
                .find(|capability| !own.contains(capability.as_str()))?
                .clone();
            Some((person.id.clone(), own_capability, outside, manager.clone()))
        })
        .map(
            |(requester, own_capability, outside_capability, _manager)| {
                (requester, own_capability, outside_capability)
            },
        )
        .expect("managed person with an outside capability");
    let approver = managers[&requester].clone();
    let non_party = company
        .people
        .iter()
        .map(|person| person.id.clone())
        .find(|id| id != &requester && id != &approver)
        .expect("third human");
    (
        requester,
        approver,
        non_party,
        own_capability,
        outside_capability,
    )
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

fn read_grant_audit(store_dir: &Path) -> Vec<AuditEvent> {
    let text = fs::read_to_string(store_dir.join("access_grants_audit.jsonl")).unwrap_or_default();
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("audit row parses"))
        .collect()
}

fn capability_doc_ids(capability_id: &str) -> BTreeSet<String> {
    let fixtures = common::repo_fixtures_dir();
    let brm: Value = serde_json::from_slice(&fs::read(fixtures.join("brm.json")).expect("brm"))
        .expect("brm parse");
    brm["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .find(|capability| capability["id"] == capability_id)
        .expect("capability exists")["document_ids"]
        .as_array()
        .expect("document_ids")
        .iter()
        .map(|id| id.as_str().expect("doc id").to_string())
        .collect()
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

#[tokio::test]
async fn approved_request_creates_read_grant_without_mutating_compiled_access() {
    let _guard = access_grant_test_lock().lock().expect("test lock");
    let store_dir = scratch("access_grant_create_store");
    let (requester, approver, non_party, capability, _outside_capability) = request_fixture();
    let world = world();
    let before_artifacts = hash_tree(&world.artifacts_dir);
    let before_idx = hash_tree(&world.idx_dir);
    let (state, _requests, grants) = grant_state(&store_dir);
    let router = app(Arc::new(state));

    let (status, created, _) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "capability", "capability_id": capability },
            "justification": "Need read context for approved project work."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request_id = created["request"]["request_id"].as_str().unwrap();

    let (status, approved, approved_bytes) = post_json(
        &router,
        &approver,
        &format!("/access-requests/{request_id}/approve"),
        json!({ "reason_code": "manager_approved" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["request"]["status"], "approved");
    assert!(!String::from_utf8_lossy(&approved_bytes).contains("document_id"));

    let (status, grant_list, list_bytes) = get(&router, &requester, "/access-grants").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!String::from_utf8_lossy(&list_bytes).contains("document_id"));
    assert!(!String::from_utf8_lossy(&list_bytes).contains("denied"));
    let grant = &grant_list["grants"].as_array().unwrap()[0];
    let grant_id = grant["grant_id"].as_str().unwrap();
    assert!(grant_id.starts_with("ag_"));
    assert_eq!(grant["request_id"], request_id);
    assert_eq!(grant["grantee_id"], requester);
    assert_eq!(grant["approver_id"], approver);
    assert_eq!(grant["permission"], "read");
    assert_eq!(grant["status"], "active");
    assert_eq!(grant["reason"], "manager_approved");
    assert_eq!(grant["target"]["capability_id"], capability);
    assert!(grants.has_active_read_for(
        &requester,
        &capability,
        grant["snapshot_version"].as_str().unwrap()
    ));

    let (status, grant_body, get_bytes) =
        get(&router, &requester, &format!("/access-grants/{grant_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(grant_body["grant"]["grant_id"], grant_id);
    assert!(!String::from_utf8_lossy(&get_bytes).contains("document_id"));

    let (status, _approver_body, _bytes) =
        get(&router, &approver, &format!("/access-grants/{grant_id}")).await;
    assert_eq!(status, StatusCode::OK);
    let (status, hidden_body, hidden_bytes) =
        get(&router, &non_party, &format!("/access-grants/{grant_id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(hidden_body["error"], "not found");
    assert!(!String::from_utf8_lossy(&hidden_bytes).contains(grant_id));

    drop(router);
    assert_eq!(before_artifacts, hash_tree(&world.artifacts_dir));
    assert_eq!(before_idx, hash_tree(&world.idx_dir));

    let audit = read_grant_audit(&store_dir);
    assert!(audit.iter().any(|row| {
        row.action == "access_grant_create"
            && row.actor_principal == approver
            && row.outcome == "allowed"
    }));
    assert!(audit.iter().any(|row| {
        row.action == "access_grant_get"
            && row.actor_principal == non_party
            && row.outcome == "refused_not_party"
    }));
}

#[tokio::test]
async fn read_grant_opens_project_workflow_context_without_document_ids() {
    let _guard = access_grant_test_lock().lock().expect("test lock");
    let store_dir = scratch("access_grant_workflow_store");
    let (requester, approver, _non_party, _own_capability, outside_capability) = request_fixture();
    let (state, _requests, _grants) = grant_state(&store_dir);
    let router = app(Arc::new(state));

    let (status, before, before_bytes) = get(
        &router,
        &requester,
        &format!("/workflow/project/{outside_capability}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(before["error"], "not found");
    assert!(!String::from_utf8_lossy(&before_bytes).contains("document_id"));

    let (status, created, _) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "project", "capability_id": outside_capability },
            "justification": "Need approved read context for this capability."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request_id = created["request"]["request_id"].as_str().unwrap();

    let (status, _approved, _) = post_json(
        &router,
        &approver,
        &format!("/access-requests/{request_id}/approve"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, workflow, workflow_bytes) = get(
        &router,
        &requester,
        &format!("/workflow/project/{outside_capability}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(workflow["capability_id"], outside_capability);
    assert!(workflow["items"].as_array().unwrap().iter().any(|item| {
        item["kind"] == "access_request"
            && item["item_id"] == request_id
            && item["status"] == "approved"
    }));
    let text = String::from_utf8_lossy(&workflow_bytes);
    assert!(!text.contains("document_id"));
    assert!(!text.contains("evidence"));
    assert!(!text.contains("denied"));
}

#[tokio::test]
async fn read_grant_can_scope_ask_to_granted_capability_context_only() {
    let _guard = access_grant_test_lock().lock().expect("test lock");
    let store_dir = scratch("access_grant_ask_store");
    let (requester, approver, _non_party, own_capability, outside_capability) = request_fixture();
    let (state, _requests, _grants) = grant_state(&store_dir);
    let router = app(Arc::new(state));

    let (status, created, _) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "project", "capability_id": outside_capability },
            "justification": "Need approved ask context for this capability."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request_id = created["request"]["request_id"].as_str().unwrap();

    let (status, _approved, _) = post_json(
        &router,
        &approver,
        &format!("/access-requests/{request_id}/approve"),
        json!({ "reason_code": "manager_approved" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, grant_list, _) = get(&router, &requester, "/access-grants").await;
    assert_eq!(status, StatusCode::OK);
    let grant_id = grant_list["grants"].as_array().unwrap()[0]["grant_id"]
        .as_str()
        .unwrap();

    let (status, ask_body, _ask_bytes) = post_json(
        &router,
        &requester,
        "/ask",
        json!({
            "query": "procedure record review quality customer stock site warehouse",
            "grant_id": grant_id,
            "capability_id": outside_capability,
            "hybrid": false,
            "judge": false
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let context = &ask_body["granted_context"];
    assert_eq!(context["grant_id"], grant_id);
    assert_eq!(context["grant_status"], "active");
    assert_eq!(context["capability"]["id"], outside_capability);
    assert_eq!(context["request_id"], request_id);
    assert_eq!(context["approver_id"], approver);
    assert_eq!(context["active"], true);
    let context_text = serde_json::to_string(context).expect("context json");
    assert!(!context_text.contains("document_id"));
    assert!(!context_text.contains("denied"));
    assert!(!context_text.contains("\"permission\":\"write\""));
    assert!(!context_text.contains("\"permission\":\"admin\""));

    let allowed = capability_doc_ids(&outside_capability);
    for result in ask_body["results"].as_array().expect("results") {
        let document_id = result["document_id"].as_str().expect("doc id");
        assert!(
            allowed.contains(document_id),
            "granted ask result {document_id} must belong to the granted capability"
        );
    }

    let (status, refused, refused_bytes) = post_json(
        &router,
        &requester,
        "/ask",
        json!({
            "query": "procedure record review quality customer stock site warehouse",
            "grant_id": grant_id,
            "capability_id": own_capability,
            "hybrid": false,
            "judge": false
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(refused["error"], "granted context is unavailable");
    let refused_text = String::from_utf8_lossy(&refused_bytes);
    assert!(!refused_text.contains(grant_id));
    assert!(!refused_text.contains(&outside_capability));
}

#[tokio::test]
async fn approver_can_revoke_and_revoked_grant_no_longer_unlocks_workflow() {
    let _guard = access_grant_test_lock().lock().expect("test lock");
    let store_dir = scratch("access_grant_revoke_store");
    let (requester, approver, _non_party, _own_capability, outside_capability) = request_fixture();
    let (state, _requests, _grants) = grant_state(&store_dir);
    let router = app(Arc::new(state));

    let (status, created, _) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "project", "capability_id": outside_capability },
            "justification": "Need temporary approved read context for this capability."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request_id = created["request"]["request_id"].as_str().unwrap();

    let (status, _approved, _) = post_json(
        &router,
        &approver,
        &format!("/access-requests/{request_id}/approve"),
        json!({ "reason_code": "manager_approved" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, grant_list, list_bytes) = get(&router, &requester, "/access-grants").await;
    assert_eq!(status, StatusCode::OK);
    let grant = &grant_list["grants"].as_array().unwrap()[0];
    let grant_id = grant["grant_id"].as_str().unwrap();
    assert_eq!(grant["status"], "active");
    let list_text = String::from_utf8_lossy(&list_bytes);
    assert!(!list_text.contains("document_id"));
    assert!(!list_text.contains("\"permission\":\"write\""));
    assert!(!list_text.contains("\"permission\":\"admin\""));
    assert!(!list_text.contains("denied"));

    let (status, workflow, workflow_bytes) = get(
        &router,
        &requester,
        &format!("/workflow/project/{outside_capability}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(workflow["capability_id"], outside_capability);
    assert!(!String::from_utf8_lossy(&workflow_bytes).contains("document_id"));

    let (status, revoked, revoked_bytes) = post_json(
        &router,
        &approver,
        &format!("/access-grants/{grant_id}/revoke"),
        json!({ "reason_code": "manager_revoked" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revoked["grant"]["status"], "revoked");
    assert_eq!(revoked["grant"]["revoked_by"], approver);
    assert_eq!(revoked["grant"]["revocation_reason"], "manager_revoked");
    let revoked_text = String::from_utf8_lossy(&revoked_bytes);
    assert!(!revoked_text.contains("document_id"));
    assert!(!revoked_text.contains("\"permission\":\"write\""));
    assert!(!revoked_text.contains("\"permission\":\"admin\""));

    let (status, after, after_bytes) = get(
        &router,
        &requester,
        &format!("/workflow/project/{outside_capability}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(after["error"], "not found");
    let after_text = String::from_utf8_lossy(&after_bytes);
    assert!(!after_text.contains("document_id"));
    assert!(!after_text.contains("denied"));

    let (status, refused_ask, refused_ask_bytes) = post_json(
        &router,
        &requester,
        "/ask",
        json!({
            "query": "procedure record review quality customer stock site warehouse",
            "grant_id": grant_id,
            "capability_id": outside_capability,
            "hybrid": false,
            "judge": false
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(refused_ask["error"], "granted context is unavailable");
    assert!(!String::from_utf8_lossy(&refused_ask_bytes).contains(grant_id));

    let audit = read_grant_audit(&store_dir);
    let allowed_create = audit.iter().position(|row| {
        row.action == "access_grant_create"
            && row.actor_principal == approver
            && row.outcome == "allowed"
    });
    let allowed_revoke = audit.iter().position(|row| {
        row.action == "access_grant_revoke"
            && row.actor_principal == approver
            && row.target == grant_id
            && row.outcome == "allowed"
    });
    assert!(allowed_create.is_some());
    assert!(allowed_revoke.is_some());
    assert!(allowed_revoke.unwrap() > allowed_create.unwrap());
}

#[tokio::test]
async fn grant_revoke_refuses_grantee_and_unrelated_actor() {
    let _guard = access_grant_test_lock().lock().expect("test lock");
    let store_dir = scratch("access_grant_revoke_refuse_store");
    let (requester, approver, non_party, _own_capability, outside_capability) = request_fixture();
    let (state, _requests, _grants) = grant_state(&store_dir);
    let router = app(Arc::new(state));

    let (status, created, _) = post_json(
        &router,
        &requester,
        "/access-requests",
        json!({
            "target": { "kind": "project", "capability_id": outside_capability },
            "justification": "Need approved read context for this capability."
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let request_id = created["request"]["request_id"].as_str().unwrap();
    let (status, _approved, _) = post_json(
        &router,
        &approver,
        &format!("/access-requests/{request_id}/approve"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, grant_list, _) = get(&router, &requester, "/access-grants").await;
    assert_eq!(status, StatusCode::OK);
    let grant_id = grant_list["grants"].as_array().unwrap()[0]["grant_id"]
        .as_str()
        .unwrap();

    let (status, refused_grantee, _) = post_json(
        &router,
        &requester,
        &format!("/access-grants/{grant_id}/revoke"),
        json!({ "reason_code": "self_revoke_not_modelled" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(refused_grantee["error"], "forbidden");

    let (status, refused_non_party, hidden_bytes) = post_json(
        &router,
        &non_party,
        &format!("/access-grants/{grant_id}/revoke"),
        json!({ "reason_code": "unrelated_actor" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(refused_non_party["error"], "not found");
    assert!(!String::from_utf8_lossy(&hidden_bytes).contains(grant_id));

    let (status, grant_body, _) =
        get(&router, &requester, &format!("/access-grants/{grant_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(grant_body["grant"]["status"], "active");

    let audit = read_grant_audit(&store_dir);
    assert!(audit.iter().any(|row| {
        row.action == "access_grant_revoke"
            && row.actor_principal == requester
            && row.outcome == "refused_not_approver"
    }));
    assert!(audit.iter().any(|row| {
        row.action == "access_grant_revoke"
            && row.actor_principal == non_party
            && row.outcome == "refused_not_party"
    }));
}

#[tokio::test]
async fn expired_grant_no_longer_unlocks_context() {
    let _guard = access_grant_test_lock().lock().expect("test lock");
    let store_dir = scratch("access_grant_expired_store");
    let (requester, approver, _non_party, _own_capability, outside_capability) = request_fixture();
    let (state, _requests, grants) = grant_state(&store_dir);
    let snapshot_version = state.snapshot_version.clone();
    let target = AccessTarget::Project {
        capability_id: outside_capability.clone(),
    };
    let request = AccessRequest {
        approver_id: approver.clone(),
        created_ordinal: 0,
        decision: Some(AccessDecision {
            actor_principal: approver,
            decided_ordinal: 1,
            outcome: STATUS_APPROVED.to_string(),
            reason_code: Some("manager_approved".to_string()),
        }),
        justification: "Expired read context fixture.".to_string(),
        request_id: "ar_expired_fixture".to_string(),
        request_key: AccessRequestStore::request_key(&requester, &target),
        requester_id: requester.clone(),
        snapshot_version: snapshot_version.clone(),
        status: STATUS_APPROVED.to_string(),
        target,
    };
    let grant = grants
        .create_from_approved_request_with_expiry(&request, Some(snapshot_version.clone()))
        .expect("create expired grant");
    let grant_id = match grant {
        service::access_grants::GrantCreateOutcome::Created(grant) => grant.grant_id.clone(),
        service::access_grants::GrantCreateOutcome::Existing(grant) => grant.grant_id.clone(),
    };
    let router = app(Arc::new(state));

    let (status, workflow, workflow_bytes) = get(
        &router,
        &requester,
        &format!("/workflow/project/{outside_capability}"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(workflow["error"], "not found");
    assert!(!String::from_utf8_lossy(&workflow_bytes).contains("document_id"));

    let (status, grant_list, list_bytes) = get(&router, &requester, "/access-grants").await;
    assert_eq!(status, StatusCode::OK);
    let grants = grant_list["grants"].as_array().unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0]["grant_id"], grant_id);
    assert_eq!(grants[0]["status"], "expired");
    let text = String::from_utf8_lossy(&list_bytes);
    assert!(!text.contains("document_id"));
    assert!(!text.contains("\"permission\":\"write\""));
    assert!(!text.contains("\"permission\":\"admin\""));
    assert!(!text.contains("denied"));

    let (status, refused_ask, refused_ask_bytes) = post_json(
        &router,
        &requester,
        "/ask",
        json!({
            "query": "procedure record review quality customer stock site warehouse",
            "grant_id": grant_id,
            "capability_id": outside_capability,
            "hybrid": false,
            "judge": false
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(refused_ask["error"], "granted context is unavailable");
    assert!(!String::from_utf8_lossy(&refused_ask_bytes).contains(&grant_id));
}
