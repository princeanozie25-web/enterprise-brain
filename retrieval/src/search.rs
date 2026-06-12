//! Allowlist-governed retrieval: BM25 always, vector + judge optionally.
//!
//! THE CORE RULE: no document outside the querying principal's compiled
//! allowlist may appear in any stage's output. The allowlist restriction is
//! applied INSIDE the search — BM25 takes it as a `TermSetQuery` Must clause
//! of the tantivy query, and the vector stage computes cosine ONLY over the
//! restriction set (allowlist ∩ partition) — never as a post-step on an
//! unfiltered ranking. The judge sees only allowed candidates' own snippets
//! and returns an order; nothing else of it survives.
//!
//! DEGRADATION DOCTRINE: query-time embedder failure degrades hybrid to
//! lexical_only (exit 0, envelope says so); judge failure leaves the fused
//! order standing (judge_applied=false); a model/dim mismatch between the
//! manifest and the configured embedder REFUSES the query — stale vectors
//! must not rank. Less compute is an acceptable degradation; less
//! governance never is.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, TermQuery, TermSetQuery};
use tantivy::schema::{Field, IndexRecordOption, Value};
use tantivy::{Index, IndexReader, ReloadPolicy, TantivyDocument, Term};

use crate::embed::EmbeddingSource;
use crate::envelope::{derive_scope_statement, query_hash, Envelope, ResultEntry};
use crate::fuse::{fuse, Bm25Source, RankSource, RRF_K};
use crate::index::{build_schema, load_manifest, sha256_hex, tokenize, Class, Manifest, TOP_K_MAX};
use crate::judge::{judge_eligible, Judge, JudgeCandidate};
use crate::local_llm::{estimate_tokens, UsageEvent};
use crate::vector::{cosine, load_vector_index, VectorEntry, VectorIndex};

/// Vector candidates fed into fusion per partition (NUMBERS).
const VECTOR_TOP_PER_PARTITION: usize = 50;

// ---------------------------------------------------------------------------
// M1 artifact consumption (strict mirrors; M1 source stays frozen)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct M1Artifact {
    #[allow(dead_code)]
    compiled_at: String,
    #[allow(dead_code)]
    denied_count: u64,
    entries: Vec<M1Entry>,
    principal_id: String,
    snapshot_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct M1Entry {
    document_id: String,
    #[serde(default)]
    effective_successor: Option<String>,
    /// Opaque to M2: mosaic bounds are an answer-layer concern.
    #[serde(default)]
    #[allow(dead_code)]
    mosaic_tags: Option<serde_json::Value>,
    reasons: Vec<String>,
    #[serde(default)]
    superseded: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct M1IndexFile {
    #[allow(dead_code)]
    compiled_at: String,
    fixture_hashes: BTreeMap<String, String>,
    principals: Vec<M1IndexRow>,
    snapshot_version: String,
    #[allow(dead_code)]
    totals: M1Totals,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct M1Totals {
    #[allow(dead_code)]
    allow_entries: u64,
    #[allow(dead_code)]
    documents: u64,
    #[allow(dead_code)]
    principals: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct M1IndexRow {
    artifact_file: String,
    artifact_sha256: String,
    #[allow(dead_code)]
    denied_count: u64,
    #[allow(dead_code)]
    entry_count: u64,
    principal_id: String,
    #[serde(default)]
    #[allow(dead_code)]
    unknown_principal: Option<bool>,
}

/// One allowlist entry's serve-time metadata.
#[derive(Debug, Clone)]
struct EntryMeta {
    reasons: Vec<String>,
    superseded: bool,
    effective_successor: Option<String>,
}

/// A principal's verified, compiled scope: the only authority any query
/// consults. Loading fails closed — a missing artifact, a hash mismatch, or
/// a snapshot disagreement refuses instead of guessing.
pub struct PrincipalScope {
    pub principal_id: String,
    pub snapshot_version: String,
    /// sha256 of the documents.json the allowlists were compiled from,
    /// cross-checked against the index manifest at query time.
    documents_sha256: String,
    entries: BTreeMap<String, EntryMeta>,
    scope_reasons: Vec<String>,
}

impl PrincipalScope {
    pub fn load(artifacts_dir: &Path, principal_id: &str) -> Result<PrincipalScope> {
        let index_path = artifacts_dir.join("index.json");
        let index_bytes = fs::read(&index_path)
            .with_context(|| format!("cannot read M1 index {}", index_path.display()))?;
        let m1_index: M1IndexFile = serde_json::from_slice(&index_bytes)
            .with_context(|| format!("M1 index {} fails schema/parse", index_path.display()))?;

        let row = m1_index
            .principals
            .iter()
            .find(|r| r.principal_id == principal_id)
            .with_context(|| {
                format!("no compiled allowlist for principal {principal_id}; denying by default")
            })?;

        let artifact_path = artifacts_dir.join(&row.artifact_file);
        let artifact_bytes = fs::read(&artifact_path)
            .with_context(|| format!("cannot read artifact {}", artifact_path.display()))?;
        if sha256_hex(&artifact_bytes) != row.artifact_sha256 {
            bail!(
                "artifact {} does not match the hash recorded in the M1 index; refusing",
                artifact_path.display()
            );
        }
        let artifact: M1Artifact = serde_json::from_slice(&artifact_bytes)
            .with_context(|| format!("artifact {} fails schema/parse", artifact_path.display()))?;
        if artifact.principal_id != principal_id {
            bail!("artifact principal_id does not match the requested principal; refusing");
        }
        if artifact.snapshot_version != m1_index.snapshot_version {
            bail!("artifact and M1 index pin different snapshots; refusing");
        }
        let documents_sha256 = m1_index
            .fixture_hashes
            .get("documents.json")
            .context("M1 index records no documents.json hash; refusing")?
            .clone();

        let mut entries: BTreeMap<String, EntryMeta> = BTreeMap::new();
        let mut scope_reasons: Vec<String> = Vec::new();
        for entry in artifact.entries {
            scope_reasons.extend(entry.reasons.iter().cloned());
            let meta = EntryMeta {
                reasons: entry.reasons,
                superseded: entry.superseded == Some(true),
                effective_successor: entry.effective_successor,
            };
            if entries.insert(entry.document_id.clone(), meta).is_some() {
                bail!("artifact repeats a document entry; refusing");
            }
        }

        Ok(PrincipalScope {
            principal_id: principal_id.to_string(),
            snapshot_version: artifact.snapshot_version,
            documents_sha256,
            entries,
            scope_reasons,
        })
    }

    /// Document ids the principal may see at all (the compiled allowlist).
    pub fn allowed_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

// ---------------------------------------------------------------------------
// Instrumentation
// ---------------------------------------------------------------------------

/// One instrumented stage: every document id the stage observed. The
/// governance harness (R-1/R-8) asserts each is inside the allowlist.
#[derive(Debug, Clone)]
pub struct StageObservation {
    pub stage: String,
    pub doc_ids: Vec<String>,
}

/// The full per-query instrumentation. In-memory only — never serialized,
/// never printed by the CLI. Reason strings are fixed labels, never dynamic
/// content, so no document id can ride them.
#[derive(Debug, Clone, Default)]
pub struct Trace {
    pub opened_partitions: Vec<String>,
    pub stages: Vec<StageObservation>,
    /// Fixed-label reason when hybrid degraded to lexical (no ids, ever).
    pub hybrid_degraded: Option<&'static str>,
    /// Some(true/false) when --judge was requested: the elision decision.
    pub judge_eligible: Option<bool>,
    /// Foreign ids in the judge's output, discarded by the search layer.
    pub judge_faults: u32,
    /// The judge was invoked but failed (timeout/unreachable/empty).
    pub judge_failed: bool,
    /// Metering rows for the Bursar sidecar (ts assigned at write time).
    pub usage_events: Vec<UsageEvent>,
}

impl Trace {
    fn observe(&mut self, stage: impl Into<String>, doc_ids: Vec<String>) {
        self.stages.push(StageObservation {
            stage: stage.into(),
            doc_ids,
        });
    }
}

// ---------------------------------------------------------------------------
// Search options
// ---------------------------------------------------------------------------

pub struct HybridParams<'a> {
    pub embedder: &'a dyn EmbeddingSource,
    pub query_embed_timeout: Duration,
}

pub struct JudgeParams<'a> {
    pub judge: &'a dyn Judge,
    pub timeout: Duration,
    /// Judge sees the top-K fused candidates (NUMBERS: 12).
    pub top_k: usize,
    /// Elision: judge runs only with at least this many fused candidates.
    pub min_candidates: usize,
    /// Elision: judge runs only when top1/top2 fused score < this ratio.
    pub max_ratio: f64,
}

pub struct SearchOptions<'a> {
    /// Top-k results to serve, 1..=50 (default 10).
    pub k: usize,
    /// Serve superseded documents (always marked) instead of suppressing them.
    pub include_superseded: bool,
    /// Vector RankSource (hybrid mode). None = lexical only.
    pub hybrid: Option<HybridParams<'a>>,
    /// Optional final-ordering judge on the fused top-K.
    pub judge: Option<JudgeParams<'a>>,
}

impl SearchOptions<'_> {
    /// The M2a behavior: BM25 only, no judge.
    pub fn lexical(k: usize, include_superseded: bool) -> SearchOptions<'static> {
        SearchOptions {
            k,
            include_superseded,
            hybrid: None,
            judge: None,
        }
    }
}

impl Default for SearchOptions<'static> {
    fn default() -> SearchOptions<'static> {
        SearchOptions::lexical(crate::index::TOP_K_DEFAULT, false)
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

struct Partition {
    class: Class,
    reader: IndexReader,
    doc_id_field: Field,
    title_field: Field,
    body_field: Field,
    /// Sorted doc ids this partition holds (from the manifest).
    member_ids: BTreeSet<String>,
}

/// An opened index: the five lexical partitions, their manifest, and the
/// verified vector stores when the index carries them. Read-only.
pub struct Engine {
    pub manifest: Manifest,
    partitions: Vec<Partition>,
    vectors: Option<VectorIndex>,
}

/// A second rank source list under a stable instrumentation id.
struct NamedSource {
    id: &'static str,
    ranking: Vec<String>,
}

impl RankSource for NamedSource {
    fn source_id(&self) -> &str {
        self.id
    }

    fn ranking(&self) -> &[String] {
        &self.ranking
    }
}

impl Engine {
    pub fn open(idx_dir: &Path) -> Result<Engine> {
        let manifest = load_manifest(idx_dir)?;
        let schema = build_schema();
        let doc_id_field = schema.get_field("doc_id").expect("schema field");
        let title_field = schema.get_field("title").expect("schema field");
        let body_field = schema.get_field("body").expect("schema field");

        let mut partitions = Vec::with_capacity(Class::ALL.len());
        for class in Class::ALL {
            let dir = idx_dir.join(class.as_str());
            let index = Index::open_in_dir(&dir)
                .with_context(|| format!("cannot open partition {}", class.as_str()))?;
            let reader = index
                .reader_builder()
                .reload_policy(ReloadPolicy::Manual)
                .try_into()
                .with_context(|| format!("cannot read partition {}", class.as_str()))?;
            let member_ids = manifest.partitions[class.as_str()]
                .doc_ids
                .iter()
                .cloned()
                .collect();
            partitions.push(Partition {
                class,
                reader,
                doc_id_field,
                title_field,
                body_field,
                member_ids,
            });
        }
        let vectors = load_vector_index(idx_dir, &manifest)?;
        Ok(Engine {
            manifest,
            partitions,
            vectors,
        })
    }

    fn vector_entry(&self, doc_id: &str) -> Option<&VectorEntry> {
        let vectors = self.vectors.as_ref()?;
        vectors.partitions.values().find_map(|p| p.get(doc_id))
    }

    /// Governed search. Returns the envelope (the only emitted object) and
    /// the instrumentation trace (for the governance harness + sidecar).
    pub fn search(
        &self,
        scope: &PrincipalScope,
        raw_query: &str,
        options: &SearchOptions,
    ) -> Result<(Envelope, Trace)> {
        if options.k == 0 || options.k > TOP_K_MAX {
            bail!("top-k must be between 1 and {TOP_K_MAX}");
        }
        if scope.documents_sha256 != self.manifest.documents_sha256 {
            bail!(
                "allowlists and index were built from different corpora \
                 (documents.json hash mismatch); refusing"
            );
        }
        // Hybrid preflight, BEFORE any embed attempt (fail closed, not
        // degraded): the index must carry vectors and they must match the
        // configured embedder exactly — stale vectors must not rank.
        if let Some(hybrid) = &options.hybrid {
            let vectors = self.vectors.as_ref().context(
                "index carries no vectors; rebuild with --hybrid before querying hybrid",
            )?;
            if vectors.model_id != hybrid.embedder.model_id() {
                bail!(
                    "index vectors were embedded with {:?} but the config says {:?}; refusing",
                    vectors.model_id,
                    hybrid.embedder.model_id()
                );
            }
            if vectors.dim != hybrid.embedder.dim() {
                bail!(
                    "index vectors have dimension {} but the config says {}; refusing",
                    vectors.dim,
                    hybrid.embedder.dim()
                );
            }
        }
        // The judge reads snippets baked into the vector store.
        if options.judge.is_some() && self.vectors.is_none() {
            bail!("the judge needs the index's vector store for snippets; rebuild with --hybrid");
        }

        let mut trace = Trace::default();
        let tokens = tokenize(raw_query);
        let normalized = tokens.join(" ");

        // The serveable set: the allowlist, minus superseded documents unless
        // explicitly included. Effective-version policy is part of the
        // restriction itself, so a suppressed version never reaches scoring.
        let serveable: BTreeSet<&str> = scope
            .entries
            .iter()
            .filter(|(_, meta)| options.include_superseded || !meta.superseded)
            .map(|(id, _)| id.as_str())
            .collect();
        trace.observe(
            "serveable_allowlist",
            serveable.iter().map(|s| s.to_string()).collect(),
        );

        // Partition restrictions, shared by the BM25 and vector sources.
        // Partition discipline: zero serveable docs -> never opened.
        let mut restrictions: Vec<(usize, Vec<&str>)> = Vec::new();
        if !tokens.is_empty() {
            for (i, partition) in self.partitions.iter().enumerate() {
                let restriction: Vec<&str> = partition
                    .member_ids
                    .iter()
                    .map(String::as_str)
                    .filter(|id| serveable.contains(*id))
                    .collect();
                if restriction.is_empty() {
                    continue;
                }
                trace
                    .opened_partitions
                    .push(partition.class.as_str().to_string());
                trace.observe(
                    format!("restriction:{}", partition.class.as_str()),
                    restriction.iter().map(|s| s.to_string()).collect(),
                );
                restrictions.push((i, restriction));
            }
        }

        // BM25 source, allowlist-restricted inside the tantivy query.
        let mut scored: Vec<(f32, String)> = Vec::new();
        for (i, restriction) in &restrictions {
            let partition = &self.partitions[*i];
            let hits = search_partition(partition, &tokens, restriction, options.k)?;
            trace.observe(
                format!("scored:{}", partition.class.as_str()),
                hits.iter().map(|(_, id)| id.clone()).collect(),
            );
            scored.extend(hits);
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let bm25_ranking: Vec<String> = scored.into_iter().map(|(_, id)| id).collect();
        trace.observe("bm25_merged", bm25_ranking.clone());

        // Vector source: exact cosine over the SAME restriction sets. An
        // embedder failure here degrades to lexical_only (doctrine).
        let mut vector_ranking: Vec<String> = Vec::new();
        let mut hybrid_ran = false;
        if let Some(hybrid) = &options.hybrid {
            if !tokens.is_empty() {
                let texts = vec![normalized.clone()];
                let embed_result = hybrid
                    .embedder
                    .embed_batch(&texts, hybrid.query_embed_timeout);
                let usage = embed_result.as_ref().ok().and_then(|o| o.usage);
                trace.usage_events.push(UsageEvent {
                    cost_usd: None,
                    estimated: usage.is_none(),
                    input_tokens: usage
                        .map(|u| u.input_tokens)
                        .unwrap_or_else(|| estimate_tokens(normalized.len())),
                    model: hybrid.embedder.model_id().to_string(),
                    output_tokens: usage.map(|u| u.output_tokens).unwrap_or(0),
                    ts: 0,
                });
                match embed_result {
                    Err(_) => {
                        trace.hybrid_degraded = Some("query_embed_failed");
                    }
                    Ok(outcome) => {
                        let query_vector = &outcome.vectors[0];
                        let vectors = self.vectors.as_ref().expect("preflight checked");
                        let mut vector_scored: Vec<(f64, String)> = Vec::new();
                        for (i, restriction) in &restrictions {
                            let partition = &self.partitions[*i];
                            let class = partition.class.as_str();
                            trace.observe(
                                format!("vector_candidates:{class}"),
                                restriction.iter().map(|s| s.to_string()).collect(),
                            );
                            let store = &vectors.partitions[class];
                            let mut hits: Vec<(f64, String)> = restriction
                                .iter()
                                .map(|id| {
                                    let entry = store
                                        .get(id)
                                        .expect("vector store covers its whole partition");
                                    (cosine(query_vector, &entry.vector), (*id).to_string())
                                })
                                .collect();
                            hits.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                            hits.truncate(VECTOR_TOP_PER_PARTITION);
                            trace.observe(
                                format!("vector_scored:{class}"),
                                hits.iter().map(|(_, id)| id.clone()).collect(),
                            );
                            vector_scored.extend(hits);
                        }
                        vector_scored
                            .sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
                        vector_ranking = vector_scored.into_iter().map(|(_, id)| id).collect();
                        trace.observe("vector_merged", vector_ranking.clone());
                        hybrid_ran = true;
                    }
                }
            }
        }
        let retrieval_mode = if hybrid_ran { "hybrid" } else { "lexical_only" };

        // Reciprocal rank fusion over the active sources.
        let bm25_source = Bm25Source::new(bm25_ranking);
        let vector_source = NamedSource {
            id: "vector",
            ranking: vector_ranking,
        };
        let mut sources: Vec<&dyn RankSource> = vec![&bm25_source];
        if hybrid_ran {
            sources.push(&vector_source);
        }
        let mut fused = fuse(&sources);
        trace.observe("fused", fused.clone());

        // Fused RRF scores (same formula as fuse.rs, recomputed here because
        // fuse() exposes order only), aligned to the fused order — these
        // drive the deterministic judge elision rule.
        let mut score_map: BTreeMap<&str, f64> = BTreeMap::new();
        for source in &sources {
            for (rank0, doc_id) in source.ranking().iter().enumerate() {
                *score_map.entry(doc_id.as_str()).or_insert(0.0) +=
                    1.0 / (RRF_K + (rank0 + 1) as f64);
            }
        }
        let fused_scores: Vec<f64> = fused.iter().map(|id| score_map[id.as_str()]).collect();

        // Optional judge on the fused top-K. Failure or elision leaves the
        // fused order standing; only an ORDER ever comes back.
        let mut judge_applied = false;
        if let Some(params) = &options.judge {
            let eligible = judge_eligible(&fused_scores, params.min_candidates, params.max_ratio);
            trace.judge_eligible = Some(eligible);
            if eligible {
                let top_k = params.top_k.min(fused.len());
                let head: Vec<String> = fused[..top_k].to_vec();
                let candidates: Vec<JudgeCandidate> = head
                    .iter()
                    .map(|id| {
                        let entry = self
                            .vector_entry(id)
                            .expect("judge preflight requires the vector store");
                        JudgeCandidate {
                            doc_id: entry.doc_id.clone(),
                            title: entry.title.clone(),
                            snippet: entry.snippet.clone(),
                        }
                    })
                    .collect();
                trace.observe("judge_input", head.clone());

                let outcome = params.judge.order(&normalized, &candidates, params.timeout);
                let estimate_basis: usize = normalized.len()
                    + candidates
                        .iter()
                        .map(|c| c.doc_id.len() + c.title.len() + c.snippet.len())
                        .sum::<usize>();
                let usage = outcome.as_ref().ok().and_then(|o| o.usage);
                trace.usage_events.push(UsageEvent {
                    cost_usd: None,
                    estimated: usage.is_none(),
                    input_tokens: usage
                        .map(|u| u.input_tokens)
                        .unwrap_or_else(|| estimate_tokens(estimate_basis)),
                    model: params.judge.model_id().to_string(),
                    output_tokens: usage.map(|u| u.output_tokens).unwrap_or(0),
                    ts: 0,
                });

                match outcome {
                    Err(_) => {
                        trace.judge_failed = true;
                    }
                    Ok(outcome) => {
                        let given: BTreeSet<&str> = head.iter().map(String::as_str).collect();
                        let mut taken: BTreeSet<String> = BTreeSet::new();
                        let mut reordered: Vec<String> = Vec::new();
                        for id in outcome.order {
                            if !given.contains(id.as_str()) {
                                // Foreign id: discarded, counted, NEVER traced
                                // (the trace itself must stay in-scope).
                                trace.judge_faults += 1;
                                continue;
                            }
                            if taken.insert(id.clone()) {
                                reordered.push(id);
                            }
                        }
                        if reordered.is_empty() {
                            trace.judge_failed = true;
                        } else {
                            // Complete the permutation: unmentioned ids keep
                            // their fused order after the judge's picks.
                            for id in &head {
                                if !taken.contains(id) {
                                    reordered.push(id.clone());
                                }
                            }
                            fused.splice(..top_k, reordered.clone());
                            trace.observe("judge_output", reordered);
                            judge_applied = true;
                        }
                    }
                }
            }
        }

        // Results, with the effective-version serve rule. M2b hardening
        // (deviation-4 order): under --include-superseded, a successor id is
        // emitted ONLY when the successor itself is in the allowlist — an
        // out-of-scope id never leaves through the metadata side door.
        let mut results = Vec::new();
        for (rank0, document_id) in fused.iter().take(options.k).enumerate() {
            let meta = scope
                .entries
                .get(document_id)
                .expect("ranked ids originate from the allowlist restriction");
            let (superseded, effective_successor) = if meta.superseded {
                let successor = meta
                    .effective_successor
                    .as_ref()
                    .filter(|s| scope.entries.contains_key(*s))
                    .cloned();
                (Some(true), successor)
            } else {
                (None, None)
            };
            results.push(ResultEntry {
                document_id: document_id.clone(),
                effective_successor,
                reasons_ref: meta.reasons.clone(),
                score_rank: (rank0 + 1) as u32,
                superseded,
            });
        }
        trace.observe(
            "results",
            results.iter().map(|r| r.document_id.clone()).collect(),
        );

        let envelope = Envelope {
            index_version: self.manifest.index_version.clone(),
            judge_applied,
            principal_id: scope.principal_id.clone(),
            query_hash: query_hash(
                &normalized,
                &scope.principal_id,
                &scope.snapshot_version,
                &self.manifest.index_version,
            ),
            results,
            retrieval_mode: retrieval_mode.to_string(),
            scope_statement: derive_scope_statement(scope.scope_reasons.iter().map(String::as_str)),
            snapshot_version: scope.snapshot_version.clone(),
        };
        Ok((envelope, trace))
    }
}

/// BM25 over one partition with the allowlist restriction as a Must clause
/// of the query itself. tantivy never sees an unrestricted ranking.
fn search_partition(
    partition: &Partition,
    tokens: &[String],
    restriction: &[&str],
    k: usize,
) -> Result<Vec<(f32, String)>> {
    let mut term_clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();
    for token in tokens {
        for field in [partition.title_field, partition.body_field] {
            term_clauses.push((
                Occur::Should,
                Box::new(TermQuery::new(
                    Term::from_field_text(field, token),
                    IndexRecordOption::WithFreqs,
                )),
            ));
        }
    }
    let text_query = BooleanQuery::new(term_clauses);

    let restriction_terms: Vec<Term> = restriction
        .iter()
        .map(|id| Term::from_field_text(partition.doc_id_field, id))
        .collect();
    let allow_query = TermSetQuery::new(restriction_terms);

    let governed_query = BooleanQuery::new(vec![
        (Occur::Must, Box::new(text_query) as Box<dyn Query>),
        (Occur::Must, Box::new(allow_query) as Box<dyn Query>),
    ]);

    let searcher = partition.reader.searcher();
    let top_docs = searcher
        .search(&governed_query, &TopDocs::with_limit(k).order_by_score())
        .context("partition search failed")?;

    let mut hits = Vec::with_capacity(top_docs.len());
    for (score, address) in top_docs {
        let doc: TantivyDocument = searcher
            .doc(address)
            .context("cannot load stored document id")?;
        let doc_id = doc
            .get_first(partition.doc_id_field)
            .and_then(|v| v.as_str())
            .context("indexed document is missing its stored id")?;
        hits.push((score, doc_id.to_string()));
    }
    // Deterministic within the partition as well: score desc, then id asc.
    hits.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(hits)
}
