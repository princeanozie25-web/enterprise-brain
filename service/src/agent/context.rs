//! THE CAPABILITY RULE. The agent runner receives an [`AgentContext`]
//! exposing EXACTLY TWO capabilities: `retrieve` (the M3a pipeline as a
//! library, invoked AS the agent principal — the intersection is enforced by
//! the M1 artifacts, nothing new) and `propose` (append to the proposal
//! store). No other object, handle, store, config, or service reference is
//! reachable from agent code: `runner.rs` imports only this module, and AG-2
//! verifies at runtime that 100 runs leave every fixture, artifact, index,
//! and config byte-identical.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::{bail, Context as _};

use crate::answer::{ask, AnswerEnvelope, AskError, AskOptions};
use crate::sidecar::UsageEvent;
use crate::AppState;

use super::proposals::{CreateOutcome, Proposal, ProposalStore};
use super::standing::AgentEntry;

/// Errors agent code can produce. A type alias so the runner imports
/// nothing beyond this module.
pub type AgentResult<T> = anyhow::Result<T>;

/// What `retrieve` hands back to agent code: the validated answer, if the
/// pipeline produced one. Nothing else — no envelopes, no traces, no scope.
pub struct Retrieved {
    pub answer: Option<RetrievedAnswer>,
}

/// The sealed-context answer exactly as M3a validated it: text whose every
/// bracketed citation passed validation, plus the cited ids in order.
pub struct RetrievedAnswer {
    pub text: String,
    pub citations: Vec<String>,
}

/// A proposal draft as agent code submits it. Validation happens on the
/// store side of the boundary — the agent is untrusted, like the generator.
#[derive(Debug, Clone)]
pub struct ProposalDraft {
    pub standing_query: String,
    pub rationale: String,
    pub citations: Vec<String>,
}

/// What `propose` reports back.
#[derive(Debug, PartialEq, Eq)]
pub enum ProposeOutcome {
    Created {
        proposal_id: String,
    },
    /// Same (agent, query, evidence) under this snapshot already proposed.
    Deduplicated,
    /// The draft failed validation; counted as a proposal fault.
    Refused {
        reason: &'static str,
    },
}

/// The two capabilities. This trait is the ONLY surface agent code sees.
pub trait AgentContext {
    fn retrieve(&mut self, query: &str) -> AgentResult<Retrieved>;
    fn propose(&mut self, draft: ProposalDraft) -> AgentResult<ProposeOutcome>;
}

/// NUMBERS: hard cap on proposals per run.
pub const PROPOSALS_PER_RUN_CAP: usize = 12;
/// NUMBERS: rationale and citation bounds.
pub const RATIONALE_MAX_CHARS: usize = 600;
pub const CITATIONS_MIN: usize = 1;
pub const CITATIONS_MAX: usize = 4;

// ---------------------------------------------------------------------------
// Production context
// ---------------------------------------------------------------------------

/// The production capability implementation, plus the instrumentation the
/// governance harness reads (traces, faults, usage). Instrumentation is
/// write-only from the agent's perspective — the trait exposes none of it.
pub struct ProductionContext<'a> {
    state: &'a AppState,
    agent: &'a AgentEntry,
    store: &'a Arc<ProposalStore>,
    /// The agent's compiled (intersection) allowlist, for draft validation.
    allowlist: BTreeSet<String>,
    pub retrieval_traces: Vec<crate::answer::AskTrace>,
    pub usage_events: Vec<UsageEvent>,
    pub created: Vec<Proposal>,
    pub deduplicated: usize,
    pub proposal_faults: u32,
    proposals_this_run: usize,
}

impl<'a> ProductionContext<'a> {
    pub fn new(
        state: &'a AppState,
        agent: &'a AgentEntry,
        store: &'a Arc<ProposalStore>,
    ) -> anyhow::Result<ProductionContext<'a>> {
        let allowlist = agent_allowlist(state, &agent.agent_id)?;
        Ok(ProductionContext {
            state,
            agent,
            store,
            allowlist,
            retrieval_traces: Vec::new(),
            usage_events: Vec::new(),
            created: Vec::new(),
            deduplicated: 0,
            proposal_faults: 0,
            proposals_this_run: 0,
        })
    }
}

/// The agent's compiled allowlist, re-verified byte-for-byte against the M1
/// index on every load (the /doc pattern).
fn agent_allowlist(state: &AppState, agent_id: &str) -> anyhow::Result<BTreeSet<String>> {
    let (artifact_file, artifact_sha) = state
        .artifact_rows
        .get(agent_id)
        .with_context(|| format!("no compiled allowlist for agent {agent_id}"))?;
    let path = state.artifacts_dir.join(artifact_file);
    let bytes =
        std::fs::read(&path).with_context(|| format!("cannot read artifact {}", path.display()))?;
    if &retrieval::index::sha256_hex(&bytes) != artifact_sha {
        bail!(
            "artifact {} does not match the M1 index hash; refusing",
            path.display()
        );
    }
    let artifact: crate::ArtifactLite = serde_json::from_slice(&bytes)
        .with_context(|| format!("artifact {} fails parse", path.display()))?;
    Ok(artifact
        .entries
        .into_iter()
        .map(|e| e.document_id)
        .collect())
}

impl AgentContext for ProductionContext<'_> {
    /// The M3a ask pipeline, invoked AS the agent principal. The agent gets
    /// the validated answer or nothing.
    fn retrieve(&mut self, query: &str) -> AgentResult<Retrieved> {
        let options = AskOptions {
            hybrid: self.agent.hybrid,
            judge: self.agent.judge,
            bypass_cache: false,
            granted_context: None,
        };
        let (bytes, trace) =
            ask(self.state, &self.agent.agent_id, query, &options).map_err(|err| match err {
                AskError::BadRequest(message) => anyhow::anyhow!("bad standing query: {message}"),
                AskError::Internal(inner) => inner,
            })?;
        let envelope: AnswerEnvelope = serde_json::from_slice(&bytes).context("envelope parses")?;
        self.usage_events.extend(trace.usage_events.iter().cloned());
        self.retrieval_traces.push(trace);
        Ok(Retrieved {
            answer: envelope.answer.map(|a| RetrievedAnswer {
                text: a.text,
                citations: a.citations,
            }),
        })
    }

    /// Validates the draft against the agent's effective scope (the same
    /// fail-closed posture as M3a citation validation) and appends it.
    /// Any failure refuses the WHOLE proposal and counts a fault.
    fn propose(&mut self, draft: ProposalDraft) -> AgentResult<ProposeOutcome> {
        if self.proposals_this_run >= PROPOSALS_PER_RUN_CAP {
            self.proposal_faults += 1;
            return Ok(ProposeOutcome::Refused {
                reason: "per-run proposal cap reached",
            });
        }
        if let Err(reason) = validate_draft(&draft, &self.allowlist) {
            self.proposal_faults += 1;
            return Ok(ProposeOutcome::Refused { reason });
        }
        let outcome = self.store.create(
            &self.agent.agent_id,
            &self.agent.owner_user_id,
            &self.state.snapshot_version,
            &self.state.engine.manifest.index_version,
            &draft,
        )?;
        Ok(match outcome {
            CreateOutcome::Created(proposal) => {
                self.proposals_this_run += 1;
                let proposal_id = proposal.proposal_id.clone();
                self.created.push(*proposal);
                ProposeOutcome::Created { proposal_id }
            }
            CreateOutcome::Deduplicated => {
                self.deduplicated += 1;
                ProposeOutcome::Deduplicated
            }
        })
    }
}

/// Draft validation, fail closed:
/// citations 1..=4, deduplicated, every one inside the agent's compiled
/// allowlist; rationale <= 600 chars; every bracketed segment in the
/// rationale must be one of the draft's own citations (so the rationale can
/// never smuggle an id its evidence list doesn't carry).
fn validate_draft(draft: &ProposalDraft, allowlist: &BTreeSet<String>) -> Result<(), &'static str> {
    let mut seen = BTreeSet::new();
    let citations: Vec<&str> = draft
        .citations
        .iter()
        .map(String::as_str)
        .filter(|c| seen.insert(*c))
        .collect();
    if citations.len() < CITATIONS_MIN {
        return Err("a proposal without citations is an unauditable claim");
    }
    if citations.len() > CITATIONS_MAX {
        return Err("too many citations");
    }
    for citation in &citations {
        if !allowlist.contains(*citation) {
            return Err("citation outside the agent's effective scope");
        }
    }
    if draft.rationale.chars().count() > RATIONALE_MAX_CHARS {
        return Err("rationale exceeds 600 chars");
    }
    let mut rest = draft.rationale.as_str();
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            break;
        };
        let token = &after[..close];
        if !citations.contains(&token) {
            return Err("rationale cites outside its own evidence list");
        }
        rest = &after[close + 1..];
    }
    if draft.standing_query.trim().is_empty() {
        return Err("empty standing query");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Run orchestration (service side of the boundary)
// ---------------------------------------------------------------------------

/// Everything one explicit run produced. Instrumentation for the harness and
/// the sidecar; never serialized wholesale into any response.
pub struct RunOutcome {
    pub agent_id: String,
    pub created: Vec<Proposal>,
    pub deduplicated: usize,
    pub proposal_faults: u32,
    pub retrieval_traces: Vec<crate::answer::AskTrace>,
    pub usage_events: Vec<UsageEvent>,
}

/// One explicit agent run: build the capability context, hand the runner the
/// standing queries, collect the outcome. No scheduler, no daemon — this is
/// only ever called from an authorized, audited invocation.
pub fn execute_run(
    state: &AppState,
    agent: &AgentEntry,
    store: &Arc<ProposalStore>,
) -> anyhow::Result<RunOutcome> {
    let mut context = ProductionContext::new(state, agent, store)?;
    super::runner::run(&mut context, &agent.standing_queries)?;
    Ok(RunOutcome {
        agent_id: agent.agent_id.clone(),
        created: context.created,
        deduplicated: context.deduplicated,
        proposal_faults: context.proposal_faults,
        retrieval_traces: context.retrieval_traces,
        usage_events: context.usage_events,
    })
}
