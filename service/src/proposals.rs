//! SHOWCASE-III — grounded workflow proposals: EB's first mutation path.
//!
//! A model PROPOSES a staged workflow whose every box is anchored to a document
//! the PROPOSER is authorized to see; the draft lands here (append-only) in
//! `pending`; only an accountable approver's decision MATERIALIZES it into real
//! workflow items. This module owns: the append-only store (mirroring
//! `access_requests`), the grounded generation orchestration (retrieval →
//! seal → box generation → per-box `grounding::ground` gate, reused unchanged),
//! the ONE `materialize` effect, and the S4 anchor-visibility redaction (no
//! verbatim document content ever crosses to an identity not authorized for
//! that document — approvers included).
//!
//! Every rule fails closed: no approver → no proposal; store down → the routes
//! 503; a parse fault or zero admitted boxes → ZERO writes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use retrieval::index::{canonical_json_bytes, sha256_hex};
use retrieval::search::{PrincipalScope, SearchOptions};
use retrieval::vector::snippet_of;
use serde::{Deserialize, Serialize};

use crate::answer::{ASK_TOP_K, CONTEXT_DOCS, CONTEXT_SNIPPET_CHARS};
use crate::generate::{parse_boxes, ContextDoc};
use crate::grounding::{ground, AnchorOnly, Claim, Grounded};
use crate::lens::load_subject_artifact;
use crate::AppState;

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_APPROVED: &str = "approved";
pub const STATUS_DENIED: &str = "denied";
/// Proposals draft into the pipeline's "Next" stage (a payload status group).
pub const PROPOSAL_STAGE: &str = "Next";
/// Generation is expensive; a proposal gets a 20s budget (one attempt).
pub const PROPOSAL_GEN_TIMEOUT_MS: u64 = 20_000;

// ---------------------------------------------------------------------------
// Stored shapes (append-only; the store is authoritative)
// ---------------------------------------------------------------------------

/// One proven anchor on an admitted box: the quote exists verbatim in the cited
/// PROPOSER-visible document at `locator`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalAnchor {
    pub doc_id: String,
    pub locator: String,
    pub quote: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalBox {
    pub box_index: u32,
    pub stage: String,
    pub title: String,
    pub description: String,
    /// >= 1 by construction — a box with no admitted anchor is refused.
    pub anchors: Vec<ProposalAnchor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundingCounts {
    pub admitted: usize,
    pub refused: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub proposal_id: String,
    pub proposer_id: String,
    pub capability_id: String,
    pub approver_id: String,
    pub title: String,
    pub goal: String,
    pub boxes: Vec<ProposalBox>,
    pub grounding: GroundingCounts,
    pub status: String,
    pub created_ordinal: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_ordinal: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    pub snapshot_version: String,
    #[serde(default)]
    pub materialized: bool,
}

/// A drafted-but-unstored proposal: the generation output before the store
/// assigns an id + ordinal. Kept distinct so the id is minted in one place.
#[derive(Debug, Clone)]
pub struct ProposalDraft {
    pub proposer_id: String,
    pub capability_id: String,
    pub approver_id: String,
    pub title: String,
    pub goal: String,
    pub boxes: Vec<ProposalBox>,
    pub grounding: GroundingCounts,
    pub snapshot_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
enum StoreEvent {
    Created {
        proposal: Proposal,
    },
    Decided {
        actor_principal: String,
        decided_ordinal: u64,
        outcome: String,
        proposal_id: String,
        status: String,
    },
    Materialized {
        proposal_id: String,
        ordinal: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEvent {
    pub action: String,
    pub actor_principal: String,
    pub ordinal: u64,
    pub outcome: String,
    pub target: String,
}

struct StoreState {
    next_audit_ordinal: u64,
    next_ordinal: u64,
    proposals: BTreeMap<String, Proposal>,
    /// wf-gen S4 condition: sha256 (hex) of the last line of each ledger
    /// file, the value the next chained row's `prev` links to. Computed at
    /// open; the event log and the audit log chain independently.
    event_chain_tip: String,
    audit_chain_tip: String,
}

pub struct WorkflowProposalStore {
    audit_path: PathBuf,
    proposals_path: PathBuf,
    state: Mutex<StoreState>,
    /// wf-gen S4 condition: the timestamp/chain clock. `None` = legacy
    /// (byte-identical, the exact pre-condition writer). `Some` = chained:
    /// every stored workflow-proposal AND audit row carries `ts` + `prev`,
    /// so approval records are tamper-evident (the standing S4 law, applied
    /// to the mutation ledger).
    clock: Option<Arc<dyn crate::clock::Clock>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecideError {
    AlreadyDecided,
    NotFound,
    Stale,
}

impl WorkflowProposalStore {
    /// Distinct filenames from M4's `proposals.jsonl` so both stores share one
    /// `--state-dir` safely (flagged in the closeout).
    pub fn open(dir: &Path) -> Result<WorkflowProposalStore> {
        Self::open_inner(dir, None)
    }

    /// wf-gen S4 condition: open the store CHAINED — every workflow-proposal
    /// and audit row carries an injected-clock `ts` and a `prev` hash, so
    /// the mutation ledger is tamper-evident. Anchors over any pre-condition
    /// rows without rewriting them (the standing S4 anchoring pattern).
    pub fn open_chained(
        dir: &Path,
        clock: Arc<dyn crate::clock::Clock>,
    ) -> Result<WorkflowProposalStore> {
        Self::open_inner(dir, Some(clock))
    }

    fn open_inner(
        dir: &Path,
        clock: Option<Arc<dyn crate::clock::Clock>>,
    ) -> Result<WorkflowProposalStore> {
        std::fs::create_dir_all(dir).with_context(|| {
            format!(
                "cannot create workflow proposal store dir {}",
                dir.display()
            )
        })?;
        let proposals_path = dir.join("wf_proposals.jsonl");
        let audit_path = dir.join("wf_proposals_audit.jsonl");
        let mut state = StoreState {
            next_audit_ordinal: 0,
            next_ordinal: 0,
            proposals: BTreeMap::new(),
            event_chain_tip: sha256_hex(b""),
            audit_chain_tip: sha256_hex(b""),
        };

        if proposals_path.exists() {
            let raw = std::fs::read(&proposals_path)
                .with_context(|| format!("cannot read {}", proposals_path.display()))?;
            if let Some(last) = last_line_bytes(&raw) {
                state.event_chain_tip = sha256_hex(last);
            }
            let text = String::from_utf8(raw).context("event log is not UTF-8")?;
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                // Strip the chain metadata (ts/prev) before the strict
                // StoreEvent parse, so the event type stays deny_unknown_fields.
                let mut value: serde_json::Value =
                    serde_json::from_str(line).context("workflow proposal row fails parse")?;
                if let Some(obj) = value.as_object_mut() {
                    obj.remove("ts");
                    obj.remove("prev");
                }
                let event: StoreEvent =
                    serde_json::from_value(value).context("workflow proposal event fails parse")?;
                state.next_ordinal += 1;
                match event {
                    StoreEvent::Created { proposal } => {
                        state
                            .proposals
                            .insert(proposal.proposal_id.clone(), proposal);
                    }
                    StoreEvent::Decided {
                        actor_principal,
                        decided_ordinal,
                        outcome,
                        proposal_id,
                        status,
                    } => {
                        let proposal = state
                            .proposals
                            .get_mut(&proposal_id)
                            .context("decided event for unknown proposal; store corrupt")?;
                        proposal.status = status;
                        proposal.decided_ordinal = Some(decided_ordinal);
                        proposal.decided_by = Some(actor_principal);
                        let _ = outcome;
                    }
                    StoreEvent::Materialized { proposal_id, .. } => {
                        let proposal = state
                            .proposals
                            .get_mut(&proposal_id)
                            .context("materialized event for unknown proposal; store corrupt")?;
                        proposal.materialized = true;
                    }
                }
            }
        }
        if audit_path.exists() {
            let raw = std::fs::read(&audit_path)
                .with_context(|| format!("cannot read {}", audit_path.display()))?;
            state.next_audit_ordinal = count_lines(&raw);
            if let Some(last) = last_line_bytes(&raw) {
                state.audit_chain_tip = sha256_hex(last);
            }
        }

        Ok(WorkflowProposalStore {
            audit_path,
            proposals_path,
            state: Mutex::new(state),
            clock,
        })
    }

    fn append(path: &Path, value: &impl Serialize) -> Result<()> {
        let bytes = canonical_json_bytes(value)?;
        Self::write_bytes(path, &bytes)
    }

    fn write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("cannot append to {}", path.display()))?;
        file.sync_data()
            .with_context(|| format!("cannot sync {}", path.display()))?;
        Ok(())
    }

    /// Append one row to a ledger file, chaining it when a clock is present.
    /// Legacy mode (no clock) takes the exact pre-condition path — the
    /// output is byte-identical. Chained mode injects `ts` + `prev` at the
    /// JSON layer (so `StoreEvent`/`AuditEvent` stay `deny_unknown_fields`)
    /// and advances the file's chain tip.
    fn append_chained(&self, path: &Path, tip: &mut String, value: &impl Serialize) -> Result<()> {
        match &self.clock {
            None => Self::append(path, value),
            Some(clock) => {
                let mut v = serde_json::to_value(value).context("ledger row serialization")?;
                let obj = v
                    .as_object_mut()
                    .context("a ledger row must be a JSON object")?;
                obj.insert("ts".to_string(), clock.now_rfc3339_ms().into());
                obj.insert("prev".to_string(), tip.clone().into());
                let bytes = canonical_json_bytes(&v)?;
                Self::write_bytes(path, &bytes)?;
                *tip = sha256_hex(&bytes);
                Ok(())
            }
        }
    }

    /// AUDIT-BEFORE-EFFECT (lens.rs precedent): the row is written AND flushed
    /// (`sync_data`) before this returns; a caller that cannot audit refuses the
    /// act. Returns the audit ordinal.
    pub fn audit(&self, action: &str, actor: &str, target: &str, outcome: &str) -> Result<u64> {
        let mut state = self.state.lock().expect("store mutex");
        let ordinal = state.next_audit_ordinal;
        let event = AuditEvent {
            action: action.to_string(),
            actor_principal: actor.to_string(),
            ordinal,
            outcome: outcome.to_string(),
            target: target.to_string(),
        };
        self.append_chained(&self.audit_path, &mut state.audit_chain_tip, &event)?;
        state.next_audit_ordinal += 1;
        Ok(ordinal)
    }

    pub fn create(&self, draft: ProposalDraft) -> Result<Proposal> {
        let mut state = self.state.lock().expect("store mutex");
        let id_hash = sha256_hex(
            format!(
                "{}\n{}\n{}\n{}\n{}",
                draft.proposer_id, draft.capability_id, draft.title, draft.goal, state.next_ordinal
            )
            .as_bytes(),
        );
        let proposal_id = format!("wp_{}", &id_hash[..16]);
        if state.proposals.contains_key(&proposal_id) {
            bail!("workflow proposal id collision; refusing (store corrupt?)");
        }
        let proposal = Proposal {
            proposal_id,
            proposer_id: draft.proposer_id,
            capability_id: draft.capability_id,
            approver_id: draft.approver_id,
            title: draft.title,
            goal: draft.goal,
            boxes: draft.boxes,
            grounding: draft.grounding,
            status: STATUS_PENDING.to_string(),
            created_ordinal: state.next_ordinal,
            decided_ordinal: None,
            decided_by: None,
            snapshot_version: draft.snapshot_version,
            materialized: false,
        };
        self.append_chained(
            &self.proposals_path,
            &mut state.event_chain_tip,
            &StoreEvent::Created {
                proposal: proposal.clone(),
            },
        )?;
        state.next_ordinal += 1;
        state
            .proposals
            .insert(proposal.proposal_id.clone(), proposal.clone());
        Ok(proposal)
    }

    pub fn get(&self, proposal_id: &str) -> Option<Proposal> {
        self.state
            .lock()
            .expect("store mutex")
            .proposals
            .get(proposal_id)
            .cloned()
    }

    pub fn decide(
        &self,
        proposal_id: &str,
        status: &str,
        actor_principal: &str,
        current_snapshot: &str,
    ) -> Result<Result<Proposal, DecideError>> {
        if status != STATUS_APPROVED && status != STATUS_DENIED {
            bail!("invalid workflow proposal decision status");
        }
        let mut state = self.state.lock().expect("store mutex");
        let Some(proposal) = state.proposals.get(proposal_id) else {
            return Ok(Err(DecideError::NotFound));
        };
        if proposal.snapshot_version != current_snapshot {
            return Ok(Err(DecideError::Stale));
        }
        if proposal.status != STATUS_PENDING {
            return Ok(Err(DecideError::AlreadyDecided));
        }
        let decided_ordinal = state.next_ordinal;
        self.append_chained(
            &self.proposals_path,
            &mut state.event_chain_tip,
            &StoreEvent::Decided {
                actor_principal: actor_principal.to_string(),
                decided_ordinal,
                outcome: status.to_string(),
                proposal_id: proposal_id.to_string(),
                status: status.to_string(),
            },
        )?;
        state.next_ordinal += 1;
        let proposal = state.proposals.get_mut(proposal_id).expect("checked above");
        proposal.status = status.to_string();
        proposal.decided_ordinal = Some(decided_ordinal);
        proposal.decided_by = Some(actor_principal.to_string());
        Ok(Ok(proposal.clone()))
    }

    /// THE ONE MATERIALIZE EFFECT (INV1, WF-G7). Grep `\.materialize\(` in
    /// service/src → this is its single call site (the approve handler, AFTER
    /// the audit flush and the approved decision). It makes an approved
    /// proposal's boxes real by recording the Materialized event; `/workflow`
    /// then reads materialized proposals via `approved_for` (a different name).
    pub fn materialize(&self, proposal_id: &str) -> Result<Option<Proposal>> {
        let mut state = self.state.lock().expect("store mutex");
        let Some(proposal) = state.proposals.get(proposal_id) else {
            return Ok(None);
        };
        if proposal.status != STATUS_APPROVED || proposal.materialized {
            return Ok(Some(proposal.clone()));
        }
        let ordinal = state.next_ordinal;
        self.append_chained(
            &self.proposals_path,
            &mut state.event_chain_tip,
            &StoreEvent::Materialized {
                proposal_id: proposal_id.to_string(),
                ordinal,
            },
        )?;
        state.next_ordinal += 1;
        let proposal = state.proposals.get_mut(proposal_id).expect("checked above");
        proposal.materialized = true;
        Ok(Some(proposal.clone()))
    }

    /// The caller's own proposals (role=proposer).
    pub fn proposed_by(&self, proposer_id: &str) -> Vec<Proposal> {
        let state = self.state.lock().expect("store mutex");
        let mut out: Vec<Proposal> = state
            .proposals
            .values()
            .filter(|p| p.proposer_id == proposer_id)
            .cloned()
            .collect();
        out.sort_by_key(|p| p.created_ordinal);
        out
    }

    /// Proposals awaiting the caller's decision (role=approver, pending only).
    pub fn inbox_for(&self, approver_id: &str) -> Vec<Proposal> {
        let state = self.state.lock().expect("store mutex");
        let mut out: Vec<Proposal> = state
            .proposals
            .values()
            .filter(|p| p.approver_id == approver_id && p.status == STATUS_PENDING)
            .cloned()
            .collect();
        out.sort_by_key(|p| p.created_ordinal);
        out
    }

    /// Approved + materialized proposals for a capability — the `/workflow`
    /// overlay source. (Distinct from `materialize`; NOT the effect.)
    pub fn approved_for(&self, capability_id: &str, snapshot: &str) -> Vec<Proposal> {
        let state = self.state.lock().expect("store mutex");
        let mut out: Vec<Proposal> = state
            .proposals
            .values()
            .filter(|p| {
                p.capability_id == capability_id
                    && p.snapshot_version == snapshot
                    && p.status == STATUS_APPROVED
                    && p.materialized
            })
            .cloned()
            .collect();
        out.sort_by_key(|p| p.created_ordinal);
        out
    }
}

// ---------------------------------------------------------------------------
// Generation orchestration (S2): authorization-before-retrieval inherited
// ---------------------------------------------------------------------------

pub enum GenerateOutcome {
    /// A drafted proposal with >= 1 admitted box, ready to store.
    Drafted(ProposalDraft),
    /// Every box refused grounding — no proposal is written, counts disclosed.
    ZeroAdmitted { refused: usize },
    /// A generation or strict-parse fault — no proposal is written.
    Fault,
}

/// Draft a grounded proposal for `proposer` against `capability_id`. Retrieval
/// is scoped to the PROPOSER's compiled allowlist (the same seal `/ask` uses);
/// each box is grounded by `grounding::ground` against the sealed bodies. Errs
/// only on internal failure (scope/corpus); a bad model output is a `Fault`
/// (honest, zero writes), not an error.
pub fn generate_proposal(
    state: &AppState,
    proposer: &str,
    capability_id: &str,
    approver_id: &str,
    title: &str,
    goal: &str,
) -> Result<GenerateOutcome> {
    let Some(generator) = &state.generator else {
        // No generator wired = the demo build without --config. Honest fault,
        // never a fabricated plan.
        return Ok(GenerateOutcome::Fault);
    };

    // 1. Authorization-before-retrieval (inherited): the proposer's compiled
    //    allowlist governs the search; the goal is the query.
    let scope = PrincipalScope::load(&state.artifacts_dir, proposer)
        .context("loading the proposer's compiled allowlist")?;
    let search_options = SearchOptions {
        k: ASK_TOP_K,
        include_superseded: false,
        hybrid: None,
        judge: None,
    };
    let (retrieval_envelope, _trace) = state
        .engine
        .search(&scope, goal, &search_options)
        .context("governed retrieval failed")?;

    // 2. Sealed context: top surviving docs as (id, title, snippet).
    let surviving: Vec<&str> = retrieval_envelope
        .results
        .iter()
        .map(|r| r.document_id.as_str())
        .collect();
    let sealed: Vec<ContextDoc> = surviving
        .iter()
        .take(CONTEXT_DOCS)
        .map(|id| {
            let meta = state
                .docs
                .get(*id)
                .context("result id missing from the verified corpus")?;
            Ok(ContextDoc {
                doc_id: (*id).to_string(),
                title: meta.title.clone(),
                snippet: snippet_of(&meta.body, CONTEXT_SNIPPET_CHARS),
            })
        })
        .collect::<Result<_>>()?;
    if sealed.is_empty() {
        // No in-scope documents support a plan — nothing to ground.
        return Ok(GenerateOutcome::ZeroAdmitted { refused: 0 });
    }
    // Full bodies for grounding — the single lookup `ground` receives.
    let sealed_bodies: BTreeMap<&str, &str> = sealed
        .iter()
        .filter_map(|d| {
            state
                .docs
                .get(&d.doc_id)
                .map(|m| (d.doc_id.as_str(), m.body.as_str()))
        })
        .collect();

    // 3. Generate boxes (one attempt) → STRICT parse. Any fault → no proposal.
    let outcome = match generator.generate_boxes(
        title,
        goal,
        &sealed,
        Duration::from_millis(PROPOSAL_GEN_TIMEOUT_MS),
    ) {
        Ok(outcome) => outcome,
        Err(err) => {
            // Operator diagnostics only — the fault KIND, never model text.
            eprintln!("proposal generation fault (generator): {err:#}");
            return Ok(GenerateOutcome::Fault);
        }
    };
    let draft_boxes = match parse_boxes(&outcome.text) {
        Ok(boxes) => boxes,
        Err(err) => {
            eprintln!("proposal generation fault (parse): {err:#}");
            return Ok(GenerateOutcome::Fault);
        }
    };

    // 4. Per-box grounding gate — DESC is the claim text so bracket-smuggling in
    //    the description is caught by `ground`; the title's brackets were caught
    //    in `parse_boxes`. Refused boxes are counted, never rendered.
    let verifier = AnchorOnly;
    let mut boxes: Vec<ProposalBox> = Vec::new();
    let mut refused = 0usize;
    for draft in &draft_boxes {
        let claim = Claim {
            text: draft.description.clone(),
            doc_id: draft.doc_id.clone(),
            quote: draft.quote.clone(),
        };
        match ground(claim, &sealed_bodies, &verifier) {
            Grounded::Admitted { anchor, .. } => {
                boxes.push(ProposalBox {
                    box_index: boxes.len() as u32,
                    stage: PROPOSAL_STAGE.to_string(),
                    title: draft.title.clone(),
                    description: draft.description.clone(),
                    anchors: vec![ProposalAnchor {
                        doc_id: anchor.doc_id,
                        locator: anchor.locator,
                        quote: draft.quote.clone(),
                    }],
                });
            }
            Grounded::Refused { .. } => refused += 1,
        }
    }
    if boxes.is_empty() {
        return Ok(GenerateOutcome::ZeroAdmitted { refused });
    }
    let admitted = boxes.len();
    Ok(GenerateOutcome::Drafted(ProposalDraft {
        proposer_id: proposer.to_string(),
        capability_id: capability_id.to_string(),
        approver_id: approver_id.to_string(),
        title: title.to_string(),
        goal: goal.to_string(),
        boxes,
        grounding: GroundingCounts { admitted, refused },
        snapshot_version: state.snapshot_version.clone(),
    }))
}

// ---------------------------------------------------------------------------
// S4 — the anchor-visibility law: verbatim content never crosses to an identity
// not authorized for that document. Applied at serve time, per VIEWER.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct AnchorView {
    pub visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BoxView {
    pub box_index: u32,
    pub stage: String,
    pub title: String,
    pub description: String,
    pub anchors: Vec<AnchorView>,
    pub sources_total: usize,
    pub sources_outside_view: usize,
}

/// The set of document ids the viewer's OWN scope covers.
fn viewer_visible_docs(state: &AppState, viewer: &str) -> Result<BTreeSet<String>> {
    let entries = load_subject_artifact(state, viewer)?.unwrap_or_default();
    Ok(entries.into_iter().map(|e| e.document_id).collect())
}

/// Redact a stored proposal's boxes for one viewer. Titles/descriptions are the
/// PROPOSER's owned prose (always shown to a party). An anchor renders in full
/// ONLY if the viewer's own scope covers its document; otherwise it is withheld
/// to exactly `{visible:false}` — no id, title, quote, or locator — and counted.
pub fn redact_boxes_for(
    state: &AppState,
    viewer: &str,
    boxes: &[ProposalBox],
) -> Result<Vec<BoxView>> {
    let visible_docs = viewer_visible_docs(state, viewer)?;
    let mut out = Vec::with_capacity(boxes.len());
    for b in boxes {
        let mut anchors = Vec::with_capacity(b.anchors.len());
        let mut outside = 0usize;
        for a in &b.anchors {
            if visible_docs.contains(&a.doc_id) {
                let title = state.docs.get(&a.doc_id).map(|m| m.title.clone());
                anchors.push(AnchorView {
                    visible: true,
                    doc_id: Some(a.doc_id.clone()),
                    title,
                    quote: Some(a.quote.clone()),
                    locator: Some(a.locator.clone()),
                });
            } else {
                outside += 1;
                anchors.push(AnchorView {
                    visible: false,
                    doc_id: None,
                    title: None,
                    quote: None,
                    locator: None,
                });
            }
        }
        out.push(BoxView {
            box_index: b.box_index,
            stage: b.stage.clone(),
            title: b.title.clone(),
            description: b.description.clone(),
            sources_total: b.anchors.len(),
            sources_outside_view: outside,
            anchors,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// wf-gen S4 condition: chain readers (mirror agent::proposals)
// ---------------------------------------------------------------------------

/// Count non-empty lines in raw ledger bytes.
fn count_lines(raw: &[u8]) -> u64 {
    raw.split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .count() as u64
}

/// The last line's raw bytes AS WRITTEN (including its trailing newline).
/// Rows always end in `\n`, so the last line runs from just after the
/// second-to-last newline through the final one. `None` for an empty file.
fn last_line_bytes(raw: &[u8]) -> Option<&[u8]> {
    let last_nl = raw.iter().rposition(|b| *b == b'\n')?;
    let start = raw[..last_nl]
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    Some(&raw[start..=last_nl])
}
