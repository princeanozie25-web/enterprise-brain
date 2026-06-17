//! The authz read surface fails closed on a mismatched/corrupt compiled model.
//!
//! AuthzView::load refuses (returns Err, in release as well as debug) an
//! artifact set whose snapshot or principal disagrees with the index — it never
//! silently accepts an inconsistent model, mirroring the compiler's own verify.

mod common;

use std::path::Path;

use common::scratch;
use wiki::authz::AuthzView;

fn write(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).expect("write fixture json");
}

const INDEX_SNAP_A: &str = r#"{"snapshot_version":"snapA","principals":[{"principal_id":"p1","artifact_file":"p1.json"}]}"#;

#[test]
fn load_accepts_a_consistent_model() {
    let dir = scratch("authz_ok");
    write(&dir, "index.json", INDEX_SNAP_A);
    write(
        &dir,
        "p1.json",
        r#"{"principal_id":"p1","denied_count":0,"entries":[],"snapshot_version":"snapA"}"#,
    );
    let view = AuthzView::load(&dir).expect("consistent model loads");
    assert_eq!(view.snapshot_version(), "snapA");
    assert_eq!(view.principal_count(), 1);
}

#[test]
fn load_refuses_snapshot_mismatch() {
    let dir = scratch("authz_snap_mismatch");
    write(&dir, "index.json", INDEX_SNAP_A);
    write(
        &dir,
        "p1.json",
        r#"{"principal_id":"p1","denied_count":0,"entries":[],"snapshot_version":"snapB"}"#,
    );
    let err = AuthzView::load(&dir)
        .expect_err("snapshot mismatch must be refused")
        .to_string();
    assert!(err.contains("snapshot"), "error names the mismatch: {err}");
}

#[test]
fn load_refuses_principal_mismatch() {
    let dir = scratch("authz_principal_mismatch");
    write(&dir, "index.json", INDEX_SNAP_A);
    write(
        &dir,
        "p1.json",
        r#"{"principal_id":"p2","denied_count":0,"entries":[],"snapshot_version":"snapA"}"#,
    );
    let err = AuthzView::load(&dir)
        .expect_err("principal mismatch must be refused")
        .to_string();
    assert!(err.contains("principal"), "error names the mismatch: {err}");
}
