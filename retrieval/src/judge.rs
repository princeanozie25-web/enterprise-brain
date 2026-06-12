//! The optional final-ordering judge.
//!
//! A judge receives ONLY (id, title, snippet) triples for documents already
//! inside the querying principal's allowlist — the snippets are the 240-char
//! extracts baked into the vector store at index time, so there is nothing
//! longer it could be handed. It returns AN ORDER over those ids and nothing
//! else survives it: no judge text ever reaches the envelope. Ids it emits
//! that it was not given are discarded by the search layer and counted as
//! judge faults (the count lives in the instrumentation trace, never in the
//! envelope).

use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::local_llm::{LocalLlmClient, TokenCounts};

/// What the judge is allowed to see about one candidate. The type is the
/// seal: there is no field for scope statements, other principals, counts,
/// or anything beyond the candidate's own surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgeCandidate {
    pub doc_id: String,
    pub title: String,
    pub snippet: String,
}

pub struct JudgeOutcome {
    /// Candidate ids, best first, as the judge claims. May be partial or
    /// contain foreign ids — the search layer filters and completes it.
    pub order: Vec<String>,
    pub usage: Option<TokenCounts>,
}

pub trait Judge {
    fn model_id(&self) -> &str;
    /// Orders `candidates` for `query` within `timeout`. One attempt, no
    /// retries. An error means the fused order stands (degradation, not
    /// failure).
    fn order(
        &self,
        query: &str,
        candidates: &[JudgeCandidate],
        timeout: Duration,
    ) -> Result<JudgeOutcome>;
}

/// The deterministic elision rule: the judge runs only when there are at
/// least `min_candidates` fused candidates AND the fused-score ratio
/// top1/top2 is strictly below `max_ratio` (a clear winner needs no judge).
pub fn judge_eligible(fused_scores_desc: &[f64], min_candidates: usize, max_ratio: f64) -> bool {
    if fused_scores_desc.len() < min_candidates.max(2) {
        return false;
    }
    let top1 = fused_scores_desc[0];
    let top2 = fused_scores_desc[1];
    if top2 <= 0.0 {
        return false;
    }
    (top1 / top2) < max_ratio
}

// ---------------------------------------------------------------------------
// Ollama judge (via the loopback carve-out)
// ---------------------------------------------------------------------------

pub struct OllamaJudge {
    client: LocalLlmClient,
    model: String,
}

impl OllamaJudge {
    pub fn new(client: LocalLlmClient, model: &str) -> OllamaJudge {
        OllamaJudge {
            client,
            model: model.to_string(),
        }
    }
}

/// Deterministic prompt: the query, then each candidate as id / title /
/// snippet, then an instruction to answer with a JSON array of ids only.
fn build_prompt(query: &str, candidates: &[JudgeCandidate]) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "You rank search results. Order the candidate documents from most to \
         least relevant to the query. Answer with ONLY a JSON array of the \
         candidate ids, best first. Use every id exactly once; add nothing.\n\n",
    );
    prompt.push_str(&format!("Query: {query}\n\nCandidates:\n"));
    for candidate in candidates {
        prompt.push_str(&format!(
            "- id: {}\n  title: {}\n  snippet: {}\n",
            candidate.doc_id, candidate.title, candidate.snippet
        ));
    }
    prompt
}

/// Extracts an id ordering from the model's reply: the first JSON string
/// array if one parses, else candidate ids by first occurrence in the text.
fn parse_order(content: &str, candidates: &[JudgeCandidate]) -> Vec<String> {
    if let (Some(start), Some(end)) = (content.find('['), content.rfind(']')) {
        if start < end {
            if let Ok(ids) = serde_json::from_str::<Vec<String>>(&content[start..=end]) {
                return ids;
            }
        }
    }
    let mut seen: Vec<(usize, String)> = candidates
        .iter()
        .filter_map(|c| content.find(&c.doc_id).map(|pos| (pos, c.doc_id.clone())))
        .collect();
    seen.sort();
    seen.into_iter().map(|(_, id)| id).collect()
}

impl Judge for OllamaJudge {
    fn model_id(&self) -> &str {
        &self.model
    }

    fn order(
        &self,
        query: &str,
        candidates: &[JudgeCandidate],
        timeout: Duration,
    ) -> Result<JudgeOutcome> {
        let body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": build_prompt(query, candidates) }],
            "stream": false,
            "options": { "temperature": 0, "seed": 7 }
        });
        let response = self.client.post_json("/api/chat", &body, timeout)?;
        let content = response
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .context("unexpected judge response shape")?;
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
        Ok(JudgeOutcome {
            order: parse_order(content, candidates),
            usage,
        })
    }
}

// ---------------------------------------------------------------------------
// Mock judge (tests are fully offline)
// ---------------------------------------------------------------------------

pub enum MockBehavior {
    /// Returns the ids in the order given.
    Identity,
    /// Returns the ids reversed.
    Reverse,
    /// Returns exactly this output, whatever it is (foreign ids included).
    Fixed(Vec<String>),
    /// Fails, exercising the judge-unreachable degradation branch.
    Fail,
}

/// Captures every input it is shown, so R-9 can assert the judge input seal.
pub struct MockJudge {
    pub behavior: MockBehavior,
    pub captured: Mutex<Vec<(String, Vec<JudgeCandidate>)>>,
}

impl MockJudge {
    pub fn new(behavior: MockBehavior) -> MockJudge {
        MockJudge {
            behavior,
            captured: Mutex::new(Vec::new()),
        }
    }
}

impl Judge for MockJudge {
    fn model_id(&self) -> &str {
        "mock-judge"
    }

    fn order(
        &self,
        query: &str,
        candidates: &[JudgeCandidate],
        _timeout: Duration,
    ) -> Result<JudgeOutcome> {
        self.captured
            .lock()
            .expect("mock judge mutex")
            .push((query.to_string(), candidates.to_vec()));
        let order = match &self.behavior {
            MockBehavior::Identity => candidates.iter().map(|c| c.doc_id.clone()).collect(),
            MockBehavior::Reverse => candidates.iter().rev().map(|c| c.doc_id.clone()).collect(),
            MockBehavior::Fixed(ids) => ids.clone(),
            MockBehavior::Fail => bail!("mock judge unreachable"),
        };
        Ok(JudgeOutcome { order, usage: None })
    }
}
