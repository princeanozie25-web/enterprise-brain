//! The org-graph node INSPECTOR's data: GET /node/{id}/summary.
//!
//! REAL governance, metadata ONLY. For a person or agent it reports the
//! compiled scope plus the REASONS access is granted — read from the same M1
//! artifacts the lens trusts — grouped and COUNTED, never the documents
//! themselves (those stay behind the audited lens click). For the org core it
//! reports the corpus's real cardinalities (the sidebar's counts). It can
//! express NO holding: no document id, no title, no sensitivity — GR-7 scans
//! for exactly that. An id that is not the org and not a compiled principal
//! gets `Ok(None)` -> the one 404 (departments and sources are summarised on
//! the client from the graph payload, never here).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use retrieval::index::{canonical_json_bytes, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::answer::AskError;
use crate::graph::ORG_NODE_ID;
use crate::lens::{display_reason, load_subject_artifact, reason_class, sentence_for, LensEntry};
use crate::AppState;

// ---------------------------------------------------------------------------
// Response (canonical JSON, sorted keys). NONE of these keys can name a doc.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ReasonGroup {
    granted: usize,
    reason: String,
    sentence: String,
}

#[derive(Debug, Serialize)]
struct AgentRef {
    id: String,
    name: String,
}

#[derive(Debug, Default, Serialize)]
struct OrgStats {
    agents: usize,
    capabilities: usize,
    departments: usize,
    document_total: usize,
    groups: usize,
    initiatives: usize,
    people: usize,
    permission_edges: usize,
    principals: usize,
    sites: usize,
    sources: usize,
    strategies: usize,
    total_decisions: usize,
    workflows: usize,
}

#[derive(Debug, Default, Serialize)]
struct NodeSummary {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    access_by_reason: Vec<ReasonGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    agents_owned: Vec<AgentRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    band: Option<u8>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    blocked_actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    corpus_documents: Option<usize>,
    demo_identity_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    department: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    grant_groups: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    groups: Vec<String>,
    id: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    manages: Option<usize>,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner_user_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    permitted_actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reports_to: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sites: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stats: Option<OrgStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visible_documents: Option<usize>,
}

// ---------------------------------------------------------------------------
// Fixture mirrors (hash-verified where pinned)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SummaryCompany {
    company: SummaryMeta,
    departments: Vec<String>,
    people: Vec<SummaryPerson>,
    agents: Vec<SummaryAgent>,
    groups: Vec<Value>,
    #[serde(default)]
    sources: Vec<String>,
    #[serde(default)]
    sites: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct SummaryMeta {
    name: String,
}

#[derive(Debug, Deserialize)]
struct SummaryPerson {
    id: String,
    name: String,
    department: String,
}

#[derive(Debug, Deserialize)]
struct SummaryAgent {
    id: String,
    name: String,
    owner_user_id: String,
    grant: SummaryGrant,
}

#[derive(Debug, Deserialize)]
struct SummaryGrant {
    #[serde(default)]
    groups: Vec<String>,
}

fn load_company(fixtures_dir: &Path, expected_sha256: &str) -> Result<SummaryCompany> {
    let path = fixtures_dir.join("company.json");
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if sha256_hex(&bytes) != expected_sha256 {
        bail!("company.json does not match the M1-pinned hash; refusing");
    }
    serde_json::from_slice(&bytes).with_context(|| format!("{} fails parse", path.display()))
}

#[derive(Debug, Deserialize)]
struct IndexTotals {
    allow_entries: usize,
    documents: usize,
    principals: usize,
}

#[derive(Debug, Deserialize)]
struct IndexFile {
    totals: IndexTotals,
}

/// The M1 index is itself the pin; reading it needs no separate hash check.
fn index_totals(state: &AppState) -> Result<IndexTotals> {
    let path = state.artifacts_dir.join("index.json");
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let index: IndexFile =
        serde_json::from_slice(&bytes).with_context(|| format!("{} fails parse", path.display()))?;
    Ok(index.totals)
}

#[derive(Debug, Default, Deserialize)]
struct BrmCounts {
    #[serde(default)]
    capabilities: Vec<Value>,
    #[serde(default)]
    initiatives: Vec<Value>,
    #[serde(default)]
    strategies: Vec<Value>,
    #[serde(default)]
    workflows: Vec<Value>,
}

fn load_brm(state: &AppState) -> Result<BrmCounts> {
    let path = state.fixtures_dir.join("brm.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(BrmCounts::default());
    };
    if let Some(expected) = &state.brm_sha256 {
        if &sha256_hex(&bytes) != expected {
            bail!("brm.json does not match the pinned hash; refusing");
        }
    }
    serde_json::from_slice(&bytes).with_context(|| format!("{} fails parse", path.display()))
}

// ---------------------------------------------------------------------------
// Reason grouping (same primary-reason law as the lens, counts only)
// ---------------------------------------------------------------------------

fn group_reasons(entries: &[LensEntry]) -> Result<Vec<ReasonGroup>> {
    let mut counts: BTreeMap<(u8, String), usize> = BTreeMap::new();
    for entry in entries {
        if entry.reasons.is_empty() {
            bail!("artifact entry with no reasons; refusing");
        }
        let mut reasons: Vec<String> = entry.reasons.iter().map(|r| display_reason(r)).collect();
        reasons.sort_by_key(|r| (reason_class(r).unwrap_or(u8::MAX), r.clone()));
        reasons.dedup();
        let primary = reasons[0].clone();
        let class = reason_class(&primary)?;
        *counts.entry((class, primary)).or_default() += 1;
    }
    let mut out = Vec::with_capacity(counts.len());
    for ((_, reason), granted) in counts {
        out.push(ReasonGroup {
            granted,
            sentence: sentence_for(&reason)?,
            reason,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// `Ok(None)` = not summarisable here (unknown id, or a dept/source node the
/// client renders from the graph payload) -> the one 404.
pub fn node_summary(state: &AppState, id: &str) -> Result<Option<Vec<u8>>, AskError> {
    if id == ORG_NODE_ID {
        return org_summary(state).map(Some).map_err(AskError::Internal);
    }

    // A person or agent must be a compiled principal (carry an M1 artifact).
    let entries = load_subject_artifact(state, id).map_err(AskError::Internal)?;
    let Some(entries) = entries else {
        return Ok(None);
    };
    let company =
        load_company(&state.fixtures_dir, &state.company_sha256).map_err(AskError::Internal)?;
    let corpus = index_totals(state).map_err(AskError::Internal)?.documents;
    let access_by_reason = group_reasons(&entries).map_err(AskError::Internal)?;

    let mut summary = NodeSummary {
        access_by_reason,
        corpus_documents: Some(corpus),
        demo_identity_mode: true,
        id: id.to_string(),
        visible_documents: Some(entries.len()),
        ..Default::default()
    };

    if let Some(agent) = company.agents.iter().find(|a| a.id == id) {
        let mut grant_groups = agent.grant.groups.clone();
        grant_groups.sort();
        grant_groups.dedup();
        summary.kind = "agent".to_string();
        summary.name = agent.name.clone();
        summary.owner_user_id = Some(agent.owner_user_id.clone());
        summary.grant_groups = grant_groups;
        // The M4 authority, stated: an agent may only read within its compiled
        // allowlist and PROPOSE drafts; everything mutating is human-gated and
        // structurally refused (see service/src/lib.rs agent-run handler).
        summary.permitted_actions = vec![
            "retrieve_within_allowlist".to_string(),
            "propose_draft".to_string(),
        ];
        summary.blocked_actions = vec![
            "approve_or_reject_proposals".to_string(),
            "mutate_records".to_string(),
            "run_as_non_owner".to_string(),
            "act_outside_allowlist".to_string(),
        ];
    } else {
        // A person.
        let scope = state.identity.statement_for(id);
        let mut groups = scope.groups.clone();
        groups.sort();
        groups.dedup();
        let mut sites = scope.sites.clone();
        sites.sort();
        summary.kind = "human".to_string();
        summary.band = scope.band;
        summary.groups = groups;
        summary.sites = sites;
        if let Some(person) = company.people.iter().find(|p| p.id == id) {
            summary.department = Some(person.department.clone());
            summary.name = person.name.clone();
        }
        if let Some(record) = state.people.as_deref().and_then(|layer| layer.get(id)) {
            summary.name = record.display_name.clone();
            summary.title = Some(record.title.clone());
            summary.reports_to = record.reports_to.clone();
            summary.manages = Some(record.manages.len());
            if summary.department.is_none() {
                summary.department = Some(record.department_label.clone());
            }
        }
        let mut owned: Vec<AgentRef> = company
            .agents
            .iter()
            .filter(|a| a.owner_user_id == id)
            .map(|a| AgentRef {
                id: a.id.clone(),
                name: a.name.clone(),
            })
            .collect();
        owned.sort_by(|a, b| a.id.cmp(&b.id));
        summary.agents_owned = owned;
    }

    canonical_json_bytes(&summary)
        .map(Some)
        .map_err(AskError::Internal)
}

fn org_summary(state: &AppState) -> Result<Vec<u8>> {
    let company = load_company(&state.fixtures_dir, &state.company_sha256)?;
    let totals = index_totals(state)?;
    let brm = load_brm(state)?;
    let people = state
        .people
        .as_deref()
        .map(|layer| layer.roster().count())
        .unwrap_or(company.people.len());

    let stats = OrgStats {
        agents: company.agents.len(),
        capabilities: brm.capabilities.len(),
        departments: company.departments.len(),
        document_total: totals.documents,
        groups: company.groups.len(),
        initiatives: brm.initiatives.len(),
        people,
        permission_edges: totals.allow_entries,
        principals: totals.principals,
        sites: company.sites.len(),
        sources: company.sources.len(),
        strategies: brm.strategies.len(),
        total_decisions: totals.principals * totals.documents,
        workflows: brm.workflows.len(),
    };

    let summary = NodeSummary {
        demo_identity_mode: true,
        id: ORG_NODE_ID.to_string(),
        kind: "org".to_string(),
        name: company.company.name.clone(),
        stats: Some(stats),
        ..Default::default()
    };
    canonical_json_bytes(&summary)
}
