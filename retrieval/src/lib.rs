//! Enterprise Brain M2a — governed lexical retrieval.
//!
//! BM25 over five per-sensitivity-class tantivy partitions, operating
//! STRICTLY INSIDE the M1 compiled allowlists: the allowlist is a doc-id
//! restriction inside every tantivy query, never a post-filter. The
//! retriever is replaceable (see [`fuse::RankSource`]); the governance
//! harness in `tests/` is the product.
//!
//! Inherited invariants: deny by default, fail closed, deterministic, no
//! wall clock, no network, no LLM, no embeddings. `ground_truth.jsonl` and
//! `traps.json` are read only by tests, never by this library.

pub mod embed;
pub mod envelope;
pub mod fuse;
pub mod index;
pub mod judge;
pub mod local_llm;
pub mod search;
pub mod vector;
