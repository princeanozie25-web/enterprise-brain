//! Enterprise Brain M1 — the Scope Compiler.
//!
//! Turns (principal, permission snapshot) into a compiled, pinned,
//! reason-traced allowlist. Pure compilation: no network, no database, no
//! retrieval, no LLM, no wall clock.
//!
//! INDEPENDENCE INVARIANT: this crate implements the access semantics from
//! the M1 specification and the raw data fixtures (`company.json`,
//! `documents.json`, `traps.json`) alone. It never reads
//! `/fixtures/ground_truth.jsonl` (the conformance harness's oracle) and is
//! not derived from the M0 generator under `/synth`.

pub mod compile;
pub mod model;
pub mod semantics;
pub mod snapshot;

use std::path::Path;

use anyhow::Result;

/// Loads, schema-checks, and structurally validates the input fixtures.
pub fn load_world(fixtures_dir: &Path) -> Result<semantics::World> {
    let (company, documents, traps) = model::load_fixtures(fixtures_dir)?;
    semantics::World::build(company, documents, traps)
}
