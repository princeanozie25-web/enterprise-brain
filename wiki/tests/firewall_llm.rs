//! DoD #3 (slice 2): the firewall holds for the LLM path.
//!
//! The new scoped-derivation modules (synth/scope/scoped) introduce a local LLM
//! and a read-only retrieval dependency. This re-asserts that none of it opens a
//! path to write, mutate, or influence the authorization model, and that adding
//! `retrieval` did not pull `scope-compiler` into the runtime.

mod common;

use common::{
    compile_artifacts, fixtures_dir, hash_tree, scratch, FakeVerifier, FixedSelector,
    RecordingSynthesizer,
};

use wiki::authz::{AuthzView, GrantOracle};
use wiki::compound::{compound_answer, CompoundStore};
use wiki::scope::{ScopeContext, ScopeGate};
use wiki::scoped::{derive_scope, Topic};
use wiki::Sources;

/// Strips `//` and `/* */` comments so scans check CODE, not prose.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

const LLM_MODULES: [(&str, &str); 5] = [
    ("synth.rs", include_str!("../src/synth.rs")),
    ("scope.rs", include_str!("../src/scope.rs")),
    ("scoped.rs", include_str!("../src/scoped.rs")),
    ("compound.rs", include_str!("../src/compound.rs")),
    ("ground.rs", include_str!("../src/ground.rs")),
];

#[test]
fn llm_modules_never_link_the_compiler() {
    for (name, src) in LLM_MODULES {
        let code = strip_comments(src);
        assert!(
            !code.contains("scope_compiler") && !code.contains("scope-compiler"),
            "runtime LLM module {name} must not link the compiler crate"
        );
    }
}

#[test]
fn llm_modules_have_no_authz_write_or_mutate_path() {
    // These tokens are authz-WRITE / compile / filesystem-write paths. (An
    // in-memory `&mut self` — e.g. CompoundStore::add growing the page store —
    // is legitimate and not an authz mutation; the authz model is read-only by
    // construction in AuthzView, enforced by the authz-specific scan in
    // firewall.rs.)
    for (name, src) in LLM_MODULES {
        let code = strip_comments(src);
        for forbidden in [
            "write_artifacts",
            "compile_set",
            "compile_principal",
            "fs::write",
            "File::create",
            "OpenOptions",
            "remove_file",
            "remove_dir",
        ] {
            assert!(
                !code.contains(forbidden),
                "LLM module {name} must not contain `{forbidden}`"
            );
        }
    }
}

#[test]
fn cargo_manifest_keeps_compiler_out_of_runtime_deps() {
    let toml = include_str!("../Cargo.toml");
    let after_deps = toml
        .split("\n[dependencies]")
        .nth(1)
        .expect("a [dependencies] section");
    let runtime = after_deps
        .split("\n[dev-dependencies]")
        .next()
        .expect("a [dev-dependencies] section follows");
    assert!(
        runtime.contains("retrieval"),
        "retrieval is the slice-2 runtime dependency"
    );
    assert!(
        !runtime.contains("scope-compiler"),
        "scope-compiler must NOT be a runtime dependency"
    );
    let dev = toml
        .split("\n[dev-dependencies]")
        .nth(1)
        .expect("dev-deps block");
    assert!(
        dev.contains("scope-compiler"),
        "scope-compiler stays a dev-dependency only"
    );
}

#[test]
fn scoped_derivation_leaves_authz_artifacts_byte_identical() {
    let artifacts = scratch("firewall_llm_artifacts");
    compile_artifacts(&artifacts);
    let before = hash_tree(&artifacts);

    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();
    let gate = ScopeGate::load(&artifacts, "p060").unwrap();
    let head: Vec<String> = gate.allowed().iter().take(6).cloned().collect();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head };
    let synth = RecordingSynthesizer::echo("fake-model");
    let verifier = FakeVerifier::always();
    let topics = vec![Topic {
        label: "t".into(),
        query: "t".into(),
    }];
    let layer = derive_scope(
        &sources, &ctx, &topics, &selector, &synth, &verifier, &authz,
    )
    .unwrap();
    assert!(!layer.claims.is_empty());

    let after = hash_tree(&artifacts);
    assert_eq!(
        before, after,
        "scoped LLM derivation must leave the compiled authz model byte-identical"
    );
}

#[test]
fn corpus_pin_matches_the_compiled_documents_hash() {
    // The fail-closed corpus pin (lib.rs generate_scoped) compares the bodies it
    // feeds the model against the documents.json hash the artifacts recorded.
    let artifacts = scratch("corpus_pin_artifacts");
    compile_artifacts(&artifacts);
    let authz = AuthzView::load(&artifacts).unwrap();

    let doc_bytes = std::fs::read(fixtures_dir().join("documents.json")).unwrap();
    let actual = retrieval::index::sha256_hex(&doc_bytes);
    assert_eq!(
        authz.documents_sha256(),
        Some(actual.as_str()),
        "artifacts pin the same documents.json the wiki feeds the model"
    );
    // A drifted/tampered corpus hashes differently — the pin would refuse it.
    let tampered = retrieval::index::sha256_hex(b"not the compiled corpus");
    assert_ne!(authz.documents_sha256(), Some(tampered.as_str()));
}

#[test]
fn compound_module_has_no_widening_toggle() {
    // No config/flag in the compounding module relaxes source eligibility or
    // widens derivation scope — fail-closed is not configurable (DoD #4).
    let code = strip_comments(include_str!("../src/compound.rs"));
    for forbidden in [
        "bypass",
        "allow_widen",
        "force_eligible",
        "skip_check",
        "skip_eligibility",
        "relax",
        "override_scope",
        "widen", // only appears in (stripped) comments, never in code
    ] {
        assert!(
            !code.contains(forbidden),
            "compound.rs code must not contain a widening path `{forbidden}`"
        );
    }
}

#[test]
fn compounding_run_leaves_authz_artifacts_byte_identical() {
    let artifacts = scratch("firewall_compound_artifacts");
    compile_artifacts(&artifacts);
    let before = hash_tree(&artifacts);

    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();
    let allowed: std::collections::BTreeSet<String> =
        authz.allowed_documents("p060").into_iter().collect();
    let gate = ScopeGate::load(&artifacts, "p060").unwrap();
    let head: Vec<String> = gate.allowed().iter().take(4).cloned().collect();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head };
    let synth = RecordingSynthesizer::echo("fake-model");
    let verifier = FakeVerifier::always();

    let mut store = CompoundStore::new();
    let mut allowed_of: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    allowed_of.insert("p060".to_string(), allowed);
    let page = compound_answer(
        &sources,
        &ctx,
        "q",
        "q",
        &selector,
        &synth,
        &verifier,
        &[],
        &allowed_of,
        0,
    )
    .unwrap();
    store.add(page, &allowed_of).unwrap();
    assert!(store.len() == 1);

    let after = hash_tree(&artifacts);
    assert_eq!(
        before, after,
        "a compounding run must leave the compiled authz model byte-identical"
    );
}
