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
//! document — provenance + scope. Slice 4 adds extractive anchoring + a judge
//! support-check on top (see `ground.rs`); semantic support is the JUDGE'S
//! assessment, not a proof — a loose-paraphrasing model is reduced, not provably
//! caught, and the live admit path admits ~zero on current local judges. The
//! fail-closed flag fires for a structured `ABOUT: <known-principal>` the model
//! does not grant; slice 5 (`mentions.rs`) additionally flags FREE-TEXT principal
//! mentions (known-roster principals, apostrophe variants included), but does NOT
//! identify non-roster entities — that would need a model, a deliberate non-goal
//! for this deterministic matcher.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use crate::authz::GrantOracle;
use crate::derive::Discrepancy;
use crate::ground::{ground_claim, Anchor, Grounded, SupportVerdict, Verifier};
use crate::mentions::{MentionFlag, Roster};
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

/// One admitted LLM-derived content claim: grounded (anchored to a verbatim
/// in-scope span) AND judge-confirmed supported (slice 4), on top of the slice-2
/// scope cite.
#[derive(Debug, Clone)]
pub struct ScopedClaim {
    pub claim: Claim,
    pub anchor: Anchor,
    pub support: SupportVerdict,
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
    /// Free-text principal mentions in admitted prose the scope is not granted
    /// about (slice 5). Additive coverage on the structured `discrepancies`
    /// flag: flagged and surfaced, fail-closed, access never widened.
    pub mention_flags: Vec<MentionFlag>,
    /// Cited an out-of-scope / unknown source (slice-2 scope gate).
    pub rejected: Vec<RejectedClaim>,
    /// No verbatim in-scope span backs the claim — unfounded (slice-4 anchoring).
    pub refused_unfounded: Vec<RejectedClaim>,
    /// Anchored, but the judge did not confirm support — withheld (slice-4, fail-closed).
    pub withheld: Vec<RejectedClaim>,
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
#[allow(clippy::too_many_arguments)]
pub fn derive_scope(
    sources: &Sources,
    ctx: &ScopeContext,
    topics: &[Topic],
    selector: &dyn DocSelector,
    synth: &dyn Synthesizer,
    verifier: &dyn Verifier,
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
        mention_flags: Vec::new(),
        rejected: Vec::new(),
        refused_unfounded: Vec::new(),
        withheld: Vec::new(),
    };

    // Slice 5: the deterministic roster, for free-text mention flagging on
    // admitted prose. Built once from the read-only sources.
    let roster = Roster::from_sources(sources);

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
        // The in-scope source bodies grounding will anchor verbatim spans against.
        let body_of: BTreeMap<&str, &str> = sources_in
            .iter()
            .map(|s| (s.doc_id.as_str(), s.text.as_str()))
            .collect();
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

            // 3b. GROUNDING (slice 4): a verbatim, in-scope, existing source span
            //     (extractive anchor) AND judge-confirmed support, fail-closed.
            //     No anchor -> refused (unfounded); judge unconfirmed -> withheld.
            let body = body_of
                .get(raw.cited_doc_id.as_str())
                .copied()
                .unwrap_or("");
            let (anchor, support) =
                match ground_claim(&raw.text, &raw.cited_doc_id, &raw.quote, body, verifier) {
                    Grounded::RefusedUnfounded { reason, .. } => {
                        layer.refused_unfounded.push(RejectedClaim {
                            text: raw.text.clone(),
                            cited_doc_id: raw.cited_doc_id.clone(),
                            reason,
                        });
                        continue;
                    }
                    Grounded::Withheld { .. } => {
                        layer.withheld.push(RejectedClaim {
                            text: raw.text.clone(),
                            cited_doc_id: raw.cited_doc_id.clone(),
                            reason: format!(
                                "judge `{}` did not confirm support; withheld",
                                verifier.model_id()
                            ),
                        });
                        continue;
                    }
                    Grounded::Admitted { anchor, support } => (anchor, support),
                };

            // 3c. FAIL-CLOSED gate (admitted claims only): an implicated principal
            //     the model does not grant the cited doc -> flag, never widen.
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

            // 3d. FREE-TEXT MENTION gate (slice 5, admitted claims only): the
            //     prose may NAME a principal even when no structured ABOUT was
            //     emitted. Flag, fail-closed, every named principal the scope is
            //     not granted about, and every ambiguous token. Additive to 3c —
            //     it never suppresses the claim and never widens access.
            layer.mention_flags.extend(roster.flag_prose(
                &gate.principal_id,
                gate.allowed(),
                &raw.text,
                &prov.cite(),
            ));

            let text = format!(
                "{} [LLM-derived in scope {} via {}]",
                raw.text.trim(),
                gate.principal_id,
                synth.model_id()
            );
            layer.claims.push(ScopedClaim {
                claim: Claim::new(text, prov),
                anchor,
                support,
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
         Compiled model `{}`. Evidence is over the synthetic Bryremead corpus with a local \
         model — not a production-scale proof.\n\n",
        layer.model, layer.allowed_count, layer.snapshot_version
    ));
    s.push_str(&format!(
        "Grounding: **{}** admitted (verbatim-anchored + support-checked, fail-closed) · \
         **{}** refused-unfounded (no verbatim in-scope span) · **{}** withheld-unsupported. \
         Support is a judge's assessment — anchored + support-checked, NOT proven faithful; on \
         current local judges the live admit path over-refuses and admits ~zero.\n\n",
        layer.claims.len(),
        layer.refused_unfounded.len(),
        layer.withheld.len()
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
            "- {}{} — `src: {}` · `anchor: \"{}\" @{}` · support: {} (judge `{}`)\n",
            c.claim.text(),
            about,
            c.claim.provenance().cite(),
            c.anchor.span_text.replace('\n', " "),
            c.anchor.locator,
            if c.support.supported {
                "confirmed"
            } else {
                "unconfirmed"
            },
            c.support.judge_model,
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

    if !layer.mention_flags.is_empty() {
        s.push_str(&format!(
            "## ⚠ Free-text mention flags ({}) — slice 5\n\n",
            layer.mention_flags.len()
        ));
        s.push_str("> Admitted prose NAMED a principal the scope is **not** granted about (or an ambiguous name token). Detected deterministically against the roster (known-roster principals only, apostrophe variants included — non-roster entities are **not** identified) — flagged, **not** reconciled; access **NOT** widened.\n\n");
        for m in &layer.mention_flags {
            let who = match &m.mentioned_id {
                Some(id) => format!("`{id}`"),
                None => format!(
                    "ambiguous `{}` → {{{}}}",
                    m.surface,
                    m.candidates.join(", ")
                ),
            };
            s.push_str(&format!("- mention {who} — `src: {}`\n", m.cited_source));
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
    if !layer.refused_unfounded.is_empty() {
        s.push_str(&format!(
            "## Refused — unfounded ({}) — no verbatim in-scope span, not written\n\n",
            layer.refused_unfounded.len()
        ));
        for r in &layer.refused_unfounded {
            s.push_str(&format!("- cited `{}` — {}\n", r.cited_doc_id, r.reason));
        }
        s.push('\n');
    }
    if !layer.withheld.is_empty() {
        s.push_str(&format!(
            "## Withheld — unsupported ({}) — anchored but judge did not confirm support (fail-closed)\n\n",
            layer.withheld.len()
        ));
        for r in &layer.withheld {
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
         authorization model. It does NOT reconcile and does NOT widen access. Counts are over the \
         synthetic Bryremead corpus with local models — evidence, not a production-scale proof.\n\n",
    );
    s.push_str(&format!(
        "- Structural (deterministic, slice 1) fail-closed flags: **{structural_flags}**\n"
    ));
    let llm_flags: usize = layers.iter().map(|l| l.discrepancies.len()).sum();
    let mention_flags: usize = layers.iter().map(|l| l.mention_flags.len()).sum();
    let refused: usize = layers.iter().map(|l| l.rejected.len()).sum();
    s.push_str(&format!(
        "- LLM structured-association fail-closed flags (slice 2, `ABOUT:`): **{llm_flags}**\n"
    ));
    s.push_str(&format!(
        "- LLM free-text principal-mention flags (slice 5, prose, fail-closed): **{mention_flags}**\n"
    ));
    s.push_str(&format!(
        "- Out-of-scope citations refused (never written): **{refused}**\n\n"
    ));

    for layer in layers {
        s.push_str(&format!(
            "## scope `{}` — {} fact(s), {} structured flag(s), {} free-text mention flag(s), {} refusal(s)\n\n",
            layer.principal_id,
            layer.claims.len(),
            layer.discrepancies.len(),
            layer.mention_flags.len(),
            layer.rejected.len()
        ));
        for d in &layer.discrepancies {
            s.push_str(&format!("- ⚠ structured: {}\n", d.detail));
        }
        for m in &layer.mention_flags {
            s.push_str(&format!("- ⚠ free-text: {}\n", m.detail));
        }
        if layer.discrepancies.is_empty() && layer.mention_flags.is_empty() {
            s.push_str("- (no access-implication or free-text-mention flags in this scope)\n");
        }
        s.push('\n');
    }
    s
}
