//! The agent runner — the code that IS the agent.
//!
//! CAPABILITY DISCIPLINE, BY CONSTRUCTION: this module imports ONLY the
//! context trait and its types (`super::context`). No store, no state, no
//! config, no service handle, no filesystem, no clock is reachable from
//! here. If an import beyond `super::context` ever appears in this file,
//! the capability rule is broken and review must refuse it.
//!
//! The runner turns standing queries into proposal drafts:
//! retrieve -> (answered?) -> draft with the answer's own citations ->
//! propose. It executes nothing, mutates nothing, approves nothing.

use super::context::{
    AgentContext, AgentResult, ProposalDraft, ProposeOutcome, CITATIONS_MAX, PROPOSALS_PER_RUN_CAP,
    RATIONALE_MAX_CHARS,
};

/// NUMBERS: at most 6 standing queries are honored per run.
pub const STANDING_QUERIES_MAX: usize = 6;

/// What the runner reports — counts only; the orchestrator on the service
/// side of the boundary holds the real records.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RunReport {
    pub queries_run: usize,
    pub proposals_created: usize,
    pub proposals_deduplicated: usize,
    pub drafts_refused: usize,
}

/// One run: each standing query is retrieved at the agent's intersection
/// scope; every answered query becomes at most one proposal draft whose
/// evidence is exactly the answer's validated citations.
pub fn run(context: &mut dyn AgentContext, standing_queries: &[String]) -> AgentResult<RunReport> {
    let mut report = RunReport::default();
    for query in standing_queries.iter().take(STANDING_QUERIES_MAX) {
        if report.proposals_created >= PROPOSALS_PER_RUN_CAP {
            break;
        }
        let retrieved = context.retrieve(query)?;
        report.queries_run += 1;

        let Some(answer) = retrieved.answer else {
            // Degraded or empty retrieval: no proposal. Less compute is an
            // acceptable degradation; an unevidenced proposal is not.
            continue;
        };
        let citations: Vec<String> = answer.citations.into_iter().take(CITATIONS_MAX).collect();
        let rationale: String = answer.text.chars().take(RATIONALE_MAX_CHARS).collect();
        let draft = ProposalDraft {
            standing_query: query.clone(),
            rationale,
            citations,
        };
        match context.propose(draft)? {
            ProposeOutcome::Created { .. } => report.proposals_created += 1,
            ProposeOutcome::Deduplicated => report.proposals_deduplicated += 1,
            ProposeOutcome::Refused { .. } => report.drafts_refused += 1,
        }
    }
    Ok(report)
}
