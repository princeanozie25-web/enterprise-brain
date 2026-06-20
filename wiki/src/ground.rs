//! Slice 4 — grounded claims: extractive anchoring + judge support-verification.
//!
//! A claim is admitted ONLY if BOTH hold:
//!   1. EXTRACTIVE ANCHORING (deterministic): it cites a verbatim source span
//!      that provably EXISTS (exact substring) in a document within the deriving
//!      scope. A claim whose span is not found verbatim in an in-scope source is
//!      REFUSED — it is unfounded.
//!   2. SUPPORT VERIFICATION (judge, fail-closed): a separate judge model
//!      confirms the span SUPPORTS the claim. Unconfirmed -> the claim is
//!      FLAGGED and WITHHELD, never silently kept.
//!
//! HONEST FRAMING: anchoring is a real deterministic proof (the claim is tied to
//! verbatim in-scope source text). Support is a JUDGE MODEL'S assessment,
//! conservative by fail-closing — it reduces hallucination risk; it is NOT a
//! proof of faithfulness. "anchored + support-checked, fail-closed", never
//! "proven correct".
//!
//! STATUS (honest, not hidden): the REFUSE (unfounded) and WITHHELD (unsupported)
//! paths are exercised; the live ADMIT path OVER-REFUSES by design and admits
//! ~zero on the current local judge models — so admit is unrealized end-to-end
//! pending a stronger judge. "Grounded claims" names the proven
//! admit/refuse/withhold STATE MACHINE, not a present-tense stream of delivered
//! admitted claims.
//!
//! No authz path here: the judge sees only a span and a claim (both in-scope),
//! over the workspace's audited loopback-only client. Data never leaves.

use std::time::Duration;

use anyhow::{Context, Result};
use retrieval::local_llm::LocalLlmClient;
use serde_json::json;

/// An extractive anchor: a verbatim span that provably exists in an in-scope
/// source, with where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub source_ref: String,
    pub span_text: String,
    pub locator: String,
}

/// A judge's support assessment. FAIL-CLOSED: only `supported == true` admits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportVerdict {
    pub supported: bool,
    pub judge_model: String,
}

/// The grounding outcome for one synthesized claim.
#[derive(Debug, Clone)]
pub enum Grounded {
    /// Anchored to a verbatim in-scope span AND judge-confirmed supported.
    Admitted {
        anchor: Anchor,
        support: SupportVerdict,
    },
    /// No verbatim in-scope span backs the claim — unfounded; refused, not kept.
    RefusedUnfounded { source_ref: String, reason: String },
    /// Anchored, but the judge did not confirm support — withheld (fail-closed).
    Withheld {
        anchor: Anchor,
        support: SupportVerdict,
    },
}

/// The support-verification seam. The judge is given ONLY the span and the
/// claim — never any out-of-scope context.
pub trait Verifier {
    fn model_id(&self) -> &str;
    /// Does `span` support `claim`? The real implementation fail-closes on any
    /// error; `ground_claim` additionally treats `Err` as "not supported".
    fn supports(&self, span: &str, claim: &str) -> Result<bool>;
}

/// Grounds one claim against the body of its cited (already in-scope) source.
/// Infallible: anchoring is deterministic, and a verifier error fail-closes to
/// "not supported" (the claim is withheld), never propagated.
pub fn ground_claim(
    claim_text: &str,
    source_ref: &str,
    quote: &str,
    source_body: &str,
    verifier: &dyn Verifier,
) -> Grounded {
    // 1. Extractive anchoring (deterministic): the quote must be a non-empty
    //    verbatim substring of the in-scope source body.
    let q = quote.trim();
    if q.is_empty() {
        return Grounded::RefusedUnfounded {
            source_ref: source_ref.to_string(),
            reason: "no extractive anchor (empty quote)".to_string(),
        };
    }
    let Some(offset) = source_body.find(q) else {
        return Grounded::RefusedUnfounded {
            source_ref: source_ref.to_string(),
            reason: "quote not found verbatim in the cited in-scope source".to_string(),
        };
    };
    let anchor = Anchor {
        source_ref: source_ref.to_string(),
        span_text: q.to_string(),
        locator: format!("{source_ref}@{offset}"),
    };

    // 2. Support verification (judge, fail-closed): any error -> not supported.
    let supported = verifier.supports(q, claim_text).unwrap_or(false);
    let support = SupportVerdict {
        supported,
        judge_model: verifier.model_id().to_string(),
    };
    if supported {
        Grounded::Admitted { anchor, support }
    } else {
        Grounded::Withheld { anchor, support }
    }
}

// ---------------------------------------------------------------------------
// The real local-Ollama judge
// ---------------------------------------------------------------------------

/// A local judge (`--judge-model`) given ONLY the span + the claim, asked YES/NO
/// over the loopback-only client. FAIL-CLOSED: only a clear leading "YES" counts
/// as supported; an empty/garbled/NO answer, or any call error, is "not
/// supported". A weaker judge therefore OVER-refuses, which is safe.
pub struct OllamaVerifier {
    client: LocalLlmClient,
    model: String,
    timeout: Duration,
}

impl OllamaVerifier {
    pub fn new(endpoint: &str, model: &str, timeout: Duration) -> Result<OllamaVerifier> {
        let client = LocalLlmClient::new(endpoint)
            .context("constructing loopback-only judge client for support verification")?;
        Ok(OllamaVerifier {
            client,
            model: model.to_string(),
            timeout,
        })
    }
}

impl Verifier for OllamaVerifier {
    fn model_id(&self) -> &str {
        &self.model
    }

    fn supports(&self, span: &str, claim: &str) -> Result<bool> {
        let prompt = format!(
            "You are a strict fact-checker. Decide whether the SOURCE TEXT DIRECTLY supports \
             the CLAIM. If the source does not clearly state or entail the claim, answer NO. \
             Answer with exactly one word: YES or NO.\n\n\
             SOURCE TEXT:\n{span}\n\nCLAIM:\n{claim}\n\nAnswer (YES or NO):"
        );
        let body = json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "think": false,
            "options": { "temperature": 0, "num_predict": 8 }
        });
        let resp = self
            .client
            .post_json("/api/generate", &body, self.timeout)
            .context("local judge generate call failed")?;
        let text = resp
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        Ok(is_affirmative(text))
    }
}

/// FAIL-CLOSED affirmation: true ONLY if the first alphabetic word of the
/// (reasoning-stripped) response is "yes".
pub fn is_affirmative(text: &str) -> bool {
    strip_think(text)
        .split(|c: char| !c.is_ascii_alphabetic())
        .find(|w| !w.is_empty())
        .is_some_and(|w| w.eq_ignore_ascii_case("yes"))
}

/// Removes `<think>…</think>` spans (reasoning-model scratchpad).
fn strip_think(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("<think>") {
        out.push_str(&rest[..open]);
        if let Some(close) = rest[open..].find("</think>") {
            rest = &rest[open + close + "</think>".len()..];
        } else {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedVerifier(bool);
    impl Verifier for FixedVerifier {
        fn model_id(&self) -> &str {
            "fixed"
        }
        fn supports(&self, _span: &str, _claim: &str) -> Result<bool> {
            Ok(self.0)
        }
    }

    #[test]
    fn empty_or_absent_quote_is_refused_unfounded() {
        let v = FixedVerifier(true);
        assert!(matches!(
            ground_claim("c", "d1", "", "body text", &v),
            Grounded::RefusedUnfounded { .. }
        ));
        assert!(matches!(
            ground_claim("c", "d1", "not present", "body text", &v),
            Grounded::RefusedUnfounded { .. }
        ));
    }

    #[test]
    fn verbatim_quote_anchors_then_support_decides() {
        let body = "the cold chain is held at 2 to 8 degrees";
        // Supported -> admitted, with the anchor located.
        match ground_claim(
            "cold chain held cold",
            "d1",
            "2 to 8 degrees",
            body,
            &FixedVerifier(true),
        ) {
            Grounded::Admitted { anchor, support } => {
                assert_eq!(anchor.span_text, "2 to 8 degrees");
                assert!(anchor.locator.contains("d1@"));
                assert!(support.supported);
            }
            other => panic!("expected Admitted, got {other:?}"),
        }
        // Anchored but unsupported -> withheld (fail-closed).
        assert!(matches!(
            ground_claim(
                "unrelated",
                "d1",
                "2 to 8 degrees",
                body,
                &FixedVerifier(false)
            ),
            Grounded::Withheld { .. }
        ));
    }

    #[test]
    fn affirmation_is_fail_closed() {
        assert!(is_affirmative("YES"));
        assert!(is_affirmative("Yes, it does."));
        assert!(is_affirmative("<think>hmm</think> yes"));
        assert!(!is_affirmative("NO"));
        assert!(!is_affirmative("Not really"));
        assert!(!is_affirmative(""));
        assert!(!is_affirmative("I think yes")); // first word is "I", fail-closed
    }
}
