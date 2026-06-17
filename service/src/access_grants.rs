//! Read-grant ledger: append-only grants derived from approved access
//! requests. A grant is a runtime entitlement record only; it does not mutate
//! compiler artifacts, retrieval indexes, or document allowlists.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{bail, Context, Result};
use retrieval::index::{canonical_json_bytes, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::access_requests::{AccessRequest, AccessTarget, STATUS_APPROVED};

pub const PERMISSION_READ: &str = "read";
pub const STATUS_ACTIVE: &str = "active";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessGrant {
    pub approver_id: String,
    pub created_ordinal: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub grant_id: String,
    pub grantee_id: String,
    pub permission: String,
    pub reason: String,
    pub request_id: String,
    pub snapshot_version: String,
    pub status: String,
    pub target: AccessTarget,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
enum StoreEvent {
    Created { grant: AccessGrant },
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
    by_request: BTreeMap<String, String>,
    grants: BTreeMap<String, AccessGrant>,
    next_audit_ordinal: u64,
    next_ordinal: u64,
}

pub struct AccessGrantStore {
    audit_path: PathBuf,
    grants_path: PathBuf,
    state: Mutex<StoreState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantCreateOutcome {
    Created(Box<AccessGrant>),
    Existing(Box<AccessGrant>),
}

impl AccessGrantStore {
    pub fn open(dir: &Path) -> Result<AccessGrantStore> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("cannot create access grant store dir {}", dir.display()))?;
        let grants_path = dir.join("access_grants.jsonl");
        let audit_path = dir.join("access_grants_audit.jsonl");
        let mut state = StoreState {
            by_request: BTreeMap::new(),
            grants: BTreeMap::new(),
            next_audit_ordinal: 0,
            next_ordinal: 0,
        };

        if grants_path.exists() {
            let text = std::fs::read_to_string(&grants_path)
                .with_context(|| format!("cannot read {}", grants_path.display()))?;
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                let event: StoreEvent =
                    serde_json::from_str(line).context("access grant event fails parse")?;
                state.next_ordinal += 1;
                match event {
                    StoreEvent::Created { grant } => {
                        state
                            .by_request
                            .insert(grant.request_id.clone(), grant.grant_id.clone());
                        state.grants.insert(grant.grant_id.clone(), grant);
                    }
                }
            }
        }

        if audit_path.exists() {
            let text = std::fs::read_to_string(&audit_path)
                .with_context(|| format!("cannot read {}", audit_path.display()))?;
            state.next_audit_ordinal =
                text.lines().filter(|line| !line.trim().is_empty()).count() as u64;
        }

        Ok(AccessGrantStore {
            audit_path,
            grants_path,
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

    pub fn create_from_approved_request(
        &self,
        request: &AccessRequest,
    ) -> Result<GrantCreateOutcome> {
        if request.status != STATUS_APPROVED {
            bail!("cannot create an access grant from a non-approved request");
        }

        let mut state = self.state.lock().expect("store mutex");
        if let Some(existing_id) = state.by_request.get(&request.request_id).cloned() {
            let existing = state
                .grants
                .get(&existing_id)
                .context("grant request index points at missing grant; store corrupt")?
                .clone();
            return Ok(GrantCreateOutcome::Existing(Box::new(existing)));
        }

        let id_hash = sha256_hex(
            format!(
                "{}\n{}\n{}",
                request.request_id, PERMISSION_READ, request.snapshot_version
            )
            .as_bytes(),
        );
        let grant_id = format!("ag_{}", &id_hash[..16]);
        if state.grants.contains_key(&grant_id) {
            bail!("access grant id collision; refusing (store corrupt?)");
        }
        let reason = request
            .decision
            .as_ref()
            .and_then(|decision| decision.reason_code.clone())
            .unwrap_or_else(|| "approved_access_request".to_string());
        let grant = AccessGrant {
            approver_id: request.approver_id.clone(),
            created_ordinal: state.next_ordinal,
            expires_at: None,
            grant_id,
            grantee_id: request.requester_id.clone(),
            permission: PERMISSION_READ.to_string(),
            reason,
            request_id: request.request_id.clone(),
            snapshot_version: request.snapshot_version.clone(),
            status: STATUS_ACTIVE.to_string(),
            target: request.target.clone(),
        };

        Self::append(
            &self.grants_path,
            &StoreEvent::Created {
                grant: grant.clone(),
            },
        )?;
        state.next_ordinal += 1;
        state
            .by_request
            .insert(grant.request_id.clone(), grant.grant_id.clone());
        state.grants.insert(grant.grant_id.clone(), grant.clone());
        Ok(GrantCreateOutcome::Created(Box::new(grant)))
    }

    pub fn get(&self, grant_id: &str) -> Option<AccessGrant> {
        self.state
            .lock()
            .expect("store mutex")
            .grants
            .get(grant_id)
            .cloned()
    }

    pub fn visible_to(&self, actor: &str) -> Vec<AccessGrant> {
        let state = self.state.lock().expect("store mutex");
        let mut grants: Vec<AccessGrant> = state
            .grants
            .values()
            .filter(|grant| grant.grantee_id == actor || grant.approver_id == actor)
            .cloned()
            .collect();
        grants.sort_by_key(|grant| grant.created_ordinal);
        grants
    }

    pub fn has_active_read_for(
        &self,
        grantee_id: &str,
        capability_id: &str,
        snapshot_version: &str,
    ) -> bool {
        let state = self.state.lock().expect("store mutex");
        state.grants.values().any(|grant| {
            grant.grantee_id == grantee_id
                && grant.permission == PERMISSION_READ
                && grant.status == STATUS_ACTIVE
                && grant.snapshot_version == snapshot_version
                && grant.target.capability_id() == capability_id
        })
    }
}
