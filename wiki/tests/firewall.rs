//! DoD #2 — the firewall, by construction and by proof.
//!
//! Three independent guarantees:
//!   1. The authz module contains NO filesystem-write / mutate call (source scan).
//!   2. No runtime source links the compiler crate (no compile/write path at all).
//!   3. Running a full generation leaves the compiled artifacts byte-for-byte
//!      unchanged (derivation never wrote to the authz model).

mod common;

use common::{compile_artifacts, fixtures_dir, hash_tree, scratch};

/// Strips `//` line comments and `/* */` block comments so the scan checks
/// CODE, not prose (the module's doc-comments legitimately say "write"/"mutate").
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

#[test]
fn authz_module_has_no_write_or_mutate_path() {
    let code = strip_comments(include_str!("../src/authz.rs"));
    // Any of these in CODE would mean the firewall could write/mutate authz.
    for forbidden in [
        "fs::write",
        "write_all",
        "File::create",
        "OpenOptions",
        "remove_file",
        "remove_dir",
        "create_dir",
        "&mut self",
        "RefCell",
        "Cell<",
        "Mutex",
        "write_artifacts",
        "compile_set",
        "compile_principal",
    ] {
        assert!(
            !code.contains(forbidden),
            "authz.rs must not contain `{forbidden}` — that would be a write/mutate path"
        );
    }
    // It must read, and only read.
    assert!(code.contains("fs::read"), "authz.rs reads the model");
}

#[test]
fn runtime_sources_never_link_the_compiler() {
    // The compiler is a DEV-dependency only. No runtime module may reference it,
    // so there is no compilable path to compile()/write_artifacts() at all.
    for (name, src) in [
        ("authz.rs", include_str!("../src/authz.rs")),
        ("derive.rs", include_str!("../src/derive.rs")),
        ("render.rs", include_str!("../src/render.rs")),
        ("sources.rs", include_str!("../src/sources.rs")),
        ("provenance.rs", include_str!("../src/provenance.rs")),
        ("lib.rs", include_str!("../src/lib.rs")),
        ("main.rs", include_str!("../src/main.rs")),
    ] {
        let code = strip_comments(src);
        assert!(
            !code.contains("scope_compiler") && !code.contains("scope-compiler"),
            "runtime source {name} must not link the compiler crate"
        );
    }
}

#[test]
fn full_generation_leaves_authz_artifacts_byte_identical() {
    let artifacts = scratch("firewall_artifacts");
    compile_artifacts(&artifacts);
    let before = hash_tree(&artifacts);
    assert!(before.contains_key("index.json"), "artifacts were produced");

    let out = scratch("firewall_out");
    let report = wiki::generate(&fixtures_dir(), &artifacts, &out).expect("generate");
    assert!(report.pages_written > 0);

    let after = hash_tree(&artifacts);
    assert_eq!(
        before, after,
        "the compiled authorization model must be byte-unchanged after a full derivation"
    );
}
