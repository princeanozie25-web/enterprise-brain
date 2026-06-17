//! Scoped LLM content derivation — the slice-2 pipeline.
//!
//! For one scope, the model is handed ONLY in-scope source documents (fetched
//! through the governed gate in `scope.rs`) and its output passes through the
//! slice-1 gates before anything is written:
//!   * PROVENANCE / SCOPE gate — a claim is admitted only if it cites a
//!     document that was in the in-scope source set it was given. A citation to
//!     anything else (out-of-scope or hallucinated) is REFUSED, not written.
//!     This makes cross-scope non-leakage structural: every written claim's
//!     source is inside the scope.
//!   * FAIL-CLOSED gate — if a claim implicates a principal the authorization
//!     model does not grant the cited document, it is FLAGGED and surfaced
//!     (a slice-1 `Discrepancy`); access is never widened to match the model.
//!
//! No path here writes, mutates, or influences the authorization model. The
//! authz model is consulted read-only through `GrantOracle`, exactly as slice 1.
//!
//! SCOPE OF THE GUARANTEE (honest limits, neither weakens non-leakage or
//! non-widening): a written claim's cite attests that its source is an in-scope
//! document — provenance + scope — not that the document semantically supports
//! the sentence (a loose-paraphrasing model is not caught). The fail-closed flag
//! fires for a structured `ABOUT: <known-principal>` the model does not grant;
//! an association stated only in free text is surfaced as text, not
//! machine-flagged. Tightening both (span-quote grounding, free-text entity
//! extraction) is future work.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use crate::authz::GrantOracle;
use crate::derive::Discrepancy;
use crate::provenance::{Claim, Provenance};
use crate::scope::{DocSelector, ScopeContext};
use crate::sources::{LineIndex, Sources, SRC_DOCUMENTS};
use crate::synth::{SourceDoc, Synthesizer};

/// How many in-scope documents to fetch per topic and hand to the model. Kept
/// small because the local CPU model's latency is dominated by prompt size.
pub const TOPIC_K: usize = 4;

/// A topic to derive content for; `query` fetches the relevant in-scope docs.
#[derive(Debug, Clone)]
pub struct Topic {
    pub label: String,
    pub query: String,
}

/// One admitted LLM-derived content claim, anchored to an in-scope source.
#[derive(Debug, Clone)]
pub struct ScopedClaim {
    pub claim: Claim,
    pub topic: String,
    pub about_principal: Option<String>,
    pub model: String,
}

/// A claim the gate refused. Recorded for audit; never written to a page. The
/// existence of this list is the proof the gate actually rejects.
#[derive(Debug, Clone)]
pub struct RejectedClaim {
    pub text: String,
    pub cited_doc_id: String,
    pub reason: String,
}

/// The derived knowledge for ONE scope.
#[derive(Debug, Clone)]
pub struct ScopedLayer {
    pub principal_id: String,
    pub snapshot_version: String,
    pub model: String,
    pub allowed_count: usize,
    /// Distinct in-scope documents that actually reached the model.
    pub sourced_docs: BTreeSet<String>,
    pub claims: Vec<ScopedClaim>,
    pub discrepancies: Vec<Discrepancy>,
    pub rejected: Vec<RejectedClaim>,
}

impl ScopedLayer {
    /// The set of source documents every admitted claim cites. By construction
    /// this is a subset of the scope's allowed set — the non-leakage guarantee.
    pub fn cited_docs(&self) -> BTreeSet<String> {
        self.claims
            .iter()
            .map(|c| c.claim.provenance().record.clone())
            .collect()
    }
}

fn doc_provenance(
    doc_id: &str,
    lines: &LineIndex,
    doc_index: &BTreeMap<&str, usize>,
) -> Provenance {
    let locator = match doc_index.get(doc_id) {
        Some(i) => format!("/documents/{i}/body"),
        None => {
            // Unreachable: an admitted cite passed the in-scope gate, so it is a
            // corpus document present in doc_index. Guard the invariant.
            debug_assert!(
                false,
                "admitted in-scope cite {doc_id} missing from doc_index"
            );
            "/documents/body".to_string()
        }
    };
    Provenance::new(SRC_DOCUMENTS, doc_id, locator, lines.line_of(doc_id))
        .expect("in-scope doc id and documents.json source are non-empty")
}

/// Derive content for a single scope. `selector` and `synth` are injected so
/// the security properties can be proven deterministically; the real callers
/// pass a governed-retrieval selector and a local-Ollama synthesizer.
pub fn derive_scope(
    sources: &Sources,
    ctx: &ScopeContext,
    topics: &[Topic],
    selector: &dyn DocSelector,
    synth: &dyn Synthesizer,
    authz: &dyn GrantOracle,
) -> Result<ScopedLayer> {
    let gate = &ctx.gate;
    let lines = &sources.lines.documents;
    let doc_index: BTreeMap<&str, usize> = sources
        .documents
        .documents
        .iter()
        .enumerate()
        .map(|(i, d)| (d.id.as_str(), i))
        .collect();

    let mut layer = ScopedLayer {
        principal_id: gate.principal_id.clone(),
        snapshot_version: gate.snapshot_version.clone(),
        model: synth.model_id().to_string(),
        allowed_count: gate.allowed_count(),
        sourced_docs: BTreeSet::new(),
        claims: Vec::new(),
        discrepancies: Vec::new(),
        rejected: Vec::new(),
    };

    for topic in topics {
        // 1. GATE: governed-retrieval selection, then re-filter to the allowlist
        //    (belt) and to documents we actually hold text for. Nothing the
        //    model sees can be outside the scope.
        let selected = selector.select(&topic.query, TOPIC_K)?;
        let sources_in: Vec<SourceDoc> = selected
            .iter()
            .filter(|id| gate.permits(id))
            .filter_map(|id| ctx.source_doc(id))
            .collect();
        if sources_in.is_empty() {
            continue;
        }
        let provided: BTreeSet<&str> = sources_in.iter().map(|s| s.doc_id.as_str()).collect();
        for s in &sources_in {
            layer.sourced_docs.insert(s.doc_id.clone());
        }

        // 2. SYNTHESIZE: the model receives ONLY `sources_in`.
        let raws = synth.synthesize(&topic.label, &sources_in)?;

        // 3. Per-claim gates.
        for raw in raws {
            // 3a. PROVENANCE / SCOPE gate: cite must be an in-scope source.
            if !provided.contains(raw.cited_doc_id.as_str()) {
                layer.rejected.push(RejectedClaim {
                    text: raw.text.clone(),
                    cited_doc_id: raw.cited_doc_id.clone(),
                    reason: "cited a document outside the scope-gated source set; refused".into(),
                });
                continue;
            }
            let prov = doc_provenance(&raw.cited_doc_id, lines, &doc_index);

            // 3b. FAIL-CLOSED gate: an implicated principal the model does not
            //     grant the cited doc -> flag, surface, never widen.
            if let Some(p) = &raw.about_principal {
                if authz.known_principal(p) && authz.why_allowed(p, &raw.cited_doc_id).is_none() {
                    layer.discrepancies.push(Discrepancy {
                        principal_id: p.clone(),
                        document_id: raw.cited_doc_id.clone(),
                        bases: vec![format!("llm:scope:{}", gate.principal_id)],
                        detail: format!(
                            "LLM (scope {}) inferred {} relates to {}, but the authorization model \
                             does not grant {} access. Flagged, not reconciled; access NOT widened.",
                            gate.principal_id, p, raw.cited_doc_id, p
                        ),
                        provenance: prov.clone(),
                    });
                }
            }

            let text = format!(
                "{} [LLM-derived in scope {} via {}]",
                raw.text.trim(),
                gate.principal_id,
                synth.model_id()
            );
            layer.claims.push(ScopedClaim {
                claim: Claim::new(text, prov),
                topic: topic.label.clone(),
                about_principal: raw.about_principal,
                model: synth.model_id().to_string(),
            });
        }
    }

    Ok(layer)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Renders one scope's derived knowledge as markdown. Every claim line carries
/// its in-scope `src:` cite, exactly like slice-1 facts.
pub fn render_scoped_layer(layer: &ScopedLayer) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "# Scoped knowledge — principal `{}`\n\n",
        layer.principal_id
    ));
    s.push_str(&format!(
        "> Derived by `{}` reading ONLY this scope's {} authorized document(s). \
         Authorization happened before retrieval; no out-of-scope document reached the model. \
         Compiled model `{}`.\n\n",
        layer.model, layer.allowed_count, layer.snapshot_version
    ));

    s.push_str(&format!("## Derived facts ({})\n\n", layer.claims.len()));
    if layer.claims.is_empty() {
        s.push_str("_No facts admitted._\n\n");
    }
    for c in &layer.claims {
        let about = match &c.about_principal {
            Some(p) => format!(" · implicates `{p}`"),
            None => String::new(),
        };
        s.push_str(&format!(
            "- {}{} — `src: {}`\n",
            c.claim.text(),
            about,
            c.claim.provenance().cite()
        ));
    }
    s.push('\n');

    if !layer.discrepancies.is_empty() {
        s.push_str(&format!(
            "## ⚠ Fail-closed flags ({})\n\n",
            layer.discrepancies.len()
        ));
        s.push_str("> The LLM implied access the authorization model does **not** grant. Flagged, not reconciled — access **NOT** widened.\n\n");
        for d in &layer.discrepancies {
            s.push_str(&format!(
                "- `{}` implicated on `{}` — {} — `src: {}`\n",
                d.principal_id,
                d.document_id,
                d.bases.join(", "),
                d.provenance.cite()
            ));
        }
        s.push('\n');
    }

    if !layer.rejected.is_empty() {
        s.push_str(&format!(
            "## Refused claims ({}) — out-of-scope citation, not written\n\n",
            layer.rejected.len()
        ));
        for r in &layer.rejected {
            s.push_str(&format!("- cited `{}` — {}\n", r.cited_doc_id, r.reason));
        }
        s.push('\n');
    }

    s
}

/// The standing drift-lint report: the slice-1 structural flags plus every
/// scope's LLM flags and refusals, surfaced together. It reports and fails
/// closed — it never reconciles or widens.
pub fn render_drift_report(structural_flags: usize, layers: &[ScopedLayer]) -> String {
    let mut s = String::new();
    s.push_str("# Drift lint — standing report\n\n");
    s.push_str(
        "> Fail-closed: this report surfaces divergence between derived structure/content and the \
         authorization model. It does NOT reconcile and does NOT widen access.\n\n",
    );
    s.push_str(&format!(
        "- Structural (deterministic, slice 1) fail-closed flags: **{structural_flags}**\n"
    ));
    let llm_flags: usize = layers.iter().map(|l| l.discrepancies.len()).sum();
    let refused: usize = layers.iter().map(|l| l.rejected.len()).sum();
    s.push_str(&format!(
        "- LLM (scoped, slice 2) fail-closed flags: **{llm_flags}**\n"
    ));
    s.push_str(&format!(
        "- Out-of-scope citations refused (never written): **{refused}**\n\n"
    ));

    for layer in layers {
        s.push_str(&format!(
            "## scope `{}` — {} fact(s), {} flag(s), {} refusal(s)\n\n",
            layer.principal_id,
            layer.claims.len(),
            layer.discrepancies.len(),
            layer.rejected.len()
        ));
        for d in &layer.discrepancies {
            s.push_str(&format!("- ⚠ {}\n", d.detail));
        }
        if layer.discrepancies.is_empty() {
            s.push_str("- (no LLM access-implication flags in this scope)\n");
        }
        s.push('\n');
    }
    s
}
