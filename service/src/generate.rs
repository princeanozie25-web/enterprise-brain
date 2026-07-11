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

    /// SHOWCASE-III: draft a STAGED WORKFLOW (box blocks) from a project name +
    /// goal over the same sealed context. Additive — the `/ask` path never
    /// calls this, so `generate()`'s behaviour (and every A/G byte) is
    /// untouched. Default fails closed: a generator that cannot draft boxes
    /// degrades to no-proposal, never to an ungrounded one.
    fn generate_boxes(
        &self,
        _title: &str,
        _goal: &str,
        _context: &[ContextDoc],
        _timeout: Duration,
    ) -> Result<GenerationOutcome> {
        bail!("this generator does not draft workflow boxes")
    }
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
            bail!(
                "generation format fault: SOURCE must be exactly one document id (saw {:?})",
                doc_id.chars().take(60).collect::<String>()
            );
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

/// SHOWCASE-III: the maximum number of draft boxes a proposal may carry. More
/// is a format fault, not a truncation — no salvage parsing.
pub const DRAFT_BOXES_MAX: usize = 8;

/// One machine-parseable draft workflow box: a short title, a one/two-sentence
/// description of what to do, and the single document + verbatim quote the box
/// is anchored to. Parsed here; ADMITTED (or refused) by `grounding` per box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftBox {
    pub title: String,
    pub description: String,
    pub doc_id: String,
    pub quote: String,
}

/// STRICT parse of the box contract. The output must be 1..=8 blocks of exactly
/// four lines each (blank lines between blocks allowed):
///
/// ```text
/// BOX: <title, <= 80 chars>
/// DESC: <one or two plain sentences>
/// SOURCE: <doc_id>
/// QUOTE: "<verbatim text>"
/// ```
///
/// ANY deviation is an error → GenerationFault → NO proposal. Square brackets
/// (the citation channel) are refused in BOTH title and description, so a box
/// cannot smuggle a citation past the per-box grounding gate (fail closed).
pub fn parse_boxes(output: &str) -> Result<Vec<DraftBox>> {
    let mut boxes: Vec<DraftBox> = Vec::new();
    let mut lines = output.lines().map(|l| l.trim_end_matches('\r'));
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue; // blank separator between blocks
        }
        let Some(title) = line.strip_prefix("BOX: ") else {
            bail!("generation format fault: expected a BOX line");
        };
        let title = title.trim();
        if title.is_empty() {
            bail!("generation format fault: empty BOX title");
        }
        if title.chars().count() > 80 {
            bail!("generation format fault: BOX title exceeds 80 characters");
        }
        if title.contains('[') || title.contains(']') {
            bail!("generation format fault: BOX title carries bracket characters reserved for citations");
        }
        let desc_line = lines
            .next()
            .context("generation format fault: missing DESC line")?;
        let Some(description) = desc_line.strip_prefix("DESC: ") else {
            bail!("generation format fault: expected a DESC line");
        };
        let description = description.trim();
        if description.is_empty() {
            bail!("generation format fault: empty DESC");
        }
        if description.contains('[') || description.contains(']') {
            bail!(
                "generation format fault: DESC carries bracket characters reserved for citations"
            );
        }
        let source_line = lines
            .next()
            .context("generation format fault: missing SOURCE line")?;
        let Some(doc_id) = source_line.strip_prefix("SOURCE: ") else {
            bail!("generation format fault: expected a SOURCE line");
        };
        let doc_id = doc_id.trim();
        if doc_id.is_empty() || doc_id.contains(char::is_whitespace) {
            bail!(
                "generation format fault: SOURCE must be exactly one document id (saw {:?})",
                doc_id.chars().take(60).collect::<String>()
            );
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
        boxes.push(DraftBox {
            title: title.to_string(),
            description: description.to_string(),
            doc_id: doc_id.to_string(),
            quote: quote.to_string(),
        });
        if boxes.len() > DRAFT_BOXES_MAX {
            bail!("generation format fault: more than {DRAFT_BOXES_MAX} boxes");
        }
    }
    if boxes.is_empty() {
        bail!("generation format fault: no boxes parsed");
    }
    Ok(boxes)
}

/// SHOWCASE-III: the box-contract prompt. Same discipline as the claim prompt
/// (quote only the provided snippets, verified later against the full body),
/// re-shaped to drafting a staged plan from a project name + goal.
fn build_box_prompt(title: &str, goal: &str, context: &[ContextDoc]) -> String {
    let mut prompt = String::from(
        "You draft a short, staged workflow to start a project, grounded ONLY \
         in the documents below. Output EXACTLY 3 boxes, then stop. For each \
         box output EXACTLY four lines in this format, with one blank line \
         between boxes and NOTHING else — no preamble, no numbering, no \
         epilogue:\n\
         \n\
         BOX: <a short step title, at most 80 characters>\n\
         DESC: <one or two plain sentences saying what to do>\n\
         SOURCE: <one document id, the id alone — nothing else on the line>\n\
         QUOTE: \"<a short passage copied character-for-character from that \
         document's text below>\"\n\
         \n\
         Format example (shape only — never copy its content):\n\
         \n\
         BOX: Review the supporting notices\n\
         DESC: Read the relevant documents before planning the first step.\n\
         SOURCE: d0000\n\
         QUOTE: \"takes effect on 1 March\"\n\
         \n\
         Rules: the QUOTE must be an exact verbatim substring of the chosen \
         document's text as printed below — copy it exactly, never paraphrase, \
         prefer 5 to 20 words, one line. Each SOURCE line carries EXACTLY ONE \
         id (like d0123) chosen from the ids below — never two ids, never a \
         title, never any other word, NEVER empty. Every box must carry a \
         non-empty SOURCE and a non-empty QUOTE — a box you cannot support \
         with a document must not be written at all. Never use square \
         brackets in the BOX or DESC lines. After the third box, output the \
         single word END on its own line and stop.\n\n",
    );
    for doc in context {
        prompt.push_str(&format!(
            "[{}] {}\n{}\n\n",
            doc.doc_id, doc.title, doc.snippet
        ));
    }
    prompt.push_str(&format!("Project: {title}\nGoal: {goal}\nBoxes:"));
    prompt
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

    fn generate_boxes(
        &self,
        title: &str,
        goal: &str,
        context: &[ContextDoc],
        timeout: Duration,
    ) -> Result<GenerationOutcome> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": build_box_prompt(title, goal, context) }],
            "stream": false,
            // The prompt asks for at most 5 boxes and an END sentinel; the
            // stop sequence terminates the completion cleanly there (a small
            // model at temperature 0 otherwise keeps emitting boxes until the
            // token cap cuts one mid-block — a strict-parse fault). If the
            // model never emits END, the cap still cuts it and the fault
            // stays honest. 1024 tokens is headroom for 5 full boxes.
            // Both sentinel spellings stop the run: the clean "END" line and
            // the template-locked "BOX: END" a small model sometimes writes.
            "options": { "temperature": 0, "seed": 7, "num_predict": 1024, "stop": ["\nEND", "\nBOX: END"] }
        });
        let response = self.client.post_json("/api/chat", &body, timeout)?;
        let text = response
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .context("unexpected generator response shape")?
            .to_string();
        Ok(GenerationOutcome { text, usage: None })
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

fn box_block(title: &str, desc: &str, doc_id: &str, quote: &str) -> String {
    format!("BOX: {title}\nDESC: {desc}\nSOURCE: {doc_id}\nQUOTE: \"{quote}\"")
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

    fn generate_boxes(
        &self,
        title: &str,
        goal: &str,
        context: &[ContextDoc],
        _timeout: Duration,
    ) -> Result<GenerationOutcome> {
        self.captured
            .lock()
            .expect("mock generator mutex")
            .push((format!("{title}\n{goal}"), context.to_vec()));
        let text = match &self.behavior {
            MockBehavior::CiteFirst => {
                let first = context.first().context("empty context")?;
                box_block(
                    "Review the grounding source",
                    &format!("Read {} before starting.", first.doc_id),
                    &first.doc_id,
                    &groundable_quote(first),
                )
            }
            MockBehavior::CiteEach => context
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    box_block(
                        &format!("Step {} grounded in {}", i + 1, d.doc_id),
                        &format!("Act on the guidance in {}.", d.doc_id),
                        &d.doc_id,
                        &groundable_quote(d),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            MockBehavior::ForeignCitation => box_block(
                "Step from nowhere",
                "This box cites a document not in scope.",
                "d9999_foreign",
                "nowhere at all",
            ),
            MockBehavior::FabricatedQuote => {
                let first = context.first().context("empty context")?;
                box_block(
                    "Misquoted step",
                    "This box quotes text found in no document.",
                    &first.doc_id,
                    "this exact sentence appears in no corpus document whatsoever",
                )
            }
            MockBehavior::WrongSourceRealQuote => {
                let first = context.first().context("empty context")?;
                box_block(
                    "Real quote, wrong source",
                    "The quote is real but pinned on a document outside scope.",
                    "d9999_foreign",
                    &groundable_quote(first),
                )
            }
            MockBehavior::Mixed => {
                let mut blocks: Vec<String> = context
                    .iter()
                    .take(2)
                    .enumerate()
                    .map(|(i, d)| {
                        box_block(
                            &format!("Step {} grounded in {}", i + 1, d.doc_id),
                            &format!("Act on the guidance in {}.", d.doc_id),
                            &d.doc_id,
                            &groundable_quote(d),
                        )
                    })
                    .collect();
                let first = context.first().context("empty context")?;
                blocks.push(box_block(
                    "Misquoted step",
                    "This box quotes text found in no document.",
                    &first.doc_id,
                    "this exact sentence appears in no corpus document whatsoever",
                ));
                blocks.push(box_block(
                    "Step from nowhere",
                    "This box cites a document not in scope.",
                    "d9999_foreign",
                    "nowhere at all",
                ));
                blocks.join("\n\n")
            }
            MockBehavior::Uncited => "A confident plan with no box blocks at all.".to_string(),
            MockBehavior::Raw(output) => output.clone(),
            MockBehavior::Fail => bail!("mock generator unreachable"),
        };
        Ok(GenerationOutcome { text, usage: None })
    }
}
