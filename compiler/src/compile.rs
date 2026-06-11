//! Allowlist compilation: turns (principal, pinned fixture snapshot) into one
//! canonical artifact per principal plus an index.
//!
//! Canonical form: artifacts serialize through `serde_json::Value`, whose
//! object representation is a sorted map, so every emitted JSON object has
//! sorted keys; entries are ordered by document id, reasons are sorted and
//! deduplicated, and `compiled_at` is the fixed epoch. Two compiles over the
//! same fixture bytes are therefore byte-identical.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::MosaicTrap;
use crate::semantics::{Decision, World};
use crate::snapshot::{self, FixtureSnapshot, FIXED_EPOCH};

/// One compiled allow. Optional keys are omitted (not null) when absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompiledEntry {
    pub document_id: String,
    /// Terminal document of the supersedes chain, present iff `superseded`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_successor: Option<String>,
    /// Mosaic trap records naming this document, passed through untouched
    /// (rule 7). M1 preserves them; it does not enforce mosaic bounds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mosaic_tags: Option<Vec<MosaicTrap>>,
    /// Stable rule ids; every allow carries at least one.
    pub reasons: Vec<String>,
    /// Present (true) iff a successor document supersedes this one. The entry
    /// stays readable (rule 6); retrieval must refuse to serve it AS current.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
}

/// The per-principal compiled artifact (`<principal_id>.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub compiled_at: String,
    /// Documents denied for this principal — a count for sanity, never an
    /// enumeration.
    pub denied_count: usize,
    pub entries: Vec<CompiledEntry>,
    pub principal_id: String,
    pub snapshot_version: String,
}

/// One row of `index.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexRow {
    pub artifact_file: String,
    pub artifact_sha256: String,
    pub denied_count: usize,
    pub entry_count: usize,
    pub principal_id: String,
    /// Present (true) iff the requested principal was not in the fixtures and
    /// fail-closed compilation produced an empty allowlist.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknown_principal: Option<bool>,
}

/// `index.json`: the compile manifest artifacts are verified against.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexFile {
    pub compiled_at: String,
    /// fixture file name -> SHA-256 of its bytes at compile time.
    pub fixture_hashes: BTreeMap<String, String>,
    pub principals: Vec<IndexRow>,
    pub snapshot_version: String,
    pub totals: Totals,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Totals {
    pub allow_entries: usize,
    pub documents: usize,
    pub principals: usize,
}

/// Compiles the allowlist for one principal. An unknown principal compiles to
/// an empty allowlist (deny-by-default; the caller logs it).
pub fn compile_principal(world: &World, snap: &FixtureSnapshot, principal_id: &str) -> Artifact {
    let mut entries: Vec<CompiledEntry> = Vec::new();
    if world.is_known_principal(principal_id) {
        // world.documents is sorted by id, so entries come out ordered.
        for doc in &world.documents {
            match world.decide(principal_id, doc) {
                Decision::Allow(reasons) => {
                    debug_assert!(!reasons.is_empty(), "every allow carries >=1 reason");
                    let superseded = world.is_superseded(&doc.id);
                    entries.push(CompiledEntry {
                        document_id: doc.id.clone(),
                        effective_successor: world.effective_successor(&doc.id).map(str::to_string),
                        mosaic_tags: world.mosaic_tags(&doc.id).map(<[MosaicTrap]>::to_vec),
                        reasons,
                        superseded: superseded.then_some(true),
                    });
                }
                Decision::Deny(_) => {}
            }
        }
    }
    Artifact {
        compiled_at: FIXED_EPOCH.to_string(),
        denied_count: world.documents.len() - entries.len(),
        entries,
        principal_id: principal_id.to_string(),
        snapshot_version: snap.snapshot_version.clone(),
    }
}

/// Canonical JSON bytes: serialize through `Value` (sorted-key objects),
/// compact encoding, trailing newline.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value).context("serializing artifact")?;
    let mut bytes = serde_json::to_vec(&value).context("encoding artifact")?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// A fully compiled run, in memory: artifacts are written only after the
/// fixture bytes are re-verified against the snapshot.
pub struct CompiledSet {
    pub snapshot: FixtureSnapshot,
    pub artifacts: Vec<Artifact>,
    pub index: IndexFile,
}

/// Artifact file name for a principal id. Ids are restricted to filename-safe
/// characters before compilation (see `compile_set`).
pub fn artifact_file_name(principal_id: &str) -> String {
    format!("{principal_id}.json")
}

fn is_filename_safe(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Compiles the requested principals (all fixture principals when `None`).
/// Returns the artifacts plus the index, and the list of unknown principal
/// ids the caller must log. Refuses ids that cannot name an artifact file.
pub fn compile_set(
    world: &World,
    snap: &FixtureSnapshot,
    requested: Option<&[String]>,
) -> Result<(CompiledSet, Vec<String>)> {
    let principal_ids: Vec<String> = match requested {
        None => world.principal_ids(),
        Some(ids) => {
            for id in ids {
                if !is_filename_safe(id) {
                    bail!("principal id {id:?} cannot name an artifact file; refusing");
                }
            }
            let mut ids: Vec<String> = ids.to_vec();
            ids.sort();
            ids.dedup();
            ids
        }
    };

    let mut artifacts = Vec::with_capacity(principal_ids.len());
    let mut rows = Vec::with_capacity(principal_ids.len());
    let mut unknown: Vec<String> = Vec::new();
    let mut allow_entries = 0usize;

    for id in &principal_ids {
        let artifact = compile_principal(world, snap, id);
        let bytes = canonical_json_bytes(&artifact)?;
        let known = world.is_known_principal(id);
        if !known {
            unknown.push(id.clone());
        }
        allow_entries += artifact.entries.len();
        rows.push(IndexRow {
            artifact_file: artifact_file_name(id),
            artifact_sha256: snapshot::sha256_hex(&bytes),
            denied_count: artifact.denied_count,
            entry_count: artifact.entries.len(),
            principal_id: id.clone(),
            unknown_principal: (!known).then_some(true),
        });
        artifacts.push(artifact);
    }

    let index = IndexFile {
        compiled_at: FIXED_EPOCH.to_string(),
        fixture_hashes: snap.file_hashes.clone(),
        principals: rows,
        snapshot_version: snap.snapshot_version.clone(),
        totals: Totals {
            allow_entries,
            documents: world.documents.len(),
            principals: principal_ids.len(),
        },
    };

    Ok((
        CompiledSet {
            snapshot: snap.clone(),
            artifacts,
            index,
        },
        unknown,
    ))
}

/// Persists a compiled set: one `<principal_id>.json` per principal plus
/// `index.json`, all in canonical form.
pub fn write_artifacts(out_dir: &Path, set: &CompiledSet) -> Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("cannot create output directory {}", out_dir.display()))?;
    for artifact in &set.artifacts {
        let path = out_dir.join(artifact_file_name(&artifact.principal_id));
        let bytes = canonical_json_bytes(artifact)?;
        fs::write(&path, bytes)
            .with_context(|| format!("cannot write artifact {}", path.display()))?;
    }
    let index_path = out_dir.join("index.json");
    let bytes = canonical_json_bytes(&set.index)?;
    fs::write(&index_path, bytes)
        .with_context(|| format!("cannot write index {}", index_path.display()))?;
    Ok(())
}

/// Verifies a compiled artifact directory against a fixture directory:
/// the pinned snapshot must match the current fixture bytes, and every
/// artifact file must match the hash recorded in the index. Any mismatch
/// refuses (fail-closed).
pub fn verify_artifacts(artifacts_dir: &Path, fixtures_dir: &Path) -> Result<IndexFile> {
    let index_path = artifacts_dir.join("index.json");
    let index_bytes = fs::read(&index_path)
        .with_context(|| format!("cannot read index {}", index_path.display()))?;
    let index: IndexFile = serde_json::from_slice(&index_bytes)
        .with_context(|| format!("index {} fails schema/parse", index_path.display()))?;

    let pinned = FixtureSnapshot {
        snapshot_version: index.snapshot_version.clone(),
        file_hashes: index.fixture_hashes.clone(),
    };
    snapshot::verify_unchanged(fixtures_dir, &pinned)
        .context("artifacts were built from a different fixture snapshot; refusing verification")?;

    for row in &index.principals {
        let path = artifacts_dir.join(&row.artifact_file);
        let bytes =
            fs::read(&path).with_context(|| format!("cannot read artifact {}", path.display()))?;
        let actual = snapshot::sha256_hex(&bytes);
        if actual != row.artifact_sha256 {
            bail!(
                "artifact {} does not match the hash recorded in the index; refusing",
                path.display()
            );
        }
        let artifact: Artifact = serde_json::from_slice(&bytes)
            .with_context(|| format!("artifact {} fails schema/parse", path.display()))?;
        if artifact.snapshot_version != index.snapshot_version {
            bail!(
                "artifact {} pins snapshot {} but the index pins {}; refusing",
                path.display(),
                artifact.snapshot_version,
                index.snapshot_version
            );
        }
    }
    Ok(index)
}
