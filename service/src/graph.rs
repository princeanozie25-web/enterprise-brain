//! AR-2: GET /graph — the Org Graph's data, scope-honest by construction.
//!
//! THE DIFFERENTIATOR, HELD AT THE DATA LAYER: every node this endpoint draws
//! is a REAL compiled principal (it carries an M1 artifact) and the payload
//! carries NO holding — no document id, no per-person count, no sensitivity.
//! The org STRUCTURE (departments, reporting lines, the roster of people +
//! titles + the agents people own) is INTERNAL-GRADE, exactly the tier the
//! AR-1 /people roster and the Atlas BRM structure already publish: the graph
//! draws from the SAME well and adds NO new exposure. Holdings live behind the
//! lens you click into, which stays scope-gated and audited as ever.
//!
//! RECONCILIATION (flagged in the AR-2 closeout): the spec opens with "filtered
//! to the actor's permitted world" and also binds the graph to be CONSISTENT
//! with the internal-grade /people roster ("adds NO new exposure"). /people is
//! demo-open and returns the whole roster, so consistency wins: a known
//! principal sees the whole org shape (every node artifact-backed, zero
//! holdings); an UNKNOWN principal gets the one 404; honest dark is the
//! renderer never padding with ghost/"+N hidden" nodes and the empty state for
//! no-standing actors. The differentiator vs the omniscient reference graphs
//! is that ours is artifact-backed and leaks no evidence — beautiful AND true.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use retrieval::index::{canonical_json_bytes, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::answer::AskError;
use crate::AppState;

// ---------------------------------------------------------------------------
// Response shapes (canonical JSON, sorted keys). NONE can express a holding.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct GraphCenter {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct GraphDept {
    pub id: String,
    pub label: String,
    /// Department key for the console's AR-1 DEPARTMENT_TINT (the reserved hues).
    pub tint_key: String,
}

#[derive(Debug, Serialize)]
pub struct GraphPerson {
    pub avatar_ref: String,
    pub department_id: String,
    pub display_name: String,
    pub id: String,
    pub is_self: bool,
    /// "anchor" (a senior, always-labelled node) or "member" (present, secondary).
    pub ring: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct GraphTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department_id: Option<String>,
    pub id: String,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub kind: String,
    pub to: String,
}

#[derive(Debug, Serialize)]
pub struct GraphResponse {
    pub actor_id: String,
    pub center: GraphCenter,
    pub departments: Vec<GraphDept>,
    pub edges: Vec<GraphEdge>,
    pub people: Vec<GraphPerson>,
    pub snapshot_version: String,
    pub tools: Vec<GraphTool>,
}

pub const ORG_NODE_ID: &str = "org";

// ---------------------------------------------------------------------------
// Company mirror (per-request, hash-verified — the structural facts only)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GraphCompany {
    company: CompanyMeta,
    departments: Vec<String>,
    people: Vec<GraphCompanyPerson>,
    agents: Vec<GraphCompanyAgent>,
}

#[derive(Debug, Deserialize)]
struct CompanyMeta {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GraphCompanyPerson {
    id: String,
    department: String,
    #[serde(default)]
    manager_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphCompanyAgent {
    id: String,
    name: String,
    owner_user_id: String,
}

fn load_company(fixtures_dir: &Path, expected_sha256: &str) -> Result<GraphCompany> {
    let path = fixtures_dir.join("company.json");
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if sha256_hex(&bytes) != expected_sha256 {
        bail!("company.json does not match the M1-pinned hash; refusing");
    }
    serde_json::from_slice(&bytes).with_context(|| format!("{} fails parse", path.display()))
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// Builds the org graph for `actor`. `Ok(None)` = no graph in this world: the
/// humanization layer is absent OR the actor is unknown — the HTTP layer
/// serves THE one 404 (the no-standing / unknown discipline). ANCHORS are the
/// Leadership-tier principals (the AR-1 seniority the humanization layer
/// already computes), deterministically — "the org's leaders are the
/// always-labelled nodes."
pub fn graph_view(state: &AppState, actor: &str) -> Result<Option<Vec<u8>>, AskError> {
    let Some(people) = state.people.as_deref() else {
        return Ok(None);
    };
    if !state.identity.is_known(actor) {
        return Ok(None);
    }
    let company =
        load_company(&state.fixtures_dir, &state.company_sha256).map_err(AskError::Internal)?;

    let center = GraphCenter {
        id: ORG_NODE_ID.to_string(),
        label: company.company.name.clone(),
    };

    // Department hubs, in the company's declared order.
    let departments: Vec<GraphDept> = company
        .departments
        .iter()
        .map(|dept| GraphDept {
            id: dept.clone(),
            label: dept.clone(),
            tint_key: dept.clone(),
        })
        .collect();
    let known_depts: BTreeMap<&str, ()> =
        company.departments.iter().map(|d| (d.as_str(), ())).collect();

    let manager_of: BTreeMap<&str, &str> = company
        .people
        .iter()
        .filter_map(|p| p.manager_id.as_deref().map(|m| (p.id.as_str(), m)))
        .collect();
    let dept_of: BTreeMap<&str, &str> = company
        .people
        .iter()
        .map(|p| (p.id.as_str(), p.department.as_str()))
        .collect();

    // People nodes from the humanization roster (internal-grade, no holdings).
    // Anchors = Leadership tier (deterministic; the AR-1 seniority).
    let mut graph_people: Vec<GraphPerson> = Vec::new();
    for record in people.roster() {
        let ring = if record.seniority == "Leadership" {
            "anchor"
        } else {
            "member"
        };
        graph_people.push(GraphPerson {
            avatar_ref: record.avatar_ref.clone(),
            department_id: record.department_label.clone(),
            display_name: record.display_name.clone(),
            id: record.id.clone(),
            is_self: record.id == actor,
            ring: ring.to_string(),
            title: record.title.clone(),
        });
    }

    // Tools / agents in the outer ring, tinted by their owner's department.
    let mut tools: Vec<GraphTool> = company
        .agents
        .iter()
        .map(|a| GraphTool {
            department_id: dept_of.get(a.owner_user_id.as_str()).map(|d| d.to_string()),
            id: a.id.clone(),
            kind: "agent".to_string(),
            label: a.name.clone(),
        })
        .collect();
    tools.sort_by(|a, b| a.id.cmp(&b.id));

    // Edges: reporting lines, department membership (person->dept->org), and
    // agent ownership. No "uses" edges exist — there are no tools in the
    // corpus, only owned agents; absence is honest (we never invent an edge).
    let mut edges: Vec<GraphEdge> = Vec::new();
    for person in &graph_people {
        edges.push(GraphEdge {
            from: person.id.clone(),
            kind: "member_of".to_string(),
            to: person.department_id.clone(),
        });
        if let Some(manager) = manager_of.get(person.id.as_str()) {
            edges.push(GraphEdge {
                from: person.id.clone(),
                kind: "reports_to".to_string(),
                to: (*manager).to_string(),
            });
        }
    }
    for dept in &departments {
        if known_depts.contains_key(dept.id.as_str()) {
            edges.push(GraphEdge {
                from: dept.id.clone(),
                kind: "member_of".to_string(),
                to: ORG_NODE_ID.to_string(),
            });
        }
    }
    for agent in &company.agents {
        edges.push(GraphEdge {
            from: agent.owner_user_id.clone(),
            kind: "owns_agent".to_string(),
            to: agent.id.clone(),
        });
    }

    let response = GraphResponse {
        actor_id: actor.to_string(),
        center,
        departments,
        edges,
        people: graph_people,
        snapshot_version: state.snapshot_version.clone(),
        tools,
    };
    canonical_json_bytes(&response)
        .map(Some)
        .map_err(AskError::Internal)
}
