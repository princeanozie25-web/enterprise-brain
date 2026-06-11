//! Serde models mirroring `/test/schemas/{company,documents,traps}.schema.json`.
//!
//! Every struct is `deny_unknown_fields`: a fixture carrying a key the schema
//! does not allow is a parse failure, and the compiler refuses entirely
//! (fail-closed). Constraints serde's type system cannot express (band ranges,
//! `const true` flags, non-empty strings, enum payload coherence) are enforced
//! by `semantics::World::build`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// company.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyFile {
    pub company: CompanyHeader,
    pub sites: Vec<Site>,
    pub departments: Vec<String>,
    pub people: Vec<Person>,
    pub groups: Vec<Group>,
    pub agents: Vec<Agent>,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanyHeader {
    pub name: String,
    pub fictional: bool,
    pub regulatory_context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub department: String,
    pub role: String,
    pub manager_id: Option<String>,
    pub employment_band: u8,
    pub site: String,
    pub start_date: String,
    pub synthetic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub description: String,
    pub member_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub grant: AgentGrant,
    pub owner_user_id: String,
    pub synthetic: bool,
}

/// An agent's explicit grant. `site` / `employment_band` are present only when
/// the grant carries them; an absent attribute can never satisfy an ABAC
/// condition (fail-closed).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentGrant {
    pub groups: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employment_band: Option<u8>,
}

// ---------------------------------------------------------------------------
// documents.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentsFile {
    pub documents: Vec<Document>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: String,
    pub source: String,
    pub title: String,
    pub body: String,
    pub author_id: String,
    pub department: String,
    pub created_at: String,
    pub sensitivity: Sensitivity,
    pub acl_refs: Vec<AclRule>,
    pub version: u32,
    pub supersedes: Option<String>,
    pub doc_type: DocType,
    pub subject_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Internal,
    Confidential,
    Restricted,
    SpecialCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocType {
    Sop,
    QualityRecord,
    HrRecord,
    BoardMinutes,
    CustomerAccount,
    WikiPage,
    MailThread,
    General,
}

/// One ACL rule on a document. `kind` decides which payload key must be set;
/// `World::build` refuses any rule whose payload does not match its kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AclRule {
    pub rule_id: String,
    pub kind: AclKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_band: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AclKind {
    Public,
    Group,
    Role,
    AttrSite,
    AttrBandMin,
}

// ---------------------------------------------------------------------------
// traps.json
// ---------------------------------------------------------------------------
//
// The compiler consumes ONLY the `mosaic` section, as opaque pass-through
// metadata for compiled entries (access rule 7). Trap records never influence
// an allow/deny decision. The other sections are modelled so the conformance
// suite (C-2) can reuse these types.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrapsFile {
    pub effective_version: Vec<EffectiveVersionTrap>,
    pub mosaic: Vec<MosaicTrap>,
    pub confused_deputy: Vec<ConfusedDeputyTrap>,
    pub manager_overreach: Vec<ManagerOverreachTrap>,
    pub cross_site: Vec<CrossSiteTrap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveVersionTrap {
    pub current_id: String,
    pub superseded_id: String,
    pub parameter_class: String,
}

/// Passed through onto compiled entries untouched; field order is alphabetical
/// so the canonical (sorted-key) serialization equals the fixture record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MosaicTrap {
    pub doc_a: String,
    pub doc_b: String,
    pub inferred_fact_class: String,
    pub principal_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfusedDeputyTrap {
    pub agent_id: String,
    pub owner_id: String,
    pub resource_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerOverreachTrap {
    pub manager_id: String,
    pub subject_id: String,
    pub resource_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossSiteTrap {
    pub principal_id: String,
    pub resource_id: String,
    pub required_site: String,
    pub principal_site: String,
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes =
        fs::read(path).with_context(|| format!("cannot read fixture {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("fixture {} fails schema/parse", path.display()))
}

/// Loads the compiler's three input fixtures from `dir`. Any read or parse
/// failure refuses the whole compile.
pub fn load_fixtures(dir: &Path) -> Result<(CompanyFile, DocumentsFile, TrapsFile)> {
    let company: CompanyFile = load_json(&dir.join("company.json"))?;
    let documents: DocumentsFile = load_json(&dir.join("documents.json"))?;
    let traps: TrapsFile = load_json(&dir.join("traps.json"))?;
    Ok((company, documents, traps))
}
