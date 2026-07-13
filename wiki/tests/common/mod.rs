#![allow(dead_code)] // each test binary uses a subset of these helpers.

//! Shared test harness.
//!
//! The read-only authz INPUT is produced exactly as `retrieval`/`service` do:
//! compile FRESH M1 artifacts into a scratch dir via the frozen compiler crate
//! (a dev-dependency). The wiki runtime never links the compiler — only this
//! harness does, and only to PRODUCE artifacts.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use wiki::ground::Verifier;
use wiki::scope::DocSelector;
use wiki::synth::{RawClaim, SourceDoc, Synthesizer};

/// The repo's real fixtures dir (read-only).
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("wiki crate sits in the repo root")
        .join("fixtures")
}

/// A fresh per-invocation scratch dir under the SYSTEM temp dir — the same
/// unique-suffix shape every sibling crate's test helper uses (rationale in
/// the body comment).
pub fn scratch(name: &str) -> PathBuf {
    // Unique per invocation: Windows scanners (Search indexer / Defender) can
    // hold a just-deleted path in delete-pending state, so re-creating the
    // SAME path races them into Os error 5 "Access is denied". A fresh suffix
    // never re-opens a dying path; prior runs' dirs are swept best-effort (a
    // locked leftover is skipped now and reaped on a later run).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // The base lives in the SYSTEM temp dir, not target/tmp: the repo sits
    // under Documents\, which Windows Search indexes by default — its crawler
    // opens freshly written index segments mid-build and the write fails with
    // os error 5. AppData\Local\Temp is outside the default index scope.
    let base = std::env::temp_dir().join("enterprise-brain-test-scratch");
    std::fs::create_dir_all(&base).expect("scratch base");
    let prefix = format!("{name}-");
    if let Ok(entries) = base.read_dir() {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let dir = base.join(format!(
        "{prefix}{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Compiles the real fixtures into `into` as M1 artifacts. This is the
/// read-only authz model the wiki then reads.
pub fn compile_artifacts(into: &Path) {
    let fixtures = fixtures_dir();
    let snap = scope_compiler::snapshot::take(&fixtures).expect("snapshot fixtures");
    let world = scope_compiler::load_world(&fixtures).expect("fixtures validate");
    let (set, unknown) =
        scope_compiler::compile::compile_set(&world, &snap, None).expect("compile M1");
    assert!(unknown.is_empty(), "all fixture principals are known");
    scope_compiler::compile::write_artifacts(into, &set).expect("write artifacts");
}

pub fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).expect("read file for hashing");
    let digest = Sha256::digest(&bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Recursively hashes every file under `dir`, keyed by path relative to `dir`.
pub fn hash_tree(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    hash_tree_into(dir, dir, &mut out);
    out
}

fn hash_tree_into(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            hash_tree_into(root, &path, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, sha256_file(&path));
        }
    }
}

// ---------------------------------------------------------------------------
// Slice-2 test doubles: a deterministic synthesizer that RECORDS exactly which
// documents it was handed (so a test can prove no out-of-scope doc reaches the
// model), and a fixed document selector (so tests need neither Ollama nor a
// tantivy index).
// ---------------------------------------------------------------------------

type GenFn = Box<dyn Fn(&[SourceDoc]) -> Vec<RawClaim>>;

pub struct RecordingSynthesizer {
    model: String,
    received: RefCell<BTreeSet<String>>,
    gen: GenFn,
}

impl RecordingSynthesizer {
    pub fn new(model: &str, gen: impl Fn(&[SourceDoc]) -> Vec<RawClaim> + 'static) -> Self {
        RecordingSynthesizer {
            model: model.to_string(),
            received: RefCell::new(BTreeSet::new()),
            gen: Box::new(gen),
        }
    }

    /// Echoes each in-scope source back as a claim citing it (no implication).
    /// The quote is a VERBATIM prefix of the source body, so slice-4 extractive
    /// anchoring (`source_body.find(quote)`) matches and the claim can be
    /// admitted under an `always()` verifier.
    pub fn echo(model: &str) -> Self {
        RecordingSynthesizer::new(model, |sources| {
            sources
                .iter()
                .map(|s| RawClaim {
                    text: format!("Source {} ({}) is in scope", s.doc_id, s.title),
                    cited_doc_id: s.doc_id.clone(),
                    quote: verbatim_prefix(&s.text, 48),
                    about_principal: None,
                })
                .collect()
        })
    }

    /// Every distinct document id this synthesizer was handed, across all calls.
    pub fn received_ids(&self) -> BTreeSet<String> {
        self.received.borrow().clone()
    }
}

impl Synthesizer for RecordingSynthesizer {
    fn model_id(&self) -> &str {
        &self.model
    }
    fn synthesize(&self, _topic: &str, sources: &[SourceDoc]) -> anyhow::Result<Vec<RawClaim>> {
        for s in sources {
            self.received.borrow_mut().insert(s.doc_id.clone());
        }
        Ok((self.gen)(sources))
    }
}

/// Returns a fixed list of doc ids (capped at k) regardless of the topic.
pub struct FixedSelector {
    pub ids: Vec<String>,
}

impl DocSelector for FixedSelector {
    fn select(&self, _topic: &str, k: usize) -> anyhow::Result<Vec<String>> {
        Ok(self.ids.iter().take(k).cloned().collect())
    }
}

/// A verbatim prefix of `text` (up to `n` chars), trimmed. By construction it is
/// an exact substring of `text`, so the slice-4 extractive anchor matches when
/// the same body is the grounding source. An empty `text` yields an empty quote,
/// which grounding refuses as unfounded (the fail-closed path).
pub fn verbatim_prefix(text: &str, n: usize) -> String {
    text.chars().take(n).collect::<String>().trim().to_string()
}

// ---------------------------------------------------------------------------
// Slice-4 test double: a deterministic support-verifier. `always()` confirms
// support (an anchored claim is admitted); `never()` denies it (an anchored
// claim is withheld, fail-closed). Neither touches a model or the network.
// ---------------------------------------------------------------------------

pub struct FakeVerifier {
    model: String,
    supported: bool,
}

impl FakeVerifier {
    pub fn always() -> Self {
        FakeVerifier {
            model: "fake-judge".to_string(),
            supported: true,
        }
    }
    pub fn never() -> Self {
        FakeVerifier {
            model: "fake-judge".to_string(),
            supported: false,
        }
    }
}

impl Verifier for FakeVerifier {
    fn model_id(&self) -> &str {
        &self.model
    }
    fn supports(&self, _span: &str, _claim: &str) -> anyhow::Result<bool> {
        Ok(self.supported)
    }
}
