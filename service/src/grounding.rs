//! K1 grounded answers — deterministic extractive anchoring, per claim.
//!
//! Pattern credit: wiki/src/ground.rs (slice-4 extractive anchoring). The
//! state machine is RETYPED here on purpose — the production service must
//! not depend on the experimental wiki engine, so nothing is imported.
//!
//! v1 verifier = ANCHOR ONLY (ruling R-B): a claim is Admitted iff its quote
//! is a verbatim substring of the cited document's FULL body AND that
//! document is inside the sealed generation context. No judge, no semantic
//! entailment — the honest claim this earns is "every sentence is anchored
//! to a verbatim passage in a source you are authorized to see", never
//! "semantically verified". The `Verifier` seam below is where a judge
//! verification pass would plug in later; this slice wires `AnchorOnly`.
//!
//! Every rule fails closed: an unknown id, an empty quote, and a
//! non-verbatim quote each REFUSE the claim. Refusal reasons are fixed
//! labels, never dynamic content, so no document text can ride them.

use std::collections::BTreeMap;

/// One parsed draft claim: a sentence, the single document it cites, and the
/// verbatim quote it stakes that citation on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub text: String,
    pub doc_id: String,
    pub quote: String,
}

/// A proven anchor: the quote exists verbatim in the cited in-context
/// document at this byte offset. `locator` = "doc_id@byte_offset".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub doc_id: String,
    pub locator: String,
}

/// The grounding outcome for one claim. Only `Admitted` renders; `Refused`
/// is counted and disclosed (envelope `grounding` counts), never rendered.
#[derive(Debug, Clone)]
pub enum Grounded {
    Admitted { claim: Claim, anchor: Anchor },
    Refused { claim: Claim, reason: &'static str },
}

/// The verification seam, mirroring wiki's `Verifier`. v1 wires only
/// [`AnchorOnly`]; a judge-backed implementation is a later toggle (R-B).
pub trait Verifier {
    fn verifier_id(&self) -> &'static str;
    /// Whether the (already anchored) span supports the claim. Fail-closed:
    /// callers treat anything but `true` as "not supported".
    fn supports(&self, span: &str, claim: &str) -> bool;
}

/// v1: anchoring IS the admission test; the verifier passes every anchored
/// claim through. The seam exists so a judge can later tighten (never
/// widen) admission without touching the pipeline.
pub struct AnchorOnly;

impl Verifier for AnchorOnly {
    fn verifier_id(&self) -> &'static str {
        "anchor-only"
    }

    fn supports(&self, _span: &str, _claim: &str) -> bool {
        true
    }
}

/// Grounds one claim against the sealed context ONLY. `sealed_bodies` maps
/// each sealed doc id to its FULL body and is the single lookup this
/// function receives — there is no code path to any other document (G-4).
pub fn ground(
    claim: Claim,
    sealed_bodies: &BTreeMap<&str, &str>,
    verifier: &dyn Verifier,
) -> Grounded {
    // Brackets are the citation channel; claim text may not carry them, or a
    // claim could smuggle citations past the per-claim gate (fail closed —
    // stricter than spec, allowed without permission).
    if claim.text.contains('[') || claim.text.contains(']') {
        return Grounded::Refused {
            claim,
            reason: "claim text carries bracket characters reserved for citations",
        };
    }
    let Some(body) = sealed_bodies.get(claim.doc_id.as_str()) else {
        return Grounded::Refused {
            claim,
            reason: "cited document is not in the sealed context",
        };
    };
    let quote = claim.quote.trim();
    if quote.is_empty() {
        return Grounded::Refused {
            claim,
            reason: "no extractive anchor (empty quote)",
        };
    }
    // Exact bytes; the only normalization is the whitespace trim above.
    let Some(offset) = body.find(quote) else {
        return Grounded::Refused {
            claim,
            reason: "quote not found verbatim in the cited source",
        };
    };
    if !verifier.supports(quote, &claim.text) {
        return Grounded::Refused {
            claim,
            reason: "anchored span not confirmed as supporting the claim",
        };
    }
    let anchor = Anchor {
        doc_id: claim.doc_id.clone(),
        locator: format!("{}@{offset}", claim.doc_id),
    };
    Grounded::Admitted { claim, anchor }
}
