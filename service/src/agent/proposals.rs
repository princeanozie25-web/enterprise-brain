//! The proposal store: append-only JSONL event log (created/decided) plus an
//! append-only audit log. Current state is a fold over events; nothing is
//! ever rewritten. Ordinal time only (the M2b pattern) — no wall clock in
//! any stored record.
//!
//! Idempotency: `proposal_key = sha256(agent_id + standing_query + sorted
//! evidence ids)`. Deduplication is scoped to the snapshot the proposal was
//! created under: within one snapshot a re-run changes nothing (AG-5), while
//! a new snapshot legitimately re-proposes over the new world (AG-6).
//!
//! Snapshot pinning, fail closed: a proposal renders its finding ONLY under
//! the snapshot it was created under. Anything else renders status +
//! standing query with the finding WITHHELD, and may not be approved or
//! rejected — stale evidence must never render, because the scope that
//! justified it may no longer exist.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use retrieval::index::{canonical_json_bytes, sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::context::ProposalDraft;

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_APPROVED: &str = "approved";
pub const STATUS_REJECTED: &str = "rejected";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub citations: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub actor_principal: String,
    pub decided_ordinal: u64,
}

/// The proposal object (canonical JSON, sorted keys).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub agent_id: String,
    pub created_ordinal: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<Decision>,
    pub finding: Finding,
    pub index_version: String,
    pub owner_user_id: String,
    pub proposal_id: String,
    pub proposal_key: String,
    pub snapshot_version: String,
    pub standing_query: String,
    pub status: String,
}

/// Append-only event rows (proposals.jsonl).
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
enum StoreEvent {
    Created {
        proposal: Proposal,
    },
    Decided {
        proposal_id: String,
        status: String,
        actor_principal: String,
        decided_ordinal: u64,
    },
}

/// Append-only audit rows (audit.jsonl): every authority-relevant attempt,
/// allowed AND refused, written BEFORE any effect. Ids and labels only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEvent {
    pub action: String,
    pub actor_principal: String,
    pub ordinal: u64,
    pub outcome: String,
    pub target: String,
}

struct StoreState {
    proposals: BTreeMap<String, Proposal>,
    /// (proposal_key, snapshot_version) -> proposal_id (dedupe scope).
    by_key: BTreeMap<(String, String), String>,
    next_ordinal: u64,
    next_audit_ordinal: u64,
}

pub struct ProposalStore {
    proposals_path: PathBuf,
    audit_path: PathBuf,
    state: Mutex<StoreState>,
}

pub enum CreateOutcome {
    Created(Box<Proposal>),
    Deduplicated,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DecideError {
    NotFound,
    Stale,
    AlreadyDecided,
}

impl ProposalStore {
    /// Opens (or creates) the store under `dir`, replaying the event log.
    pub fn open(dir: &Path) -> Result<ProposalStore> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("cannot create proposal store dir {}", dir.display()))?;
        let proposals_path = dir.join("proposals.jsonl");
        let audit_path = dir.join("audit.jsonl");

        let mut state = StoreState {
            proposals: BTreeMap::new(),
            by_key: BTreeMap::new(),
            next_ordinal: 0,
            next_audit_ordinal: 0,
        };
        if proposals_path.exists() {
            let text = std::fs::read_to_string(&proposals_path)
                .with_context(|| format!("cannot read {}", proposals_path.display()))?;
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let event: StoreEvent =
                    serde_json::from_str(line).context("proposal store event fails parse")?;
                state.next_ordinal += 1;
                match event {
                    StoreEvent::Created { proposal } => {
                        state.by_key.insert(
                            (
                                proposal.proposal_key.clone(),
                                proposal.snapshot_version.clone(),
                            ),
                            proposal.proposal_id.clone(),
                        );
                        state
                            .proposals
                            .insert(proposal.proposal_id.clone(), proposal);
                    }
                    StoreEvent::Decided {
                        proposal_id,
                        status,
                        actor_principal,
                        decided_ordinal,
                    } => {
                        let proposal = state
                            .proposals
                            .get_mut(&proposal_id)
                            .context("decided event for unknown proposal; store corrupt")?;
                        proposal.status = status;
                        proposal.decision = Some(Decision {
                            actor_principal,
                            decided_ordinal,
                        });
                    }
                }
            }
        }
        if audit_path.exists() {
            let text = std::fs::read_to_string(&audit_path)
                .with_context(|| format!("cannot read {}", audit_path.display()))?;
            state.next_audit_ordinal = text.lines().filter(|l| !l.trim().is_empty()).count() as u64;
        }
        Ok(ProposalStore {
            proposals_path,
            audit_path,
            state: Mutex::new(state),
        })
    }

    fn append(path: &Path, value: &impl Serialize) -> Result<()> {
        let bytes = canonical_json_bytes(value)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("cannot open {}", path.display()))?;
        file.write_all(&bytes)
            .with_context(|| format!("cannot append to {}", path.display()))?;
        file.sync_data()
            .with_context(|| format!("cannot sync {}", path.display()))?;
        Ok(())
    }

    /// The idempotency key: sha256(agent_id + standing_query + sorted
    /// evidence ids), newline-separated.
    pub fn proposal_key(agent_id: &str, standing_query: &str, citations: &[String]) -> String {
        let mut sorted: Vec<&str> = citations.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        sorted.dedup();
        let preimage = format!("{agent_id}\n{standing_query}\n{}", sorted.join("\n"));
        sha256_hex(preimage.as_bytes())
    }

    /// Appends a new proposal unless (key, snapshot) already exists.
    /// The draft is assumed validated by the capability layer.
    pub fn create(
        &self,
        agent_id: &str,
        owner_user_id: &str,
        snapshot_version: &str,
        index_version: &str,
        draft: &ProposalDraft,
    ) -> Result<CreateOutcome> {
        let key = Self::proposal_key(agent_id, &draft.standing_query, &draft.citations);
        let mut state = self.state.lock().expect("store mutex");
        if state
            .by_key
            .contains_key(&(key.clone(), snapshot_version.to_string()))
        {
            return Ok(CreateOutcome::Deduplicated);
        }
        // The id is unique per (key, snapshot): the same evidence proposed
        // again under a NEW snapshot is a NEW proposal — the old one stays
        // in the log forever (append-only) and renders stale.
        let id_hash = sha256_hex(format!("{key}\n{snapshot_version}").as_bytes());
        let proposal_id = format!("prop_{}", &id_hash[..16]);
        if state.proposals.contains_key(&proposal_id) {
            bail!("proposal id collision; refusing (store corrupt?)");
        }
        let proposal = Proposal {
            agent_id: agent_id.to_string(),
            created_ordinal: state.next_ordinal,
            decision: None,
            finding: Finding {
                citations: draft.citations.clone(),
                rationale: draft.rationale.clone(),
            },
            index_version: index_version.to_string(),
            owner_user_id: owner_user_id.to_string(),
            proposal_id,
            proposal_key: key.clone(),
            snapshot_version: snapshot_version.to_string(),
            standing_query: draft.standing_query.clone(),
            status: STATUS_PENDING.to_string(),
        };
        Self::append(
            &self.proposals_path,
            &StoreEvent::Created {
                proposal: proposal.clone(),
            },
        )?;
        state.next_ordinal += 1;
        state.by_key.insert(
            (key, snapshot_version.to_string()),
            proposal.proposal_id.clone(),
        );
        state
            .proposals
            .insert(proposal.proposal_id.clone(), proposal.clone());
        Ok(CreateOutcome::Created(Box::new(proposal)))
    }

    /// Records one audit row (append + sync) and returns its ordinal. Always
    /// called BEFORE the effect it describes.
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
        Self::append(&self.audit_path, &event)?;
        state.next_audit_ordinal += 1;
        Ok(ordinal)
    }

    /// Applies an approve/reject. Authority (owner-only, human-only) is the
    /// caller's burden and is audited there; here the fail-closed state
    /// rules hold: the proposal must exist, must be pending, and must be
    /// pinned to the CURRENT snapshot. Approval changes STATUS and nothing
    /// else.
    pub fn decide(
        &self,
        proposal_id: &str,
        status: &str,
        actor_principal: &str,
        current_snapshot: &str,
    ) -> Result<Result<Proposal, DecideError>> {
        if status != STATUS_APPROVED && status != STATUS_REJECTED {
            bail!("invalid decision status");
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
        Self::append(
            &self.proposals_path,
            &StoreEvent::Decided {
                proposal_id: proposal_id.to_string(),
                status: status.to_string(),
                actor_principal: actor_principal.to_string(),
                decided_ordinal,
            },
        )?;
        state.next_ordinal += 1;
        let proposal = state.proposals.get_mut(proposal_id).expect("checked above");
        proposal.status = status.to_string();
        proposal.decision = Some(Decision {
            actor_principal: actor_principal.to_string(),
            decided_ordinal,
        });
        Ok(Ok(proposal.clone()))
    }

    pub fn get(&self, proposal_id: &str) -> Option<Proposal> {
        self.state
            .lock()
            .expect("store mutex")
            .proposals
            .get(proposal_id)
            .cloned()
    }

    /// All proposals owned by `owner`, in created order.
    pub fn owned_by(&self, owner: &str) -> Vec<Proposal> {
        let state = self.state.lock().expect("store mutex");
        let mut owned: Vec<Proposal> = state
            .proposals
            .values()
            .filter(|p| p.owner_user_id == owner)
            .cloned()
            .collect();
        owned.sort_by_key(|p| p.created_ordinal);
        owned
    }

    pub fn count(&self) -> usize {
        self.state.lock().expect("store mutex").proposals.len()
    }
}

/// Render a proposal for serving. Pinned-snapshot proposals render in full;
/// anything else renders WITH ITS FINDING WITHHELD — no rationale, no
/// citations — plus the stale marker and the re-run hint.
pub fn render(proposal: &Proposal, current_snapshot: &str) -> Result<Value> {
    if proposal.snapshot_version == current_snapshot {
        return Ok(serde_json::to_value(proposal)?);
    }
    let mut withheld = serde_json::Map::new();
    withheld.insert("agent_id".into(), proposal.agent_id.clone().into());
    withheld.insert("created_ordinal".into(), proposal.created_ordinal.into());
    if let Some(decision) = &proposal.decision {
        withheld.insert("decision".into(), serde_json::to_value(decision)?);
    }
    withheld.insert(
        "owner_user_id".into(),
        proposal.owner_user_id.clone().into(),
    );
    withheld.insert("proposal_id".into(), proposal.proposal_id.clone().into());
    withheld.insert("proposal_key".into(), proposal.proposal_key.clone().into());
    withheld.insert("refresh".into(), "re-run to refresh".into());
    withheld.insert("stale".into(), true.into());
    withheld.insert(
        "standing_query".into(),
        proposal.standing_query.clone().into(),
    );
    withheld.insert("status".into(), proposal.status.clone().into());
    Ok(Value::Object(withheld))
}
