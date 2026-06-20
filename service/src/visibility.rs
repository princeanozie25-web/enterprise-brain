//! AUTH-2 (FC-A2): the scope -> org-graph projection. THE single authority on
//! metadata visibility, exactly as the M1 allowlist is THE authority on
//! document access. Every metadata surface (/graph, /node/*, /lens) filters
//! through this; no surface invents its own visibility rule.
//!
//! DD-2 (structural core): a STANDING principal sees their own department's
//! structure (the people in their department), their reporting relationships
//! (their manager and their direct reports), the agents they own, the org core,
//! and the org's shared source systems. STANDING = non-empty group membership;
//! a principal with no groups (p_void) — or an unknown principal — sees NOTHING.
//!
//! Fail-closed and pre-assembly: callers compute the projection, then build the
//! response from the in-scope set only. Out-of-scope data is never assembled and
//! then stripped. Grant/capability reachability is an additive follow-up and is
//! deliberately NOT part of this structural core (it does not affect the
//! person/agent/org metadata oracle).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context, Result};
use retrieval::index::sha256_hex;
use serde::Deserialize;

use crate::answer::AskError;
use crate::graph::ORG_NODE_ID;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct VisCompany {
    #[serde(default)]
    people: Vec<VisPerson>,
    #[serde(default)]
    agents: Vec<VisAgent>,
    #[serde(default)]
    sources: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct VisPerson {
    id: String,
    department: String,
    #[serde(default)]
    manager_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VisAgent {
    id: String,
    owner_user_id: String,
}

fn load_company(state: &AppState) -> Result<VisCompany> {
    let path = state.fixtures_dir.join("company.json");
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if sha256_hex(&bytes) != state.company_sha256 {
        bail!("company.json does not match the M1-pinned hash; refusing");
    }
    serde_json::from_slice(&bytes).with_context(|| format!("{} fails parse", path.display()))
}

/// The org-graph nodes a principal may see (structural core). Empty for a
/// principal with no standing (p_void / unknown).
#[derive(Debug, Default)]
pub struct Visibility {
    pub standing: bool,
    pub org: bool,
    pub people: BTreeSet<String>,
    pub departments: BTreeSet<String>,
    pub agents: BTreeSet<String>,
    pub sources: BTreeSet<String>,
}

impl Visibility {
    /// Is a single node id in scope? Covers org / person / agent / department /
    /// source. Out-of-scope (and unknown) -> false (fail-closed).
    pub fn node_visible(&self, node_id: &str) -> bool {
        (node_id == ORG_NODE_ID && self.org)
            || self.people.contains(node_id)
            || self.agents.contains(node_id)
            || self.departments.contains(node_id)
            || self.sources.contains(node_id)
    }
}

/// `true` iff the actor carries group scope. p_void (no groups) and unknown
/// principals have no standing -> they see no metadata.
pub fn has_standing(state: &AppState, actor: &str) -> bool {
    state.identity.is_known(actor) && !state.identity.statement_for(actor).groups.is_empty()
}

/// Compute the visibility projection for `actor` (structural core, DD-2).
/// Fail-closed: no standing -> the EMPTY projection.
pub fn compute(state: &AppState, actor: &str) -> Result<Visibility, AskError> {
    if !has_standing(state, actor) {
        return Ok(Visibility::default());
    }
    let company = load_company(state).map_err(AskError::Internal)?;

    let dept_of: BTreeMap<&str, &str> = company
        .people
        .iter()
        .map(|p| (p.id.as_str(), p.department.as_str()))
        .collect();
    let manager_of: BTreeMap<&str, &str> = company
        .people
        .iter()
        .filter_map(|p| p.manager_id.as_deref().map(|m| (p.id.as_str(), m)))
        .collect();

    let actor_dept = dept_of.get(actor).copied(); // None when the actor is an agent
    let actor_manager = manager_of.get(actor).copied();

    // People: self + own department + manager + direct reports.
    let mut people = BTreeSet::new();
    if dept_of.contains_key(actor) {
        people.insert(actor.to_string());
    }
    for person in &company.people {
        let same_department = actor_dept.is_some() && Some(person.department.as_str()) == actor_dept;
        let is_actor_manager = actor_manager == Some(person.id.as_str());
        let reports_to_actor = manager_of.get(person.id.as_str()).copied() == Some(actor);
        if same_department || is_actor_manager || reports_to_actor {
            people.insert(person.id.clone());
        }
    }

    // Departments: the departments of every visible person (own + cross-dept
    // manager/report), plus the actor's own department.
    let mut departments = BTreeSet::new();
    for pid in &people {
        if let Some(dept) = dept_of.get(pid.as_str()) {
            departments.insert((*dept).to_string());
        }
    }
    if let Some(dept) = actor_dept {
        departments.insert(dept.to_string());
    }

    // Agents the actor owns; the org's shared source systems.
    let agents: BTreeSet<String> = company
        .agents
        .iter()
        .filter(|a| a.owner_user_id == actor)
        .map(|a| a.id.clone())
        .collect();
    let sources: BTreeSet<String> = company.sources.iter().cloned().collect();

    Ok(Visibility {
        standing: true,
        org: true,
        people,
        departments,
        agents,
        sources,
    })
}
