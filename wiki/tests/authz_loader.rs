//! The authz read surface fails closed on a mismatched/corrupt/tampered model.
//!
//! AuthzView::load refuses (returns Err, in release as well as debug) an artifact
//! set whose per-artifact CONTENT HASH, snapshot, or principal disagrees with the
//! index — it never silently accepts an inconsistent or tampered model, mirroring
//! the compiler's `verify_artifacts` and `retrieval::PrincipalScope`'s discipline.

mod common;

use std::path::Path;

use common::scratch;
use wiki::authz::AuthzView;

fn write(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).expect("write fixture json");
}

/// The same sha256 the loader (and PrincipalScope, and the compiler) compute over
/// raw artifact bytes — so a test's recorded hash matches the loader's check.
fn sha256(body: &str) -> String {
    retrieval::index::sha256_hex(body.as_bytes())
}

/// Writes `p1.json` = `body` and an `index.json` (snapshot `snap`) whose single
/// row records `recorded_hash` as p1.json's content hash. Pass the real hash for a
/// valid index; pass a wrong one to simulate a corrupt/tampered artifact.
fn write_model(dir: &Path, body: &str, snap: &str, recorded_hash: &str) {
    write(dir, "p1.json", body);
    let index = format!(
        r#"{{"snapshot_version":"{snap}","principals":[{{"principal_id":"p1","artifact_file":"p1.json","artifact_sha256":"{recorded_hash}"}}]}}"#
    );
    write(dir, "index.json", &index);
}

const ARTIFACT_A: &str =
    r#"{"principal_id":"p1","denied_count":0,"entries":[],"snapshot_version":"snapA"}"#;

#[test]
fn load_accepts_a_consistent_model() {
    let dir = scratch("authz_ok");
    write_model(&dir, ARTIFACT_A, "snapA", &sha256(ARTIFACT_A));
    let view = AuthzView::load(&dir).expect("consistent model loads");
    assert_eq!(view.snapshot_version(), "snapA");
    assert_eq!(view.principal_count(), 1);
}

#[test]
fn load_refuses_artifact_hash_mismatch() {
    // The index records a hash that does NOT match p1.json's bytes — a corrupt or
    // tampered artifact. Must refuse, fail-closed, before trusting its content.
    let dir = scratch("authz_hash_mismatch");
    write_model(&dir, ARTIFACT_A, "snapA", &sha256("not the artifact bytes"));
    let err = AuthzView::load(&dir)
        .expect_err("a content-hash mismatch must be refused")
        .to_string();
    assert!(
        err.contains("hash"),
        "error names the integrity mismatch: {err}"
    );
}

#[test]
fn load_refuses_snapshot_mismatch() {
    let dir = scratch("authz_snap_mismatch");
    // The hash MATCHES the body, so the snapshot check (not the hash check) refuses.
    let body = r#"{"principal_id":"p1","denied_count":0,"entries":[],"snapshot_version":"snapB"}"#;
    write_model(&dir, body, "snapA", &sha256(body));
    let err = AuthzView::load(&dir)
        .expect_err("snapshot mismatch must be refused")
        .to_string();
    assert!(err.contains("snapshot"), "error names the mismatch: {err}");
}

#[test]
fn load_refuses_principal_mismatch() {
    let dir = scratch("authz_principal_mismatch");
    // The hash MATCHES the body, so the principal check (not the hash check) refuses.
    let body = r#"{"principal_id":"p2","denied_count":0,"entries":[],"snapshot_version":"snapA"}"#;
    write_model(&dir, body, "snapA", &sha256(body));
    let err = AuthzView::load(&dir)
        .expect_err("principal mismatch must be refused")
        .to_string();
    assert!(err.contains("principal"), "error names the mismatch: {err}");
}
