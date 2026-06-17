//! Read-only views over the synthetic Bryremead sources.
//!
//! These are the crate's OWN `Deserialize`-only structs. They model exactly the
//! fields the knowledge layer derives from and nothing else; unknown fields are
//! ignored (these humanized overlay files carry more than slice 1 needs). The
//! crate never serializes a source back, and never writes into `fixtures/` —
//! [`Sources::load`] only reads. Inputs are immutable here by construction.
//!
//! Alongside the parsed structs, [`Sources`] keeps a [`LineIndex`] per file so
//! every derived claim can cite the 1-based line of its source record.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

// Stable, honest display names used in provenance cites.
pub const SRC_PEOPLE: &str = "fixtures/people.json";
pub const SRC_DOCUMENTS: &str = "fixtures/documents.json";
pub const SRC_COMPANY: &str = "fixtures/company.json";
pub const SRC_BRM: &str = "fixtures/brm.json";

// ---------------------------------------------------------------------------
// fixtures/people.json  (the AR-1 humanized roster)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct PeopleFile {
    pub people: Vec<RosterPerson>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RosterPerson {
    pub id: String,
    pub display_name: String,
    pub title: String,
    pub department_label: String,
    pub seniority: String,
    /// A display name (not an id), or `null` at the org root (the CEO).
    #[serde(default)]
    pub reports_to: Option<String>,
    #[serde(default)]
    pub manages: Vec<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub work_style: Option<String>,
    #[serde(default)]
    pub projects: Vec<RosterProject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RosterProject {
    pub capability_id: String,
    pub capability_name: String,
    pub initiative_name: String,
    pub strategy_name: String,
    pub workflow_name: String,
    pub role: String,
    pub status: String,
}

// ---------------------------------------------------------------------------
// fixtures/documents.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct DocumentsFile {
    pub documents: Vec<DocRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DocRecord {
    pub id: String,
    pub title: String,
    pub doc_type: String,
    pub department: String,
    pub sensitivity: String,
    pub author_id: String,
    pub version: u32,
    #[serde(default)]
    pub supersedes: Option<String>,
    #[serde(default)]
    pub subject_id: Option<String>,
    #[serde(default)]
    pub acl_refs: Vec<AclRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AclRef {
    pub rule_id: String,
    pub kind: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub site: Option<String>,
    #[serde(default)]
    pub min_band: Option<u8>,
}

// ---------------------------------------------------------------------------
// fixtures/company.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyFile {
    pub company: CompanyHeader,
    #[serde(default)]
    pub departments: Vec<String>,
    #[serde(default)]
    pub groups: Vec<GroupRecord>,
    #[serde(default)]
    pub agents: Vec<AgentRecord>,
    #[serde(default)]
    pub people: Vec<CompanyPerson>,
    #[serde(default)]
    pub sites: Vec<SiteRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyHeader {
    pub name: String,
    #[serde(default)]
    pub fictional: bool,
    #[serde(default)]
    pub regulatory_context: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub member_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub owner_user_id: String,
    #[serde(default)]
    pub synthetic: bool,
    pub grant: AgentGrant,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentGrant {
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub site: Option<String>,
    #[serde(default)]
    pub employment_band: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyPerson {
    pub id: String,
    pub name: String,
    pub department: String,
    #[serde(default)]
    pub manager_id: Option<String>,
    #[serde(default)]
    pub site: Option<String>,
    #[serde(default)]
    pub employment_band: Option<u8>,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SiteRecord {
    pub id: String,
    pub name: String,
}

// ---------------------------------------------------------------------------
// fixtures/brm.json  (capabilities = the "projects"; workflows)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct BrmFile {
    #[serde(default)]
    pub capabilities: Vec<CapabilityRecord>,
    #[serde(default)]
    pub workflows: Vec<WorkflowRecord>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CapabilityRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub workflow_id: Option<String>,
    #[serde(default)]
    pub document_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowRecord {
    pub id: String,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Line index: record key -> 1-based line of its `"id"` anchor in the raw file
// ---------------------------------------------------------------------------

/// Maps a record's `id` value to the 1-based line where its `"id": "…"` key
/// appears in the raw, pretty-printed source. Built by a single linear scan;
/// purely for human-anchored provenance. First occurrence wins.
#[derive(Debug, Clone, Default)]
pub struct LineIndex {
    lines: BTreeMap<String, usize>,
}

impl LineIndex {
    pub fn build(raw: &str) -> Self {
        let mut lines = BTreeMap::new();
        for (i, line) in raw.lines().enumerate() {
            if let Some(id) = extract_id_value(line.trim_start()) {
                lines.entry(id.to_string()).or_insert(i + 1);
            }
        }
        Self { lines }
    }

    pub fn line_of(&self, key: &str) -> Option<usize> {
        self.lines.get(key).copied()
    }
}

/// From a line whose key is exactly `"id"` (not `author_id`, `rule_id`, …),
/// return the quoted value. Tolerates the pretty-printed `"id": "value"` shape.
fn extract_id_value(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("\"id\":")?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Per-file line indices, parallel to the parsed sources.
#[derive(Debug, Clone, Default)]
pub struct SourceLines {
    pub people: LineIndex,
    pub documents: LineIndex,
    pub company: LineIndex,
    pub brm: LineIndex,
}

/// Every synthetic source the knowledge layer reads, parsed once, plus the line
/// indices used for provenance. Read-only: nothing here can write a fixture.
#[derive(Debug, Clone)]
pub struct Sources {
    pub people: PeopleFile,
    pub documents: DocumentsFile,
    pub company: CompanyFile,
    pub brm: BrmFile,
    pub lines: SourceLines,
}

fn read_raw(dir: &Path, name: &str) -> Result<String> {
    let path = dir.join(name);
    fs::read_to_string(&path).with_context(|| format!("cannot read source {}", path.display()))
}

fn parse<T: for<'de> Deserialize<'de>>(raw: &str, name: &str) -> Result<T> {
    serde_json::from_str(raw).with_context(|| format!("source {name} fails to parse"))
}

impl Sources {
    /// Reads and parses all four sources from `fixtures_dir`. Read-only.
    pub fn load(fixtures_dir: &Path) -> Result<Self> {
        let people_raw = read_raw(fixtures_dir, "people.json")?;
        let documents_raw = read_raw(fixtures_dir, "documents.json")?;
        let company_raw = read_raw(fixtures_dir, "company.json")?;
        let brm_raw = read_raw(fixtures_dir, "brm.json")?;

        let lines = SourceLines {
            people: LineIndex::build(&people_raw),
            documents: LineIndex::build(&documents_raw),
            company: LineIndex::build(&company_raw),
            brm: LineIndex::build(&brm_raw),
        };

        Ok(Self {
            people: parse(&people_raw, "people.json")?,
            documents: parse(&documents_raw, "documents.json")?,
            company: parse(&company_raw, "company.json")?,
            brm: parse(&brm_raw, "brm.json")?,
            lines,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_index_keys_only_on_exact_id_field() {
        let raw = "{\n  \"author_id\": \"p011\",\n  \"id\": \"d0001\",\n  \"rule_id\": \"r:x\"\n}";
        let idx = LineIndex::build(raw);
        // Only the exact `"id"` line is indexed; *_id keys are ignored.
        assert_eq!(idx.line_of("d0001"), Some(3));
        assert_eq!(idx.line_of("p011"), None);
        assert_eq!(idx.line_of("r:x"), None);
    }
}
