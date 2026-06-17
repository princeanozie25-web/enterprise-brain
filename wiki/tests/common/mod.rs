#![allow(dead_code)] // each test binary uses a subset of these helpers.

//! Shared test harness.
//!
//! The read-only authz INPUT is produced exactly as `retrieval`/`service` do:
//! compile FRESH M1 artifacts into a scratch dir via the frozen compiler crate
//! (a dev-dependency). The wiki runtime never links the compiler — only this
//! harness does, and only to PRODUCE artifacts.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The repo's real fixtures dir (read-only).
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("wiki crate sits in the repo root")
        .join("fixtures")
}

/// A fresh per-test scratch dir under the cargo-managed tmp dir, with the
/// Windows-friendly reset retry the sibling crates use (a just-deleted dir can
/// linger briefly under Defender/AV).
pub fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    for attempt in 0..16 {
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
        if fs::create_dir_all(&dir).is_ok()
            && fs::read_dir(&dir)
                .map(|mut e| e.next().is_none())
                .unwrap_or(false)
        {
            return dir;
        }
        std::thread::sleep(std::time::Duration::from_millis(20 * (attempt.min(5) + 1)));
    }
    panic!("scratch dir {name} could not be reset");
}

/// Compiles the real fixtures into `into` as M1 artifacts. This is the
/// read-only authz model the wiki then reads.
pub fn compile_artifacts(into: &Path) {
    let fixtures = fixtures_dir();
    let snap = scope_compiler::snapshot::take(&fixtures).expect("snapshot fixtures");
    let world = scope_compiler::load_world(&fixtures).expect("fixtures validate");
    let (set, unknown) =
        scope_compiler::compile::compile_set(&world, &snap, None).expect("compile M1");
    assert!(unknown.is_empty(), "all fixture principals are known");
    scope_compiler::compile::write_artifacts(into, &set).expect("write artifacts");
}

pub fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).expect("read file for hashing");
    let digest = Sha256::digest(&bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Recursively hashes every file under `dir`, keyed by path relative to `dir`.
pub fn hash_tree(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    hash_tree_into(dir, dir, &mut out);
    out
}

fn hash_tree_into(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            hash_tree_into(root, &path, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, sha256_file(&path));
        }
    }
}
