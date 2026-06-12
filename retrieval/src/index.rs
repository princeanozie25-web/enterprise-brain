//! Per-sensitivity-class index partitions.
//!
//! The corpus is split into exactly five tantivy indexes, one per sensitivity
//! class, so a query only ever opens the partitions its principal can touch
//! (partition discipline, R-5). A canonical partition manifest — per-partition
//! sorted doc ids plus content hashes and the documents.json hash — is hashed
//! into `index_version`. Tantivy's on-disk bytes contain generated segment
//! ids, so the manifest, not the files, is the identity of an index build:
//! rebuilding from the same fixture bytes yields the same `index_version`
//! (R-6).
//!
//! Indexing is principal-free (all 600 documents are indexed); governance
//! happens at query time, inside the query (see `search.rs`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tantivy::schema::{Schema, STORED, STRING, TEXT};
use tantivy::{doc, Index};

/// The five sensitivity classes, in canonical partition order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    Public,
    Internal,
    Confidential,
    Restricted,
    SpecialCategory,
}

impl Class {
    pub const ALL: [Class; 5] = [
        Class::Public,
        Class::Internal,
        Class::Confidential,
        Class::Restricted,
        Class::SpecialCategory,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Class::Public => "public",
            Class::Internal => "internal",
            Class::Confidential => "confidential",
            Class::Restricted => "restricted",
            Class::SpecialCategory => "special_category",
        }
    }
}

/// The slice of /fixtures/documents.json the indexer consumes. Unknown keys
/// are tolerated (M1 already schema-validated the corpus); unknown
/// sensitivity values are not — they would have no partition to live in.
#[derive(Debug, Deserialize)]
struct DocumentsFile {
    documents: Vec<DocRecord>,
}

#[derive(Debug, Deserialize)]
struct DocRecord {
    id: String,
    title: String,
    body: String,
    sensitivity: Class,
}

/// One partition's manifest row: identity is the sorted doc-id list plus a
/// hash of the indexed content (id, title, body in id order).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PartitionManifest {
    pub content_sha256: String,
    pub doc_ids: Vec<String>,
}

/// `manifest.json` at the index root. `index_version` is the SHA-256 of this
/// file's canonical bytes with the `index_version` field itself set to "".
///
/// M2b: when vectors are built (`vector::build_vectors`), the manifest gains
/// a `vectors` section — embedding model id, dimension, and the sha256 of
/// each per-partition vector file — and `index_version` is recomputed over
/// it, so the vectors are part of the index identity. A lexical-only build
/// omits the key entirely and its manifest bytes (and `index_version`) are
/// identical to M2a's.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub documents_sha256: String,
    pub format: String,
    pub index_version: String,
    pub partitions: BTreeMap<String, PartitionManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vectors: Option<VectorsManifest>,
}

/// The vector half of the index identity (M2b).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VectorsManifest {
    pub dim: u32,
    /// partition class -> sha256 of its vector file bytes.
    pub files: BTreeMap<String, String>,
    pub model_id: String,
}

/// Bumped whenever schema/tokenizer/serving semantics change identity.
const FORMAT: &str = "m2a-bm25/1 tokenizer=default schema=doc_id,title,body";

pub const TOP_K_DEFAULT: usize = 10;
pub const TOP_K_MAX: usize = 50;

/// The tantivy schema shared by all partitions: raw stored doc id + BM25 text.
pub fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("doc_id", STRING | STORED);
    builder.add_text_field("title", TEXT);
    builder.add_text_field("body", TEXT);
    builder.build()
}

/// Mirrors tantivy's "default" analyzer (SimpleTokenizer + LowerCaser +
/// RemoveLong(40)) so query-side normalization matches what was indexed.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty() && t.chars().count() < 40)
        .map(|t| t.to_lowercase())
        .collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Canonical JSON bytes: sorted keys (via `serde_json::Value`), compact,
/// trailing newline. Identical to M1's artifact encoding.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value).context("serializing to canonical JSON")?;
    let mut bytes = serde_json::to_vec(&value).context("encoding canonical JSON")?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub(crate) fn compute_index_version(manifest: &Manifest) -> Result<String> {
    let mut unversioned = manifest.clone();
    unversioned.index_version = String::new();
    Ok(sha256_hex(&canonical_json_bytes(&unversioned)?))
}

/// Builds the five partitions under `out_dir` and writes `manifest.json`.
/// Refuses a non-empty output directory (a stale half-index must never be
/// mistaken for a fresh build) and any structural defect in the corpus.
pub fn build_index(fixtures_dir: &Path, out_dir: &Path) -> Result<Manifest> {
    let documents_path = fixtures_dir.join("documents.json");
    let bytes = fs::read(&documents_path)
        .with_context(|| format!("cannot read fixture {}", documents_path.display()))?;
    let documents_sha256 = sha256_hex(&bytes);
    let parsed: DocumentsFile = serde_json::from_slice(&bytes)
        .with_context(|| format!("fixture {} fails schema/parse", documents_path.display()))?;

    let mut by_class: BTreeMap<Class, Vec<&DocRecord>> = BTreeMap::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for doc in &parsed.documents {
        if doc.id.is_empty() || doc.title.is_empty() || doc.body.is_empty() {
            bail!("document with empty id/title/body; refusing to index");
        }
        if !seen.insert(&doc.id) {
            bail!("duplicate document id {}; refusing to index", doc.id);
        }
        by_class.entry(doc.sensitivity).or_default().push(doc);
    }

    if out_dir.exists() && fs::read_dir(out_dir)?.next().is_some() {
        bail!(
            "output directory {} is not empty; refusing to overwrite an existing index",
            out_dir.display()
        );
    }
    fs::create_dir_all(out_dir)
        .with_context(|| format!("cannot create index directory {}", out_dir.display()))?;

    let schema = build_schema();
    let doc_id_field = schema.get_field("doc_id").expect("schema field");
    let title_field = schema.get_field("title").expect("schema field");
    let body_field = schema.get_field("body").expect("schema field");

    let mut partitions: BTreeMap<String, PartitionManifest> = BTreeMap::new();
    for class in Class::ALL {
        let mut docs = by_class.remove(&class).unwrap_or_default();
        docs.sort_by(|a, b| a.id.cmp(&b.id));

        let partition_dir = out_dir.join(class.as_str());
        fs::create_dir_all(&partition_dir)?;
        let index = Index::create_in_dir(&partition_dir, schema.clone())
            .with_context(|| format!("cannot create partition {}", class.as_str()))?;
        // One writer thread + one commit: a deterministic single-segment
        // partition, so BM25 statistics never depend on thread scheduling.
        let mut writer = index
            .writer_with_num_threads(1, 50_000_000)
            .context("cannot open index writer")?;

        let mut content = Sha256::new();
        let mut doc_ids = Vec::with_capacity(docs.len());
        for doc_record in &docs {
            content.update(doc_record.id.as_bytes());
            content.update(b"\n");
            content.update(doc_record.title.as_bytes());
            content.update(b"\n");
            content.update(doc_record.body.as_bytes());
            content.update(b"\n");
            doc_ids.push(doc_record.id.clone());
            writer
                .add_document(doc!(
                    doc_id_field => doc_record.id.as_str(),
                    title_field => doc_record.title.as_str(),
                    body_field => doc_record.body.as_str(),
                ))
                .context("cannot add document to partition")?;
        }
        writer.commit().context("cannot commit partition")?;

        partitions.insert(
            class.as_str().to_string(),
            PartitionManifest {
                content_sha256: sha256_hex(&content.finalize()),
                doc_ids,
            },
        );
    }

    let mut manifest = Manifest {
        documents_sha256,
        format: FORMAT.to_string(),
        index_version: String::new(),
        partitions,
        vectors: None,
    };
    manifest.index_version = compute_index_version(&manifest)?;

    let manifest_path = out_dir.join("manifest.json");
    fs::write(&manifest_path, canonical_json_bytes(&manifest)?)
        .with_context(|| format!("cannot write {}", manifest_path.display()))?;
    Ok(manifest)
}

/// Loads and re-verifies `manifest.json`: a manifest whose recomputed hash
/// does not match its recorded `index_version` refuses (fail-closed).
pub fn load_manifest(idx_dir: &Path) -> Result<Manifest> {
    let path = idx_dir.join("manifest.json");
    let bytes =
        fs::read(&path).with_context(|| format!("cannot read manifest {}", path.display()))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .with_context(|| format!("manifest {} fails schema/parse", path.display()))?;
    if manifest.partitions.len() != Class::ALL.len()
        || !Class::ALL
            .iter()
            .all(|c| manifest.partitions.contains_key(c.as_str()))
    {
        bail!("manifest must describe exactly the five sensitivity partitions");
    }
    if let Some(vectors) = &manifest.vectors {
        if vectors.files.len() != Class::ALL.len()
            || !Class::ALL
                .iter()
                .all(|c| vectors.files.contains_key(c.as_str()))
        {
            bail!("manifest vectors must cover exactly the five sensitivity partitions");
        }
    }
    let expected = compute_index_version(&manifest)?;
    if manifest.index_version != expected {
        bail!("manifest index_version does not match its contents; refusing");
    }
    Ok(manifest)
}
