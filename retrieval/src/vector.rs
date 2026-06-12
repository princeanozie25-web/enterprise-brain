//! Per-partition vector stores: exact brute-force cosine, no ANN dependency.
//! (TurboVec-class libraries arrive later behind the same `EmbeddingSource`
//! / rank-source seams, per design docs.)
//!
//! Vectors are computed at index time — an index that silently lacks vectors
//! is a lie, so an embedder failure during the build FAILS the build. Each
//! partition's vectors live in `vectors_<class>.json` beside it, carrying
//! per-document title + a deterministic body snippet (the ONLY text the
//! judge can ever see) + the raw embedding. The embedding model id, the
//! dimension, and the sha256 of every vector file land in the manifest and
//! are therefore hashed into `index_version` (R-12).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::embed::EmbeddingSource;
use crate::index::{
    canonical_json_bytes, compute_index_version, load_manifest, sha256_hex, Class, Manifest,
    VectorsManifest,
};

/// Texts are embedded as `title\nbody` — one deterministic surface for both
/// index-time corpus embedding and the committed fixture generator.
pub fn embedding_text(title: &str, body: &str) -> String {
    format!("{title}\n{body}")
}

/// Deterministic judge snippet: the first `chars` characters of the body.
pub fn snippet_of(body: &str, chars: usize) -> String {
    body.chars().take(chars).collect()
}

/// Embedding batch size for index builds (each batch gets the configured
/// per-batch timeout). Sized so a CPU-only local embedder clears the
/// 5000ms/batch budget with headroom.
const INDEX_EMBED_BATCH: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorEntry {
    pub doc_id: String,
    pub snippet: String,
    pub title: String,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorFile {
    dim: u32,
    entries: Vec<VectorEntry>,
    model_id: String,
}

/// One partition's vectors, loaded and verified.
pub struct PartitionVectors {
    entries: Vec<VectorEntry>,
    by_id: BTreeMap<String, usize>,
}

impl PartitionVectors {
    pub fn get(&self, doc_id: &str) -> Option<&VectorEntry> {
        self.by_id.get(doc_id).map(|&i| &self.entries[i])
    }
}

/// The verified vector half of an opened index.
pub struct VectorIndex {
    pub model_id: String,
    pub dim: u32,
    pub partitions: BTreeMap<String, PartitionVectors>,
}

fn vector_file_name(class: Class) -> String {
    format!("vectors_{}.json", class.as_str())
}

/// The slice of documents.json the vector builder needs (title/body for
/// embedding text and snippets).
#[derive(Debug, Deserialize)]
struct DocumentsFile {
    documents: Vec<DocRecord>,
}

#[derive(Debug, Deserialize)]
struct DocRecord {
    id: String,
    title: String,
    body: String,
}

/// Embeds the whole corpus and writes per-partition vector files, then
/// updates `manifest.json` (vectors section + recomputed `index_version`).
/// ANY embedder failure fails the build — nonzero, no partial vectors.
pub fn build_vectors(
    fixtures_dir: &Path,
    idx_dir: &Path,
    embedder: &dyn EmbeddingSource,
    batch_timeout: Duration,
    snippet_chars: usize,
) -> Result<Manifest> {
    let mut manifest = load_manifest(idx_dir)?;
    if manifest.vectors.is_some() {
        bail!("index already carries vectors; refusing to overwrite");
    }

    let documents_path = fixtures_dir.join("documents.json");
    let bytes = fs::read(&documents_path)
        .with_context(|| format!("cannot read fixture {}", documents_path.display()))?;
    if sha256_hex(&bytes) != manifest.documents_sha256 {
        bail!("documents.json does not match the corpus this index was built from; refusing");
    }
    let parsed: DocumentsFile = serde_json::from_slice(&bytes)
        .with_context(|| format!("fixture {} fails schema/parse", documents_path.display()))?;
    let mut docs: BTreeMap<&str, &DocRecord> = BTreeMap::new();
    for doc in &parsed.documents {
        docs.insert(&doc.id, doc);
    }

    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for class in Class::ALL {
        let doc_ids = &manifest.partitions[class.as_str()].doc_ids;
        let mut entries: Vec<VectorEntry> = Vec::with_capacity(doc_ids.len());
        for chunk in doc_ids.chunks(INDEX_EMBED_BATCH.max(1)) {
            let texts: Vec<String> = chunk
                .iter()
                .map(|id| {
                    let doc = docs
                        .get(id.as_str())
                        .context("manifest names a document the corpus does not contain")?;
                    Ok(embedding_text(&doc.title, &doc.body))
                })
                .collect::<Result<_>>()?;
            let outcome = embedder
                .embed_batch(&texts, batch_timeout)
                .context("index-time embedding failed; an index missing vectors is a lie")?;
            if outcome.vectors.len() != chunk.len() {
                bail!("embedder returned a partial batch; refusing");
            }
            for (id, vector) in chunk.iter().zip(outcome.vectors) {
                if vector.len() != embedder.dim() as usize {
                    bail!("embedder returned a vector of the wrong dimension; refusing");
                }
                let doc = docs[id.as_str()];
                entries.push(VectorEntry {
                    doc_id: id.clone(),
                    snippet: snippet_of(&doc.body, snippet_chars),
                    title: doc.title.clone(),
                    vector,
                });
            }
        }
        let file = VectorFile {
            dim: embedder.dim(),
            entries,
            model_id: embedder.model_id().to_string(),
        };
        let file_bytes = canonical_json_bytes(&file)?;
        let path = idx_dir.join(vector_file_name(class));
        fs::write(&path, &file_bytes)
            .with_context(|| format!("cannot write {}", path.display()))?;
        files.insert(class.as_str().to_string(), sha256_hex(&file_bytes));
    }

    manifest.vectors = Some(VectorsManifest {
        dim: embedder.dim(),
        files,
        model_id: embedder.model_id().to_string(),
    });
    manifest.index_version = String::new();
    manifest.index_version = compute_index_version(&manifest)?;
    let manifest_path = idx_dir.join("manifest.json");
    fs::write(&manifest_path, canonical_json_bytes(&manifest)?)
        .with_context(|| format!("cannot write {}", manifest_path.display()))?;
    Ok(manifest)
}

/// Loads and verifies the vector files named by the manifest: byte hashes
/// must match the manifest (which `index_version` already pins), ids must be
/// exactly the partition's doc ids, and every vector must have the declared
/// dimension. Any mismatch refuses — stale vectors must not rank.
pub fn load_vector_index(idx_dir: &Path, manifest: &Manifest) -> Result<Option<VectorIndex>> {
    let Some(vectors_manifest) = &manifest.vectors else {
        return Ok(None);
    };
    let mut partitions = BTreeMap::new();
    for class in Class::ALL {
        let path = idx_dir.join(vector_file_name(class));
        let bytes = fs::read(&path)
            .with_context(|| format!("cannot read vector file {}", path.display()))?;
        let expected = &vectors_manifest.files[class.as_str()];
        if &sha256_hex(&bytes) != expected {
            bail!(
                "vector file {} does not match the manifest hash; refusing",
                path.display()
            );
        }
        let file: VectorFile = serde_json::from_slice(&bytes)
            .with_context(|| format!("vector file {} fails parse", path.display()))?;
        if file.model_id != vectors_manifest.model_id || file.dim != vectors_manifest.dim {
            bail!("vector file disagrees with the manifest model/dim; refusing");
        }
        let manifest_ids = &manifest.partitions[class.as_str()].doc_ids;
        if file.entries.len() != manifest_ids.len()
            || file
                .entries
                .iter()
                .zip(manifest_ids)
                .any(|(e, id)| &e.doc_id != id)
        {
            bail!("vector file does not cover exactly its partition's documents; refusing");
        }
        let mut by_id = BTreeMap::new();
        for (i, entry) in file.entries.iter().enumerate() {
            if entry.vector.len() != vectors_manifest.dim as usize {
                bail!("vector entry has the wrong dimension; refusing");
            }
            by_id.insert(entry.doc_id.clone(), i);
        }
        partitions.insert(
            class.as_str().to_string(),
            PartitionVectors {
                entries: file.entries,
                by_id,
            },
        );
    }
    Ok(Some(VectorIndex {
        model_id: vectors_manifest.model_id.clone(),
        dim: vectors_manifest.dim,
        partitions,
    }))
}

/// Exact cosine similarity with f64 accumulation (deterministic: fixed
/// iteration order, no SIMD reordering). Zero-norm inputs score 0.
pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (*x as f64, *y as f64);
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a.sqrt() * norm_b.sqrt())
}
