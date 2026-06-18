//! P1-a regression (architecture review): `AuthzView::load` must verify each
//! per-principal artifact's content hash — the same `artifact_sha256` discipline
//! `retrieval::search::PrincipalScope::load` and the compiler's `verify_artifacts`
//! enforce — so a TAMPERED compiled model (an allow injected on disk, labels left
//! intact, index.json NOT rehashed) is refused fail-closed on load, not silently
//! trusted into a widened grant. Before this fix the tampered model loaded.

mod common;

use std::fs;

use common::{compile_artifacts, scratch};
use wiki::authz::AuthzView;

/// Valid, correctly-hashed artifacts still load — no regression for good inputs.
#[test]
fn untampered_artifacts_load_successfully() {
    let artifacts = scratch("authz_tamper_valid");
    compile_artifacts(&artifacts);
    AuthzView::load(&artifacts).expect("untampered, correctly-hashed artifacts load");
}

/// CENTERPIECE: a per-principal artifact body is edited to ADD an allow, keeping
/// the snapshot + principal labels intact and WITHOUT rehashing index.json. The
/// content-hash check must now refuse the load (fail-closed). Before the fix this
/// widened the granted set silently.
#[test]
fn tampered_artifact_body_is_refused_fail_closed() {
    let artifacts = scratch("authz_tamper_evil");
    compile_artifacts(&artifacts);
    // Baseline: the valid model loads before tampering.
    AuthzView::load(&artifacts).expect("valid model loads before tampering");

    // Pick the first per-principal artifact named by the index.
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(artifacts.join("index.json")).unwrap()).unwrap();
    let artifact_file = index["principals"][0]["artifact_file"]
        .as_str()
        .expect("index row has an artifact_file");
    let artifact_path = artifacts.join(artifact_file);

    // Tamper the BODY only: inject an allow entry. Snapshot/principal labels stay
    // intact and index.json is NOT rehashed, so the recorded artifact_sha256 now
    // disagrees with the bytes on disk — exactly the review's repro.
    let mut artifact: serde_json::Value =
        serde_json::from_slice(&fs::read(&artifact_path).unwrap()).unwrap();
    artifact["entries"]
        .as_array_mut()
        .expect("artifact has an entries array")
        .push(serde_json::json!({
            "document_id": "d_tampered_grant",
            "reasons": ["tamper:injected-allow"]
        }));
    fs::write(&artifact_path, serde_json::to_vec(&artifact).unwrap()).unwrap();

    // The integrity check must refuse the tampered model instead of loading the
    // widened grant.
    let err = AuthzView::load(&artifacts)
        .expect_err("a tampered artifact body must be refused, not silently trusted");
    let msg = format!("{err:#}").to_lowercase();
    assert!(
        msg.contains("hash") && msg.contains("match"),
        "the refusal cites the content-hash mismatch: {err:#}"
    );
}
