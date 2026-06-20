//! AP-3: GET /atlas — the capability surface. The BRM graph (strategy →
//! initiative → workflow → capability, from /fixtures/brm.json) rendered
//! whole, with each capability's realizing documents filtered to the
//! VIEWER's own compiled allowlist.
//!
//! THE RULING: STRUCTURE IS INTERNAL-GRADE; EVIDENCE IS GOVERNED. The names
//! and shape of the BRM render for any principal with a non-empty compiled
//! allowlist; the documents under each capability are the viewer's own. A
//! capability whose realizing documents all sit outside the viewer's scope
//! is a structural node with an EMPTY docs array — no count, no placeholder,
//! no sentence implying hidden content. The response shapes below carry id,
//! name, nesting, and the viewer's docs, and NOTHING else: they are
//! structurally incapable of expressing totals, hidden counts, or coverage.
//!
//! TRUST ROOT: the frozen M1 manifest pins company.json / documents.json /
//! traps.json but not brm.json, so the SERVICE is the BRM's root of trust.
//! At startup the file is strictly parsed (unknown fields refuse), its
//! referential closure is checked level by level, every mapped document is
//! verified against the M1-pinned corpus, and the byte hash is PINNED;
//! every /atlas request re-reads the file and refuses on any drift from
//! that pin. A pre-startup flip confined to display names is the accepted
//! residual (no external pin exists without amending the frozen M1
//! manifest — flagged in the AP-3 closeout). A missing file is a world
//! without an atlas: /atlas answers THE one 404, the M4 absent-capability
//! precedent. Everything else fails closed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use retrieval::index::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::answer::AskError;
use crate::lens::{load_subject_artifact, LensEntry};
use crate::{humanize, AppState, DocMeta};

// ---------------------------------------------------------------------------
// BRM fixture mirror (strict — unknown fields refuse)
// ---------------------------------------------------------------------------

// Fields are crate-visible since AP-6: the Lane derives assignments from
// the same validated graph.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrmLite {
    pub(crate) capabilities: Vec<BrmCapability>,
    pub(crate) initiatives: Vec<BrmInitiative>,
    pub(crate) strategies: Vec<BrmStrategy>,
    pub(crate) workflows: Vec<BrmWorkflow>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrmStrategy {
    pub(crate) id: String,
    pub(crate) initiative_ids: Vec<String>,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrmInitiative {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) strategy_id: String,
    pub(crate) workflow_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrmWorkflow {
    pub(crate) capability_ids: Vec<String>,
    pub(crate) id: String,
    pub(crate) initiative_id: String,
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrmCapability {
    pub(crate) document_ids: Vec<String>,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) workflow_id: String,
}

// ---------------------------------------------------------------------------
// Startup validation + pin
// ---------------------------------------------------------------------------

fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>, level: &str) -> Result<BTreeSet<&'a str>> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            bail!("brm.json: duplicate {level} id {id:?}");
        }
    }
    Ok(seen)
}

/// One level of the closure check: every child id a parent lists must exist,
/// agree about its parent, and be claimed by EXACTLY one parent; every child
/// must be claimed. Anything less and the graph is not the graph.
fn check_partition<'a>(
    parents: impl Iterator<Item = (&'a str, &'a [String])>,
    child_parent: &BTreeMap<&str, &str>,
    level: &str,
) -> Result<()> {
    let mut claimed: BTreeSet<&str> = BTreeSet::new();
    for (parent_id, child_ids) in parents {
        for child_id in child_ids {
            if !claimed.insert(child_id.as_str()) {
                bail!("brm.json: {level} {child_id:?} is claimed by two parents");
            }
            match child_parent.get(child_id.as_str()) {
                Some(recorded) if *recorded == parent_id => {}
                Some(_) => bail!(
                    "brm.json: {level} {child_id:?} disagrees with {parent_id:?} about its parent"
                ),
                None => bail!("brm.json: {parent_id:?} references unknown {level} {child_id:?}"),
            }
        }
    }
    if claimed.len() != child_parent.len() {
        bail!(
            "brm.json: {} {level} rows exist but {} are claimed; refusing an orphaned graph",
            child_parent.len(),
            claimed.len()
        );
    }
    Ok(())
}

/// Referential closure + corpus membership. The corpus map comes from
/// documents.json AFTER its M1-pinned hash verified, so a capability that
/// maps a document the corpus does not carry refuses startup.
pub(crate) fn validate_brm(brm: &BrmLite, docs: &BTreeMap<String, DocMeta>) -> Result<()> {
    unique_ids(brm.strategies.iter().map(|s| s.id.as_str()), "strategy")?;
    unique_ids(brm.initiatives.iter().map(|i| i.id.as_str()), "initiative")?;
    unique_ids(brm.workflows.iter().map(|w| w.id.as_str()), "workflow")?;
    unique_ids(brm.capabilities.iter().map(|c| c.id.as_str()), "capability")?;

    let initiative_parent: BTreeMap<&str, &str> = brm
        .initiatives
        .iter()
        .map(|i| (i.id.as_str(), i.strategy_id.as_str()))
        .collect();
    check_partition(
        brm.strategies
            .iter()
            .map(|s| (s.id.as_str(), s.initiative_ids.as_slice())),
        &initiative_parent,
        "initiative",
    )?;

    let workflow_parent: BTreeMap<&str, &str> = brm
        .workflows
        .iter()
        .map(|w| (w.id.as_str(), w.initiative_id.as_str()))
        .collect();
    check_partition(
        brm.initiatives
            .iter()
            .map(|i| (i.id.as_str(), i.workflow_ids.as_slice())),
        &workflow_parent,
        "workflow",
    )?;

    let capability_parent: BTreeMap<&str, &str> = brm
        .capabilities
        .iter()
        .map(|c| (c.id.as_str(), c.workflow_id.as_str()))
        .collect();
    check_partition(
        brm.workflows
            .iter()
            .map(|w| (w.id.as_str(), w.capability_ids.as_slice())),
        &capability_parent,
        "capability",
    )?;

    for capability in &brm.capabilities {
        let mut seen = BTreeSet::new();
        for document_id in &capability.document_ids {
            if !seen.insert(document_id.as_str()) {
                bail!(
                    "brm.json: capability {:?} maps document {document_id:?} twice",
                    capability.id
                );
            }
            if !docs.contains_key(document_id) {
                bail!(
                    "brm.json: capability {:?} maps document {document_id:?} which the \
                     M1-verified corpus does not carry; refusing",
                    capability.id
                );
            }
        }
    }
    Ok(())
}

/// Startup entry: read, strictly parse, validate, and pin brm.json.
/// `Ok(None)` = the file does not exist — a world without an atlas (the
/// /atlas route answers THE one 404). Any present-but-wrong file refuses
/// startup. Returns the validated graph alongside the pin — AP-6's lane
/// derivation consumes the SAME bytes the pin certifies.
pub(crate) fn pin_brm(
    fixtures_dir: &Path,
    docs: &BTreeMap<String, DocMeta>,
) -> Result<Option<(String, BrmLite)>> {
    let path = fixtures_dir.join("brm.json");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| format!("cannot read {}", path.display()));
        }
    };
    let brm: BrmLite = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} fails strict parse", path.display()))?;
    validate_brm(&brm, docs)
        .with_context(|| format!("{} fails integrity validation", path.display()))?;
    Ok(Some((sha256_hex(&bytes), brm)))
}

// ---------------------------------------------------------------------------
// Response shapes (canonical JSON, sorted keys)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AtlasDoc {
    pub document_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_successor: Option<String>,
    pub sensitivity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct AtlasCapability {
    /// VIEWER-SCOPED. Empty when none of the capability's mapped documents
    /// are in the actor's allowlist — an empty array IS the whole statement.
    pub docs: Vec<AtlasDoc>,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct AtlasWorkflow {
    pub capabilities: Vec<AtlasCapability>,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct AtlasInitiative {
    pub id: String,
    pub name: String,
    pub workflows: Vec<AtlasWorkflow>,
}

#[derive(Debug, Serialize)]
pub struct AtlasStrategy {
    pub id: String,
    pub initiatives: Vec<AtlasInitiative>,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct AtlasResponse {
    /// AR-1: the viewer's own directory card (display only; the BRM structure
    /// names no other principal — per-viewer org names arrive in AR-2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<humanize::PersonCard>,
    pub actor_id: String,
    /// Honesty contract (identity.rs): every response carries this.
    pub demo_identity_mode: bool,
    pub snapshot_version: String,
    pub strategies: Vec<AtlasStrategy>,
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

fn build_structure(
    brm: &BrmLite,
    entries: &[LensEntry],
    allowlist: &BTreeSet<&str>,
    docs: &BTreeMap<String, DocMeta>,
) -> Result<Vec<AtlasStrategy>> {
    let entry_of: BTreeMap<&str, &LensEntry> = entries
        .iter()
        .map(|e| (e.document_id.as_str(), e))
        .collect();
    let initiatives: BTreeMap<&str, &BrmInitiative> =
        brm.initiatives.iter().map(|i| (i.id.as_str(), i)).collect();
    let workflows: BTreeMap<&str, &BrmWorkflow> =
        brm.workflows.iter().map(|w| (w.id.as_str(), w)).collect();
    let capabilities: BTreeMap<&str, &BrmCapability> = brm
        .capabilities
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();

    // Sorted by id at EVERY level — the same order for every viewer, so two
    // bodies can only ever differ in their docs arrays.
    let mut sorted_strategies: Vec<&BrmStrategy> = brm.strategies.iter().collect();
    sorted_strategies.sort_by(|a, b| a.id.cmp(&b.id));

    let mut out_strategies = Vec::with_capacity(sorted_strategies.len());
    for strategy in sorted_strategies {
        let mut initiative_ids: Vec<&str> =
            strategy.initiative_ids.iter().map(String::as_str).collect();
        initiative_ids.sort_unstable();
        let mut out_initiatives = Vec::with_capacity(initiative_ids.len());
        for initiative_id in initiative_ids {
            let initiative = initiatives
                .get(initiative_id)
                .context("brm initiative reference missing after validation; refusing")?;
            let mut workflow_ids: Vec<&str> =
                initiative.workflow_ids.iter().map(String::as_str).collect();
            workflow_ids.sort_unstable();
            let mut out_workflows = Vec::with_capacity(workflow_ids.len());
            for workflow_id in workflow_ids {
                let workflow = workflows
                    .get(workflow_id)
                    .context("brm workflow reference missing after validation; refusing")?;
                let mut capability_ids: Vec<&str> =
                    workflow.capability_ids.iter().map(String::as_str).collect();
                capability_ids.sort_unstable();
                let mut out_capabilities = Vec::with_capacity(capability_ids.len());
                for capability_id in capability_ids {
                    let capability = capabilities
                        .get(capability_id)
                        .context("brm capability reference missing after validation; refusing")?;
                    let mut document_ids: Vec<&str> =
                        capability.document_ids.iter().map(String::as_str).collect();
                    document_ids.sort_unstable();
                    let mut cap_docs = Vec::new();
                    for document_id in document_ids {
                        // THE GOVERNED EDGE: an out-of-scope mapped document
                        // never reaches serialization in any form.
                        if !allowlist.contains(document_id) {
                            continue;
                        }
                        let meta = docs
                            .get(document_id)
                            .context("capability maps a document the corpus does not carry")?;
                        let entry = entry_of
                            .get(document_id)
                            .context("allowlisted document missing its artifact entry")?;
                        let superseded = entry.superseded == Some(true);
                        // R-13 redaction, exactly as /lens and /doc: the
                        // successor id is emitted only when the successor
                        // itself is in the viewer's allowlist.
                        let effective_successor = if superseded {
                            entry
                                .effective_successor
                                .as_ref()
                                .filter(|s| allowlist.contains(s.as_str()))
                                .cloned()
                        } else {
                            None
                        };
                        cap_docs.push(AtlasDoc {
                            document_id: document_id.to_string(),
                            effective_successor,
                            sensitivity: meta.sensitivity.clone(),
                            superseded: superseded.then_some(true),
                            title: meta.title.clone(),
                        });
                    }
                    out_capabilities.push(AtlasCapability {
                        docs: cap_docs,
                        id: capability.id.clone(),
                        name: capability.name.clone(),
                    });
                }
                out_workflows.push(AtlasWorkflow {
                    capabilities: out_capabilities,
                    id: workflow.id.clone(),
                    name: workflow.name.clone(),
                });
            }
            out_initiatives.push(AtlasInitiative {
                id: initiative.id.clone(),
                name: initiative.name.clone(),
                workflows: out_workflows,
            });
        }
        out_strategies.push(AtlasStrategy {
            id: strategy.id.clone(),
            initiatives: out_initiatives,
            name: strategy.name.clone(),
        });
    }
    Ok(out_strategies)
}

/// Builds the atlas body for the actor. `Ok(None)` = this world has no
/// brm.json — the HTTP layer serves THE one 404. The EMPTY-ALLOWLIST RULE
/// (M2a precedent — a principal M1 refused gets nothing): an actor with no
/// artifact row, or an artifact with no entries, receives `strategies: []`,
/// not the structure. Structure visibility is pegged to having any standing
/// at all.
pub fn atlas_view(state: &AppState, actor: &str) -> Result<Option<Vec<u8>>, AskError> {
    let Some(pinned_sha) = &state.brm_sha256 else {
        return Ok(None);
    };
    let path = state.fixtures_dir.join("brm.json");
    let bytes = std::fs::read(&path)
        .with_context(|| format!("cannot read {}", path.display()))
        .map_err(AskError::Internal)?;
    if &sha256_hex(&bytes) != pinned_sha {
        return Err(AskError::Internal(anyhow::anyhow!(
            "brm.json no longer matches the hash pinned at startup; refusing"
        )));
    }
    let brm: BrmLite = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} fails parse", path.display()))
        .map_err(AskError::Internal)?;

    // The actor's standing: the compiled artifact, re-verified against its
    // M1 index hash on every load — stale artifacts refuse, as everywhere.
    let entries = load_subject_artifact(state, actor)
        .map_err(AskError::Internal)?
        .unwrap_or_default();
    let allowlist: BTreeSet<&str> = entries.iter().map(|e| e.document_id.as_str()).collect();

    let strategies = if allowlist.is_empty() {
        Vec::new()
    } else {
        build_structure(&brm, &entries, &allowlist, &state.docs).map_err(AskError::Internal)?
    };
    let response = AtlasResponse {
        actor: humanize::card_for(state.people.as_deref(), actor),
        actor_id: actor.to_string(),
        demo_identity_mode: true,
        snapshot_version: state.snapshot_version.clone(),
        strategies,
    };
    retrieval::index::canonical_json_bytes(&response)
        .map(Some)
        .map_err(AskError::Internal)
}
