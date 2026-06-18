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
pub mod compound;
pub mod derive;
pub mod ground;
pub mod mentions;
pub mod provenance;
pub mod render;
pub mod scope;
pub mod scoped;
pub mod sources;
pub mod synth;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};

pub use authz::{AuthzView, GrantOracle};
pub use compound::{compound_answer, CompoundStore, CompoundedPage, ScopeStamp, SourceRef};
pub use derive::{derive_all, DerivedLayer};
pub use ground::{Anchor, OllamaVerifier, SupportVerdict, Verifier};
pub use mentions::{MentionFlag, Roster};
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
    /// Slice 5: free-text principal-mention flags (distinct from the structured
    /// `fail_closed_flags`; additive coverage, never widening).
    pub mention_flags: usize,
    pub refused: usize,
    /// Grounding (slice 4): no verbatim in-scope span — unfounded.
    pub refused_unfounded: usize,
    /// Grounding (slice 4): anchored but judge did not confirm support.
    pub withheld: usize,
}

/// Summary of a scoped generation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedGenerateReport {
    pub scopes: Vec<ScopeResult>,
    pub structural_flags: usize,
    pub pages_written: usize,
    pub model: String,
    pub judge_model: String,
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
    judge_model: &str,
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
    let verifier = OllamaVerifier::new(endpoint, judge_model, timeout)?;

    // Slice-1 structural flags feed the standing drift report.
    let structural_flags = derive_all(&sources, &authz).all_discrepancies().len();

    let mut layers = Vec::new();
    for principal in principals {
        let gate = ScopeGate::load(artifacts_dir, principal)?;
        let ctx = ScopeContext::build(gate, &sources);
        let selector = RetrievalSelector::open(idx_dir, artifacts_dir, principal)?;
        let topics = topics_for_scope(&sources, principal);
        let layer = derive_scope(
            &sources, &ctx, &topics, &selector, &synth, &verifier, &authz,
        )?;
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
                mention_flags: l.mention_flags.len(),
                refused: l.rejected.len(),
                refused_unfounded: l.refused_unfounded.len(),
                withheld: l.withheld.len(),
            })
            .collect(),
        structural_flags,
        pages_written,
        model: model.to_string(),
        judge_model: judge_model.to_string(),
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

// ---------------------------------------------------------------------------
// Slice 3 — query-compounding (real orchestration)
// ---------------------------------------------------------------------------

/// Per-scope outcome of a compounding run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompoundScopeResult {
    pub principal_id: String,
    pub allowed_count: usize,
    pub round1_page: String,
    pub round1_claims: usize,
    pub round2_page: String,
    pub round2_claims: usize,
    /// Compounded pages that were ELIGIBLE as sources for this scope's round-2
    /// question (fail-closed: only pages whose scope ⊆ this scope, same snapshot).
    pub round2_eligible: Vec<String>,
    /// Grounding (slice 4), summed across both rounds.
    pub refused_unfounded: usize,
    pub withheld: usize,
    /// Slice 5: free-text principal-mention flags, summed across both rounds.
    pub mention_flags: usize,
}

/// Summary of a compounding run.
#[derive(Debug, Clone)]
pub struct CompoundGenerateReport {
    pub scopes: Vec<CompoundScopeResult>,
    pub snapshot: String,
    pub model: String,
    pub judge_model: String,
    pub pages_written: usize,
}

const ROUND1_Q: &str =
    "What do the documents in this scope establish about its routine operations, records, and arrangements?";
const ROUND1_QUERY: &str = "controlled document procedure records storage handling";
const ROUND2_Q: &str =
    "Building on the earlier summary, what do the customer and account arrangements and procedures show?";
const ROUND2_QUERY: &str = "customer account procedure records arrangement returns";

/// Runs the 2-round compounding flow across `principals` with the LIVE model:
/// round 1 answers a question per scope (raw in-scope sources only); round 2
/// answers a follow-up per scope, offered the round-1 pages that are FAIL-CLOSED
/// eligible (same snapshot, `allowed(S) ⊆ allowed(T)`). Every compounded page is
/// scope-stamped and its transitive closure is asserted within its scope on
/// write. Reads fixtures/artifacts/idx; writes only under `out_dir`.
#[allow(clippy::too_many_arguments)]
pub fn generate_compounded(
    fixtures_dir: &Path,
    artifacts_dir: &Path,
    idx_dir: &Path,
    out_dir: &Path,
    principals: &[String],
    endpoint: &str,
    model: &str,
    judge_model: &str,
    timeout: Duration,
) -> Result<CompoundGenerateReport> {
    let sources = Sources::load(fixtures_dir)?;
    let authz = AuthzView::load(artifacts_dir)?;

    // Fail-closed corpus pin (as in generate_scoped).
    let doc_bytes = fs::read(fixtures_dir.join("documents.json"))
        .context("reading documents.json for the corpus pin")?;
    let actual = retrieval::index::sha256_hex(&doc_bytes);
    match authz.documents_sha256() {
        Some(pinned) if pinned == actual => {}
        Some(pinned) => bail!(
            "documents.json drifted from the compiled corpus (compiled {pinned}, current {actual}); refusing"
        ),
        None => bail!("artifacts index records no documents.json hash; refusing"),
    }

    let synth = OllamaSynthesizer::new(endpoint, model, timeout)?;
    let verifier = OllamaVerifier::new(endpoint, judge_model, timeout)?;
    let snap = authz.snapshot_version().to_string();

    // Allowed set per scope (read-only, from the compiled model).
    let mut allowed_of: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for p in principals {
        allowed_of.insert(p.clone(), authz.allowed_documents(p).into_iter().collect());
    }

    let mut store = CompoundStore::new(snap.clone());
    // principal -> (round1 page id, claims, refused-unfounded, withheld, mention-flags)
    let mut r1: BTreeMap<String, (String, usize, usize, usize, usize)> = BTreeMap::new();

    // Round 1 — no prior pages are eligible.
    for p in principals {
        let gate = ScopeGate::load(artifacts_dir, p)?;
        let ctx = ScopeContext::build(gate, &sources);
        let selector = RetrievalSelector::open(idx_dir, artifacts_dir, p)?;
        let page = compound_answer(
            &sources,
            &ctx,
            ROUND1_Q,
            ROUND1_QUERY,
            &selector,
            &synth,
            &verifier,
            &[],
            &allowed_of,
            store.next_ordinal(),
        )?;
        let row = (
            page.page_id.clone(),
            page.claims.len(),
            page.refused_unfounded.len(),
            page.withheld.len(),
            page.mention_flags.len(),
        );
        store.add(page, &allowed_of)?;
        r1.insert(p.clone(), row);
    }

    // Round 2 — offered the fail-closed eligible prior pages.
    let mut results = Vec::new();
    for p in principals {
        let allowed = allowed_of.get(p).expect("allowed set").clone();
        // Scope the immutable borrow of `store` so the later `add` (mutable) is free.
        let (page, eligible_ids) = {
            let eligible = store.eligible_for(&snap, &allowed, &allowed_of);
            let eligible_ids: Vec<String> = eligible.iter().map(|e| e.page_id.clone()).collect();
            let gate = ScopeGate::load(artifacts_dir, p)?;
            let ctx = ScopeContext::build(gate, &sources);
            let selector = RetrievalSelector::open(idx_dir, artifacts_dir, p)?;
            let page = compound_answer(
                &sources,
                &ctx,
                ROUND2_Q,
                ROUND2_QUERY,
                &selector,
                &synth,
                &verifier,
                &eligible,
                &allowed_of,
                store.next_ordinal(),
            )?;
            (page, eligible_ids)
        };
        let (r2_id, r2_claims, r2_ref, r2_wh, r2_mf) = (
            page.page_id.clone(),
            page.claims.len(),
            page.refused_unfounded.len(),
            page.withheld.len(),
            page.mention_flags.len(),
        );
        store.add(page, &allowed_of)?;
        let (r1_id, r1_claims, r1_ref, r1_wh, r1_mf) = r1.get(p).cloned().unwrap_or_default();
        results.push(CompoundScopeResult {
            principal_id: p.clone(),
            allowed_count: allowed.len(),
            round1_page: r1_id,
            round1_claims: r1_claims,
            round2_page: r2_id,
            round2_claims: r2_claims,
            round2_eligible: eligible_ids,
            refused_unfounded: r1_ref + r2_ref,
            withheld: r1_wh + r2_wh,
            mention_flags: r1_mf + r2_mf,
        });
    }

    // Write every page (with its closure proof) + a run report under out/compounded/.
    let mut pages = Vec::new();
    for page in store.pages() {
        pages.push(render::RenderedPage {
            relpath: format!("compounded/{}.md", page.page_id),
            markdown: compound::render_compounded_page(&store, page),
        });
    }
    pages.push(render::RenderedPage {
        relpath: "compounded/_report.md".to_string(),
        markdown: render_compound_report(&snap, model, judge_model, &results),
    });
    let pages_written = write_pages(out_dir, &pages)?;

    Ok(CompoundGenerateReport {
        scopes: results,
        snapshot: snap,
        model: model.to_string(),
        judge_model: judge_model.to_string(),
        pages_written,
    })
}

fn render_compound_report(
    snapshot: &str,
    model: &str,
    judge_model: &str,
    results: &[CompoundScopeResult],
) -> String {
    let mut s = String::new();
    s.push_str("# Compounding run — eligibility + no-laundering + grounding report\n\n");
    s.push_str(&format!(
        "> snapshot `{snapshot}` · model `{model}` · judge `{judge_model}` · synthetic corpus, local \
         model. On write each page's transitive raw-doc closure is asserted ⊆ its stamped scope \
         (no laundering), the citation DAG kept acyclic, and the store is pinned to one snapshot \
         (a page stamped for another snapshot is refused). Eligibility then requires same snapshot \
         AND allowed(S) ⊆ allowed(T) before a stored page may SOURCE a later question; no widening \
         toggle. Admitted claims are verbatim-anchored + support-checked (judge, fail-closed) — NOT \
         proven faithful; on current local judges the live admit path over-refuses and admits ~zero.\n\n"
    ));
    for r in results {
        s.push_str(&format!(
            "## scope `{}` ({} allowed docs)\n\n",
            r.principal_id, r.allowed_count
        ));
        s.push_str(&format!(
            "- round 1: `{}` — {} claim(s)\n",
            r.round1_page, r.round1_claims
        ));
        s.push_str(&format!(
            "- round 2: `{}` — {} claim(s)\n",
            r.round2_page, r.round2_claims
        ));
        s.push_str(&format!(
            "- round 2 eligible compounded sources: {}\n",
            if r.round2_eligible.is_empty() {
                "(none)".to_string()
            } else {
                r.round2_eligible.join(", ")
            }
        ));
        s.push_str(&format!(
            "- grounding: {} refused-unfounded, {} withheld-unsupported (both rounds)\n",
            r.refused_unfounded, r.withheld
        ));
        s.push_str(&format!(
            "- free-text principal-mention flags (slice 5, fail-closed, access NOT widened): {}\n\n",
            r.mention_flags
        ));
    }
    s
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
