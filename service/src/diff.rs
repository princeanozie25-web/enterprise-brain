//! AP-4: GET /lens/diff?left=<id>&right=<id> — two principals side by side,
//! every difference attributed to the rule responsible.
//!
//! THE LAW: SET EXACTNESS. (left_only docs) ∪ (shared docs) equals the left
//! principal's compiled allowlist EXACTLY — no sampling, no truncation —
//! and symmetrically for the right. The three columns are a partition of
//! the two artifacts, built by set membership (every entry routes to
//! exactly one column) and re-proven by the AD-suite against the artifacts
//! themselves.
//!
//! Charter §6.3, extended: the diff reveals BOTH subjects' worlds to the
//! actor in ONE act, and that act is audited ONCE — `lens_diff`, with both
//! sides on the row — BEFORE anything renders. R-13 redaction applies in
//! all three columns: each exclusive column redacts against its own side's
//! allowlist; a shared row serves two worlds at once, so its successor
//! field uses the INTERSECTION — it may not over-describe either side.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use retrieval::index::{canonical_json_bytes, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::answer::AskError;
use crate::lens::{display_reason, load_subject_artifact, reason_class, sentence_for, LensEntry};
use crate::{humanize, AppState, DocMeta};

// ---------------------------------------------------------------------------
// Response shapes (canonical JSON, sorted keys)
// ---------------------------------------------------------------------------

/// Passport-lite: names from the hash-verified company.json, nothing more.
#[derive(Debug, Serialize)]
pub struct DiffPassport {
    pub id: String,
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocRow {
    pub document_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_successor: Option<String>,
    pub sensitivity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct DiffSection {
    pub docs: Vec<DocRow>,
    pub reason: String,
    pub sentence: String,
}

#[derive(Debug, Serialize)]
pub struct SharedRow {
    /// True iff the two sides' PRIMARY reasons differ (the priority law
    /// applied per side before comparison).
    pub divergent_route: bool,
    pub doc: DocRow,
    /// Verbatim from each side's artifact — unsorted, unnormalized.
    pub left_reasons: Vec<String>,
    pub right_reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DiffResponse {
    /// AR-1: the viewer's own directory card (display only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<humanize::PersonCard>,
    pub actor_id: String,
    /// Honesty contract (identity.rs): every response carries this.
    pub demo_identity_mode: bool,
    pub left: DiffPassport,
    /// AR-1: the left principal's directory card (name/title/department/avatar
    /// — org-structural, no evidence). Absent with no humanization layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_human: Option<humanize::PersonCard>,
    pub left_only: Vec<DiffSection>,
    pub right: DiffPassport,
    /// AR-1: the right principal's directory card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_human: Option<humanize::PersonCard>,
    pub right_only: Vec<DiffSection>,
    /// Sorted by document_id ascending (the console leads with divergent
    /// rows; the SERVICE order is the neutral one).
    pub shared: Vec<SharedRow>,
    pub snapshot_version: String,
}

// ---------------------------------------------------------------------------
// Passports (per-request, hash-verified company read)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DiffCompany {
    people: Vec<DiffNamed>,
    agents: Vec<DiffNamed>,
}

#[derive(Debug, Deserialize)]
struct DiffNamed {
    id: String,
    name: String,
}

fn load_passports(
    state: &AppState,
    left_id: &str,
    right_id: &str,
) -> Result<(DiffPassport, DiffPassport)> {
    let path = state.fixtures_dir.join("company.json");
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if sha256_hex(&bytes) != state.company_sha256 {
        bail!("company.json does not match the M1-pinned hash; refusing");
    }
    let company: DiffCompany = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} fails parse", path.display()))?;
    let resolve = |id: &str| -> Result<DiffPassport> {
        if let Some(person) = company.people.iter().find(|p| p.id == id) {
            Ok(DiffPassport {
                id: person.id.clone(),
                kind: "human".to_string(),
                name: person.name.clone(),
            })
        } else if let Some(agent) = company.agents.iter().find(|a| a.id == id) {
            Ok(DiffPassport {
                id: agent.id.clone(),
                kind: "agent".to_string(),
                name: agent.name.clone(),
            })
        } else {
            // In the M1 index but not in company.json: the worlds disagree.
            bail!("principal in the M1 index but not in company.json; refusing")
        }
    };
    Ok((resolve(left_id)?, resolve(right_id)?))
}

// ---------------------------------------------------------------------------
// Column construction
// ---------------------------------------------------------------------------

/// The priority-law primary of one side's verbatim reason set: normalize the
/// PUBLIC display section, then minimum by (class, lexicographic). Total —
/// any unknown reason refuses the whole response, never a partial render.
/// Crate-visible since AP-5: the PDF renderer prints the same chips.
pub(crate) fn primary_reason(reasons: &[String]) -> Result<String> {
    if reasons.is_empty() {
        bail!("artifact entry with no reasons; refusing");
    }
    let mut normalized: Vec<String> = reasons.iter().map(|r| display_reason(r)).collect();
    for reason in &normalized {
        reason_class(reason)?;
        sentence_for(reason)?;
    }
    normalized.sort_by_key(|r| (reason_class(r).unwrap_or(u8::MAX), r.clone()));
    normalized.dedup();
    Ok(normalized[0].clone())
}

/// One row. R-13: the successor id is emitted only when the successor is
/// inside `successor_scope` — the owning side's allowlist for an exclusive
/// column, the intersection of BOTH for a shared row.
fn doc_row(
    entry: &LensEntry,
    docs: &BTreeMap<String, DocMeta>,
    successor_scope: &BTreeSet<&str>,
) -> Result<DocRow> {
    let meta = docs
        .get(&entry.document_id)
        .context("artifact names a document the corpus does not carry")?;
    let superseded = entry.superseded == Some(true);
    let effective_successor = if superseded {
        entry
            .effective_successor
            .as_ref()
            .filter(|s| successor_scope.contains(s.as_str()))
            .cloned()
    } else {
        None
    };
    Ok(DocRow {
        document_id: entry.document_id.clone(),
        effective_successor,
        sensitivity: meta.sensitivity.clone(),
        superseded: superseded.then_some(true),
        title: meta.title.clone(),
    })
}

/// One exclusive column, the priority law as in /lens: group by primary,
/// sections sorted (class, lexicographic) which puts PUBLIC:all last, docs
/// by document_id within a section.
fn build_exclusive(
    entries: &[&LensEntry],
    docs: &BTreeMap<String, DocMeta>,
    own_allowlist: &BTreeSet<&str>,
) -> Result<Vec<DiffSection>> {
    let mut sections: BTreeMap<(u8, String), Vec<DocRow>> = BTreeMap::new();
    for entry in entries {
        let primary = primary_reason(&entry.reasons)?;
        let class = reason_class(&primary)?;
        sections
            .entry((class, primary))
            .or_default()
            .push(doc_row(entry, docs, own_allowlist)?);
    }
    let mut out = Vec::with_capacity(sections.len());
    for ((_, reason), mut rows) in sections {
        rows.sort_by(|a, b| a.document_id.cmp(&b.document_id));
        out.push(DiffSection {
            docs: rows,
            sentence: sentence_for(&reason)?,
            reason,
        });
    }
    Ok(out)
}

/// Partitions two artifacts into the three columns. SET EXACTNESS holds by
/// construction — every left entry routes to exactly one of left_only /
/// shared by membership of its id in the right set, symmetrically for the
/// right — and the AD-suite re-proves it against the artifacts. Public for
/// the harness (the AL-4 precedent: the law is testable without HTTP).
pub fn build_diff_columns(
    left_entries: &[LensEntry],
    right_entries: &[LensEntry],
    docs: &BTreeMap<String, DocMeta>,
) -> Result<(Vec<DiffSection>, Vec<DiffSection>, Vec<SharedRow>)> {
    let left_allow: BTreeSet<&str> = left_entries
        .iter()
        .map(|e| e.document_id.as_str())
        .collect();
    let right_allow: BTreeSet<&str> = right_entries
        .iter()
        .map(|e| e.document_id.as_str())
        .collect();
    let right_by_id: BTreeMap<&str, &LensEntry> = right_entries
        .iter()
        .map(|e| (e.document_id.as_str(), e))
        .collect();

    let mut left_exclusive: Vec<&LensEntry> = Vec::new();
    let mut shared_pairs: Vec<(&LensEntry, &LensEntry)> = Vec::new();
    for entry in left_entries {
        match right_by_id.get(entry.document_id.as_str()) {
            Some(right_entry) => shared_pairs.push((entry, right_entry)),
            None => left_exclusive.push(entry),
        }
    }
    let right_exclusive: Vec<&LensEntry> = right_entries
        .iter()
        .filter(|e| !left_allow.contains(e.document_id.as_str()))
        .collect();

    // A shared row serves two worlds at once: its successor scope is the
    // INTERSECTION (AD-4 — never over-describe either side), and the two
    // artifacts must agree about the document's supersedence, because that
    // is corpus fact, not scope fact. Disagreement means the worlds have
    // drifted: refuse.
    let shared_scope: BTreeSet<&str> = left_allow.intersection(&right_allow).copied().collect();
    let mut shared = Vec::with_capacity(shared_pairs.len());
    for (left_entry, right_entry) in shared_pairs {
        if left_entry.superseded != right_entry.superseded
            || left_entry.effective_successor != right_entry.effective_successor
        {
            bail!(
                "the two artifacts disagree about {}'s supersedence; refusing",
                left_entry.document_id
            );
        }
        let left_primary = primary_reason(&left_entry.reasons)?;
        let right_primary = primary_reason(&right_entry.reasons)?;
        shared.push(SharedRow {
            divergent_route: left_primary != right_primary,
            doc: doc_row(left_entry, docs, &shared_scope)?,
            left_reasons: left_entry.reasons.clone(),
            right_reasons: right_entry.reasons.clone(),
        });
    }
    shared.sort_by(|a, b| a.doc.document_id.cmp(&b.doc.document_id));

    Ok((
        build_exclusive(&left_exclusive, docs, &left_allow)?,
        build_exclusive(&right_exclusive, docs, &right_allow)?,
        shared,
    ))
}

// ---------------------------------------------------------------------------
// Authorization seam
// ---------------------------------------------------------------------------

/// THE SWAP POINT for the diff — sibling of lens::authorize_cross_lens, one
/// class stricter. Under demo_identity_mode the diff is permitted to any
/// header principal but audited as ONE act — action `lens_diff`, both sides
/// on the row — BEFORE the response renders. A diff is an AGGREGATION
/// INSTRUMENT: in a real deployment this function is admin-classed beyond
/// even lens_view (deny unless the actor holds the diff-admin grant), and
/// the actor derives from the SESSION — an actor borne by a URL or any
/// other request surface is refused. Swap THIS function and nothing else
/// moves. Returns the audit ordinal of the one act — AP-5's attestation
/// cites it.
fn authorize_lens_diff(state: &AppState, actor: &str, left: &str, right: &str) -> Result<u64> {
    let Some(store) = &state.proposals else {
        // No audit sink configured: the one act cannot be recorded, so it
        // cannot happen. Fail closed.
        bail!("lens diff requires the audit store (--state-dir); refusing");
    };
    store.audit_diff(actor, left, right, "allowed_demo")
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// Builds the diff body for (actor, left, right). `Ok(None)` = either id is
/// unknown — the HTTP layer serves THE one 404. A self-diff is a category
/// error (400), checked before any lookup so the answer is the same for
/// known and unknown ids. The audit row is written only on the render path:
/// refusals leave no `lens_diff` row.
/// Returns the body bytes plus the audit ordinal of the one `lens_diff`
/// act — AP-5's evidence export cites it.
pub fn diff_view(
    state: &AppState,
    actor: &str,
    left_id: &str,
    right_id: &str,
) -> Result<Option<(Vec<u8>, u64)>, AskError> {
    if left_id == right_id {
        return Err(AskError::BadRequest(
            "a diff of a lens against itself is a category error".to_string(),
        ));
    }
    let Some(left_entries) = load_subject_artifact(state, left_id).map_err(AskError::Internal)?
    else {
        return Ok(None);
    };
    let Some(right_entries) = load_subject_artifact(state, right_id).map_err(AskError::Internal)?
    else {
        return Ok(None);
    };
    let (mut left, mut right) =
        load_passports(state, left_id, right_id).map_err(AskError::Internal)?;
    // AR-1: humanized passports show the generated display name on both sides.
    if let Some(record) = state.people.as_deref().and_then(|l| l.get(left_id)) {
        left.name = record.display_name.clone();
    }
    if let Some(record) = state.people.as_deref().and_then(|l| l.get(right_id)) {
        right.name = record.display_name.clone();
    }

    // ONE audited act, before anything renders.
    let act_ordinal =
        authorize_lens_diff(state, actor, left_id, right_id).map_err(AskError::Internal)?;

    let (left_only, right_only, shared) =
        build_diff_columns(&left_entries, &right_entries, &state.docs)
            .map_err(AskError::Internal)?;
    let response = DiffResponse {
        actor: humanize::card_for(state.people.as_deref(), actor),
        actor_id: actor.to_string(),
        demo_identity_mode: true,
        left_human: humanize::card_for(state.people.as_deref(), left_id),
        left,
        left_only,
        right_human: humanize::card_for(state.people.as_deref(), right_id),
        right,
        right_only,
        shared,
        snapshot_version: state.snapshot_version.clone(),
    };
    canonical_json_bytes(&response)
        .map(|bytes| Some((bytes, act_ordinal)))
        .map_err(AskError::Internal)
}
