//! Embedding sources behind one trait, so the vector store neither knows nor
//! cares whether vectors came from a local model server or committed test
//! fixtures. The trait is also where metering surfaces: implementations
//! report exact token counts when their API does, and `None` otherwise (the
//! caller then estimates bytes/4 and marks the row estimated).

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::json;

use crate::index::sha256_hex;
use crate::local_llm::{LocalLlmClient, TokenCounts};

pub struct EmbedOutcome {
    pub vectors: Vec<Vec<f32>>,
    /// Exact API token counts when reported; None means "estimate me".
    pub usage: Option<TokenCounts>,
}

pub trait EmbeddingSource {
    fn model_id(&self) -> &str;
    fn dim(&self) -> u32;
    /// Embeds a batch of texts within `timeout`. Implementations make at
    /// most one attempt (no retries) and must return exactly one vector of
    /// `dim()` per input text or an error — never a partial batch.
    fn embed_batch(&self, texts: &[String], timeout: Duration) -> Result<EmbedOutcome>;
}

// ---------------------------------------------------------------------------
// Ollama (via the loopback carve-out)
// ---------------------------------------------------------------------------

pub struct OllamaEmbeddings {
    client: LocalLlmClient,
    model: String,
    dim: u32,
}

impl OllamaEmbeddings {
    pub fn new(client: LocalLlmClient, model: &str, dim: u32) -> OllamaEmbeddings {
        OllamaEmbeddings {
            client,
            model: model.to_string(),
            dim,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
}

impl EmbeddingSource for OllamaEmbeddings {
    fn model_id(&self) -> &str {
        &self.model
    }

    fn dim(&self) -> u32 {
        self.dim
    }

    fn embed_batch(&self, texts: &[String], timeout: Duration) -> Result<EmbedOutcome> {
        let body = json!({ "model": self.model, "input": texts });
        let response = self.client.post_json("/api/embed", &body, timeout)?;
        let parsed: OllamaEmbedResponse =
            serde_json::from_value(response).context("unexpected embed response shape")?;
        if parsed.embeddings.len() != texts.len() {
            bail!(
                "embedder returned {} vectors for {} texts; refusing partial batch",
                parsed.embeddings.len(),
                texts.len()
            );
        }
        for vector in &parsed.embeddings {
            if vector.len() != self.dim as usize {
                bail!(
                    "embedder returned dimension {} but config promises {}; refusing",
                    vector.len(),
                    self.dim
                );
            }
        }
        Ok(EmbedOutcome {
            vectors: parsed.embeddings,
            usage: parsed.prompt_eval_count.map(|input_tokens| TokenCounts {
                input_tokens,
                output_tokens: 0,
            }),
        })
    }
}

// ---------------------------------------------------------------------------
// Committed fixture vectors (tests are fully offline)
// ---------------------------------------------------------------------------

/// Reads committed fixture vectors, keyed by sha256 of the exact text. A
/// text with no committed vector is an error — which is precisely how the
/// offline harness exercises the embedder-failure degradation branches.
pub struct FileEmbeddings {
    model_id: String,
    dim: u32,
    by_text_sha: HashMap<String, Vec<f32>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmbeddingFixtureFile {
    dim: u32,
    model_id: String,
    texts: HashMap<String, EmbeddingFixtureEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmbeddingFixtureEntry {
    /// Human orientation only (first chars of the source text).
    #[allow(dead_code)]
    hint: String,
    vector: Vec<f32>,
}

impl FileEmbeddings {
    pub fn load(paths: &[&Path]) -> Result<FileEmbeddings> {
        let mut model_id: Option<String> = None;
        let mut dim: Option<u32> = None;
        let mut by_text_sha = HashMap::new();
        for path in paths {
            let bytes = std::fs::read(path)
                .with_context(|| format!("cannot read embedding fixture {}", path.display()))?;
            let file: EmbeddingFixtureFile = serde_json::from_slice(&bytes)
                .with_context(|| format!("embedding fixture {} fails parse", path.display()))?;
            if *model_id.get_or_insert_with(|| file.model_id.clone()) != file.model_id {
                bail!("embedding fixtures disagree on model_id; refusing");
            }
            if *dim.get_or_insert(file.dim) != file.dim {
                bail!("embedding fixtures disagree on dim; refusing");
            }
            for (sha, entry) in file.texts {
                if entry.vector.len() != file.dim as usize {
                    bail!("embedding fixture vector for {sha} has the wrong dimension");
                }
                by_text_sha.insert(sha, entry.vector);
            }
        }
        Ok(FileEmbeddings {
            model_id: model_id.context("no embedding fixture files given")?,
            dim: dim.context("no embedding fixture files given")?,
            by_text_sha,
        })
    }
}

impl EmbeddingSource for FileEmbeddings {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dim(&self) -> u32 {
        self.dim
    }

    fn embed_batch(&self, texts: &[String], _timeout: Duration) -> Result<EmbedOutcome> {
        let mut vectors = Vec::with_capacity(texts.len());
        for text in texts {
            let sha = sha256_hex(text.as_bytes());
            let vector = self
                .by_text_sha
                .get(&sha)
                .context("no committed embedding for the given text")?;
            vectors.push(vector.clone());
        }
        Ok(EmbedOutcome {
            vectors,
            usage: None,
        })
    }
}
