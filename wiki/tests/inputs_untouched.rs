//! DoD #5 — immutable inputs.
//!
//! A full generation reads the fixtures and the compiled authz artifacts and
//! writes ONLY under `out/`. This proves both input trees are byte-unchanged
//! across the run.

mod common;

use common::{compile_artifacts, fixtures_dir, hash_tree, scratch};

#[test]
fn fixtures_and_artifacts_are_byte_unchanged_by_generation() {
    let fixtures = fixtures_dir();
    let artifacts = scratch("inputs_artifacts");
    compile_artifacts(&artifacts);

    let fixtures_before = hash_tree(&fixtures);
    let artifacts_before = hash_tree(&artifacts);

    let out = scratch("inputs_out");
    let report = wiki::generate(&fixtures, &artifacts, &out).expect("generate");
    assert!(report.pages_written > 0);

    let fixtures_after = hash_tree(&fixtures);
    let artifacts_after = hash_tree(&artifacts);

    assert_eq!(
        fixtures_before, fixtures_after,
        "fixtures (roster, corpus, company, brm, oracle) must be byte-unchanged"
    );
    assert_eq!(
        artifacts_before, artifacts_after,
        "compiled authz artifacts must be byte-unchanged"
    );

    // And the output really did land under out/ (nowhere else to look).
    assert!(out.join("index.md").is_file());
}
