//! Snapshot pinning: every compile is bound to the exact bytes of its input
//! fixtures, and artifacts refuse verification against anything else.
//!
//! `snapshot_version` is the SHA-256 of a canonical manifest listing each
//! input fixture by name with the SHA-256 of its raw bytes, in the fixed
//! order of [`INPUT_FILES`]:
//!
//! ```text
//! company.json <hex>\n
//! documents.json <hex>\n
//! traps.json <hex>\n
//! ```
//!
//! No wall clock exists anywhere in this crate: artifacts carry
//! [`FIXED_EPOCH`] as `compiled_at`, so two compiles over the same fixture
//! bytes are byte-identical.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

/// The fixed compile timestamp mandated by the milestone (no wall clock).
pub const FIXED_EPOCH: &str = "2026-01-05T00:00:00Z";

/// The compiler's input fixtures, in canonical manifest order.
/// `ground_truth.jsonl` is deliberately NOT an input: only the conformance
/// harness may read it.
pub const INPUT_FILES: [&str; 3] = ["company.json", "documents.json", "traps.json"];

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A pinned view of the input fixture bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureSnapshot {
    /// SHA-256 over the canonical manifest of per-file hashes.
    pub snapshot_version: String,
    /// fixture file name -> SHA-256 of its raw bytes.
    pub file_hashes: BTreeMap<String, String>,
}

/// Hashes the input fixtures in `fixtures_dir`. A missing or unreadable file
/// refuses the compile.
pub fn take(fixtures_dir: &Path) -> Result<FixtureSnapshot> {
    let mut file_hashes = BTreeMap::new();
    let mut manifest = String::new();
    for name in INPUT_FILES {
        let path = fixtures_dir.join(name);
        let bytes =
            fs::read(&path).with_context(|| format!("cannot read fixture {}", path.display()))?;
        let hash = sha256_hex(&bytes);
        manifest.push_str(name);
        manifest.push(' ');
        manifest.push_str(&hash);
        manifest.push('\n');
        file_hashes.insert(name.to_string(), hash);
    }
    Ok(FixtureSnapshot {
        snapshot_version: sha256_hex(manifest.as_bytes()),
        file_hashes,
    })
}

/// Re-hashes the fixtures and refuses if any byte changed since `snapshot`
/// was taken. Used both mid-compile (inputs must not move under us) and by
/// `verify` (artifacts must match the fixtures they claim to pin).
pub fn verify_unchanged(fixtures_dir: &Path, snapshot: &FixtureSnapshot) -> Result<()> {
    let current = take(fixtures_dir)?;
    if current == *snapshot {
        return Ok(());
    }
    let mut changed: Vec<&str> = Vec::new();
    for name in INPUT_FILES {
        if current.file_hashes.get(name) != snapshot.file_hashes.get(name) {
            changed.push(name);
        }
    }
    bail!(
        "fixture bytes do not match the pinned snapshot (snapshot_version {} != {}; changed: {})",
        current.snapshot_version,
        snapshot.snapshot_version,
        changed.join(", ")
    );
}
