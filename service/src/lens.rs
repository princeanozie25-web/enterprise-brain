//! AP-2: GET /lens/{subject_id} — a principal's entire governed world,
//! rendered: subject passport, holdings grouped by REASON (the completeness
//! law: every doc in the subject's compiled M1 artifact appears EXACTLY once
//! as a primary entry), owned agents, all under the actor/subject audit
//! discipline.
//!
//! CHARTER §6.3, BY DESIGN (not an oversight): this endpoint reveals the
//! SUBJECT's world to the actor without filtering against the actor's own
//! scope. That disclosure is exactly what the cross-lens audit row exists
//! for — the actor's act of looking is the governed event. Document-level
//! access stays actor-scoped through /doc as ever.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use retrieval::index::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::answer::AskError;
use crate::AppState;

/// Reason priority classes: SUBJECT > REBAC > ABAC > AGENT > PUBLIC,
/// lexicographic within a class.
fn reason_class(reason: &str) -> Result<u8> {
    if reason == "SUBJECT:self" {
        Ok(0)
    } else if reason.starts_with("REBAC:") {
        Ok(1)
    } else if reason.starts_with("ABAC:") {
        Ok(2)
    } else if reason.starts_with("AGENT:") {
        Ok(3)
    } else if reason == "PUBLIC:all" || reason == "PUBLIC:sensitivity" {
        Ok(4)
    } else {
        bail!("unknown reason id {reason:?}");
    }
}

/// The human sentence for a reason id — a server-side table keyed by reason
/// prefix, total over every rule-id prefix M1 emits. An unknown reason id
/// fails closed (the whole response refuses) rather than inventing a
/// sentence.
pub fn sentence_for(reason: &str) -> Result<String> {
    if reason == "SUBJECT:self" {
        return Ok("You see this because it is your own record.".to_string());
    }
    if reason == "PUBLIC:all" || reason == "PUBLIC:sensitivity" {
        return Ok("You see this because it is public to every principal.".to_string());
    }
    if reason == "REBAC:public" {
        return Ok("You see this because it is published to everyone.".to_string());
    }
    if let Some(role) = reason.strip_prefix("REBAC:role:") {
        return Ok(format!("You see this because you hold the role {role}."));
    }
    if let Some(group) = reason.strip_prefix("REBAC:") {
        return Ok(format!("You see this because you are in {group}."));
    }
    if let Some(site) = reason.strip_prefix("ABAC:site_match:") {
        return Ok(format!("You see this because your site matches {site}."));
    }
    if let Some(band) = reason.strip_prefix("ABAC:band_min:") {
        return Ok(format!(
            "You see this because your band meets the minimum of {band}."
        ));
    }
    if reason == "ABAC:special_category_hr" {
        return Ok("You see this because you are in the HR group.".to_string());
    }
    if reason == "ABAC:special_category_subject" {
        return Ok("You see this because you are its subject.".to_string());
    }
    if reason == "AGENT:intersect(owner)" {
        return Ok(
            "You see this because your grant and your owner's access intersect here.".to_string(),
        );
    }
    bail!("no sentence for unknown reason id {reason:?}; refusing to invent one");
}

/// Public-sensitivity docs group under one section, last.
const PUBLIC_SECTION: &str = "PUBLIC:all";

fn display_reason(reason: &str) -> String {
    if reason == "PUBLIC:sensitivity" {
        PUBLIC_SECTION.to_string()
    } else {
        reason.to_string()
    }
}

// ---------------------------------------------------------------------------
// Response shapes (canonical JSON, sorted keys)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct LensSubject {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    pub groups: Vec<String>,
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_user_id: Option<String>,
    pub sites: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LensDoc {
    pub also_via: Vec<String>,
    pub document_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_successor: Option<String>,
    pub sensitivity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct LensSection {
    pub docs: Vec<LensDoc>,
    pub reason: String,
    pub sentence: String,
}

#[derive(Debug, Serialize)]
pub struct LensAgent {
    pub agent_id: String,
    pub grant_groups: Vec<String>,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct LensResponse {
    pub actor_id: String,
    pub agents: Vec<LensAgent>,
    pub cross_lens: bool,
    pub holdings: Vec<LensSection>,
    pub snapshot_version: String,
    pub subject: LensSubject,
}

// ---------------------------------------------------------------------------
// Company directory (per-request, hash-verified)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CompanyLite {
    people: Vec<PersonLite>,
    agents: Vec<AgentLite>,
    groups: Vec<GroupLite>,
}

#[derive(Debug, Deserialize)]
struct PersonLite {
    id: String,
    name: String,
    department: String,
    employment_band: u8,
    site: String,
}

#[derive(Debug, Deserialize)]
struct AgentLite {
    id: String,
    name: String,
    owner_user_id: String,
    grant: GrantLite,
}

#[derive(Debug, Deserialize)]
struct GrantLite {
    groups: Vec<String>,
    #[serde(default)]
    site: Option<String>,
    #[serde(default)]
    employment_band: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct GroupLite {
    id: String,
    member_ids: Vec<String>,
}

fn load_company(fixtures_dir: &Path, expected_sha256: &str) -> Result<CompanyLite> {
    let path = fixtures_dir.join("company.json");
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if sha256_hex(&bytes) != expected_sha256 {
        bail!("company.json does not match the M1-pinned hash; refusing");
    }
    serde_json::from_slice(&bytes).with_context(|| format!("{} fails parse", path.display()))
}

// ---------------------------------------------------------------------------
// Artifact mirror (reasons included; lens-local to keep ArtifactLite frozen)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct LensArtifact {
    entries: Vec<LensEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LensEntry {
    pub document_id: String,
    pub reasons: Vec<String>,
    #[serde(default)]
    pub superseded: Option<bool>,
    #[serde(default)]
    pub effective_successor: Option<String>,
}

/// Crate-visible since AP-3: /atlas loads the ACTOR's standing through the
/// same verified path (one loader, one hash discipline).
pub(crate) fn load_subject_artifact(
    state: &AppState,
    subject_id: &str,
) -> Result<Option<Vec<LensEntry>>> {
    let Some((artifact_file, artifact_sha)) = state.artifact_rows.get(subject_id) else {
        return Ok(None);
    };
    let path = state.artifacts_dir.join(artifact_file);
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if &sha256_hex(&bytes) != artifact_sha {
        bail!(
            "artifact {} does not match the M1 index hash; refusing",
            path.display()
        );
    }
    let artifact: LensArtifact = serde_json::from_slice(&bytes)
        .with_context(|| format!("artifact {} fails parse", path.display()))?;
    Ok(Some(artifact.entries))
}

// ---------------------------------------------------------------------------
// Holdings: the completeness law
// ---------------------------------------------------------------------------

/// Groups artifact entries into reason sections. Every entry appears EXACTLY
/// once as a primary doc; the primary reason is the minimum by (class
/// priority, lexicographic); remaining reasons become also_via chips.
/// Sections sort the same way, which puts PUBLIC:all last. Total over the
/// sentence table — any unknown reason refuses the whole response.
pub fn build_holdings(
    entries: &[LensEntry],
    docs: &BTreeMap<String, crate::DocMeta>,
    allowlist: &std::collections::BTreeSet<String>,
) -> Result<Vec<LensSection>> {
    let mut sections: BTreeMap<(u8, String), Vec<LensDoc>> = BTreeMap::new();
    for entry in entries {
        if entry.reasons.is_empty() {
            bail!("artifact entry with no reasons; refusing");
        }
        let mut reasons: Vec<String> = entry.reasons.iter().map(|r| display_reason(r)).collect();
        reasons.sort_by_key(|r| (reason_class(r).unwrap_or(u8::MAX), r.clone()));
        reasons.dedup();
        // Fail closed on ANY unknown reason, primary or secondary.
        for reason in &reasons {
            reason_class(reason)?;
            sentence_for(reason)?;
        }
        let primary = reasons[0].clone();
        let also_via: Vec<String> = reasons[1..].to_vec();

        let meta = docs
            .get(&entry.document_id)
            .context("artifact names a document the corpus does not carry")?;
        let superseded = entry.superseded == Some(true);
        // R-13 redaction: the successor id is emitted only when the
        // successor itself is in the subject's allowlist.
        let effective_successor = if superseded {
            entry
                .effective_successor
                .as_ref()
                .filter(|s| allowlist.contains(s.as_str()))
                .cloned()
        } else {
            None
        };
        let class = reason_class(&primary)?;
        sections.entry((class, primary)).or_default().push(LensDoc {
            also_via,
            document_id: entry.document_id.clone(),
            effective_successor,
            sensitivity: meta.sensitivity.clone(),
            superseded: superseded.then_some(true),
            title: meta.title.clone(),
        });
    }

    let mut holdings = Vec::with_capacity(sections.len());
    for ((_, reason), mut section_docs) in sections {
        section_docs.sort_by(|a, b| a.document_id.cmp(&b.document_id));
        holdings.push(LensSection {
            docs: section_docs,
            sentence: sentence_for(&reason)?,
            reason,
        });
    }
    Ok(holdings)
}

// ---------------------------------------------------------------------------
// Authorization seam
// ---------------------------------------------------------------------------

/// THE ONE-FUNCTION SWAP POINT. Under demo_identity_mode a cross-lens view
/// is permitted but audited BEFORE the response renders. In a real
/// deployment this function becomes an admin-classed permission check
/// (deny unless the actor holds the lens-admin grant) — swap THIS function
/// and nothing else moves.
fn authorize_cross_lens(state: &AppState, actor: &str, subject: &str) -> Result<()> {
    if actor == subject {
        return Ok(());
    }
    let Some(store) = &state.proposals else {
        // No audit sink configured: a cross-lens view cannot be recorded,
        // so it cannot happen. Fail closed.
        bail!("cross-lens viewing requires the audit store (--state-dir); refusing");
    };
    store.audit("lens_view", actor, subject, "allowed_demo")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// Builds the lens body for (actor, subject). `Ok(None)` = unknown subject —
/// the HTTP layer serves THE one 404 (identical for unknown and malformed).
pub fn lens_view(
    state: &AppState,
    actor: &str,
    subject_id: &str,
) -> Result<Option<Vec<u8>>, AskError> {
    let entries = load_subject_artifact(state, subject_id).map_err(AskError::Internal)?;
    let Some(entries) = entries else {
        return Ok(None);
    };
    let company =
        load_company(&state.fixtures_dir, &state.company_sha256).map_err(AskError::Internal)?;

    // Audit BEFORE anything renders (the act being governed is the look).
    authorize_cross_lens(state, actor, subject_id).map_err(AskError::Internal)?;

    let mut groups_of: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for group in &company.groups {
        for member in &group.member_ids {
            groups_of
                .entry(member.as_str())
                .or_default()
                .push(group.id.clone());
        }
    }

    let subject = if let Some(person) = company.people.iter().find(|p| p.id == subject_id) {
        let mut groups = groups_of.remove(person.id.as_str()).unwrap_or_default();
        groups.sort();
        LensSubject {
            band: Some(person.employment_band),
            department: Some(person.department.clone()),
            groups,
            id: person.id.clone(),
            kind: "human".to_string(),
            name: person.name.clone(),
            owner_user_id: None,
            sites: vec![person.site.clone()],
        }
    } else if let Some(agent) = company.agents.iter().find(|a| a.id == subject_id) {
        let mut groups = agent.grant.groups.clone();
        groups.sort();
        groups.dedup();
        LensSubject {
            band: agent.grant.employment_band,
            department: None,
            groups,
            id: agent.id.clone(),
            kind: "agent".to_string(),
            name: agent.name.clone(),
            owner_user_id: Some(agent.owner_user_id.clone()),
            sites: agent.grant.site.clone().into_iter().collect(),
        }
    } else {
        // In the M1 index but not in company.json: the worlds disagree.
        return Err(AskError::Internal(anyhow::anyhow!(
            "subject in the M1 index but not in company.json; refusing"
        )));
    };

    let allowlist: std::collections::BTreeSet<String> =
        entries.iter().map(|e| e.document_id.clone()).collect();
    let holdings = build_holdings(&entries, &state.docs, &allowlist).map_err(AskError::Internal)?;

    // Owned agents: humans only (an agent owns nothing).
    let mut agents: Vec<LensAgent> = if subject.kind == "human" {
        company
            .agents
            .iter()
            .filter(|a| a.owner_user_id == subject_id)
            .map(|a| {
                let mut grant_groups = a.grant.groups.clone();
                grant_groups.sort();
                grant_groups.dedup();
                LensAgent {
                    agent_id: a.id.clone(),
                    grant_groups,
                    name: a.name.clone(),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    agents.sort_by(|a, b| a.agent_id.cmp(&b.agent_id));

    let response = LensResponse {
        actor_id: actor.to_string(),
        agents,
        cross_lens: actor != subject_id,
        holdings,
        snapshot_version: state.snapshot_version.clone(),
        subject,
    };
    retrieval::index::canonical_json_bytes(&response)
        .map(Some)
        .map_err(AskError::Internal)
}
