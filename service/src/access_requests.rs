//! Access request ledger: append-only request/decision events plus a separate
//! append-only audit log. This records human requests and review decisions
//! only; it never expands compiled allowlists or mutates retrieval state.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use retrieval::index::{canonical_json_bytes, sha256_hex};
use serde::{Deserialize, Serialize};

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_APPROVED: &str = "approved";
pub const STATUS_DENIED: &str = "denied";
pub const STATUS_CANCELLED: &str = "cancelled";
pub const STATUS_EXPIRED: &str = "expired";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccessTarget {
    Capability { capability_id: String },
    Project { capability_id: String },
}

impl AccessTarget {
    pub fn capability_id(&self) -> &str {
        match self {
            AccessTarget::Capability { capability_id } => capability_id,
            AccessTarget::Project { capability_id } => capability_id,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            AccessTarget::Capability { .. } => "capability",
            AccessTarget::Project { .. } => "project",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessDecision {
    pub actor_principal: String,
    pub decided_ordinal: u64,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessRequest {
    pub approver_id: String,
    pub created_ordinal: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<AccessDecision>,
    pub justification: String,
    pub request_id: String,
    pub request_key: String,
    pub requester_id: String,
    pub snapshot_version: String,
    pub status: String,
    pub target: AccessTarget,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
enum StoreEvent {
    Created {
        request: AccessRequest,
    },
    Decided {
        actor_principal: String,
        decided_ordinal: u64,
        outcome: String,
        reason_code: Option<String>,
        request_id: String,
        status: String,
    },
}

/// Append-only audit rows for access-request acts. Kept separate from the
/// proposal audit stream so both stores can share one state dir safely.
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
    by_key: BTreeMap<(String, String), String>,
    next_audit_ordinal: u64,
    next_ordinal: u64,
    requests: BTreeMap<String, AccessRequest>,
}

pub struct AccessRequestStore {
    audit_path: PathBuf,
    requests_path: PathBuf,
    state: Mutex<StoreState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOutcome {
    Created(Box<AccessRequest>),
    Existing(Box<AccessRequest>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecideError {
    AlreadyDecided,
    NotFound,
    Stale,
}

impl AccessRequestStore {
    pub fn open(dir: &Path) -> Result<AccessRequestStore> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("cannot create access request store dir {}", dir.display()))?;
        let requests_path = dir.join("access_requests.jsonl");
        let audit_path = dir.join("access_requests_audit.jsonl");
        let mut state = StoreState {
            by_key: BTreeMap::new(),
            next_audit_ordinal: 0,
            next_ordinal: 0,
            requests: BTreeMap::new(),
        };

        if requests_path.exists() {
            let text = std::fs::read_to_string(&requests_path)
                .with_context(|| format!("cannot read {}", requests_path.display()))?;
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let event: StoreEvent =
                    serde_json::from_str(line).context("access request event fails parse")?;
                state.next_ordinal += 1;
                match event {
                    StoreEvent::Created { request } => {
                        state.by_key.insert(
                            (
                                request.request_key.clone(),
                                request.snapshot_version.clone(),
                            ),
                            request.request_id.clone(),
                        );
                        state.requests.insert(request.request_id.clone(), request);
                    }
                    StoreEvent::Decided {
                        actor_principal,
                        decided_ordinal,
                        outcome,
                        reason_code,
                        request_id,
                        status,
                    } => {
                        let request = state
                            .requests
                            .get_mut(&request_id)
                            .context("decided event for unknown access request; store corrupt")?;
                        request.status = status;
                        request.decision = Some(AccessDecision {
                            actor_principal,
                            decided_ordinal,
                            outcome,
                            reason_code,
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

        Ok(AccessRequestStore {
            audit_path,
            requests_path,
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

    pub fn request_key(requester_id: &str, target: &AccessTarget) -> String {
        let preimage = format!(
            "{}\n{}\n{}",
            requester_id,
            target.kind(),
            target.capability_id()
        );
        sha256_hex(preimage.as_bytes())
    }

    pub fn create(
        &self,
        requester_id: &str,
        target: AccessTarget,
        justification: &str,
        approver_id: &str,
        snapshot_version: &str,
    ) -> Result<CreateOutcome> {
        let key = Self::request_key(requester_id, &target);
        let mut state = self.state.lock().expect("store mutex");
        if let Some(existing_id) = state
            .by_key
            .get(&(key.clone(), snapshot_version.to_string()))
            .cloned()
        {
            let existing = state
                .requests
                .get(&existing_id)
                .context("request key points at missing request; store corrupt")?
                .clone();
            return Ok(CreateOutcome::Existing(Box::new(existing)));
        }

        let id_hash = sha256_hex(format!("{key}\n{snapshot_version}").as_bytes());
        let request_id = format!("ar_{}", &id_hash[..16]);
        if state.requests.contains_key(&request_id) {
            bail!("access request id collision; refusing (store corrupt?)");
        }
        let request = AccessRequest {
            approver_id: approver_id.to_string(),
            created_ordinal: state.next_ordinal,
            decision: None,
            justification: justification.to_string(),
            request_id,
            request_key: key.clone(),
            requester_id: requester_id.to_string(),
            snapshot_version: snapshot_version.to_string(),
            status: STATUS_PENDING.to_string(),
            target,
        };
        Self::append(
            &self.requests_path,
            &StoreEvent::Created {
                request: request.clone(),
            },
        )?;
        state.next_ordinal += 1;
        state.by_key.insert(
            (key, snapshot_version.to_string()),
            request.request_id.clone(),
        );
        state
            .requests
            .insert(request.request_id.clone(), request.clone());
        Ok(CreateOutcome::Created(Box::new(request)))
    }

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

    pub fn decide(
        &self,
        request_id: &str,
        status: &str,
        actor_principal: &str,
        reason_code: Option<String>,
        current_snapshot: &str,
    ) -> Result<Result<AccessRequest, DecideError>> {
        if status != STATUS_APPROVED && status != STATUS_DENIED {
            bail!("invalid access request decision status");
        }
        let mut state = self.state.lock().expect("store mutex");
        let Some(request) = state.requests.get(request_id) else {
            return Ok(Err(DecideError::NotFound));
        };
        if request.snapshot_version != current_snapshot {
            return Ok(Err(DecideError::Stale));
        }
        if request.status != STATUS_PENDING {
            return Ok(Err(DecideError::AlreadyDecided));
        }
        let decided_ordinal = state.next_ordinal;
        Self::append(
            &self.requests_path,
            &StoreEvent::Decided {
                actor_principal: actor_principal.to_string(),
                decided_ordinal,
                outcome: status.to_string(),
                reason_code: reason_code.clone(),
                request_id: request_id.to_string(),
                status: status.to_string(),
            },
        )?;
        state.next_ordinal += 1;
        let request = state.requests.get_mut(request_id).expect("checked above");
        request.status = status.to_string();
        request.decision = Some(AccessDecision {
            actor_principal: actor_principal.to_string(),
            decided_ordinal,
            outcome: status.to_string(),
            reason_code,
        });
        Ok(Ok(request.clone()))
    }

    pub fn get(&self, request_id: &str) -> Option<AccessRequest> {
        self.state
            .lock()
            .expect("store mutex")
            .requests
            .get(request_id)
            .cloned()
    }

    pub fn requested_by(&self, requester_id: &str) -> Vec<AccessRequest> {
        let state = self.state.lock().expect("store mutex");
        let mut requests: Vec<AccessRequest> = state
            .requests
            .values()
            .filter(|request| request.requester_id == requester_id)
            .cloned()
            .collect();
        requests.sort_by_key(|request| request.created_ordinal);
        requests
    }

    pub fn inbox_for(&self, approver_id: &str) -> Vec<AccessRequest> {
        let state = self.state.lock().expect("store mutex");
        let mut requests: Vec<AccessRequest> = state
            .requests
            .values()
            .filter(|request| {
                request.approver_id == approver_id && request.status == STATUS_PENDING
            })
            .cloned()
            .collect();
        requests.sort_by_key(|request| request.created_ordinal);
        requests
    }
}
