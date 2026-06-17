//! Enterprise Brain — the org knowledge layer (Wiki Engine, slice 1).
//!
//! Derives markdown entity pages (people, departments, projects, tools) from
//! the synthetic Bryremead sources, deterministically, with provenance on
//! every claim. It is **firewalled** from the compiled authorization model: it
//! may READ the model (to display "why allowed" and to flag, fail-closed,
//! derived associations the model does not grant) but has NO path to write,
//! mutate, or influence it.
//!
//! What slice 1 deliberately does NOT do (slice 2 and beyond): LLM-driven
//! derivation from unstructured docs, the continuous drift lint, query
//! compounding, and any external-system connector. This is the cage; the
//! animal comes later.
//!
//! ## Invariants enforced here
//! 1. No model decides permissions — derivation never assigns/infers/modifies one.
//! 2. No write path to authz — [`authz`] is read-only by construction; the
//!    crate does not depend on `scope-compiler` at runtime.
//! 3. Fail closed — derived-implies-ungranted is flagged and surfaced, never widened.
//! 4. Provenance required — [`provenance::Claim`] cannot exist without a source.
//! 5. Immutable inputs — [`generate`] reads fixtures/artifacts and writes ONLY
//!    under `out_dir`.

pub mod authz;
pub mod derive;
pub mod provenance;
pub mod render;
pub mod sources;

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

pub use authz::{AuthzView, GrantOracle};
pub use derive::{derive_all, DerivedLayer};
pub use provenance::{Claim, Provenance};
pub use sources::Sources;

/// Counts from one generation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateReport {
    pub people: usize,
    pub departments: usize,
    pub projects: usize,
    pub tools: usize,
    pub pages_written: usize,
    pub fail_closed_flags: usize,
    pub snapshot_version: String,
}

/// Generates the full layer: load sources + the compiled authz model (read
/// only), derive deterministically, render markdown, and write it under
/// `out_dir`. Nothing outside `out_dir` is ever written.
pub fn generate(
    fixtures_dir: &Path,
    artifacts_dir: &Path,
    out_dir: &Path,
) -> Result<GenerateReport> {
    let sources = Sources::load(fixtures_dir)?;
    let authz = AuthzView::load(artifacts_dir)?;
    let layer = derive_all(&sources, &authz);
    let pages = render::render_layer(&layer);
    let pages_written = write_pages(out_dir, &pages)?;

    Ok(GenerateReport {
        people: layer.people.len(),
        departments: layer.departments.len(),
        projects: layer.projects.len(),
        tools: layer.tools.len(),
        pages_written,
        fail_closed_flags: layer.all_discrepancies().len(),
        snapshot_version: authz.snapshot_version().to_string(),
    })
}

/// Writes rendered pages under `out_dir` and nowhere else. Refuses any relpath
/// that could escape `out_dir` (defence in depth — relpaths are built from
/// filename-safe ids, but the guard makes "writes stay in out/" explicit).
fn write_pages(out_dir: &Path, pages: &[render::RenderedPage]) -> Result<usize> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("cannot create output dir {}", out_dir.display()))?;
    let mut written = 0usize;
    for page in pages {
        if !is_safe_relpath(&page.relpath) {
            bail!(
                "refusing to write outside out/: unsafe relpath {:?}",
                page.relpath
            );
        }
        let path = out_dir.join(&page.relpath);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        fs::write(&path, page.markdown.as_bytes())
            .with_context(|| format!("cannot write page {}", path.display()))?;
        written += 1;
    }
    Ok(written)
}

/// A page relpath is safe iff every `/`-separated component is a non-empty,
/// non-`..` name of only `[A-Za-z0-9._-]`. This rejects absolute paths, drive
/// letters (`C:\…`), UNC/backslash components, `:` and `..` traversal wherever
/// they appear — so `out_dir.join(relpath)` can never escape `out_dir`,
/// independent of how the relpath was built upstream.
fn is_safe_relpath(relpath: &str) -> bool {
    !relpath.is_empty()
        && relpath.split('/').all(|c| {
            !c.is_empty()
                && c != ".."
                && c.chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        })
}

#[cfg(test)]
mod tests {
    use super::is_safe_relpath;

    #[test]
    fn relpath_guard_rejects_escapes() {
        // Legitimate page paths.
        assert!(is_safe_relpath("index.md"));
        assert!(is_safe_relpath("people/p001.md"));
        assert!(is_safe_relpath("tools/agent_qa_drafter.md"));
        // Escapes / unsafe components — all rejected.
        for bad in [
            "",
            "..",
            "../evil.md",
            "people/../../evil.md",
            "/abs.md",
            "\\abs.md",
            "C:\\Windows\\evil.md",
            "C:relative.md",
            "people\\evil.md",
            "foo:bar.md",
            "people//p001.md",
        ] {
            assert!(!is_safe_relpath(bad), "must reject {bad:?}");
        }
    }
}
