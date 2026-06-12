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

/// Deterministic prompt: instructions, the sealed documents, the question.
fn build_prompt(query: &str, context: &[ContextDoc]) -> String {
    let mut prompt = String::from(
        "Answer the question using ONLY the documents below. After every \
         claim, cite the supporting document id in square brackets, e.g. \
         [d0123]. Use only the ids listed below. If the documents do not \
         answer the question, say so in one sentence and cite the closest \
         relevant document. Keep the answer under four sentences.\n\n",
    );
    for doc in context {
        prompt.push_str(&format!(
            "[{}] {}\n{}\n\n",
            doc.doc_id, doc.title, doc.snippet
        ));
    }
    prompt.push_str(&format!("Question: {query}\nAnswer:"));
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
            "options": { "temperature": 0, "seed": 7, "num_predict": 220 }
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
    /// Cites the first context document.
    CiteFirst,
    /// Cites every context document, in order.
    CiteEach,
    /// Cites a document id it was never given (must be refused).
    ForeignCitation,
    /// Produces a confident answer with no citations (must be refused).
    Uncited,
    /// Fails, exercising the generator-unreachable degradation branch.
    Fail,
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
                format!("Deterministic mock answer grounded in [{}].", first.doc_id)
            }
            MockBehavior::CiteEach => {
                let cites: Vec<String> =
                    context.iter().map(|d| format!("[{}]", d.doc_id)).collect();
                format!("Deterministic mock answer citing {}.", cites.join(" "))
            }
            MockBehavior::ForeignCitation => {
                "Confident claim from nowhere [d9999_foreign].".to_string()
            }
            MockBehavior::Uncited => "Confident claim with no citation at all.".to_string(),
            MockBehavior::Fail => bail!("mock generator unreachable"),
        };
        Ok(GenerationOutcome { text, usage: None })
    }
}
