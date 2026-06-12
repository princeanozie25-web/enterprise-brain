//! The REAL scope statement, built from the identity model (company.json)
//! instead of M2a's reason-string derivation (which stays untouched in the
//! retrieval CLI). The company fixture is verified against the hash the M1
//! compile pinned before anything trusts it — fail closed on drift.
//!
//! Unknown principals get the empty statement: deny by default, and the
//! response shape never distinguishes "unknown" from "granted nothing".

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use retrieval::envelope::ScopeStatement;
use retrieval::index::sha256_hex;
use serde::Deserialize;

/// Minimal mirror of the company fixture — only what scope needs. M1 already
/// schema-validated the file; unknown fields are tolerated here.
#[derive(Debug, Deserialize)]
struct CompanyFile {
    people: Vec<Person>,
    groups: Vec<Group>,
    agents: Vec<Agent>,
}

#[derive(Debug, Deserialize)]
struct Person {
    id: String,
    employment_band: u8,
    site: String,
}

#[derive(Debug, Deserialize)]
struct Group {
    id: String,
    member_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Agent {
    id: String,
    grant: AgentGrant,
}

#[derive(Debug, Deserialize)]
struct AgentGrant {
    groups: Vec<String>,
    #[serde(default)]
    site: Option<String>,
    #[serde(default)]
    employment_band: Option<u8>,
}

/// Identity -> scope statement, for every principal the fixtures define.
pub struct IdentityModel {
    statements: BTreeMap<String, ScopeStatement>,
}

impl IdentityModel {
    /// Loads company.json, refusing if its bytes do not match the hash the
    /// M1 compile pinned (`expected_sha256`).
    pub fn load(fixtures_dir: &Path, expected_sha256: &str) -> Result<IdentityModel> {
        let path = fixtures_dir.join("company.json");
        let bytes = std::fs::read(&path)
            .with_context(|| format!("cannot read fixture {}", path.display()))?;
        if sha256_hex(&bytes) != expected_sha256 {
            bail!(
                "company.json does not match the fixture hash pinned by the M1 compile; refusing"
            );
        }
        let company: CompanyFile = serde_json::from_slice(&bytes)
            .with_context(|| format!("fixture {} fails parse", path.display()))?;

        let mut groups_of: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        for group in &company.groups {
            for member in &group.member_ids {
                groups_of
                    .entry(member.as_str())
                    .or_default()
                    .insert(group.id.clone());
            }
        }

        let mut statements = BTreeMap::new();
        for person in &company.people {
            statements.insert(
                person.id.clone(),
                ScopeStatement {
                    band: Some(person.employment_band),
                    groups: groups_of
                        .get(person.id.as_str())
                        .map(|set| set.iter().cloned().collect())
                        .unwrap_or_default(),
                    sites: vec![person.site.clone()],
                },
            );
        }
        for agent in &company.agents {
            let mut agent_groups: Vec<String> = agent.grant.groups.clone();
            agent_groups.sort();
            agent_groups.dedup();
            statements.insert(
                agent.id.clone(),
                ScopeStatement {
                    band: agent.grant.employment_band,
                    groups: agent_groups,
                    sites: agent.grant.site.clone().into_iter().collect(),
                },
            );
        }
        Ok(IdentityModel { statements })
    }

    pub fn is_known(&self, principal_id: &str) -> bool {
        self.statements.contains_key(principal_id)
    }

    pub fn count(&self) -> usize {
        self.statements.len()
    }

    /// The principal's scope statement; the empty statement for anyone the
    /// identity model does not define (deny by default, indistinguishable
    /// from a principal granted nothing).
    pub fn statement_for(&self, principal_id: &str) -> ScopeStatement {
        self.statements
            .get(principal_id)
            .cloned()
            .unwrap_or_else(empty_statement)
    }
}

pub fn empty_statement() -> ScopeStatement {
    ScopeStatement {
        band: None,
        groups: Vec::new(),
        sites: Vec::new(),
    }
}
