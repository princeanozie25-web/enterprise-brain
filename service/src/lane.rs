//! AP-6: the Lane — the v4a Workflow Surface, DISPLAY ONLY. "v4a (display)
//! earns trust cheaply; v4b (act) spends it." Nothing in this module acts.
//!
//! THE V4A INVARIANTS, AS CODE:
//! 1. `EffectClass` carries both vocabulary words, but the box constructor
//!    REFUSES `side_effecting` — the amber class is unconstructable here,
//!    preserving the v4b door without opening it.
//! 2. The lane is SELF-ONLY BY CONSTRUCTION: `lane_view` takes the header
//!    actor and nothing else; there is no subject parameter to misuse.
//! 3. The manager rollup is a separate, structurally anonymous view: status
//!    counts by capability at the N=5 floor, no names, no per-person
//!    fields, and a fixed honesty statement naming what it cannot see.
//! 4. No box self-completes; no box self-injects: status changes are human
//!    acts (audited before effect); agent proposals become boxes only on
//!    the owner's explicit accept.
//! 5. SOP FAIL-CLOSED: a box bound to a superseded SOP whose effective
//!    version is outside the worker's scope renders BLOCKED — it never
//!    proceeds on a withdrawn procedure and never hints at the successor.
//! 6. Every box carries its honesty line: the actor's scope statement,
//!    phrased as what the scope IS, never as counts of what it hides.
//!
//! DERIVED ASSIGNMENTS (the data ruling — /synth is frozen): at startup,
//! for each human principal, take the capabilities whose realizing
//! documents' departments match the principal's department AND where the
//! principal has >=1 visible realizing doc; rank by visible-doc count
//! (tie-break capability id ascending); cap at 8 boxes per person. Every
//! derived box carries derived: true. Agents get no lane.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use retrieval::envelope::ScopeStatement;
use retrieval::index::{canonical_json_bytes, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::answer::AskError;
use crate::atlas::BrmLite;
use crate::diff::DocRow;
use crate::lens::load_subject_artifact;
use crate::{humanize, AppState, DocMeta};

/// The rollup's fixed honesty statement, byte-exact (AW-5).
pub const ROLLUP_HONESTY: &str = "This view shows assignment status by capability. \
It cannot see activity, time, load, or any individual.";

pub const STATUSES: [&str; 5] = ["active", "blocked", "candidate", "dismissed", "done"];

// ---------------------------------------------------------------------------
// Invariant 1: the effect-class vocabulary with a refusing constructor
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EffectClass {
    #[serde(rename = "read_only")]
    ReadOnly,
    /// In the vocabulary so v4b's door stays visible — and unconstructable
    /// into any box in this build, proven by AW-3.
    #[serde(rename = "side_effecting")]
    SideEffecting,
}

// ---------------------------------------------------------------------------
// The graph view the lane derives from (public so the AW harness can build
// synthetic graphs; the production instance converts from the atlas's
// validated, pinned BRM)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LaneGraph {
    pub capabilities: Vec<LaneCapability>,
    pub initiatives: Vec<LaneInitiative>,
    pub strategies: Vec<LaneStrategy>,
    pub workflows: Vec<LaneWorkflow>,
}

#[derive(Debug, Clone)]
pub struct LaneCapability {
    pub document_ids: Vec<String>,
    pub id: String,
    pub name: String,
    pub workflow_id: String,
}

#[derive(Debug, Clone)]
pub struct LaneWorkflow {
    pub id: String,
    pub initiative_id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct LaneInitiative {
    pub id: String,
    pub name: String,
    pub strategy_id: String,
    pub workflow_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LaneStrategy {
    pub id: String,
    pub name: String,
}

impl LaneGraph {
    pub(crate) fn from_brm(brm: &BrmLite) -> LaneGraph {
        LaneGraph {
            capabilities: brm
                .capabilities
                .iter()
                .map(|c| LaneCapability {
                    document_ids: c.document_ids.clone(),
                    id: c.id.clone(),
                    name: c.name.clone(),
                    workflow_id: c.workflow_id.clone(),
                })
                .collect(),
            initiatives: brm
                .initiatives
                .iter()
                .map(|i| LaneInitiative {
                    id: i.id.clone(),
                    name: i.name.clone(),
                    strategy_id: i.strategy_id.clone(),
                    workflow_ids: i.workflow_ids.clone(),
                })
                .collect(),
            strategies: brm
                .strategies
                .iter()
                .map(|s| LaneStrategy {
                    id: s.id.clone(),
                    name: s.name.clone(),
                })
                .collect(),
            workflows: brm
                .workflows
                .iter()
                .map(|w| LaneWorkflow {
                    id: w.id.clone(),
                    initiative_id: w.initiative_id.clone(),
                    name: w.name.clone(),
                })
                .collect(),
        }
    }
}

/// The artifact facts the derivation consumes (harvested in the startup
/// sweep that already hash-verifies every artifact).
#[derive(Debug, Clone)]
pub struct LaneEntryFacts {
    pub document_id: String,
    pub superseded: Option<bool>,
    pub effective_successor: Option<String>,
}

// ---------------------------------------------------------------------------
// Shapes (canonical JSON, sorted keys)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceNode {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Provenance {
    pub initiative: ProvenanceNode,
    pub strategy: ProvenanceNode,
    pub workflow: ProvenanceNode,
}

#[derive(Debug, Serialize)]
pub struct Deviation {
    pub kind: String,
}

#[derive(Debug, Serialize)]
pub struct LaneBox {
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
    pub box_id: String,
    pub capability: ProvenanceNode,
    pub derived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deviation: Option<Deviation>,
    pub effect_class: EffectClass,
    pub evidence: Vec<DocRow>,
    pub honesty: ScopeStatement,
    pub provenance: Provenance,
    pub snapshot_version: String,
    pub sop_state: String,
    pub status: String,
    pub why: String,
}

/// Everything a box needs except status/honesty/snapshot (request-time).
#[derive(Debug, Clone, Serialize)]
pub struct BoxSeed {
    pub blocked: bool,
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
    pub box_id: String,
    pub capability: ProvenanceNode,
    pub evidence: Vec<DocRow>,
    pub provenance: Provenance,
    pub why: String,
}

pub struct LaneBoxParts {
    pub blocked: bool,
    pub blocked_by: Vec<String>,
    pub blocks: Vec<String>,
    pub box_id: String,
    pub capability: ProvenanceNode,
    pub derived: bool,
    pub evidence: Vec<DocRow>,
    pub honesty: ScopeStatement,
    pub provenance: Provenance,
    pub snapshot_version: String,
    pub status: String,
    pub why: String,
}

impl LaneBox {
    /// THE ONLY WAY a box exists. Invariant 1: the amber class refuses.
    pub fn try_new(parts: LaneBoxParts, effect_class: EffectClass) -> Result<LaneBox> {
        if effect_class == EffectClass::SideEffecting {
            bail!("v4a is display-only: the side_effecting class is unconstructable in this build");
        }
        let (sop_state, deviation) = if parts.blocked {
            (
                "blocked_superseded".to_string(),
                Some(Deviation {
                    kind: "superseded_sop".to_string(),
                }),
            )
        } else {
            ("current".to_string(), None)
        };
        Ok(LaneBox {
            blocked_by: parts.blocked_by,
            blocks: parts.blocks,
            box_id: parts.box_id,
            capability: parts.capability,
            derived: parts.derived,
            deviation,
            effect_class,
            evidence: parts.evidence,
            honesty: parts.honesty,
            provenance: parts.provenance,
            snapshot_version: parts.snapshot_version,
            sop_state,
            status: parts.status,
            why: parts.why,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct LaneResponse {
    /// AR-1: the worker's own directory card (the lane is self-only, so this
    /// is the actor's identity header; display only). Absent with no
    /// humanization layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<humanize::PersonCard>,
    pub actor_id: String,
    pub boxes: Vec<LaneBox>,
    pub snapshot_version: String,
}

#[derive(Debug, Serialize)]
pub struct InboxPreview {
    pub agent_id: String,
    pub citations: Vec<String>,
    pub proposal_id: String,
    pub standing_query: String,
}

#[derive(Debug, Serialize)]
pub struct InboxResponse {
    pub actor_id: String,
    pub proposals: Vec<InboxPreview>,
    pub snapshot_version: String,
}

#[derive(Debug, Serialize)]
pub struct RollupRow {
    pub capability_id: String,
    pub status_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Serialize)]
pub struct RollupResponse {
    pub capabilities: Vec<RollupRow>,
    pub honesty: String,
    pub snapshot_version: String,
}

// ---------------------------------------------------------------------------
// Derivation
// ---------------------------------------------------------------------------

fn sha16(preimage: &str) -> String {
    sha256_hex(preimage.as_bytes())[..16].to_string()
}

/// One human's derived lane. Pure over its inputs (AW-4/AW-7 feed synthetic
/// ones); the rule verbatim in the module docs and the README.
pub fn seeds_for_human(
    principal: &str,
    department: &str,
    entries: &[LaneEntryFacts],
    docs: &BTreeMap<String, DocMeta>,
    graph: &LaneGraph,
    snapshot_version: &str,
) -> Result<Vec<BoxSeed>> {
    let allow: BTreeSet<&str> = entries.iter().map(|e| e.document_id.as_str()).collect();
    let facts: BTreeMap<&str, &LaneEntryFacts> = entries
        .iter()
        .map(|e| (e.document_id.as_str(), e))
        .collect();
    let workflows: BTreeMap<&str, &LaneWorkflow> =
        graph.workflows.iter().map(|w| (w.id.as_str(), w)).collect();
    let initiatives: BTreeMap<&str, &LaneInitiative> = graph
        .initiatives
        .iter()
        .map(|i| (i.id.as_str(), i))
        .collect();
    let strategies: BTreeMap<&str, &LaneStrategy> = graph
        .strategies
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();

    // Candidates: department-matched capabilities with visible evidence.
    let mut candidates: Vec<(&LaneCapability, Vec<&str>)> = Vec::new();
    for capability in &graph.capabilities {
        let mut department_match = false;
        for document_id in &capability.document_ids {
            let meta = docs
                .get(document_id)
                .context("capability maps a document the corpus does not carry")?;
            if meta.department == department {
                department_match = true;
            }
        }
        if !department_match {
            continue;
        }
        let mut visible: Vec<&str> = capability
            .document_ids
            .iter()
            .map(String::as_str)
            .filter(|d| allow.contains(d))
            .collect();
        if visible.is_empty() {
            continue;
        }
        visible.sort_unstable();
        candidates.push((capability, visible));
    }
    // Rank by visible-doc count desc, tie-break capability id asc; cap 8.
    candidates.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.id.cmp(&b.0.id)));
    candidates.truncate(8);

    let mut seeds = Vec::with_capacity(candidates.len());
    for (capability, visible) in &candidates {
        let workflow = workflows
            .get(capability.workflow_id.as_str())
            .context("capability references an unknown workflow")?;
        let initiative = initiatives
            .get(workflow.initiative_id.as_str())
            .context("workflow references an unknown initiative")?;
        let strategy = strategies
            .get(initiative.strategy_id.as_str())
            .context("initiative references an unknown strategy")?;

        let mut blocked = false;
        let mut evidence = Vec::with_capacity(visible.len());
        for document_id in visible {
            let meta = docs
                .get(*document_id)
                .context("capability maps a document the corpus does not carry")?;
            let fact = facts
                .get(document_id)
                .context("visible document missing its artifact facts")?;
            let superseded = fact.superseded == Some(true);
            // Invariant 5 + R-13: a withdrawn procedure whose effective
            // version sits outside the worker's scope blocks the box and
            // the successor id never serializes.
            let successor_in_scope = fact
                .effective_successor
                .as_ref()
                .map(|s| allow.contains(s.as_str()))
                .unwrap_or(false);
            if superseded && !successor_in_scope {
                blocked = true;
            }
            let effective_successor = if superseded && successor_in_scope {
                fact.effective_successor.clone()
            } else {
                None
            };
            evidence.push(DocRow {
                document_id: (*document_id).to_string(),
                effective_successor,
                sensitivity: meta.sensitivity.clone(),
                superseded: superseded.then_some(true),
                title: meta.title.clone(),
            });
        }
        seeds.push(BoxSeed {
            blocked,
            blocked_by: Vec::new(),
            blocks: Vec::new(),
            box_id: sha16(&format!(
                "{principal}\n{}\n{snapshot_version}",
                capability.id
            )),
            capability: ProvenanceNode {
                id: capability.id.clone(),
                name: capability.name.clone(),
            },
            evidence,
            provenance: Provenance {
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
            },
            why: format!(
                "Part of {} under {}, serving {}.",
                workflow.name, initiative.name, strategy.name
            ),
        });
    }

    // blocked_by / blocks from the BRM workflow ordering: the workflow
    // sequence within an initiative (workflow ids ascending), joined over
    // THIS person's own boxes only.
    let capability_workflow: BTreeMap<&str, (&str, &str)> = candidates
        .iter()
        .map(|(c, _)| {
            let wf = workflows[c.workflow_id.as_str()];
            (c.id.as_str(), (wf.id.as_str(), wf.initiative_id.as_str()))
        })
        .collect();
    let by_initiative_workflow: BTreeMap<(&str, &str), Vec<String>> = {
        let mut map: BTreeMap<(&str, &str), Vec<String>> = BTreeMap::new();
        for seed in &seeds {
            let (wf, init) = capability_workflow[seed.capability.id.as_str()];
            map.entry((init, wf)).or_default().push(seed.box_id.clone());
        }
        map
    };
    let sequences: BTreeMap<&str, Vec<&str>> = initiatives
        .values()
        .map(|i| {
            let mut sequence: Vec<&str> = i.workflow_ids.iter().map(String::as_str).collect();
            sequence.sort_unstable();
            (i.id.as_str(), sequence)
        })
        .collect();
    for seed in &mut seeds {
        let (wf, init) = capability_workflow[seed.capability.id.as_str()];
        let sequence = &sequences[init];
        let position = sequence.iter().position(|w| *w == wf).unwrap_or(0);
        if position > 0 {
            if let Some(prev) = by_initiative_workflow.get(&(init, sequence[position - 1])) {
                seed.blocked_by = prev.clone();
            }
        }
        if position + 1 < sequence.len() {
            if let Some(next) = by_initiative_workflow.get(&(init, sequence[position + 1])) {
                seed.blocks = next.clone();
            }
        }
    }
    Ok(seeds)
}

#[derive(Debug, Deserialize)]
struct LaneCompany {
    people: Vec<LanePerson>,
}

#[derive(Debug, Deserialize)]
struct LanePerson {
    id: String,
    department: String,
}

/// Startup derivation over every human principal — deterministic from the
/// hash-verified inputs only (AW-7: two startups, byte-identical lanes).
pub(crate) fn derive_lanes(
    fixtures_dir: &Path,
    company_sha256: &str,
    docs: &BTreeMap<String, DocMeta>,
    graph: &LaneGraph,
    entries: &BTreeMap<String, Vec<LaneEntryFacts>>,
    snapshot_version: &str,
) -> Result<BTreeMap<String, Vec<BoxSeed>>> {
    let path = fixtures_dir.join("company.json");
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if sha256_hex(&bytes) != company_sha256 {
        bail!("company.json does not match the M1-pinned hash; refusing");
    }
    let company: LaneCompany = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} fails parse", path.display()))?;

    let mut lanes = BTreeMap::new();
    for person in &company.people {
        let Some(person_entries) = entries.get(&person.id) else {
            continue;
        };
        let seeds = seeds_for_human(
            &person.id,
            &person.department,
            person_entries,
            docs,
            graph,
            snapshot_version,
        )?;
        if !seeds.is_empty() {
            lanes.insert(person.id.clone(), seeds);
        }
    }
    Ok(lanes)
}

// ---------------------------------------------------------------------------
// The box store (append-only JSONL; the M4 pattern; ordinal time only)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedBox {
    pub agent_id: String,
    pub box_id: String,
    pub capability_id: String,
    pub citations: Vec<String>,
    pub created_ordinal: u64,
    pub principal: String,
    pub proposal_id: String,
    pub snapshot_version: String,
    pub standing_query: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
enum LaneEvent {
    StatusChanged {
        box_id: String,
        ordinal: u64,
        principal: String,
        to: String,
    },
    Accepted {
        record: AcceptedBox,
    },
}

struct BoxState {
    statuses: BTreeMap<String, String>,
    accepted: BTreeMap<String, AcceptedBox>,
    next_ordinal: u64,
}

pub struct BoxStore {
    path: PathBuf,
    state: Mutex<BoxState>,
}

impl BoxStore {
    pub fn open(dir: &Path) -> Result<BoxStore> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("cannot create box store dir {}", dir.display()))?;
        let path = dir.join("boxes.jsonl");
        let mut state = BoxState {
            statuses: BTreeMap::new(),
            accepted: BTreeMap::new(),
            next_ordinal: 0,
        };
        if path.exists() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let event: LaneEvent =
                    serde_json::from_str(line).context("box store event fails parse")?;
                state.next_ordinal += 1;
                match event {
                    LaneEvent::StatusChanged { box_id, to, .. } => {
                        state.statuses.insert(box_id, to);
                    }
                    LaneEvent::Accepted { record } => {
                        state.accepted.insert(record.box_id.clone(), record);
                    }
                }
            }
        }
        Ok(BoxStore {
            path,
            state: Mutex::new(state),
        })
    }

    fn append(&self, event: &LaneEvent) -> Result<()> {
        let bytes = canonical_json_bytes(event)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("cannot open {}", self.path.display()))?;
        file.write_all(&bytes)
            .with_context(|| format!("cannot append to {}", self.path.display()))?;
        file.sync_data()
            .with_context(|| format!("cannot sync {}", self.path.display()))?;
        Ok(())
    }

    pub fn status_of(&self, box_id: &str) -> Option<String> {
        self.state
            .lock()
            .expect("box store mutex")
            .statuses
            .get(box_id)
            .cloned()
    }

    pub fn record_status(&self, box_id: &str, principal: &str, to: &str) -> Result<()> {
        let mut state = self.state.lock().expect("box store mutex");
        let event = LaneEvent::StatusChanged {
            box_id: box_id.to_string(),
            ordinal: state.next_ordinal,
            principal: principal.to_string(),
            to: to.to_string(),
        };
        self.append(&event)?;
        state.next_ordinal += 1;
        state.statuses.insert(box_id.to_string(), to.to_string());
        Ok(())
    }

    pub fn record_accepted(&self, mut record: AcceptedBox) -> Result<AcceptedBox> {
        let mut state = self.state.lock().expect("box store mutex");
        record.created_ordinal = state.next_ordinal;
        let event = LaneEvent::Accepted {
            record: record.clone(),
        };
        self.append(&event)?;
        state.next_ordinal += 1;
        state.accepted.insert(record.box_id.clone(), record.clone());
        Ok(record)
    }

    pub fn accepted_for(&self, principal: &str) -> Vec<AcceptedBox> {
        let state = self.state.lock().expect("box store mutex");
        state
            .accepted
            .values()
            .filter(|r| r.principal == principal)
            .cloned()
            .collect()
    }

    pub fn all_accepted(&self) -> Vec<AcceptedBox> {
        let state = self.state.lock().expect("box store mutex");
        state.accepted.values().cloned().collect()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn effective_status(blocked: bool, stored: Option<String>) -> String {
    if blocked {
        // Invariant 5: blocked wins; the store never legally holds a
        // transition for a blocked box.
        "blocked".to_string()
    } else {
        stored.unwrap_or_else(|| "candidate".to_string())
    }
}

fn render_seed(state: &AppState, actor: &str, seed: &BoxSeed) -> Result<LaneBox> {
    let stored = state
        .lane_boxes
        .as_ref()
        .and_then(|store| store.status_of(&seed.box_id));
    LaneBox::try_new(
        LaneBoxParts {
            blocked: seed.blocked,
            blocked_by: seed.blocked_by.clone(),
            blocks: seed.blocks.clone(),
            box_id: seed.box_id.clone(),
            capability: seed.capability.clone(),
            derived: true,
            evidence: seed.evidence.clone(),
            honesty: state.identity.statement_for(actor),
            provenance: seed.provenance.clone(),
            snapshot_version: state.snapshot_version.clone(),
            status: effective_status(seed.blocked, stored),
            why: seed.why.clone(),
        },
        EffectClass::ReadOnly,
    )
}

/// Renders an inbox-born box: evidence is the proposal's citations
/// re-checked against the CURRENT allowlist (scope at render, R-13 as
/// everywhere), provenance from the capability it bound to at accept time.
fn render_accepted(
    state: &AppState,
    actor: &str,
    record: &AcceptedBox,
    allow: &BTreeSet<&str>,
    facts: &BTreeMap<&str, &LaneEntryFacts>,
    graph: &LaneGraph,
) -> Result<LaneBox> {
    let capability = graph
        .capabilities
        .iter()
        .find(|c| c.id == record.capability_id)
        .context("accepted box bound to an unknown capability")?;
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

    let mut citations: Vec<&str> = record
        .citations
        .iter()
        .map(String::as_str)
        .filter(|d| allow.contains(d))
        .collect();
    citations.sort_unstable();
    citations.dedup();
    let mut blocked = false;
    let mut evidence = Vec::with_capacity(citations.len());
    for document_id in citations {
        let Some(meta) = state.docs.get(document_id) else {
            bail!("accepted box cites a document the corpus does not carry");
        };
        let fact = facts.get(document_id);
        let superseded = fact.map(|f| f.superseded == Some(true)).unwrap_or(false);
        let successor_in_scope = fact
            .and_then(|f| f.effective_successor.as_ref())
            .map(|s| allow.contains(s.as_str()))
            .unwrap_or(false);
        if superseded && !successor_in_scope {
            blocked = true;
        }
        evidence.push(DocRow {
            document_id: document_id.to_string(),
            effective_successor: if superseded && successor_in_scope {
                fact.and_then(|f| f.effective_successor.clone())
            } else {
                None
            },
            sensitivity: meta.sensitivity.clone(),
            superseded: superseded.then_some(true),
            title: meta.title.clone(),
        });
    }
    let stored = state
        .lane_boxes
        .as_ref()
        .and_then(|store| store.status_of(&record.box_id));
    LaneBox::try_new(
        LaneBoxParts {
            blocked,
            blocked_by: Vec::new(),
            blocks: Vec::new(),
            box_id: record.box_id.clone(),
            capability: ProvenanceNode {
                id: capability.id.clone(),
                name: capability.name.clone(),
            },
            derived: false,
            evidence,
            honesty: state.identity.statement_for(actor),
            provenance: Provenance {
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
            },
            snapshot_version: state.snapshot_version.clone(),
            status: effective_status(blocked, stored),
            why: format!(
                "Accepted from {}: {}",
                record.agent_id, record.standing_query
            ),
        },
        EffectClass::ReadOnly,
    )
}

// ---------------------------------------------------------------------------
// Views
// ---------------------------------------------------------------------------

/// GET /lane — SELF-ONLY BY CONSTRUCTION (invariant 2): the actor is the
/// only input. Humans get their derived + accepted boxes; agents and
/// unknown principals get the same empty shape (boxes are human work).
/// `Ok(None)` = this world has no BRM, THE one 404.
pub fn lane_view(state: &AppState, actor: &str) -> Result<Option<Vec<u8>>, AskError> {
    let Some(graph) = &state.lane_graph else {
        return Ok(None);
    };
    // The per-request staleness gate every governed read uses: the actor's
    // artifact re-verifies against its M1 hash (unknown actor -> no
    // standing -> the empty lane).
    let entries = load_subject_artifact(state, actor)
        .map_err(AskError::Internal)?
        .unwrap_or_default();
    let allow: BTreeSet<&str> = entries.iter().map(|e| e.document_id.as_str()).collect();
    let lane_facts: Vec<LaneEntryFacts> = entries
        .iter()
        .map(|e| LaneEntryFacts {
            document_id: e.document_id.clone(),
            superseded: e.superseded,
            effective_successor: e.effective_successor.clone(),
        })
        .collect();
    let facts: BTreeMap<&str, &LaneEntryFacts> = lane_facts
        .iter()
        .map(|f| (f.document_id.as_str(), f))
        .collect();

    let mut boxes = Vec::new();
    if let Some(seeds) = state.lane_seeds.get(actor) {
        for seed in seeds {
            boxes.push(render_seed(state, actor, seed).map_err(AskError::Internal)?);
        }
    }
    if let Some(store) = &state.lane_boxes {
        let mut accepted = store.accepted_for(actor);
        accepted.sort_by_key(|r| r.created_ordinal);
        for record in &accepted {
            if record.snapshot_version != state.snapshot_version {
                // Snapshot-pinned, fail closed: a box accepted under another
                // world does not render (the M4 staleness doctrine).
                continue;
            }
            boxes.push(
                render_accepted(state, actor, record, &allow, &facts, graph)
                    .map_err(AskError::Internal)?,
            );
        }
    }
    let response = LaneResponse {
        actor: humanize::card_for(state.people.as_deref(), actor),
        actor_id: actor.to_string(),
        boxes,
        snapshot_version: state.snapshot_version.clone(),
    };
    canonical_json_bytes(&response)
        .map(Some)
        .map_err(AskError::Internal)
}

pub enum StatusOutcome {
    Applied(Vec<u8>),
    NotFound,
    /// Illegal transition or blocked box: refused, ROWLESS (AW-8).
    Refused(String),
}

/// The whole transition law, public for the AW harness: candidate may
/// start or be set aside; active may finish; nothing else moves, and
/// nothing moves a blocked box.
pub fn transition_is_legal(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("candidate", "active") | ("candidate", "dismissed") | ("active", "done")
    )
}

/// POST /lane/box/{id}/status — a human act on the actor's OWN box,
/// audited before effect. Anyone else's box id is indistinguishable from a
/// nonexistent one.
pub fn status_change(
    state: &AppState,
    actor: &str,
    box_id: &str,
    to: &str,
) -> Result<StatusOutcome, AskError> {
    if state.lane_graph.is_none() {
        return Ok(StatusOutcome::NotFound);
    }
    if !["active", "done", "dismissed"].contains(&to) {
        return Ok(StatusOutcome::Refused(format!(
            "{to:?} is not a status a human can set"
        )));
    }
    let (Some(store), Some(audit_store)) = (&state.lane_boxes, &state.proposals) else {
        return Err(AskError::Internal(anyhow::anyhow!(
            "box status changes require the audit store (--state-dir); refusing"
        )));
    };

    // The actor's own box: a derived seed or an accepted record.
    let seed_blocked = state
        .lane_seeds
        .get(actor)
        .and_then(|seeds| seeds.iter().find(|s| s.box_id == box_id))
        .map(|s| s.blocked);
    let accepted = store
        .accepted_for(actor)
        .into_iter()
        .find(|r| r.box_id == box_id);
    let blocked = match (seed_blocked, &accepted) {
        (Some(blocked), _) => blocked,
        (None, Some(record)) => {
            // Re-derive the accepted box's blocked state from the current
            // artifact (same rule as render).
            let entries = load_subject_artifact(state, actor)
                .map_err(AskError::Internal)?
                .unwrap_or_default();
            let allow: BTreeSet<&str> = entries.iter().map(|e| e.document_id.as_str()).collect();
            record.citations.iter().any(|d| {
                entries
                    .iter()
                    .find(|e| &e.document_id == d)
                    .map(|e| {
                        e.superseded == Some(true)
                            && !e
                                .effective_successor
                                .as_ref()
                                .map(|s| allow.contains(s.as_str()))
                                .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
        }
        (None, None) => return Ok(StatusOutcome::NotFound),
    };
    if blocked {
        return Ok(StatusOutcome::Refused(
            "this box is blocked on a superseded procedure; no transition proceeds".to_string(),
        ));
    }
    let current = effective_status(false, store.status_of(box_id));
    if !transition_is_legal(&current, to) {
        return Ok(StatusOutcome::Refused(format!(
            "no legal transition from {current:?} to {to:?}"
        )));
    }

    // Audit BEFORE effect (action: box_status) — the human act is the
    // governed event.
    audit_store
        .audit("box_status", actor, &format!("{box_id}->{to}"), "allowed")
        .map_err(AskError::Internal)?;
    store
        .record_status(box_id, actor, to)
        .map_err(AskError::Internal)?;

    #[derive(Serialize)]
    struct StatusResponse {
        box_id: String,
        demo_identity_mode: bool,
        status: String,
    }
    canonical_json_bytes(&StatusResponse {
        box_id: box_id.to_string(),
        demo_identity_mode: true,
        status: to.to_string(),
    })
    .map(StatusOutcome::Applied)
    .map_err(AskError::Internal)
}

/// GET /lane/inbox — the actor's agents' PENDING, current-snapshot
/// proposals as candidate previews. A read-only join over the M4 store.
pub fn inbox_view(state: &AppState, actor: &str) -> Result<Option<Vec<u8>>, AskError> {
    let Some(store) = &state.proposals else {
        return Ok(None);
    };
    if state.lane_graph.is_none() {
        return Ok(None);
    }
    let mut proposals: Vec<InboxPreview> = store
        .owned_by(actor)
        .into_iter()
        .filter(|p| {
            p.status == crate::agent::proposals::STATUS_PENDING
                && p.snapshot_version == state.snapshot_version
        })
        .map(|p| InboxPreview {
            agent_id: p.agent_id,
            citations: p.finding.citations,
            proposal_id: p.proposal_id,
            standing_query: p.standing_query,
        })
        .collect();
    proposals.sort_by(|a, b| a.proposal_id.cmp(&b.proposal_id));
    let response = InboxResponse {
        actor_id: actor.to_string(),
        proposals,
        snapshot_version: state.snapshot_version.clone(),
    };
    canonical_json_bytes(&response)
        .map(Some)
        .map_err(AskError::Internal)
}

pub enum InboxOutcome {
    Done(Vec<u8>),
    NotFound,
    Forbidden,
    Conflict(String),
}

/// POST /lane/inbox/{id}/accept|dismiss — M4's authority pattern verbatim:
/// agent principals are STRUCTURALLY refused (403 + audit), only the owning
/// human decides, stale and decided proposals refuse. Accept binds the
/// proposal's evidence to its best-overlap capability and materializes a
/// candidate box; the proposal is approved through the EXISTING M4
/// machinery. Both audits write BEFORE both effects.
pub fn inbox_decide(
    state: &AppState,
    actor: &str,
    proposal_id: &str,
    accept: bool,
) -> Result<InboxOutcome, AskError> {
    use crate::agent::proposals::{STATUS_APPROVED, STATUS_PENDING, STATUS_REJECTED};

    let (Some(registry), Some(proposals), Some(boxes), Some(graph)) = (
        &state.agents,
        &state.proposals,
        &state.lane_boxes,
        &state.lane_graph,
    ) else {
        return Ok(InboxOutcome::NotFound);
    };
    let action = if accept { "box_accept" } else { "box_dismiss" };
    let audit = |outcome: &str| proposals.audit(action, actor, proposal_id, outcome);

    if registry.is_agent_principal(actor) {
        audit("refused_agent_principal").map_err(AskError::Internal)?;
        return Ok(InboxOutcome::Forbidden);
    }
    if !registry.is_human(actor) {
        audit("refused_unknown_principal").map_err(AskError::Internal)?;
        return Ok(InboxOutcome::Forbidden);
    }
    let Some(proposal) = proposals.get(proposal_id) else {
        audit("refused_not_found").map_err(AskError::Internal)?;
        return Ok(InboxOutcome::NotFound);
    };
    if proposal.owner_user_id != actor {
        audit("refused_not_owner").map_err(AskError::Internal)?;
        return Ok(InboxOutcome::Forbidden);
    }
    if proposal.snapshot_version != state.snapshot_version {
        audit("refused_stale").map_err(AskError::Internal)?;
        return Ok(InboxOutcome::Conflict(
            "stale proposal: re-run to refresh".to_string(),
        ));
    }
    if proposal.status != STATUS_PENDING {
        audit("refused_already_decided").map_err(AskError::Internal)?;
        return Ok(InboxOutcome::Conflict("already decided".to_string()));
    }

    if !accept {
        // Both audits before the one effect (the M4 decision row is the
        // lane's to write here — it drives the decision).
        audit("allowed").map_err(AskError::Internal)?;
        proposals
            .audit("proposal_reject", actor, proposal_id, "allowed")
            .map_err(AskError::Internal)?;
        proposals
            .decide(proposal_id, STATUS_REJECTED, actor, &state.snapshot_version)
            .map_err(AskError::Internal)?
            .map_err(|_| AskError::Internal(anyhow::anyhow!("decide refused after checks")))?;
        #[derive(Serialize)]
        struct DismissResponse {
            demo_identity_mode: bool,
            proposal_id: String,
            proposal_status: String,
        }
        return canonical_json_bytes(&DismissResponse {
            demo_identity_mode: true,
            proposal_id: proposal_id.to_string(),
            proposal_status: STATUS_REJECTED.to_string(),
        })
        .map(InboxOutcome::Done)
        .map_err(AskError::Internal);
    }

    // ACCEPT: bind to the capability whose mapped documents best overlap
    // the proposal's citations (deterministic: max overlap, tie-break id
    // ascending). Zero overlap cannot materialize a box.
    let citation_set: BTreeSet<&str> = proposal
        .finding
        .citations
        .iter()
        .map(String::as_str)
        .collect();
    let mut best: Option<(usize, &LaneCapability)> = None;
    for capability in &graph.capabilities {
        let overlap = capability
            .document_ids
            .iter()
            .filter(|d| citation_set.contains(d.as_str()))
            .count();
        if overlap == 0 {
            continue;
        }
        best = match best {
            None => Some((overlap, capability)),
            Some((count, current))
                if overlap > count || (overlap == count && capability.id < current.id) =>
            {
                Some((overlap, capability))
            }
            keep => keep,
        };
    }
    let Some((_, capability)) = best else {
        audit("refused_unmapped").map_err(AskError::Internal)?;
        return Ok(InboxOutcome::Conflict(
            "proposal evidence maps to no capability".to_string(),
        ));
    };

    // Both audits, then both effects (AW-6 orders them).
    audit("allowed").map_err(AskError::Internal)?;
    proposals
        .audit("proposal_approve", actor, proposal_id, "allowed")
        .map_err(AskError::Internal)?;
    proposals
        .decide(proposal_id, STATUS_APPROVED, actor, &state.snapshot_version)
        .map_err(AskError::Internal)?
        .map_err(|_| AskError::Internal(anyhow::anyhow!("decide refused after checks")))?;
    let record = boxes
        .record_accepted(AcceptedBox {
            agent_id: proposal.agent_id.clone(),
            box_id: sha16(&format!(
                "{actor}\n{proposal_id}\n{}",
                state.snapshot_version
            )),
            capability_id: capability.id.clone(),
            citations: proposal.finding.citations.clone(),
            created_ordinal: 0,
            principal: actor.to_string(),
            proposal_id: proposal_id.to_string(),
            snapshot_version: state.snapshot_version.clone(),
            standing_query: proposal.standing_query.clone(),
        })
        .map_err(AskError::Internal)?;

    #[derive(Serialize)]
    struct AcceptResponse {
        box_id: String,
        capability_id: String,
        demo_identity_mode: bool,
        proposal_id: String,
        proposal_status: String,
        status: String,
    }
    canonical_json_bytes(&AcceptResponse {
        box_id: record.box_id,
        capability_id: record.capability_id,
        demo_identity_mode: true,
        proposal_id: proposal_id.to_string(),
        proposal_status: STATUS_APPROVED.to_string(),
        status: "candidate".to_string(),
    })
    .map(InboxOutcome::Done)
    .map_err(AskError::Internal)
}

/// GET /lane/rollup — invariant 3. Status counts by capability over
/// capabilities with >=5 assigned principals ONLY; below the floor a
/// capability is ABSENT, because a rollup row about 3 people is a person
/// with extra steps. No names, no per-person fields, no activity — none is
/// collected anywhere to begin with.
pub fn rollup_view(state: &AppState) -> Result<Option<Vec<u8>>, AskError> {
    if state.lane_graph.is_none() {
        return Ok(None);
    }
    // capability -> (assigned principals, status -> count)
    let mut per_capability: BTreeMap<&str, (BTreeSet<&str>, BTreeMap<&str, u64>)> = BTreeMap::new();
    let stored_status = |box_id: &str| -> Option<String> {
        state
            .lane_boxes
            .as_ref()
            .and_then(|store| store.status_of(box_id))
    };
    for (principal, seeds) in &state.lane_seeds {
        for seed in seeds {
            let status = effective_status(seed.blocked, stored_status(&seed.box_id));
            let entry = per_capability
                .entry(seed.capability.id.as_str())
                .or_default();
            entry.0.insert(principal.as_str());
            *entry
                .1
                .entry(
                    STATUSES
                        .iter()
                        .find(|s| **s == status)
                        .copied()
                        .unwrap_or("candidate"),
                )
                .or_insert(0) += 1;
        }
    }
    let accepted_all = state
        .lane_boxes
        .as_ref()
        .map(|store| store.all_accepted())
        .unwrap_or_default();
    for record in &accepted_all {
        if record.snapshot_version != state.snapshot_version {
            continue;
        }
        let Some(capability) = state
            .lane_graph
            .as_ref()
            .and_then(|g| g.capabilities.iter().find(|c| c.id == record.capability_id))
        else {
            continue;
        };
        let status = effective_status(false, stored_status(&record.box_id));
        let entry = per_capability.entry(capability.id.as_str()).or_default();
        entry.0.insert(record.principal.as_str());
        *entry
            .1
            .entry(
                STATUSES
                    .iter()
                    .find(|s| **s == status)
                    .copied()
                    .unwrap_or("candidate"),
            )
            .or_insert(0) += 1;
    }

    let mut capabilities = Vec::new();
    for (capability_id, (assigned, counts)) in &per_capability {
        if assigned.len() < 5 {
            continue; // ABSENT, not dashed.
        }
        let mut status_counts = BTreeMap::new();
        for status in STATUSES {
            status_counts.insert(status.to_string(), *counts.get(status).unwrap_or(&0));
        }
        capabilities.push(RollupRow {
            capability_id: (*capability_id).to_string(),
            status_counts,
        });
    }
    let response = RollupResponse {
        capabilities,
        honesty: ROLLUP_HONESTY.to_string(),
        snapshot_version: state.snapshot_version.clone(),
    };
    canonical_json_bytes(&response)
        .map(Some)
        .map_err(AskError::Internal)
}
