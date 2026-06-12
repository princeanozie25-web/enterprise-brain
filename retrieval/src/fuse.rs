//! Reciprocal rank fusion over N rank sources.
//!
//! M2a feeds exactly one source (allowlist-filtered BM25), but the interface
//! is the seam where M2b's vector search and judge will plug in unchanged:
//! anything that can produce a deterministic in-scope ranking is a
//! [`RankSource`].
//!
//! Determinism: RRF score is `Σ 1/(K + rank)` with K = 60 over sources in
//! their given order; ties break by document id ascending. Sources must only
//! ever rank in-scope documents — fusion neither filters nor un-filters.

/// The RRF k-constant fixed by the milestone.
pub const RRF_K: f64 = 60.0;

/// A ranked list of in-scope document ids, best first.
pub trait RankSource {
    /// Stable identifier for instrumentation (e.g. "bm25").
    fn source_id(&self) -> &str;

    /// Document ids, best first, deterministic, no duplicates. Every id MUST
    /// already be inside the querying principal's compiled allowlist; fusion
    /// trusts its sources and the governance harness (R-1) verifies them.
    fn ranking(&self) -> &[String];
}

/// Fuses N sources with reciprocal rank fusion. Output is ordered by fused
/// score descending, then document id ascending, and contains each document
/// id exactly once.
pub fn fuse(sources: &[&dyn RankSource]) -> Vec<String> {
    let mut scores: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for source in sources {
        for (rank0, doc_id) in source.ranking().iter().enumerate() {
            *scores.entry(doc_id.clone()).or_insert(0.0) += 1.0 / (RRF_K + (rank0 + 1) as f64);
        }
    }
    let mut fused: Vec<(String, f64)> = scores.into_iter().collect();
    fused.sort_by(|a, b| {
        b.1.total_cmp(&a.1) // fused score descending
            .then_with(|| a.0.cmp(&b.0)) // then document id ascending
    });
    fused.into_iter().map(|(doc_id, _)| doc_id).collect()
}

/// The single M2a source: a BM25 ranking already merged across partitions.
pub struct Bm25Source {
    ranking: Vec<String>,
}

impl Bm25Source {
    pub fn new(ranking: Vec<String>) -> Bm25Source {
        Bm25Source { ranking }
    }
}

impl RankSource for Bm25Source {
    fn source_id(&self) -> &str {
        "bm25"
    }

    fn ranking(&self) -> &[String] {
        &self.ranking
    }
}
