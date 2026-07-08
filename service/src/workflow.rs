//! Read-only project workflow projection.
//!
//! This is an execution surface, not a graph expansion: every item here is
//! projected from an existing governed fact (lane box, accepted agent box, or
//! access-request ledger row). It deliberately carries no evidence rows and no
//! document ids.

use std::collections::BTreeSet;

use anyhow::{Context, Result};
use retrieval::index::canonical_json_bytes;
use serde::Serialize;

use crate::access_requests::{AccessRequest, STATUS_PENDING};
use crate::answer::AskError;
use crate::lane::{LaneGraph, ProvenanceNode};
use crate::lens::load_subject_artifact;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowProvenance {
    pub capability: ProvenanceNode,
    pub initiative: ProvenanceNode,
    pub strategy: ProvenanceNode,
    pub workflow: ProvenanceNode,
}

#[derive(Debug, Serialize)]
pub struct WorkflowItem {
    pub capability_id: String,
    pub dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub item_id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approver_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub provenance: WorkflowProvenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requester_id: Option<String>,
    pub snapshot_version: String,
    pub status: String,
    pub title: String,
    // SHOWCASE-III: present ONLY on materialized-proposal items. Absent on every
    // existing item (skip_serializing_if=None) → the fixture/lane/access-request
    // items serialize byte-identically.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchors: Option<Vec<crate::proposals::AnchorView>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources_outside_view: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectWorkflowResponse {
    pub actor_id: String,
    pub capability_id: String,
    pub demo_identity_mode: bool,
    pub items: Vec<WorkflowItem>,
    pub provenance: WorkflowProvenance,
    pub snapshot_version: String,
}

fn provenance_for(graph: &LaneGraph, capability_id: &str) -> Result<Option<WorkflowProvenance>> {
    let Some(capability) = graph.capabilities.iter().find(|c| c.id == capability_id) else {
        return Ok(None);
    };
    let workflow = graph
        .workflows
        .iter()
        .find(|w| w.id == capability.workflow_id)
        .context("capability references an unknown workflow")?;
    let initiative = graph
        .initiatives
        .iter()
        .find(|i| i.id == workflow.initiative_id)
        .context("workflow references an unknown initiative")?;
    let strategy = graph
        .strategies
        .iter()
        .find(|s| s.id == initiative.strategy_id)
        .context("initiative references an unknown strategy")?;

    Ok(Some(WorkflowProvenance {
        capability: ProvenanceNode {
            id: capability.id.clone(),
            name: capability.name.clone(),
        },
        initiative: ProvenanceNode {
            id: initiative.id.clone(),
            name: initiative.name.clone(),
        },
        strategy: ProvenanceNode {
            id: strategy.id.clone(),
            name: strategy.name.clone(),
        },
        workflow: ProvenanceNode {
            id: workflow.id.clone(),
            name: workflow.name.clone(),
        },
    }))
}

fn access_request_capability(request: &AccessRequest) -> &str {
    request.target.capability_id()
}

fn access_request_matches(request: &AccessRequest, capability_id: &str, snapshot: &str) -> bool {
    request.snapshot_version == snapshot && access_request_capability(request) == capability_id
}

fn actor_has_project_context(state: &AppState, actor: &str, capability_id: &str) -> bool {
    if state
        .lane_seeds
        .get(actor)
        .map(|seeds| seeds.iter().any(|seed| seed.capability.id == capability_id))
        .unwrap_or(false)
    {
        return true;
    }

    if state
        .lane_boxes
        .as_ref()
        .map(|store| {
            store.accepted_for(actor).into_iter().any(|record| {
                record.snapshot_version == state.snapshot_version
                    && record.capability_id == capability_id
            })
        })
        .unwrap_or(false)
    {
        return true;
    }

    if state
        .access_requests
        .as_ref()
        .map(|store| {
            store
                .requested_by(actor)
                .into_iter()
                .chain(store.inbox_for(actor))
                .any(|request| {
                    access_request_matches(&request, capability_id, &state.snapshot_version)
                        && request.status == STATUS_PENDING
                })
        })
        .unwrap_or(false)
    {
        return true;
    }

    state
        .access_grants
        .as_ref()
        .map(|store| store.has_active_read_for(actor, capability_id, &state.snapshot_version))
        .unwrap_or(false)
}

fn access_request_item(request: AccessRequest, provenance: &WorkflowProvenance) -> WorkflowItem {
    WorkflowItem {
        capability_id: access_request_capability(&request).to_string(),
        dependencies: Vec::new(),
        agent_id: None,
        item_id: request.request_id,
        kind: "access_request".to_string(),
        approver_id: Some(request.approver_id),
        owner_id: None,
        provenance: provenance.clone(),
        requester_id: Some(request.requester_id),
        snapshot_version: request.snapshot_version,
        status: request.status,
        title: format!("Access request for {}", provenance.capability.name),
        description: None,
        anchors: None,
        sources_outside_view: None,
        proposal_id: None,
    }
}

fn accepted_box_blocked(state: &AppState, actor: &str, citations: &[String]) -> Result<bool> {
    let entries = load_subject_artifact(state, actor)?.unwrap_or_default();
    let allow: BTreeSet<&str> = entries
        .iter()
        .map(|entry| entry.document_id.as_str())
        .collect();
    Ok(citations
        .iter()
        .map(String::as_str)
        .filter(|document_id| allow.contains(document_id))
        .any(|document_id| {
            entries
                .iter()
                .find(|entry| entry.document_id == document_id)
                .map(|entry| {
                    entry.superseded == Some(true)
                        && !entry
                            .effective_successor
                            .as_ref()
                            .map(|successor| allow.contains(successor.as_str()))
                            .unwrap_or(false)
                })
                .unwrap_or(false)
        }))
}

fn status_for(blocked: bool, stored: Option<String>) -> String {
    if blocked {
        "blocked".to_string()
    } else {
        stored.unwrap_or_else(|| "candidate".to_string())
    }
}

fn project_item_rank(item: &WorkflowItem) -> (u8, &str) {
    let kind_rank = match item.kind.as_str() {
        "lane_box" => 0,
        "accepted_agent_box" => 1,
        "access_request" => 2,
        _ => 9,
    };
    (kind_rank, item.item_id.as_str())
}

/// GET /workflow/project/{capability_id}.
///
/// `Ok(None)` means the world has no BRM, the actor is unknown/no-standing, the
/// capability id is not real, or the actor has no native/pending/granted
/// project context for that capability. A real, authorized capability can
/// return an empty item list; the frontend renders that as an honest
/// unavailable execution state, not a fabricated roadmap.
pub fn project_workflow_view(
    state: &AppState,
    actor: &str,
    capability_id: &str,
) -> Result<Option<Vec<u8>>, AskError> {
    let capability_id = capability_id.trim();
    if capability_id.is_empty() {
        return Err(AskError::BadRequest(
            "capability id must not be empty".to_string(),
        ));
    }
    if !state.identity.is_known(actor) {
        return Ok(None);
    }
    if load_subject_artifact(state, actor)
        .map_err(AskError::Internal)?
        .is_none()
    {
        return Ok(None);
    }
    let Some(graph) = &state.lane_graph else {
        return Ok(None);
    };
    let Some(provenance) = provenance_for(graph, capability_id).map_err(AskError::Internal)? else {
        return Ok(None);
    };
    if !actor_has_project_context(state, actor, capability_id) {
        return Ok(None);
    }

    let mut items = Vec::new();
    if let Some(seeds) = state.lane_seeds.get(actor) {
        for seed in seeds
            .iter()
            .filter(|seed| seed.capability.id == capability_id)
        {
            let stored = state
                .lane_boxes
                .as_ref()
                .and_then(|store| store.status_of(&seed.box_id));
            items.push(WorkflowItem {
                capability_id: seed.capability.id.clone(),
                dependencies: seed.blocked_by.clone(),
                agent_id: None,
                item_id: seed.box_id.clone(),
                kind: "lane_box".to_string(),
                approver_id: None,
                owner_id: Some(actor.to_string()),
                provenance: WorkflowProvenance {
                    capability: seed.capability.clone(),
                    initiative: seed.provenance.initiative.clone(),
                    strategy: seed.provenance.strategy.clone(),
                    workflow: seed.provenance.workflow.clone(),
                },
                requester_id: None,
                snapshot_version: state.snapshot_version.clone(),
                status: status_for(seed.blocked, stored),
                title: seed.capability.name.clone(),
                description: None,
                anchors: None,
                sources_outside_view: None,
                proposal_id: None,
            });
        }
    }

    if let Some(store) = &state.lane_boxes {
        let mut accepted = store.accepted_for(actor);
        accepted.sort_by_key(|record| record.created_ordinal);
        for record in accepted.into_iter().filter(|record| {
            record.snapshot_version == state.snapshot_version
                && record.capability_id == capability_id
        }) {
            let stored = store.status_of(&record.box_id);
            let blocked = accepted_box_blocked(state, actor, &record.citations)
                .map_err(AskError::Internal)?;
            items.push(WorkflowItem {
                capability_id: record.capability_id,
                dependencies: Vec::new(),
                agent_id: Some(record.agent_id),
                item_id: record.box_id,
                kind: "accepted_agent_box".to_string(),
                approver_id: None,
                owner_id: Some(record.principal),
                provenance: provenance.clone(),
                requester_id: None,
                snapshot_version: record.snapshot_version,
                status: status_for(blocked, stored),
                title: record.standing_query,
                description: None,
                anchors: None,
                sources_outside_view: None,
                proposal_id: None,
            });
        }
    }

    // SHOWCASE-III: the 4th merge source — APPROVED + materialized proposals for
    // this capability become "planned" lane_box items. Their anchors are S4-
    // redacted for the VIEWER (the requesting actor): the proposer sees full
    // anchors; anyone whose scope lacks a doc sees only the withheld marker.
    if let Some(store) = &state.wf_proposals {
        for proposal in store.approved_for(capability_id, &state.snapshot_version) {
            let box_views = crate::proposals::redact_boxes_for(state, actor, &proposal.boxes)
                .map_err(AskError::Internal)?;
            for view in box_views {
                items.push(WorkflowItem {
                    capability_id: proposal.capability_id.clone(),
                    dependencies: Vec::new(),
                    agent_id: None,
                    item_id: format!("{}#{}", proposal.proposal_id, view.box_index),
                    kind: "lane_box".to_string(),
                    approver_id: None,
                    owner_id: Some(proposal.proposer_id.clone()),
                    provenance: provenance.clone(),
                    requester_id: None,
                    snapshot_version: proposal.snapshot_version.clone(),
                    status: "planned".to_string(),
                    title: view.title.clone(),
                    description: Some(view.description.clone()),
                    sources_outside_view: Some(view.sources_outside_view),
                    proposal_id: Some(proposal.proposal_id.clone()),
                    anchors: Some(view.anchors),
                });
            }
        }
    }

    if let Some(store) = &state.access_requests {
        let mut seen = BTreeSet::new();
        let request_rows = store
            .requested_by(actor)
            .into_iter()
            .chain(store.inbox_for(actor));
        for request in request_rows {
            if request.snapshot_version != state.snapshot_version {
                continue;
            }
            if access_request_capability(&request) != capability_id {
                continue;
            }
            if seen.insert(request.request_id.clone()) {
                items.push(access_request_item(request, &provenance));
            }
        }
    }

    items.sort_by(|a, b| project_item_rank(a).cmp(&project_item_rank(b)));
    let response = ProjectWorkflowResponse {
        actor_id: actor.to_string(),
        capability_id: capability_id.to_string(),
        demo_identity_mode: true,
        items,
        provenance,
        snapshot_version: state.snapshot_version.clone(),
    };
    canonical_json_bytes(&response)
        .map(Some)
        .map_err(AskError::Internal)
}
