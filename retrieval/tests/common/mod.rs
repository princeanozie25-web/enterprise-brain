//! Shared plumbing for the hybrid governance harness and the embedding
//! fixture generator. Tests are fully offline: embeddings come from the
//! committed fixture files, whose provenance is the deterministic
//! hash-projection below (NOT a model) — regenerate with
//! `cargo test -p retrieval --test fixture_gen -- --ignored`.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Synthetic fixture embedding model identity. The dimension is fixture-own
/// (256); real nomic-embed-text vectors are 768 and never appear in tests.
pub const FIXTURE_MODEL_ID: &str = "fixture-synthetic-256-v1";
pub const FIXTURE_DIM: u32 = 256;

/// The 40-document subset corpus the hybrid harness runs on: every
/// sensitivity class, two supersedes pairs, the manager-overreach HR
/// records, cross-site docs, all four in-subset mosaic pairs, and the
/// confused-deputy resources for both trapped agents.
pub const SUBSET_DOC_IDS: [&str; 40] = [
    "d0001", "d0002", "d0003", "d0004", // sop supersede pairs (internal)
    "d0091", "d0092", "d0093", // hr_records / manager-overreach traps (special)
    "d0121", "d0122", "d0123", "d0124", "d0125", "d0126", // board minutes (restricted)
    "d0133", "d0138", // cross-site customer accounts (confidential)
    "d0134", "d0139", // plain customer accounts (confidential)
    "d0193", "d0194", "d0195", "d0196", // mosaic pairs (finance)
    "d0199", "d0200", "d0205", "d0206", // mosaic pairs / confused-deputy resources
    "d0213", "d0214", "d0215", "d0216", "d0217", "d0218", // public corpus
    "d0273", "d0274", "d0275", "d0276", "d0277", // wiki pages (internal)
    "d0353", "d0354", // mail threads (internal)
    "d0513", "d0517", // band-min gated docs (internal)
];

/// The 12 committed query texts. All are lowercase simple words, so each
/// text IS its own normalized form (the engine embeds normalized queries).
pub const QUERY_TEXTS: [&str; 12] = [
    "temperature range storage procedure",
    "humidity monitoring warehouse",
    "payroll salary review",
    "board minutes strategy investment",
    "customer account credit terms",
    "hr record employment band",
    "cold chain transit hours",
    "quality compliance deviation",
    "site stock value report",
    "wiki onboarding it systems",
    "retention days records schedule",
    "goods despatch picking note",
];

pub fn repo_fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("retrieval crate sits in the repo root")
        .join("fixtures")
}

pub fn embedding_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

pub fn embedding_fixture_paths() -> (PathBuf, PathBuf) {
    let dir = embedding_fixture_dir();
    (
        dir.join("embeddings_docs.json"),
        dir.join("embeddings_queries.json"),
    )
}

/// Deterministic hash-projection embedding: each token adds ±1 into four
/// sha256-chosen buckets; the vector is L2-normalized and quantized to three
/// decimals. Token overlap produces cosine structure, which is everything a
/// governance harness needs from semantics.
pub fn synthetic_embedding(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; FIXTURE_DIM as usize];
    for token in retrieval::index::tokenize(text) {
        let h = Sha256::digest(token.as_bytes());
        for k in 0..4usize {
            let bucket = u32::from_le_bytes([h[4 * k], h[4 * k + 1], h[4 * k + 2], h[4 * k + 3]])
                as usize
                % v.len();
            let sign = if h[16 + k] & 1 == 0 { 1.0 } else { -1.0 };
            v[bucket] += sign;
        }
    }
    let norm = v
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x = ((*x as f64 / norm * 1000.0).round() / 1000.0) as f32;
        }
    }
    v
}

/// Writes the subset corpus fixtures into `dest`: company.json verbatim,
/// documents.json filtered to [`SUBSET_DOC_IDS`], traps.json filtered to
/// rows whose documents all live in the subset. The M1 compiler then
/// produces real allowlists for all 124 principals over this corpus.
pub fn write_subset_fixtures(dest: &Path) {
    let src = repo_fixtures_dir();
    fs::copy(src.join("company.json"), dest.join("company.json")).expect("copy company.json");

    let subset: BTreeSet<&str> = SUBSET_DOC_IDS.into_iter().collect();
    let full: Value =
        serde_json::from_slice(&fs::read(src.join("documents.json")).expect("read documents"))
            .expect("parse documents");
    let docs: Vec<Value> = full["documents"]
        .as_array()
        .expect("documents array")
        .iter()
        .filter(|d| subset.contains(d["id"].as_str().expect("doc id")))
        .cloned()
        .collect();
    assert_eq!(docs.len(), SUBSET_DOC_IDS.len(), "every subset doc exists");
    let documents = serde_json::json!({ "documents": docs });
    fs::write(
        dest.join("documents.json"),
        retrieval::index::canonical_json_bytes(&documents).expect("encode documents"),
    )
    .expect("write documents.json");

    let traps: Value =
        serde_json::from_slice(&fs::read(src.join("traps.json")).expect("read traps"))
            .expect("parse traps");
    let keep = |row: &Value, doc_keys: &[&str]| -> bool {
        doc_keys
            .iter()
            .all(|k| subset.contains(row[k].as_str().expect("trap doc ref")))
    };
    let filter = |family: &str, doc_keys: &[&str]| -> Vec<Value> {
        traps[family]
            .as_array()
            .expect("trap family array")
            .iter()
            .filter(|row| keep(row, doc_keys))
            .cloned()
            .collect()
    };
    let subset_traps = serde_json::json!({
        "confused_deputy": filter("confused_deputy", &["resource_id"]),
        "cross_site": filter("cross_site", &["resource_id"]),
        "effective_version": filter("effective_version", &["current_id", "superseded_id"]),
        "manager_overreach": filter("manager_overreach", &["resource_id"]),
        "mosaic": filter("mosaic", &["doc_a", "doc_b"]),
    });
    fs::write(
        dest.join("traps.json"),
        retrieval::index::canonical_json_bytes(&subset_traps).expect("encode traps"),
    )
    .expect("write traps.json");
}
