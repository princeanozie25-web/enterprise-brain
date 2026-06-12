//! Standing-query configuration. One registry, built at startup, fail
//! closed: unknown agent ids, agents absent from the fixtures, empty or
//! oversized query sets all refuse. Owners come from the hash-verified
//! company fixture, never from config — config cannot reassign an agent.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use retrieval::index::sha256_hex;
use serde::Deserialize;

use super::runner::STANDING_QUERIES_MAX;

/// One configured agent, with its fixture-derived owner.
#[derive(Debug, Clone)]
pub struct AgentEntry {
    pub agent_id: String,
    pub owner_user_id: String,
    pub standing_queries: Vec<String>,
    pub hybrid: bool,
    pub judge: bool,
}

/// The agent registry: configured agents plus the fixture-wide principal
/// classification (agent vs human) the authority checks depend on.
pub struct AgentRegistry {
    configured: BTreeMap<String, AgentEntry>,
    fixture_agents: BTreeSet<String>,
    people: BTreeSet<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentsConfigFile {
    agents: Vec<AgentConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentConfig {
    agent_id: String,
    standing_queries: Vec<String>,
    #[serde(default)]
    hybrid: bool,
    #[serde(default)]
    judge: bool,
}

/// Minimal company mirror: principal classification + agent ownership.
#[derive(Debug, Deserialize)]
struct CompanyLite {
    people: Vec<IdOnly>,
    agents: Vec<AgentLite>,
}

#[derive(Debug, Deserialize)]
struct IdOnly {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AgentLite {
    id: String,
    owner_user_id: String,
}

impl AgentRegistry {
    /// Loads the registry: company.json re-verified against the M1-pinned
    /// hash, then the standing config validated against it.
    pub fn load(
        config_path: &Path,
        fixtures_dir: &Path,
        expected_company_sha256: &str,
    ) -> Result<AgentRegistry> {
        let company_path = fixtures_dir.join("company.json");
        let company_bytes = std::fs::read(&company_path)
            .with_context(|| format!("cannot read fixture {}", company_path.display()))?;
        if sha256_hex(&company_bytes) != expected_company_sha256 {
            bail!("company.json does not match the M1-pinned hash; refusing");
        }
        let company: CompanyLite = serde_json::from_slice(&company_bytes)
            .with_context(|| format!("fixture {} fails parse", company_path.display()))?;
        let owners: BTreeMap<String, String> = company
            .agents
            .iter()
            .map(|a| (a.id.clone(), a.owner_user_id.clone()))
            .collect();
        let fixture_agents: BTreeSet<String> = owners.keys().cloned().collect();
        let people: BTreeSet<String> = company.people.into_iter().map(|p| p.id).collect();

        let config_bytes = std::fs::read(config_path)
            .with_context(|| format!("cannot read agents config {}", config_path.display()))?;
        let config: AgentsConfigFile = serde_json::from_slice(&config_bytes)
            .with_context(|| format!("agents config {} fails parse", config_path.display()))?;
        if config.agents.is_empty() {
            bail!("agents config configures no agents; refusing");
        }

        let mut configured = BTreeMap::new();
        for agent in config.agents {
            let Some(owner) = owners.get(&agent.agent_id) else {
                bail!(
                    "agents config names unknown agent {:?}; refusing",
                    agent.agent_id
                );
            };
            if agent.standing_queries.is_empty() {
                bail!(
                    "agent {:?} has an empty standing-query set; refusing",
                    agent.agent_id
                );
            }
            if agent.standing_queries.len() > STANDING_QUERIES_MAX {
                bail!(
                    "agent {:?} has more than {STANDING_QUERIES_MAX} standing queries; refusing",
                    agent.agent_id
                );
            }
            if agent.standing_queries.iter().any(|q| q.trim().is_empty()) {
                bail!(
                    "agent {:?} has an empty standing query; refusing",
                    agent.agent_id
                );
            }
            let entry = AgentEntry {
                agent_id: agent.agent_id.clone(),
                owner_user_id: owner.clone(),
                standing_queries: agent.standing_queries,
                hybrid: agent.hybrid,
                judge: agent.judge,
            };
            if configured.insert(agent.agent_id.clone(), entry).is_some() {
                bail!(
                    "agents config configures {:?} twice; refusing",
                    agent.agent_id
                );
            }
        }
        Ok(AgentRegistry {
            configured,
            fixture_agents,
            people,
        })
    }

    pub fn configured(&self, agent_id: &str) -> Option<&AgentEntry> {
        self.configured.get(agent_id)
    }

    /// True for ANY fixture agent (configured or not) — the structural
    /// refusal for approve/reject and run-invocation applies to all of them.
    pub fn is_agent_principal(&self, principal_id: &str) -> bool {
        self.fixture_agents.contains(principal_id)
    }

    pub fn is_human(&self, principal_id: &str) -> bool {
        self.people.contains(principal_id)
    }

    pub fn configured_count(&self) -> usize {
        self.configured.len()
    }
}
