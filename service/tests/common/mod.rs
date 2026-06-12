//! Shared plumbing for the A-suite. Tests are fully offline: embeddings come
//! from committed fixtures (the SAME deterministic hash-projection as the
//! M2b retrieval fixtures, here covering ALL 600 documents — closing the
//! M2b review gap), the judge and generator are mocks, and no test opens a
//! non-loopback socket.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Must match the M2b retrieval fixtures exactly, so the service docs file
/// and the retrieval queries file merge into one `FileEmbeddings`.
pub const FIXTURE_MODEL_ID: &str = "fixture-synthetic-256-v1";
pub const FIXTURE_DIM: u32 = 256;

/// The 12 committed query texts (verbatim from the M2b fixtures; each text
/// is its own normalized form).
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
        .expect("service crate sits in the repo root")
        .join("fixtures")
}

pub fn service_fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// The full-corpus document embeddings (this crate's fixture).
pub fn docs_embeddings_path() -> PathBuf {
    service_fixture_dir().join("embeddings_docs_full.json")
}

/// The committed query embeddings (reused from the M2b retrieval fixtures).
pub fn query_embeddings_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join("retrieval")
        .join("tests")
        .join("fixtures")
        .join("embeddings_queries.json")
}

/// Identical to the M2b generator: each token adds ±1 into four
/// sha256-chosen buckets; L2-normalized, quantized to three decimals.
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

/// Deterministic LCG so the harness needs no rand dependency.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn new(seed: u64) -> Lcg {
        Lcg(seed)
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() as usize) % items.len()]
    }
}
