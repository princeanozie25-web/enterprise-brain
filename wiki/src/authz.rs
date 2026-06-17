//! THE FIREWALL — the only place the wiki touches the compiled authorization
//! model, and it touches it READ-ONLY.
//!
//! Enforced by construction:
//!   * This module deserializes the compiled artifacts through its own
//!     `Deserialize`-ONLY mirror types. None of them derive `Serialize`, so the
//!     authz model cannot be re-encoded out of this crate.
//!   * Loading uses `std::fs::read` exclusively. There is no `fs::write`,
//!     `File::create`, `OpenOptions`, `remove_*`, or any other write/mutate
//!     call anywhere in this file — a test scans this module's own source to
//!     prove it.
//!   * [`AuthzView`] exposes only `&self` query methods; it has no `&mut`
//!     method and no interior mutability, so a holder cannot widen, narrow, or
//!     otherwise change a grant.
//!   * The crate does NOT depend on `scope-compiler` at runtime, so there is no
//!     compilable path to `compile`/`write_artifacts` at all.
//!
//! Derivation never sees a concrete `AuthzView`; it depends on the read-only
//! [`GrantOracle`] trait, which exposes queries and nothing that mutates.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

// --- Deserialize-only mirror of the compiler's artifact format --------------
// (compiler::compile::{Artifact, CompiledEntry, IndexFile, IndexRow}, read side
// only). A test compiles real artifacts and asserts these parse them, keeping
// the mirror honest without linking the compiler into the runtime.

#[derive(Debug, Clone, Deserialize)]
struct CompiledEntryView {
    document_id: String,
    #[serde(default)]
    reasons: Vec<String>,
    #[serde(default)]
    superseded: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct ArtifactView {
    principal_id: String,
    denied_count: usize,
    #[serde(default)]
    entries: Vec<CompiledEntryView>,
    snapshot_version: String,
}

#[derive(Debug, Clone, Deserialize)]
struct IndexRowView {
    principal_id: String,
    artifact_file: String,
}

#[derive(Debug, Clone, Deserialize)]
struct IndexView {
    snapshot_version: String,
    principals: Vec<IndexRowView>,
}

// --- The read-only view -----------------------------------------------------

/// One principal's compiled grants, flattened for lookup.
#[derive(Debug, Clone, Default)]
struct PrincipalGrants {
    denied_count: usize,
    /// document id -> stable rule-id reasons (the "why allowed" trace).
    allows: BTreeMap<String, Vec<String>>,
    superseded: BTreeSet<String>,
}

/// A read-only view of the compiled authorization model. Construct with
/// [`AuthzView::load`]; thereafter it answers queries and can do nothing else.
#[derive(Debug, Clone)]
pub struct AuthzView {
    snapshot_version: String,
    by_principal: BTreeMap<String, PrincipalGrants>,
}

impl AuthzView {
    /// Loads `index.json` and every per-principal artifact it names from
    /// `artifacts_dir`. Reads only — never writes, never mutates the directory.
    pub fn load(artifacts_dir: &Path) -> Result<Self> {
        let index_path = artifacts_dir.join("index.json");
        let index_bytes = fs::read(&index_path)
            .with_context(|| format!("cannot read authz index {}", index_path.display()))?;
        let index: IndexView = serde_json::from_slice(&index_bytes)
            .with_context(|| format!("authz index {} fails to parse", index_path.display()))?;

        let mut by_principal = BTreeMap::new();
        for row in &index.principals {
            let path = artifacts_dir.join(&row.artifact_file);
            let bytes = fs::read(&path)
                .with_context(|| format!("cannot read authz artifact {}", path.display()))?;
            let artifact: ArtifactView = serde_json::from_slice(&bytes)
                .with_context(|| format!("authz artifact {} fails to parse", path.display()))?;
            // Fail closed: the artifact must pin the same snapshot as the index
            // (mirrors the compiler's own verify discipline; never silently
            // accept a mismatched model, even in release).
            if artifact.snapshot_version != index.snapshot_version {
                bail!(
                    "authz artifact {} pins snapshot {} but the index pins {}; refusing",
                    path.display(),
                    artifact.snapshot_version,
                    index.snapshot_version
                );
            }

            let mut grants = PrincipalGrants {
                denied_count: artifact.denied_count,
                ..Default::default()
            };
            for entry in artifact.entries {
                if entry.superseded == Some(true) {
                    grants.superseded.insert(entry.document_id.clone());
                }
                grants.allows.insert(entry.document_id, entry.reasons);
            }
            // Fail closed: the index row and the artifact must name the same principal.
            if row.principal_id != artifact.principal_id {
                bail!(
                    "authz index row names principal {} but artifact {} names {}; refusing",
                    row.principal_id,
                    path.display(),
                    artifact.principal_id
                );
            }
            by_principal.insert(artifact.principal_id, grants);
        }

        Ok(Self {
            snapshot_version: index.snapshot_version,
            by_principal,
        })
    }

    pub fn snapshot_version(&self) -> &str {
        &self.snapshot_version
    }

    /// Number of principals in the compiled model.
    pub fn principal_count(&self) -> usize {
        self.by_principal.len()
    }
}

/// The read-only authorization seam derivation is allowed to consult. Every
/// method answers a question; none can change a grant. Derivation depends on
/// `&dyn GrantOracle`, so it provably has no handle that mutates the model.
pub trait GrantOracle {
    /// `Some(reasons)` if the principal is granted the document by the compiled
    /// model (with its stable rule-id reason trace), else `None` (deny-by-default).
    fn why_allowed(&self, principal: &str, document: &str) -> Option<Vec<String>>;

    /// All documents the compiled model grants this principal, sorted.
    fn allowed_documents(&self, principal: &str) -> Vec<String>;

    /// The model's denied count for this principal, or `None` if unknown.
    fn denied_count(&self, principal: &str) -> Option<usize>;

    /// Whether the principal exists in the compiled model.
    fn known_principal(&self, principal: &str) -> bool;

    /// Whether the granted document is readable-but-superseded (rule 6).
    fn is_superseded(&self, principal: &str, document: &str) -> bool;

    /// The pinned snapshot version of the compiled model being read.
    fn snapshot_version(&self) -> &str;
}

impl GrantOracle for AuthzView {
    fn why_allowed(&self, principal: &str, document: &str) -> Option<Vec<String>> {
        self.by_principal
            .get(principal)
            .and_then(|g| g.allows.get(document))
            .cloned()
    }

    fn allowed_documents(&self, principal: &str) -> Vec<String> {
        self.by_principal
            .get(principal)
            .map(|g| g.allows.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn denied_count(&self, principal: &str) -> Option<usize> {
        self.by_principal.get(principal).map(|g| g.denied_count)
    }

    fn known_principal(&self, principal: &str) -> bool {
        self.by_principal.contains_key(principal)
    }

    fn is_superseded(&self, principal: &str, document: &str) -> bool {
        self.by_principal
            .get(principal)
            .map(|g| g.superseded.contains(document))
            .unwrap_or(false)
    }

    fn snapshot_version(&self) -> &str {
        &self.snapshot_version
    }
}
