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
pub mod scope;
pub mod scoped;
pub mod sources;
pub mod synth;

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};

pub use authz::{AuthzView, GrantOracle};
pub use derive::{derive_all, DerivedLayer};
pub use provenance::{Claim, Provenance};
pub use scope::{RetrievalSelector, ScopeContext, ScopeGate};
pub use scoped::{derive_scope, ScopedLayer, Topic};
pub use sources::Sources;
pub use synth::{OllamaSynthesizer, Synthesizer};

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

// ---------------------------------------------------------------------------
// Slice 2 — scoped LLM content derivation (real orchestration)
// ---------------------------------------------------------------------------

/// Per-scope outcome of a scoped generation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeResult {
    pub principal_id: String,
    pub allowed_count: usize,
    pub sourced_docs: usize,
    pub claims: usize,
    pub fail_closed_flags: usize,
    pub refused: usize,
}

/// Summary of a scoped generation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedGenerateReport {
    pub scopes: Vec<ScopeResult>,
    pub structural_flags: usize,
    pub pages_written: usize,
    pub model: String,
}

/// Derives scoped content knowledge for each principal using the LIVE local
/// model, writing one layer per scope plus the drift report under `out_dir`.
/// The LLM sees only each scope's authorized documents (fetched through
/// governed retrieval). Reads fixtures/artifacts/idx; writes only under `out_dir`.
#[allow(clippy::too_many_arguments)]
pub fn generate_scoped(
    fixtures_dir: &Path,
    artifacts_dir: &Path,
    idx_dir: &Path,
    out_dir: &Path,
    principals: &[String],
    endpoint: &str,
    model: &str,
    timeout: Duration,
) -> Result<ScopedGenerateReport> {
    let sources = Sources::load(fixtures_dir)?;
    let authz = AuthzView::load(artifacts_dir)?;

    // Fail-closed corpus pin: the documents.json bodies we feed the model MUST be
    // the same corpus the allowlists were compiled from. retrieval's index guards
    // its own copy at query time; this guards the body-feed, which reads fixtures
    // directly. A drift between the two would let an in-scope id carry a stale or
    // substituted body — refuse rather than derive over it.
    let doc_bytes = fs::read(fixtures_dir.join("documents.json"))
        .context("reading documents.json for the corpus pin")?;
    let actual = retrieval::index::sha256_hex(&doc_bytes);
    match authz.documents_sha256() {
        Some(pinned) if pinned == actual => {}
        Some(pinned) => bail!(
            "documents.json drifted from the compiled corpus (compiled {pinned}, current {actual}); \
             refusing scoped derivation"
        ),
        None => bail!("artifacts index records no documents.json hash; refusing scoped derivation"),
    }

    let synth = OllamaSynthesizer::new(endpoint, model, timeout)?;

    // Slice-1 structural flags feed the standing drift report.
    let structural_flags = derive_all(&sources, &authz).all_discrepancies().len();

    let mut layers = Vec::new();
    for principal in principals {
        let gate = ScopeGate::load(artifacts_dir, principal)?;
        let ctx = ScopeContext::build(gate, &sources);
        let selector = RetrievalSelector::open(idx_dir, artifacts_dir, principal)?;
        let topics = topics_for_scope(&sources, principal);
        let layer = derive_scope(&sources, &ctx, &topics, &selector, &synth, &authz)?;
        layers.push(layer);
    }

    let mut pages = Vec::new();
    for layer in &layers {
        pages.push(render::RenderedPage {
            relpath: format!("scopes/{}.md", layer.principal_id),
            markdown: scoped::render_scoped_layer(layer),
        });
    }
    pages.push(render::RenderedPage {
        relpath: "lint/drift-report.md".to_string(),
        markdown: scoped::render_drift_report(structural_flags, &layers),
    });
    let pages_written = write_pages(out_dir, &pages)?;

    Ok(ScopedGenerateReport {
        scopes: layers
            .iter()
            .map(|l| ScopeResult {
                principal_id: l.principal_id.clone(),
                allowed_count: l.allowed_count,
                sourced_docs: l.sourced_docs.len(),
                claims: l.claims.len(),
                fail_closed_flags: l.discrepancies.len(),
                refused: l.rejected.len(),
            })
            .collect(),
        structural_flags,
        pages_written,
        model: model.to_string(),
    })
}

/// Topics for a scope, drawn from the principal's own projects (people.json),
/// capped; falls back to the department, then a generic topic.
fn topics_for_scope(_sources: &Sources, _principal_id: &str) -> Vec<Topic> {
    // High-recall corpus themes that retrieve material in ANY scope. The
    // PER-SCOPE specificity is supplied by the gate — each scope's governed
    // search returns only its own authorized documents — NOT by the topic; a
    // principal's capability names span departments and rarely match their own
    // scope's document text. `_sources`/`_principal_id` are accepted for future
    // principal-tailored seeds.
    vec![
        Topic {
            label: "Controlled documents and procedures".into(),
            query: "controlled document procedure storage records".into(),
        },
        Topic {
            label: "Records, training and responsibilities".into(),
            query: "records retention training review responsibilities".into(),
        },
        Topic {
            label: "Operations, handling and accounts".into(),
            query: "stock handling temperature customer account".into(),
        },
    ]
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
