//! The Bursar seam, unchanged from M2b: one JSONL row per model call, model
//! id + token numbers only, `cost_usd` null for local models, never content.
//! Generation rows reuse the exact M2b shape; `ts` ordinals are assigned at
//! write time and continue across appends.

use std::path::Path;

use anyhow::Result;
pub use retrieval::local_llm::{append_usage_sidecar, UsageEvent};
use retrieval::local_llm::{estimate_tokens, TokenCounts};

/// Builds the metering row for one generation call. `estimate_basis` is the
/// byte length of (query + sealed context) for the bytes/4 fallback when the
/// local API reports no counts.
pub fn generation_event(
    model: &str,
    usage: Option<TokenCounts>,
    estimate_basis: usize,
) -> UsageEvent {
    UsageEvent {
        cost_usd: None,
        estimated: usage.is_none(),
        input_tokens: usage
            .map(|u| u.input_tokens)
            .unwrap_or_else(|| estimate_tokens(estimate_basis)),
        model: model.to_string(),
        output_tokens: usage.map(|u| u.output_tokens).unwrap_or(0),
        ts: 0,
    }
}

/// Appends all of an ask's usage rows (retrieval embed/judge + generation).
pub fn append_all(path: &Path, events: &[UsageEvent]) -> Result<()> {
    append_usage_sidecar(path, events)
}
