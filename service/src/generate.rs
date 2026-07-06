//! The answer generator. UNTRUSTED by construction: it receives the sealed
//! context and the user's question, and everything it produces goes through
//! citation validation before any byte of it is returned. The input type is
//! the seal — there is no field for scope statements, other principals,
//! corpus statistics, or system internals.

use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use retrieval::local_llm::{LocalLlmClient, TokenCounts};
use serde_json::json;

/// One sealed-context document: everything the generator may see about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextDoc {
    pub doc_id: String,
    pub title: String,
    /// Deterministic extraction, <= 480 chars of the document body.
    pub snippet: String,
}

pub struct GenerationOutcome {
    pub text: String,
    pub usage: Option<TokenCounts>,
}

pub trait Generator: Send + Sync {
    fn model_id(&self) -> &str;
    /// One attempt, no retries; an error means the answer degrades to
    /// retrieval-only (less compute, never less governance).
    fn generate(
        &self,
        query: &str,
        context: &[ContextDoc],
        timeout: Duration,
    ) -> Result<GenerationOutcome>;
}

// ---------------------------------------------------------------------------
// Ollama generator (via the loopback carve-out)
// ---------------------------------------------------------------------------

pub struct OllamaGenerator {
    client: LocalLlmClient,
    model: String,
}

impl OllamaGenerator {
    pub fn new(client: LocalLlmClient, model: &str) -> OllamaGenerator {
        OllamaGenerator {
            client,
            model: model.to_string(),
        }
    }
}

/// K1: the maximum number of draft claims a generation may carry. More is a
/// format fault, not a truncation — no salvage parsing.
pub const DRAFT_CLAIMS_MAX: usize = 6;

/// One machine-parseable draft claim as the generator emitted it: a
/// sentence, the single document it cites, and the verbatim quote it stakes
/// that citation on. Parsed here; ADMITTED (or refused) by `grounding`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftClaim {
    pub text: String,
    pub doc_id: String,
    pub quote: String,
}

/// STRICT parse of the generation contract. The output must be 1..=6 blocks
/// of exactly three lines each (blank lines between blocks allowed):
///
/// ```text
/// CLAIM: <one plain sentence>
/// SOURCE: <doc_id>
/// QUOTE: "<verbatim text>"
/// ```
///
/// ANY deviation — preamble, epilogue, a missing line, an unquoted QUOTE, a
/// multi-token SOURCE, more than 6 claims — is an error, which the pipeline
/// maps to a GenerationFault (degrade to retrieval-only). No salvage
/// parsing, no partial-block acceptance.
pub fn parse_claims(output: &str) -> Result<Vec<DraftClaim>> {
    let mut claims: Vec<DraftClaim> = Vec::new();
    let mut lines = output.lines().map(|l| l.trim_end_matches('\r'));
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue; // blank separator between blocks
        }
        let Some(text) = line.strip_prefix("CLAIM: ") else {
            bail!("generation format fault: expected a CLAIM line");
        };
        let text = text.trim();
        if text.is_empty() {
            bail!("generation format fault: empty CLAIM");
        }
        let source_line = lines
            .next()
            .context("generation format fault: missing SOURCE line")?;
        let Some(doc_id) = source_line.strip_prefix("SOURCE: ") else {
            bail!("generation format fault: expected a SOURCE line");
        };
        let doc_id = doc_id.trim();
        if doc_id.is_empty() || doc_id.contains(char::is_whitespace) {
            bail!("generation format fault: SOURCE must be exactly one document id");
        }
        let quote_line = lines
            .next()
            .context("generation format fault: missing QUOTE line")?;
        let Some(rest) = quote_line.strip_prefix("QUOTE: \"") else {
            bail!("generation format fault: expected a QUOTE line in double quotes");
        };
        let Some(quote) = rest.strip_suffix('"') else {
            bail!("generation format fault: QUOTE must end with a closing double quote");
        };
        claims.push(DraftClaim {
            text: text.to_string(),
            doc_id: doc_id.to_string(),
            quote: quote.to_string(),
        });
        if claims.len() > DRAFT_CLAIMS_MAX {
            bail!("generation format fault: more than {DRAFT_CLAIMS_MAX} claims");
        }
    }
    if claims.is_empty() {
        bail!("generation format fault: no claims parsed");
    }
    Ok(claims)
}

/// Deterministic prompt: the claim-block contract, the sealed documents, the
/// question. The generator is instructed to quote ONLY text visible in the
/// provided snippets; grounding later verifies each quote against the FULL
/// document body (a snippet is a body prefix, so honest quotes always pass).
fn build_prompt(query: &str, context: &[ContextDoc]) -> String {
    let mut prompt = String::from(
        "You extract grounded claims from documents. Answer the question \
         using ONLY the documents below. Output at most 6 claims. For each \
         claim output EXACTLY three lines in this format, with one blank \
         line between claims and NOTHING else — no preamble, no numbering, \
         no epilogue:\n\
         \n\
         CLAIM: <one plain sentence stating one fact>\n\
         SOURCE: <the id of the one document that states it>\n\
         QUOTE: \"<a short passage copied character-for-character from that \
         document's text below>\"\n\
         \n\
         Format example (shape only — never copy its content):\n\
         \n\
         CLAIM: The example policy takes effect in March.\n\
         SOURCE: d0000\n\
         QUOTE: \"takes effect on 1 March\"\n\
         \n\
         Rules: the QUOTE line must wrap the passage in double quotes \
         exactly as the example shows. The QUOTE must be an exact verbatim \
         substring of the chosen document's text as printed below — copy it \
         exactly, never paraphrase, keep the original capitalization, \
         prefer 5 to 20 words, keep it on one line. Use one SOURCE id per \
         claim, chosen from the ids below. Never use square brackets inside \
         the CLAIM sentence. If the documents do not answer the question, \
         output one claim stating the closest relevant fact they do \
         contain.\n\n",
    );
    for doc in context {
        prompt.push_str(&format!(
            "[{}] {}\n{}\n\n",
            doc.doc_id, doc.title, doc.snippet
        ));
    }
    prompt.push_str(&format!("Question: {query}\nClaims:"));
    prompt
}

impl Generator for OllamaGenerator {
    fn model_id(&self) -> &str {
        &self.model
    }

    fn generate(
        &self,
        query: &str,
        context: &[ContextDoc],
        timeout: Duration,
    ) -> Result<GenerationOutcome> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": build_prompt(query, context) }],
            "stream": false,
            // num_predict sized for 6 claim blocks with quotes; temperature 0
            // + fixed seed keep the draft stable for the strict parser.
            "options": { "temperature": 0, "seed": 7, "num_predict": 512 }
        });
        let response = self.client.post_json("/api/chat", &body, timeout)?;
        let text = response
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .context("unexpected generator response shape")?
            .to_string();
        let usage = match (
            response.get("prompt_eval_count").and_then(|v| v.as_u64()),
            response.get("eval_count").and_then(|v| v.as_u64()),
        ) {
            (Some(input_tokens), Some(output_tokens)) => Some(TokenCounts {
                input_tokens,
                output_tokens,
            }),
            _ => None,
        };
        Ok(GenerationOutcome { text, usage })
    }
}

// ---------------------------------------------------------------------------
// Mock generator (tests are fully offline)
// ---------------------------------------------------------------------------

pub enum MockBehavior {
    /// One well-formed, groundable claim on the first context document.
    CiteFirst,
    /// One well-formed, groundable claim per context document, in order.
    CiteEach,
    /// A well-formed claim citing a document it was never given (grounding
    /// must refuse it — the sealed-context rule, per claim).
    ForeignCitation,
    /// A well-formed claim on the first context document whose quote exists
    /// in NO document (grounding must refuse it — the verbatim rule).
    FabricatedQuote,
    /// A well-formed claim citing a document OUTSIDE the context while its
    /// quote verbatim-exists in the FIRST context document (grounding must
    /// refuse it — the anchor binds to the CITED doc, not to any doc).
    WrongSourceRealQuote,
    /// One grounded claim per context document, then one fabricated-quote
    /// claim and one foreign-citation claim (drop-with-disclosure counts).
    Mixed,
    /// Confident free prose with no claim blocks (a format fault).
    Uncited,
    /// Exactly this raw output, whatever it is (parser edge cases).
    Raw(String),
    /// Fails, exercising the generator-unreachable degradation branch.
    Fail,
}

/// A deterministic, groundable quote for a context document: the snippet's
/// first line, capped at 40 chars. The snippet is a body prefix, so this is
/// always a verbatim substring of the full body.
fn groundable_quote(doc: &ContextDoc) -> String {
    doc.snippet
        .chars()
        .take_while(|c| *c != '\n')
        .take(40)
        .collect::<String>()
        .trim_end()
        .to_string()
}

fn claim_block(text: &str, doc_id: &str, quote: &str) -> String {
    format!("CLAIM: {text}\nSOURCE: {doc_id}\nQUOTE: \"{quote}\"")
}

/// Captures every input it is shown, so A-1 can assert the generation seal.
pub struct MockGenerator {
    pub behavior: MockBehavior,
    pub captured: Mutex<Vec<(String, Vec<ContextDoc>)>>,
}

impl MockGenerator {
    pub fn new(behavior: MockBehavior) -> MockGenerator {
        MockGenerator {
            behavior,
            captured: Mutex::new(Vec::new()),
        }
    }
}

impl Generator for MockGenerator {
    fn model_id(&self) -> &str {
        "mock-generator"
    }

    fn generate(
        &self,
        query: &str,
        context: &[ContextDoc],
        _timeout: Duration,
    ) -> Result<GenerationOutcome> {
        self.captured
            .lock()
            .expect("mock generator mutex")
            .push((query.to_string(), context.to_vec()));
        let text = match &self.behavior {
            MockBehavior::CiteFirst => {
                let first = context.first().context("empty context")?;
                claim_block(
                    &format!("Deterministic mock claim grounded in {}.", first.doc_id),
                    &first.doc_id,
                    &groundable_quote(first),
                )
            }
            MockBehavior::CiteEach => context
                .iter()
                .map(|d| {
                    claim_block(
                        &format!("Deterministic mock claim grounded in {}.", d.doc_id),
                        &d.doc_id,
                        &groundable_quote(d),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            MockBehavior::ForeignCitation => claim_block(
                "Confident claim from nowhere.",
                "d9999_foreign",
                "nowhere at all",
            ),
            MockBehavior::FabricatedQuote => {
                let first = context.first().context("empty context")?;
                claim_block(
                    "Confidently misquoted claim.",
                    &first.doc_id,
                    "this exact sentence appears in no corpus document whatsoever",
                )
            }
            MockBehavior::WrongSourceRealQuote => {
                let first = context.first().context("empty context")?;
                claim_block(
                    "Real quote pinned on the wrong document.",
                    "d9999_foreign",
                    &groundable_quote(first),
                )
            }
            MockBehavior::Mixed => {
                let mut blocks: Vec<String> = context
                    .iter()
                    .take(2)
                    .map(|d| {
                        claim_block(
                            &format!("Deterministic mock claim grounded in {}.", d.doc_id),
                            &d.doc_id,
                            &groundable_quote(d),
                        )
                    })
                    .collect();
                let first = context.first().context("empty context")?;
                blocks.push(claim_block(
                    "Confidently misquoted claim.",
                    &first.doc_id,
                    "this exact sentence appears in no corpus document whatsoever",
                ));
                blocks.push(claim_block(
                    "Confident claim from nowhere.",
                    "d9999_foreign",
                    "nowhere at all",
                ));
                blocks.join("\n\n")
            }
            MockBehavior::Uncited => "Confident claim with no citation at all.".to_string(),
            MockBehavior::Raw(output) => output.clone(),
            MockBehavior::Fail => bail!("mock generator unreachable"),
        };
        Ok(GenerationOutcome { text, usage: None })
    }
}
