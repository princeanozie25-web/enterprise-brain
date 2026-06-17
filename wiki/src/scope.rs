//! Scope gating — the authorization boundary the LLM derives inside.
//!
//! [`ScopeGate`] is a thin wrapper over `retrieval::search::PrincipalScope`,
//! the workspace's proven fail-closed allowlist loader. `allowed()` is exactly
//! the oracle's allowed set for the principal — the universe the synthesizer
//! may ever draw from. [`ScopeContext`] pairs the gate with the bodies of ONLY
//! the in-scope documents. [`DocSelector`] fetches relevance — the real
//! implementation routes through governed search (allowlist applied INSIDE the
//! query), so what reaches the model is gated by the same machinery behind the
//! 74,400-decision conformance suite, not by new wiki code.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use retrieval::search::{Engine, PrincipalScope, SearchOptions};

use crate::sources::Sources;
use crate::synth::SourceDoc;

/// The compiled authorization scope of one principal. Loaded by retrieval's
/// fail-closed `PrincipalScope` (artifact hash + snapshot checks). The
/// `allowed` set is the oracle's allowed set — nothing outside it is in scope.
pub struct ScopeGate {
    pub principal_id: String,
    pub snapshot_version: String,
    allowed: BTreeSet<String>,
}

impl ScopeGate {
    pub fn load(artifacts_dir: &Path, principal_id: &str) -> Result<ScopeGate> {
        let scope = PrincipalScope::load(artifacts_dir, principal_id)
            .with_context(|| format!("loading compiled scope for {principal_id}"))?;
        let allowed: BTreeSet<String> = scope.allowed_ids().map(str::to_string).collect();
        Ok(ScopeGate {
            principal_id: principal_id.to_string(),
            snapshot_version: scope.snapshot_version.clone(),
            allowed,
        })
    }

    /// The oracle's allowed document set for this scope.
    pub fn allowed(&self) -> &BTreeSet<String> {
        &self.allowed
    }

    /// Whether a document id is inside this scope. The single in/out decision.
    pub fn permits(&self, doc_id: &str) -> bool {
        self.allowed.contains(doc_id)
    }

    pub fn allowed_count(&self) -> usize {
        self.allowed.len()
    }
}

/// A scope gate plus the bodies of its in-scope documents. Out-of-scope docs
/// are not present here at all — the synthesizer is built only from these.
pub struct ScopeContext {
    pub gate: ScopeGate,
    /// in-scope doc id -> (title, body). Only documents the gate permits.
    docs: BTreeMap<String, (String, String)>,
}

impl ScopeContext {
    /// Builds the context, keeping ONLY in-scope documents' text.
    pub fn build(gate: ScopeGate, sources: &Sources) -> ScopeContext {
        let mut docs = BTreeMap::new();
        for d in &sources.documents.documents {
            if gate.permits(&d.id) {
                docs.insert(d.id.clone(), (d.title.clone(), d.body.clone()));
            }
        }
        ScopeContext { gate, docs }
    }

    /// A source doc for an in-scope id, or `None` for anything out of scope.
    /// The pipeline never asks for an out-of-scope id; this is a second gate.
    pub fn source_doc(&self, doc_id: &str) -> Option<SourceDoc> {
        self.docs.get(doc_id).map(|(title, body)| SourceDoc {
            doc_id: doc_id.to_string(),
            title: title.clone(),
            text: body.clone(),
        })
    }

    /// The in-scope document ids that actually carry text (a subset-or-equal of
    /// `gate.allowed()`; equal when every allowed id is present in the corpus).
    pub fn in_scope_ids(&self) -> BTreeSet<String> {
        self.docs.keys().cloned().collect()
    }
}

/// Selects up to `k` in-scope document ids relevant to a topic. Every returned
/// id MUST be inside the scope; the pipeline filters again, defensively.
pub trait DocSelector {
    fn select(&self, topic: &str, k: usize) -> Result<Vec<String>>;
}

/// The real selector: governed lexical search over the principal's scope. The
/// allowlist is a Must-clause INSIDE the tantivy query, so results are gated by
/// retrieval, never post-filtered here.
pub struct RetrievalSelector {
    engine: Engine,
    scope: PrincipalScope,
}

impl RetrievalSelector {
    pub fn open(
        idx_dir: &Path,
        artifacts_dir: &Path,
        principal_id: &str,
    ) -> Result<RetrievalSelector> {
        let engine = Engine::open(idx_dir).context("opening governed retrieval index")?;
        let scope = PrincipalScope::load(artifacts_dir, principal_id)
            .with_context(|| format!("loading scope for {principal_id}"))?;
        Ok(RetrievalSelector { engine, scope })
    }
}

impl DocSelector for RetrievalSelector {
    fn select(&self, topic: &str, k: usize) -> Result<Vec<String>> {
        let opts = SearchOptions::lexical(k, false);
        let (envelope, _trace) = self.engine.search(&self.scope, topic, &opts)?;
        Ok(envelope
            .results
            .into_iter()
            .map(|r| r.document_id)
            .collect())
    }
}
