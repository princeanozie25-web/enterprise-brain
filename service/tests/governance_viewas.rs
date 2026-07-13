//! AUTH-3 (FC-A3) — admin-classed, audited view-as. The matrix that AUTH-2's
//! denial set to 404, now restored under a gate:
//!   * cross-principal /lens/{other} and /diff are allowed iff demo_identity_mode
//!     OR the viewer is admin; otherwise the one 404 (the AUTH-2 boundary);
//!   * every ALLOWED view-as is AUDITED BEFORE RENDER (viewer, target, action);
//!   * an UNAUDITABLE view-as is forbidden (fail-closed);
//!   * view-as returns the TARGET's own scoped view — never a scope bypass.
//! The demo-mode 200+audit path is also exercised by governance_al / _ad / _ae;
//! this suite owns the real-mode (demo off) denial + the fail-closed + no-bypass
//! cases that need a toggled flag.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use service::agent::proposals::{AuditEvent, ProposalStore};
use service::{app, AppState};
use tower::ServiceExt;

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
    // never re-opens a dying path.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // The base lives in the SYSTEM temp dir, not target/tmp: the repo sits
    // under Documents\, which Windows Search indexes by default — its crawler
    // opens freshly written index segments mid-build and the write fails with
    // os error 5. AppData\Local\Temp is outside the default index scope.
    let base = std::env::temp_dir().join("enterprise-brain-test-scratch");
    std::fs::create_dir_all(&base).expect("scratch base");
    // CREATE-ONLY: the unique pid+seq suffix already guarantees no collision,
    // so this helper never deletes a sibling. Reaping by shared name-prefix
    // raced a concurrently running test in the same binary that used the same
    // name and deleted its live dir (failed estate_probes on Linux CI). Stale
    // dirs from old runs are harmless; the OS temp cleaner reaps them.
    let dir = base.join(format!(
        "{name}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// AUTH-3 state. `demo` toggles demo-identity mode (real mode = off, admin-only);
/// `store_dir` wires the append-only audit sink (None = no sink → view-as cannot
/// be recorded → forbidden).
fn viewas_state(demo: bool, store_dir: Option<&Path>) -> AppState {
    let state = AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("people")
    .with_demo_identity_mode(demo);
    match store_dir {
        Some(dir) => state.with_proposals(Arc::new(ProposalStore::open(dir).expect("open audit store"))),
        None => state,
    }
}

async fn get_lens(router: &axum::Router, actor: &str, subject: &str) -> (StatusCode, Vec<u8>) {
    let auth = common::bearer(router, actor).await;
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/lens/{subject}"))
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec();
    (status, bytes)
}

async fn get_diff(router: &axum::Router, actor: &str, left: &str, right: &str) -> (StatusCode, Vec<u8>) {
    let auth = common::bearer(router, actor).await;
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/lens/diff?left={left}&right={right}"))
                .header("authorization", auth)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec();
    (status, bytes)
}

fn read_audit(store_dir: &Path) -> Vec<AuditEvent> {
    fs::read_to_string(store_dir.join("audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("audit row"))
        .collect()
}

// VA-1: demo mode -> cross-lens allowed, audited before render (viewer, target).
#[tokio::test]
async fn va1_demo_mode_cross_lens_allowed_and_audited() {
    let store = scratch("va1_store");
    let router = app(Arc::new(viewas_state(true, Some(&store))));

    let (status, body) = get_lens(&router, "p060", "p088").await;
    assert_eq!(status, StatusCode::OK, "demo-mode cross-lens -> 200");
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["cross_lens"], Value::Bool(true), "marked as a cross-lens act");
    assert_eq!(v["actor_id"], "p060", "the viewer is recorded");

    let audit = read_audit(&store);
    let row = audit
        .iter()
        .find(|e| e.action == "lens_view")
        .expect("a lens_view audit row exists (audited before render)");
    assert_eq!(row.actor_principal, "p060", "audit names the viewer");
    assert_eq!(row.target, "p088", "audit names the target");
    println!("VA-1: demo-mode cross-lens p060->p088 -> 200, audited (viewer+target)");
}

// VA-2: demo mode -> diff allowed, audited (lens_diff).
#[tokio::test]
async fn va2_demo_mode_diff_allowed_and_audited() {
    let store = scratch("va2_store");
    let router = app(Arc::new(viewas_state(true, Some(&store))));

    let (status, _) = get_diff(&router, "p060", "p016", "p088").await;
    assert_eq!(status, StatusCode::OK, "demo-mode diff -> 200");
    assert!(
        read_audit(&store).iter().any(|e| e.action == "lens_diff"),
        "a lens_diff audit row exists"
    );
    println!("VA-2: demo-mode diff -> 200, audited (lens_diff)");
}

// VA-3 (the core AUTH-3 gate): real mode (demo off) + non-admin -> 404, and the
// denied view-as is unaudited. Self-lens still works (not a view-as).
#[tokio::test]
async fn va3_real_mode_non_admin_view_as_denied_and_unaudited() {
    let store = scratch("va3_store");
    let router = app(Arc::new(viewas_state(false, Some(&store)))); // demo OFF; corpus has no admin

    let (ls, _) = get_lens(&router, "p060", "p088").await;
    assert_eq!(ls, StatusCode::NOT_FOUND, "real-mode non-admin cross-lens -> 404");
    let (ds, _) = get_diff(&router, "p060", "p016", "p088").await;
    assert_eq!(ds, StatusCode::NOT_FOUND, "real-mode non-admin diff -> 404");

    // A self lens is not a view-as: it works even with demo off.
    let (ss, _) = get_lens(&router, "p060", "p060").await;
    assert_eq!(ss, StatusCode::OK, "self-lens -> 200 even in real mode");

    assert!(
        read_audit(&store)
            .iter()
            .all(|e| e.action != "lens_view" && e.action != "lens_diff"),
        "a denied view-as writes no audit row"
    );
    println!("VA-3: real mode (demo off), non-admin view-as -> 404, unaudited; self-lens still 200");
}

// VA-4: view-as returns the TARGET's own scoped view — never more. The holdings
// p060 sees viewing p088 == the holdings p088 sees viewing itself.
#[tokio::test]
async fn va4_view_as_returns_targets_scoped_view_no_bypass() {
    let store = scratch("va4_store");
    let router = app(Arc::new(viewas_state(true, Some(&store))));

    let (status, body) = get_lens(&router, "p060", "p088").await;
    assert_eq!(status, StatusCode::OK);
    let view_as: Value = serde_json::from_slice(&body).unwrap();

    let (sstatus, sbody) = get_lens(&router, "p088", "p088").await;
    assert_eq!(sstatus, StatusCode::OK);
    let self_view: Value = serde_json::from_slice(&sbody).unwrap();

    assert_eq!(
        view_as["holdings"], self_view["holdings"],
        "view-as of p088 shows exactly p088's own scoped holdings (no bypass)"
    );
    println!("VA-4: view-as(p060->p088).holdings == self(p088).holdings — target's scoped view, no bypass");
}

// VA-5: an UNAUDITABLE view-as is forbidden (fail-closed). Demo on, but no audit
// sink -> the act cannot be recorded -> it must not render. Self needs no audit.
#[tokio::test]
async fn va5_unauditable_view_as_is_forbidden_fail_closed() {
    let router = app(Arc::new(viewas_state(true, None))); // no audit store

    let (status, _) = get_lens(&router, "p060", "p088").await;
    assert_ne!(status, StatusCode::OK, "unauditable cross-lens must NOT render");
    let (dstatus, _) = get_diff(&router, "p060", "p016", "p088").await;
    assert_ne!(dstatus, StatusCode::OK, "unauditable diff must NOT render");

    // A self lens is not an audited act -> still 200 without a store.
    let (ss, _) = get_lens(&router, "p060", "p060").await;
    assert_eq!(ss, StatusCode::OK, "self-lens needs no audit");
    println!("VA-5: no audit sink -> view-as forbidden (fail-closed); self-lens still 200");
}
