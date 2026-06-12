//! The result envelope: the ONLY thing a query ever emits.
//!
//! Structurally count-free: the type has no field that could carry a
//! suppressed/hidden/filtered count, partition statistic, or any total beyond
//! `results.len()` (R-4). A "3 results hidden" line is a mosaic channel, so
//! the type cannot say it. Adding such a field fails the governance harness's
//! exhaustive key whitelist.
//!
//! Serialization is canonical: sorted keys (via `serde_json::Value`), compact
//! encoding, trailing newline — identical queries produce byte-identical
//! envelopes (R-6).

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::index::{canonical_json_bytes, sha256_hex, tokenize};

/// One served result. `reasons_ref` carries the M1 rule-id references for
/// this (principal, document) verbatim from the compiled artifact — the
/// envelope never invents access justifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultEntry {
    pub document_id: String,
    /// Present only under --include-superseded, copied from the M1 artifact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_successor: Option<String>,
    pub reasons_ref: Vec<String>,
    /// 1-based rank in fused order. Raw scores are never serialized.
    pub score_rank: u32,
    /// Present (true) only under --include-superseded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
}

/// Describes the scope that produced the results, derived ONLY from the M1
/// artifact's reason strings (the query path reads no company fixture):
/// groups from `REBAC:<group>`, sites from `ABAC:site_match:<site>`, band as
/// the highest satisfied `ABAC:band_min:<n>` (null when no band condition was
/// exercised anywhere in the allowlist).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeStatement {
    pub band: Option<u8>,
    pub groups: Vec<String>,
    pub sites: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub index_version: String,
    /// True only when a judge ran AND its order was applied. Elision,
    /// failure, and lexical mode all say false — never why (that would be a
    /// side channel about what was considered).
    pub judge_applied: bool,
    pub principal_id: String,
    pub query_hash: String,
    pub results: Vec<ResultEntry>,
    /// "lexical_only" | "hybrid": the pipeline that actually ranked. A
    /// degraded hybrid query says "lexical_only" — the envelope never lies
    /// about what produced its results.
    pub retrieval_mode: String,
    pub scope_statement: ScopeStatement,
    pub snapshot_version: String,
}

impl Envelope {
    /// Canonical bytes for stdout / comparison.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>> {
        canonical_json_bytes(self)
    }
}

/// The normalized form of a query: lowercase index tokens joined by single
/// spaces. This is what gets hashed and what gets searched — nothing else
/// about the raw string survives.
pub fn normalize_query(raw: &str) -> String {
    tokenize(raw).join(" ")
}

/// `sha256(normalized query + principal + snapshot_version + index_version)`,
/// newline-separated in that order.
pub fn query_hash(
    normalized_query: &str,
    principal_id: &str,
    snapshot_version: &str,
    index_version: &str,
) -> String {
    let preimage =
        format!("{normalized_query}\n{principal_id}\n{snapshot_version}\n{index_version}\n");
    sha256_hex(preimage.as_bytes())
}

/// Derives the scope statement from M1 reason strings across the allowlist.
pub fn derive_scope_statement<'a>(reasons: impl Iterator<Item = &'a str>) -> ScopeStatement {
    let mut groups: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut sites: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut band: Option<u8> = None;

    for reason in reasons {
        if let Some(group) = reason.strip_prefix("REBAC:") {
            // REBAC:public and REBAC:role:<r> are grants, not group scope.
            if group != "public" && !group.starts_with("role:") {
                groups.insert(group.to_string());
            }
        } else if let Some(site) = reason.strip_prefix("ABAC:site_match:") {
            sites.insert(site.to_string());
        } else if let Some(n) = reason.strip_prefix("ABAC:band_min:") {
            if let Ok(n) = n.parse::<u8>() {
                band = Some(band.map_or(n, |b| b.max(n)));
            }
        }
    }

    ScopeStatement {
        band,
        groups: groups.into_iter().collect(),
        sites: sites.into_iter().collect(),
    }
}
