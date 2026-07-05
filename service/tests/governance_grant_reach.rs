//! AUTH-2b (FC-A2 completion) — grant/capability reachability on the metadata
//! surfaces. The metadata oracle (metadata_conformance.rs) proves the projection
//! matches the independent expectation across all 15,500 pairs; THIS suite is the
//! positive witness that grant-reachability actually FIRES end-to-end (a 0/0
//! oracle alone could not distinguish a real grant edge from a symmetric no-op in
//! both projection and expectation). Every id below is a verified corpus fact.
//!
//! Corpus anchors:
//!   * p001 (Quality & Compliance) holds grp_board (a cross-cutting group) whose
//!     members include p087 (HR) and p060 (Finance) — cross-department reach.
//!   * p002 is ALSO Quality & Compliance but is NOT in grp_board — the no-bypass
//!     discriminator: same department as p001, yet cannot see p087.
//!   * agent_qa_drafter (owner p093, Sales & Accounts) holds grp_qa_release whose
//!     members are all Quality & Compliance (incl p001) — the agent's grant
//!     reaches nodes its owner p093 cannot.
//!   * p_void holds no group — no standing, no grants, sees nothing.

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

fn meta_state() -> AppState {
    AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("load + verify people.json")
}

/// HTTP status of GET /node/{id}/summary as `actor`.
async fn node_status(router: &axum::Router, actor: &str, id: &str) -> StatusCode {
    let auth = common::bearer(router, actor).await;
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/node/{id}/summary"))
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

/// The person-node ids GET /graph returns for `actor`. Panics if /graph is not 200.
async fn graph_people(router: &axum::Router, actor: &str) -> Vec<String> {
    let auth = common::bearer(router, actor).await;
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/graph")
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "/graph for {actor} -> 200");
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    v["people"]
        .as_array()
        .expect("graph has a people array")
        .iter()
        .map(|p| p["id"].as_str().expect("person id").to_string())
        .collect()
}

// GR-1: a cross-department grant node is visible to the grant holder. p001 (QC)
// sees p087 (HR) and p060 (Finance) via grp_board, on BOTH /node and /graph.
#[tokio::test]
async fn gr1_cross_department_grant_node_is_visible() {
    let router = app(Arc::new(meta_state()));
    assert_eq!(node_status(&router, "p001", "p087").await, StatusCode::OK, "p001 -> p087 (HR) via grp_board");
    assert_eq!(node_status(&router, "p001", "p060").await, StatusCode::OK, "p001 -> p060 (Finance) via grp_board");
    let people = graph_people(&router, "p001").await;
    assert!(people.contains(&"p087".to_string()), "p087 appears in p001's /graph");
    assert!(people.contains(&"p060".to_string()), "p060 appears in p001's /graph");
    println!("GR-1: p001 reaches cross-dept p087/p060 via grp_board on /node + /graph");
}

// GR-2 (NO BYPASS — the discriminator): p002 is the SAME department as p001 but
// does NOT hold grp_board, so it cannot see p087. Identical department, different
// grant -> different reach. A node is visible ONLY because the actor holds the grant.
#[tokio::test]
async fn gr2_without_the_grant_the_node_is_not_visible() {
    let router = app(Arc::new(meta_state()));
    assert_eq!(
        node_status(&router, "p002", "p087").await,
        StatusCode::NOT_FOUND,
        "p002 (QC, not in grp_board) cannot see p087 (HR) -> 404"
    );
    let people = graph_people(&router, "p002").await;
    assert!(!people.contains(&"p087".to_string()), "p087 is absent from p002's /graph");
    // And p002 IS in p001's department, so p001 still sees p002 structurally (GR-3).
    println!("GR-2: p002 (same dept as p001, no grp_board) -> p087 NOT visible (no bypass)");
}

// GR-3: structural visibility is UNCHANGED — grant-reachability only adds. p001
// still sees a same-department peer (structural), and still does NOT see a node
// it neither shares a department nor a group with.
#[tokio::test]
async fn gr3_structural_visibility_is_unchanged() {
    let router = app(Arc::new(meta_state()));
    assert_eq!(node_status(&router, "p001", "p002").await, StatusCode::OK, "p001 -> p002 same-dept (structural) still 200");
    assert_eq!(
        node_status(&router, "p001", "p018").await,
        StatusCode::NOT_FOUND,
        "p001 -> p018 (Warehouse, no shared dept/group) still 404"
    );
    println!("GR-3: structural unchanged — p001 sees same-dept p002, still cannot see unrelated p018");
}

// GR-4: an AGENT's grant reaches nodes its OWNER cannot. agent_qa_drafter holds
// grp_qa_release (all Quality & Compliance), so it reaches p001; its owner p093
// (Sales & Accounts, only grp_sales_accounts) does not.
#[tokio::test]
async fn gr4_agent_grant_reaches_beyond_its_owner() {
    let router = app(Arc::new(meta_state()));
    assert_eq!(
        node_status(&router, "agent_qa_drafter", "p001").await,
        StatusCode::OK,
        "agent_qa_drafter -> p001 via grp_qa_release"
    );
    assert_eq!(
        node_status(&router, "p093", "p001").await,
        StatusCode::NOT_FOUND,
        "owner p093 (Sales) does NOT hold the agent's grant -> p001 not visible"
    );
    println!("GR-4: agent_qa_drafter reaches p001 via its grant; owner p093 does not");
}

// GR-5: p_void holds no group — no standing, no grants. Empty graph, every node 404.
#[tokio::test]
async fn gr5_p_void_has_no_standing_and_no_grants() {
    let router = app(Arc::new(meta_state()));
    let people = graph_people(&router, "p_void").await; // known principal, empty projection -> 200 empty
    assert!(people.is_empty(), "p_void's /graph has no people");
    assert_eq!(node_status(&router, "p_void", "p087").await, StatusCode::NOT_FOUND, "p_void -> p087 404");
    assert_eq!(node_status(&router, "p_void", "org").await, StatusCode::NOT_FOUND, "p_void -> org 404 (no standing)");
    println!("GR-5: p_void — no standing, no grants, empty graph, all nodes 404");
}
