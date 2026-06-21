//! Read-only role/scope posture for the current actor.
//!
//! This is not an authorization layer. It derives a compact contract from
//! already-governed server facts so the UI can label command surfaces honestly
//! while every sensitive endpoint remains responsible for its own filtering.

use retrieval::index::canonical_json_bytes;
use serde::Serialize;

use crate::access_requests::AccessRequestStore;
use crate::answer::AskError;
use crate::humanize::HumanRecord;
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct DepartmentScope {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub band: Option<u8>,
    pub department_id: String,
    pub seniority: String,
}

#[derive(Debug, Serialize)]
pub struct TeamScope {
    pub direct_report_count: usize,
    pub has_team_scope: bool,
}

#[derive(Debug, Serialize)]
pub struct ProjectScope {
    pub capability_ids: Vec<String>,
    pub project_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ApprovalScope {
    pub has_approval_scope: bool,
    pub pending_count: usize,
}

#[derive(Debug, Serialize)]
pub struct RoleScopeSummary {
    pub actor_id: String,
    pub admin_surface_allowed: bool,
    pub approval_scope: ApprovalScope,
    pub bursar_surface_allowed: bool,
    pub confidence: String,
    pub demo_identity_mode: bool,
    pub department_scope: DepartmentScope,
    pub derived_level: String,
    pub enforcement: String,
    pub governance_surface_allowed: bool,
    pub project_scope: ProjectScope,
    pub reasons: Vec<String>,
    pub team_scope: TeamScope,
}

fn is_executive_candidate(record: &HumanRecord) -> bool {
    let title = record.title.to_ascii_lowercase();
    record.department_label == "Executive"
        || title.contains("chief ")
        || title.contains("chief")
        || title.contains("director")
        || title.contains("company secretary")
}

fn is_department_head(record: &HumanRecord) -> bool {
    let title = record.title.to_ascii_lowercase();
    !record.manages.is_empty() && title.contains("head")
}

fn pending_approval_count(store: Option<&AccessRequestStore>, actor: &str) -> usize {
    store
        .map(|store| store.inbox_for(actor).len())
        .unwrap_or_default()
}

pub fn role_scope_summary(state: &AppState, actor: &str) -> Result<Option<Vec<u8>>, AskError> {
    if !state.identity.is_known(actor) {
        return Ok(None);
    }
    let Some(people) = state.people.as_deref() else {
        return Ok(None);
    };
    let Some(record) = people.get(actor) else {
        return Ok(None);
    };

    let scope_statement = state.identity.statement_for(actor);
    let direct_report_count = record.manages.len();
    let pending_count = pending_approval_count(state.access_requests.as_deref(), actor);
    let mut capability_ids: Vec<String> = record
        .projects
        .iter()
        .map(|project| project.capability_id.clone())
        .collect();
    capability_ids.sort();
    capability_ids.dedup();

    let mut reasons = vec![
        "humanized actor profile is present".to_string(),
        format!("department fact: {}", record.department_label),
        format!("seniority signal: {}", record.seniority),
    ];
    if direct_report_count > 0 {
        reasons.push(format!(
            "reporting line has {direct_report_count} direct reports"
        ));
    }
    if !capability_ids.is_empty() {
        reasons.push(format!(
            "project scope has {} visible capability assignments",
            capability_ids.len()
        ));
    }
    if pending_count > 0 {
        reasons.push(format!(
            "approval inbox has {pending_count} pending requests"
        ));
    }

    let (derived_level, confidence) = if is_executive_candidate(record) {
        reasons.push("executive-like title/department is only a candidate signal".to_string());
        ("executive_candidate", "medium")
    } else if is_department_head(record) {
        ("department_head", "high")
    } else if direct_report_count > 0 {
        ("team_lead", "high")
    } else {
        ("employee", "high")
    };

    reasons.push("no explicit super-admin primitive exists".to_string());
    reasons.push("no explicit Bursar or governance-admin primitive exists".to_string());
    reasons.push("sensitive surfaces remain disallowed by this contract".to_string());

    let summary = RoleScopeSummary {
        actor_id: actor.to_string(),
        admin_surface_allowed: false,
        approval_scope: ApprovalScope {
            has_approval_scope: pending_count > 0,
            pending_count,
        },
        bursar_surface_allowed: false,
        confidence: confidence.to_string(),
        demo_identity_mode: true,
        department_scope: DepartmentScope {
            band: scope_statement.band,
            department_id: record.department_label.clone(),
            seniority: record.seniority.clone(),
        },
        derived_level: derived_level.to_string(),
        enforcement: "derived_only".to_string(),
        governance_surface_allowed: false,
        project_scope: ProjectScope {
            project_count: capability_ids.len(),
            capability_ids,
        },
        reasons,
        team_scope: TeamScope {
            direct_report_count,
            has_team_scope: direct_report_count > 0,
        },
    };

    canonical_json_bytes(&summary)
        .map(Some)
        .map_err(AskError::Internal)
}

/// AUTH-3 (FC-A3): is `actor` an admin for the purpose of view-as? THE admin
/// signal — the same `admin_surface_allowed` posture `/me/scope` reports. The
/// corpus carries no super-admin primitive, so this is `false` for every
/// principal today (honest framing). When a real admin capability is added,
/// this is the single place it turns on — and nothing else in the gate moves.
pub fn is_admin(_state: &AppState, _actor: &str) -> bool {
    false
}

/// AUTH-3: may `actor` perform a cross-principal view-as (`/lens/{other}`,
/// `/diff`)? Free under `demo_identity_mode` (Aperture charter §6.3); otherwise
/// admin-only. There is NO non-admin view-as outside demo mode. Every ALLOWED
/// view-as is still audited before render (see `lens::authorize_cross_lens` /
/// `diff::authorize_lens_diff`), and an unauditable view-as is forbidden.
pub fn view_as_allowed(state: &AppState, actor: &str) -> bool {
    state.demo_identity_mode || is_admin(state, actor)
}
