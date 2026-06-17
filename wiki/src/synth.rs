//! The LLM seam for scoped content derivation.
//!
//! A [`Synthesizer`] receives ONLY in-scope source documents (the caller
//! guarantees this — see `scoped::derive_scope`) and returns raw statements,
//! each tagging the source doc id it claims to draw from. The trait is the
//! choke point that lets the security tests (non-leakage, provenance,
//! fail-closed) run deterministically with a recording fake, while the real
//! [`OllamaSynthesizer`] talks to a local model — so the firewall and the
//! scope-gate are proven without depending on a live LLM.
//!
//! The real client reuses `retrieval::local_llm::LocalLlmClient`, the
//! workspace's audited loopback-ONLY HTTP client (refuses non-loopback hosts
//! and `https` at construction). Data never leaves the machine.

use std::time::Duration;

use anyhow::{Context, Result};
use retrieval::local_llm::LocalLlmClient;
use serde_json::json;

/// One in-scope source document handed to the synthesizer. Constructing this
/// for a doc is the caller's assertion that the doc is inside the scope.
#[derive(Debug, Clone)]
pub struct SourceDoc {
    pub doc_id: String,
    pub title: String,
    pub text: String,
}

/// One raw statement from the synthesizer. `cited_doc_id` is UNTRUSTED until
/// the pipeline validates it against the in-scope source set; `quote` is the
/// verbatim source span the model claims supports the fact (slice-4 extractive
/// anchor — validated downstream against the cited source); `about_principal`
/// is an optional entity the statement implicates, checked fail-closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawClaim {
    pub text: String,
    pub cited_doc_id: String,
    pub quote: String,
    pub about_principal: Option<String>,
}

/// The synthesis seam. Implementations MUST treat `sources` as the only
/// material they may use; they never reach for anything else.
pub trait Synthesizer {
    fn model_id(&self) -> &str;

    /// Synthesize statements about `topic` from `sources` (all in-scope).
    fn synthesize(&self, topic: &str, sources: &[SourceDoc]) -> Result<Vec<RawClaim>>;
}

// ---------------------------------------------------------------------------
// Prompt + parse (pure; shared by the real client and unit tests)
// ---------------------------------------------------------------------------

/// How many chars of each source body to include (keeps prompts bounded; the
/// local model is CPU-bound, so prompt size dominates latency).
const BODY_CHARS: usize = 300;

pub fn build_prompt(topic: &str, sources: &[SourceDoc]) -> String {
    let mut p = String::new();
    p.push_str(
        "You are summarizing an internal corpus. Use ONLY the documents listed below. \
         Do not use outside knowledge. Every fact MUST cite the SOURCE document id it came \
         from, and you may ONLY cite ids that appear in this list.\n\n",
    );
    p.push_str(&format!("TOPIC: {topic}\n\nDOCUMENTS:\n"));
    for d in sources {
        let body: String = d.text.chars().take(BODY_CHARS).collect();
        p.push_str(&format!("[{}] {}\n{}\n\n", d.doc_id, d.title, body));
    }
    p.push_str(
        "Output one line per fact, EXACTLY in this format, and nothing else:\n\
         FACT: <one sentence> || SOURCE: <doc_id> || QUOTE: <a short EXACT phrase \
         copied verbatim from that document that supports the fact> || ABOUT: <person_id or NONE>\n\
         Cite only doc ids from the list above. The QUOTE must be copied word-for-word from the \
         cited document (no paraphrase) and must not contain the characters ||. Use ABOUT only for \
         a person id the document names, else NONE. Produce at most 6 facts.",
    );
    p
}

/// Parse the model's text into raw claims, robust to BOTH the inline
/// `FACT: … || SOURCE: … || ABOUT: …` shape and the multi-line shape some
/// models emit (FACT / SOURCE / ABOUT on separate lines). Strips a leading
/// reasoning block (`<think>…</think>`) first. Validation is the pipeline's job.
pub fn parse_synthesis(text: &str) -> Vec<RawClaim> {
    let cleaned = strip_think(text);
    let mut out = Vec::new();
    let mut fact: Option<String> = None;
    let mut source: Option<String> = None;
    let mut quote: Option<String> = None;
    let mut about: Option<String> = None;

    for line in cleaned.lines() {
        for seg in line.split("||") {
            let seg = seg.trim();
            if let Some(v) = seg.strip_prefix("FACT:") {
                // A new fact begins: flush the previous complete one first.
                push_claim(&mut fact, &mut source, &mut quote, &mut about, &mut out);
                fact = Some(v.trim().to_string());
            } else if let Some(v) = seg.strip_prefix("SOURCE:") {
                source = Some(v.trim().to_string());
            } else if let Some(v) = seg.strip_prefix("QUOTE:") {
                quote = Some(v.trim().to_string());
            } else if let Some(v) = seg.strip_prefix("ABOUT:") {
                about = Some(v.trim().to_string());
            }
        }
    }
    push_claim(&mut fact, &mut source, &mut quote, &mut about, &mut out);
    out
}

/// Emits a claim iff a non-empty fact AND source were collected, then clears all
/// slots (so a partial group is dropped, not carried forward). A missing QUOTE
/// becomes an empty span — which the downstream extractive-anchoring check
/// refuses (fail-closed), so an unquoted claim is never admitted.
fn push_claim(
    fact: &mut Option<String>,
    source: &mut Option<String>,
    quote: &mut Option<String>,
    about: &mut Option<String>,
    out: &mut Vec<RawClaim>,
) {
    let about_principal = about
        .take()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty() && !a.eq_ignore_ascii_case("none"));
    let q = quote
        .take()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if let (Some(f), Some(s)) = (fact.take(), source.take()) {
        if !f.trim().is_empty() && !s.trim().is_empty() {
            out.push(RawClaim {
                text: f.trim().to_string(),
                cited_doc_id: s.trim().to_string(),
                quote: q,
                about_principal,
            });
        }
    }
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

// ---------------------------------------------------------------------------
// The real local-Ollama synthesizer
// ---------------------------------------------------------------------------

/// Talks to a local Ollama `/api/generate` via the loopback-only client. The
/// model id is a config value (never hard-coded), so swapping models is a
/// config change for synthesis QUALITY, not a code change for CORRECTNESS.
pub struct OllamaSynthesizer {
    client: LocalLlmClient,
    model: String,
    timeout: Duration,
}

impl OllamaSynthesizer {
    pub fn new(endpoint: &str, model: &str, timeout: Duration) -> Result<OllamaSynthesizer> {
        let client = LocalLlmClient::new(endpoint)
            .context("constructing loopback-only LLM client for synthesis")?;
        Ok(OllamaSynthesizer {
            client,
            model: model.to_string(),
            timeout,
        })
    }
}

impl Synthesizer for OllamaSynthesizer {
    fn model_id(&self) -> &str {
        &self.model
    }

    fn synthesize(&self, topic: &str, sources: &[SourceDoc]) -> Result<Vec<RawClaim>> {
        if sources.is_empty() {
            return Ok(Vec::new());
        }
        let prompt = build_prompt(topic, sources);
        let body = json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            // Disable the reasoning block on thinking models (e.g. deepseek-r1)
            // for speed; harmless for non-thinking models. Any <think> that does
            // appear is stripped during parsing. Output is capped — we only need
            // a handful of FACT lines.
            "think": false,
            "options": { "temperature": 0, "num_predict": 512 }
        });
        let resp = self
            .client
            .post_json("/api/generate", &body, self.timeout)
            .context("local Ollama generate call failed")?;
        let text = resp
            .get("response")
            .and_then(|v| v.as_str())
            .context("Ollama response has no 'response' field")?;
        Ok(parse_synthesis(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_fact_lines_with_quotes_and_strips_thinking() {
        let text = "<think>let me reason... SOURCE could be d9999</think>\n\
                    FACT: Cold chain is monitored at 2-8C || SOURCE: d0001 || QUOTE: between 2C and 8C || ABOUT: NONE\n\
                    noise line\n\
                    FACT: Pablo owns the QA agent || SOURCE: d0002 || QUOTE: owner of the QA agent || ABOUT: p093\n";
        let claims = parse_synthesis(text);
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].cited_doc_id, "d0001");
        assert_eq!(claims[0].quote, "between 2C and 8C");
        assert_eq!(claims[0].about_principal, None);
        assert_eq!(claims[1].cited_doc_id, "d0002");
        assert_eq!(claims[1].quote, "owner of the QA agent");
        assert_eq!(claims[1].about_principal.as_deref(), Some("p093"));
        // The reasoning block's stray "d9999" never becomes a citation.
        assert!(claims.iter().all(|c| c.cited_doc_id != "d9999"));
    }

    #[test]
    fn a_missing_quote_parses_to_an_empty_span() {
        // No QUOTE segment -> empty span, which extractive anchoring refuses.
        let claims = parse_synthesis("FACT: x || SOURCE: d0001 || ABOUT: NONE\n");
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].quote, "");
    }
}
