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
use std::sync::{Arc, Mutex};

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
    /// AP-4: the two sides of a `lens_diff` act. Absent on every other
    /// action (optional + defaulted, so pre-AP-4 rows parse and pre-AP-4
    /// writers stay byte-identical).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
    pub ordinal: u64,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
    pub target: String,
    /// S0: token-path attribution (`agent_token` rows only; optional +
    /// defaulted so every pre-S0 row parses and every pre-S0 writer stays
    /// byte-identical). Claims only — never the token or its signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_tid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_oid: Option<String>,
    /// Normalized client attribution (`azp`, v1 `appid`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_azp: Option<String>,
    /// `xms_par_app_azp` when present — logged per Microsoft's guidance
    /// (sign-in logs always include the parent id); NEVER an authority key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_parent_azp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_aud: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_uti: Option<String>,
    /// S1: `/v1` request attribution (`v1_*` rows only; optional + defaulted
    /// so every pre-S1 row parses and every pre-S1 writer stays
    /// byte-identical). `query` is the retrieve query, stored verbatim up to
    /// 2,048 chars; `candidates` are the returned document ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<String>>,
    /// S2b: what a successful `v1_document` served (`"full"` once the
    /// machine surface serves whole bodies). Optional + defaulted: every
    /// pre-S2b row parses and every pre-S2b writer stays byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    /// S3: which source a `/v1` document/retrieve row concerns
    /// (`"primary"` | `"s3"`). Optional + defaulted so every pre-S3 row
    /// parses and every pre-S3 writer stays byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// S4: RFC3339 UTC millisecond timestamp, from the injected clock.
    /// Present ONLY on rows written by a clock-configured (S4-era) store;
    /// absent on every pre-S4 row and on any store opened without a clock,
    /// so the exact-byte determinism of the legacy writers is preserved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<String>,
    /// S4: the tamper-evidence link — sha256 (hex) of the PREVIOUS line's
    /// raw bytes as written (the first row of a fresh file links to
    /// sha256 of the empty string). Present only on S4-era rows; the chain
    /// anchors OVER legacy rows without rewriting them. Absent = pre-S4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev: Option<String>,
}

/// S0: the claim attribution carried on one `agent_token` audit row.
#[derive(Debug, Clone, Default)]
pub struct TokenAuditFields {
    pub tid: Option<String>,
    pub oid: Option<String>,
    pub azp: Option<String>,
    pub parent_azp: Option<String>,
    pub aud: Option<String>,
    pub uti: Option<String>,
}

struct StoreState {
    proposals: BTreeMap<String, Proposal>,
    /// (proposal_key, snapshot_version) -> proposal_id (dedupe scope).
    by_key: BTreeMap<(String, String), String>,
    next_ordinal: u64,
    next_audit_ordinal: u64,
    /// S4: sha256 (hex) of the LAST audit line's raw bytes as written —
    /// the value the next S4-era row's `prev` links to. Computed at open
    /// from the existing file (or sha256 of the empty string for a fresh
    /// one), so the chain anchors over any legacy tail without rewriting it.
    audit_chain_tip: String,
}

pub struct ProposalStore {
    proposals_path: PathBuf,
    audit_path: PathBuf,
    state: Mutex<StoreState>,
    /// S4: the timestamp/chain clock. `None` = legacy mode (no `ts`, no
    /// `prev` — pre-S4 byte-identical output, used by every existing
    /// caller and the legacy exact-JSON pin). `Some` = S4 mode: rows carry
    /// `ts` + `prev` and the ledger is hash-chained.
    clock: Option<Arc<dyn crate::clock::Clock>>,
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
    /// Legacy mode: no clock — rows carry no `ts`/`prev` and every writer
    /// stays byte-identical to pre-S4 (the exact-JSON pin depends on this).
    pub fn open(dir: &Path) -> Result<ProposalStore> {
        Self::open_inner(dir, None)
    }

    /// S4: opens the store in CHAINED mode — every new audit row carries an
    /// injected-clock `ts` and a `prev` hash, and the ledger is
    /// tamper-evident. The chain anchors over any pre-S4 tail already in
    /// the file without rewriting it.
    pub fn open_chained(dir: &Path, clock: Arc<dyn crate::clock::Clock>) -> Result<ProposalStore> {
        Self::open_inner(dir, Some(clock))
    }

    fn open_inner(
        dir: &Path,
        clock: Option<Arc<dyn crate::clock::Clock>>,
    ) -> Result<ProposalStore> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("cannot create proposal store dir {}", dir.display()))?;
        let proposals_path = dir.join("proposals.jsonl");
        let audit_path = dir.join("audit.jsonl");

        let mut state = StoreState {
            proposals: BTreeMap::new(),
            by_key: BTreeMap::new(),
            next_ordinal: 0,
            next_audit_ordinal: 0,
            audit_chain_tip: sha256_hex(b""),
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
            let raw = std::fs::read(&audit_path)
                .with_context(|| format!("cannot read {}", audit_path.display()))?;
            state.next_audit_ordinal = count_lines(&raw);
            // S4: anchor the chain to the LAST line's raw bytes as written
            // (whatever era it belongs to), so the next S4 row links over it.
            if let Some(last) = last_line_bytes(&raw) {
                state.audit_chain_tip = sha256_hex(last);
            }
        }
        Ok(ProposalStore {
            proposals_path,
            audit_path,
            state: Mutex::new(state),
            clock,
        })
    }

    /// S4: append one audit row under the store lock, stamping `ts` + `prev`
    /// and advancing the chain tip when in chained mode. In legacy mode
    /// (no clock) the event is written exactly as before — byte-identical.
    /// Returns the row's ordinal.
    fn append_audit(&self, state: &mut StoreState, mut event: AuditEvent) -> Result<u64> {
        let ordinal = state.next_audit_ordinal;
        event.ordinal = ordinal;
        if let Some(clock) = &self.clock {
            event.ts = Some(clock.now_rfc3339_ms());
            event.prev = Some(state.audit_chain_tip.clone());
        }
        let bytes = canonical_json_bytes(&event)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.audit_path)
            .with_context(|| format!("cannot open {}", self.audit_path.display()))?;
        file.write_all(&bytes)
            .with_context(|| format!("cannot append to {}", self.audit_path.display()))?;
        file.sync_data()
            .with_context(|| format!("cannot sync {}", self.audit_path.display()))?;
        // The tip becomes THIS row's bytes — the next row links to it.
        if self.clock.is_some() {
            state.audit_chain_tip = sha256_hex(&bytes);
        }
        state.next_audit_ordinal += 1;
        Ok(ordinal)
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
            left: None,
            ordinal,
            outcome: outcome.to_string(),
            right: None,
            target: target.to_string(),
            token_tid: None,
            token_oid: None,
            token_azp: None,
            token_parent_azp: None,
            token_aud: None,
            token_uti: None,
            query: None,
            candidates: None,
            payload: None,
            source: None,
            ts: None,
            prev: None,
        };
        self.append_audit(&mut state, event)
    }

    /// AP-4: the `lens_diff` audit row — ONE act with TWO subjects, never
    /// two lens_view rows. Same append+sync discipline as `audit`; `target`
    /// keeps the legacy `left|right` form so the log greps uniformly.
    pub fn audit_diff(&self, actor: &str, left: &str, right: &str, outcome: &str) -> Result<u64> {
        let mut state = self.state.lock().expect("store mutex");
        let ordinal = state.next_audit_ordinal;
        let event = AuditEvent {
            action: "lens_diff".to_string(),
            actor_principal: actor.to_string(),
            left: Some(left.to_string()),
            ordinal,
            outcome: outcome.to_string(),
            right: Some(right.to_string()),
            target: format!("{left}|{right}"),
            token_tid: None,
            token_oid: None,
            token_azp: None,
            token_parent_azp: None,
            token_aud: None,
            token_uti: None,
            query: None,
            candidates: None,
            payload: None,
            source: None,
            ts: None,
            prev: None,
        };
        self.append_audit(&mut state, event)
    }

    /// S0: one `agent_token` audit row — EVERY token-path decision, allow
    /// AND deny, written through the same append+sync ledger (EB-6; denies
    /// double as the EB-7 monitoring signal). `actor` is the resolved EB
    /// principal when resolution was reached, else the literal
    /// `"unresolved"`; `target` is the attempted `METHOD /path`; `outcome`
    /// is the ladder reason code (or `authorized`). Attribution claims only
    /// — the raw token and its signature are NEVER logged.
    pub fn audit_agent_token(
        &self,
        actor: &str,
        target: &str,
        outcome: &str,
        token: &TokenAuditFields,
    ) -> Result<u64> {
        let mut state = self.state.lock().expect("store mutex");
        let ordinal = state.next_audit_ordinal;
        let event = AuditEvent {
            action: "agent_token".to_string(),
            actor_principal: actor.to_string(),
            left: None,
            ordinal,
            outcome: outcome.to_string(),
            right: None,
            target: target.to_string(),
            token_tid: token.tid.clone(),
            token_oid: token.oid.clone(),
            token_azp: token.azp.clone(),
            token_parent_azp: token.parent_azp.clone(),
            token_aud: token.aud.clone(),
            token_uti: token.uti.clone(),
            query: None,
            candidates: None,
            payload: None,
            source: None,
            ts: None,
            prev: None,
        };
        self.append_audit(&mut state, event)
    }

    /// S1: one `/v1` surface audit row — EVERY `/v1` request, allow AND
    /// deny, before its effect reaches the wire. `action` names the surface
    /// (`v1_retrieve` / `v1_document` / `v1_whoami` / `v1_unknown_route`);
    /// `outcome` is `authorized` or the ledger-only deny reason; `query`
    /// (retrieve only) is stored verbatim capped at 2,048 chars by the
    /// caller; `candidates` (retrieve only) are the returned document ids.
    /// Same discipline as every other row: claims attribution only — the
    /// raw token and its signature are NEVER logged.
    #[allow(clippy::too_many_arguments)]
    pub fn audit_v1(
        &self,
        action: &str,
        actor: &str,
        target: &str,
        outcome: &str,
        token: &TokenAuditFields,
        query: Option<&str>,
        candidates: Option<&[String]>,
        payload: Option<&str>,
        source: Option<&str>,
    ) -> Result<u64> {
        let mut state = self.state.lock().expect("store mutex");
        let ordinal = state.next_audit_ordinal;
        let event = AuditEvent {
            action: action.to_string(),
            actor_principal: actor.to_string(),
            left: None,
            ordinal,
            outcome: outcome.to_string(),
            right: None,
            target: target.to_string(),
            token_tid: token.tid.clone(),
            token_oid: token.oid.clone(),
            token_azp: token.azp.clone(),
            token_parent_azp: token.parent_azp.clone(),
            token_aud: token.aud.clone(),
            token_uti: token.uti.clone(),
            query: query.map(str::to_string),
            candidates: candidates.map(<[String]>::to_vec),
            payload: payload.map(str::to_string),
            source: source.map(str::to_string),
            ts: None,
            prev: None,
        };
        self.append_audit(&mut state, event)
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

// ---------------------------------------------------------------------------
// S4: hash-chain helpers + tamper-evidence verification
// ---------------------------------------------------------------------------

/// Count non-empty lines in raw ledger bytes (the audit ordinal at open).
fn count_lines(raw: &[u8]) -> u64 {
    raw.split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .count() as u64
}

/// The last line's raw bytes AS WRITTEN — including its trailing newline
/// (every row is written by `canonical_json_bytes`, which appends `\n`, so
/// the chain tip hashes the newline too). `None` for an empty file. Rows
/// always end in `\n`, so the last line runs from just after the
/// second-to-last newline through the final one.
fn last_line_bytes(raw: &[u8]) -> Option<&[u8]> {
    let last_nl = raw.iter().rposition(|b| *b == b'\n')?;
    let start = raw[..last_nl]
        .iter()
        .rposition(|b| *b == b'\n')
        .map(|p| p + 1)
        .unwrap_or(0);
    Some(&raw[start..=last_nl])
}

/// The outcome of verifying a ledger's hash chain.
#[derive(Debug, PartialEq, Eq)]
pub enum LedgerVerification {
    /// The chain is intact: every S4-era row's `prev` matches the sha256 of
    /// the previous line's bytes as written (legacy rows are anchored over,
    /// never rewritten).
    Clean { rows: u64, chained_rows: u64 },
    /// The chain breaks at this 0-based ordinal — the first row whose `prev`
    /// does not match the recomputed hash of its predecessor (a flipped
    /// byte, a deleted line, an inserted row).
    Broken { ordinal: u64, detail: String },
}

/// S4 `verify-ledger`: walk a ledger file and recompute the hash chain.
/// Every row carrying a `prev` (S4-era) must link to the sha256 of the
/// previous line's raw bytes as written; the very first row of a file links
/// to sha256 of the empty string. Legacy rows (no `prev`) are not checked
/// against a predecessor but ARE hashed as predecessors, so the chain
/// anchors across the era boundary. Reports the first breaking ordinal or
/// CLEAN. (Exposed as a library fn; a test-invocable path and the
/// `service verify-ledger <path>` binary subcommand both call it.)
pub fn verify_ledger(path: &Path) -> Result<LedgerVerification> {
    let raw = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    // Reconstruct each row's bytes-as-written (segment + '\n').
    let mut rows: Vec<&[u8]> = Vec::new();
    let mut start = 0usize;
    for (i, b) in raw.iter().enumerate() {
        if *b == b'\n' {
            if i + 1 > start {
                let seg = &raw[start..=i];
                // Skip a purely-empty line (just "\n").
                if seg.len() > 1 {
                    rows.push(seg);
                }
            }
            start = i + 1;
        }
    }
    let empty_hash = sha256_hex(b"");
    let mut chained = 0u64;
    for (ordinal, row) in rows.iter().enumerate() {
        // Parse just the optional `prev` field (ignore everything else).
        let text = std::str::from_utf8(row)
            .with_context(|| format!("row {ordinal} is not valid UTF-8"))?;
        let value: Value = serde_json::from_str(text.trim_end())
            .with_context(|| format!("row {ordinal} fails parse"))?;
        let Some(prev) = value.get("prev").and_then(Value::as_str) else {
            continue; // legacy row — anchored over, not checked
        };
        chained += 1;
        let expected = if ordinal == 0 {
            empty_hash.clone()
        } else {
            sha256_hex(rows[ordinal - 1])
        };
        if prev != expected {
            return Ok(LedgerVerification::Broken {
                ordinal: ordinal as u64,
                detail: format!("prev={prev} but predecessor hashes to {expected}"),
            });
        }
    }
    Ok(LedgerVerification::Clean {
        rows: rows.len() as u64,
        chained_rows: chained,
    })
}
