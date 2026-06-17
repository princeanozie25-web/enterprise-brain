//! Deterministic derivation of the org knowledge layer.
//!
//! Reads structured roster/corpus facts and turns them into entity pages whose
//! every claim carries a [`Provenance`]. No LLM, no wall clock, no randomness —
//! two runs over the same source bytes produce the same layer.
//!
//! The authorization model is consulted only through the read-only
//! [`GrantOracle`] seam, and only to (a) display each person's governed access
//! and (b) flag, fail-closed, any derived association that implies access the
//! model does not grant. Derivation never assigns, infers, or widens a
//! permission: the "granted" set shown on a page is exactly what the oracle
//! returns; discrepancies are surfaced alongside it, never folded into it.

use std::collections::{BTreeMap, BTreeSet};

use crate::authz::GrantOracle;
use crate::provenance::{Claim, Provenance};
use crate::sources::{
    AgentRecord, CapabilityRecord, DocRecord, LineIndex, RosterPerson, Sources, SRC_BRM,
    SRC_COMPANY, SRC_DOCUMENTS, SRC_PEOPLE,
};

/// How many granted documents / flagged discrepancies a page lists inline
/// before summarizing the remainder as a count.
pub const SAMPLE_LIMIT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Person,
    Department,
    Project,
    Tool,
    Index,
}

impl PageKind {
    /// The `out/` subdirectory pages of this kind live in.
    pub fn dir(self) -> &'static str {
        match self {
            PageKind::Person => "people",
            PageKind::Department => "departments",
            PageKind::Project => "projects",
            PageKind::Tool => "tools",
            PageKind::Index => "",
        }
    }
}

/// A typed link to another page in the layer (rendered relative to `out/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossLink {
    pub kind: PageKind,
    pub id: String,
    pub label: String,
}

/// The read-only display of a principal's governed access, taken verbatim from
/// the compiled model. The `allowed_*` fields are the oracle's answer and
/// nothing else — derivation cannot add to them.
#[derive(Debug, Clone)]
pub struct GovernedAccess {
    pub principal_id: String,
    pub snapshot_version: String,
    pub allowed_total: usize,
    pub denied_count: Option<usize>,
    pub sample: Vec<AllowedDoc>,
}

#[derive(Debug, Clone)]
pub struct AllowedDoc {
    pub document_id: String,
    pub title: String,
    pub reasons: Vec<String>,
    pub superseded: bool,
}

/// A fail-closed flag: a derived association implied access the model does not
/// grant. It is recorded and surfaced; access is never widened to match it.
#[derive(Debug, Clone)]
pub struct Discrepancy {
    pub principal_id: String,
    pub document_id: String,
    pub bases: Vec<String>,
    pub detail: String,
    pub provenance: Provenance,
}

/// One derived entity page.
#[derive(Debug, Clone)]
pub struct Page {
    pub kind: PageKind,
    pub id: String,
    pub title: String,
    pub claims: Vec<Claim>,
    pub links: Vec<CrossLink>,
    pub governed_access: Option<GovernedAccess>,
    pub discrepancies: Vec<Discrepancy>,
}

/// The whole generated layer.
#[derive(Debug, Clone)]
pub struct DerivedLayer {
    pub people: Vec<Page>,
    pub departments: Vec<Page>,
    pub projects: Vec<Page>,
    pub tools: Vec<Page>,
    pub index: Page,
}

impl DerivedLayer {
    /// All non-index pages, in stable order.
    pub fn entity_pages(&self) -> impl Iterator<Item = &Page> {
        self.people
            .iter()
            .chain(&self.departments)
            .chain(&self.projects)
            .chain(&self.tools)
    }

    /// Every fail-closed flag across the layer.
    pub fn all_discrepancies(&self) -> Vec<&Discrepancy> {
        self.entity_pages()
            .flat_map(|p| p.discrepancies.iter())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Derivation context: index the sources once for cross-referencing.
// ---------------------------------------------------------------------------

struct Ctx<'a> {
    src: &'a Sources,
    person_idx: BTreeMap<&'a str, usize>,
    person_by_id: BTreeMap<&'a str, &'a RosterPerson>,
    person_id_by_name: BTreeMap<&'a str, &'a str>,
    doc_idx: BTreeMap<&'a str, usize>,
    doc_by_id: BTreeMap<&'a str, &'a DocRecord>,
    cap_idx: BTreeMap<&'a str, usize>,
    cap_by_id: BTreeMap<&'a str, &'a CapabilityRecord>,
    agent_idx: BTreeMap<&'a str, usize>,
    workflow_name: BTreeMap<&'a str, &'a str>,
    /// capability id -> sorted person ids on it (roster-derived).
    cap_people: BTreeMap<String, BTreeSet<String>>,
}

impl<'a> Ctx<'a> {
    fn build(src: &'a Sources) -> Self {
        let mut person_idx = BTreeMap::new();
        let mut person_by_id = BTreeMap::new();
        let mut person_id_by_name = BTreeMap::new();
        let mut cap_people: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (i, p) in src.people.people.iter().enumerate() {
            person_idx.insert(p.id.as_str(), i);
            person_by_id.insert(p.id.as_str(), p);
            person_id_by_name.insert(p.display_name.as_str(), p.id.as_str());
            for proj in &p.projects {
                cap_people
                    .entry(proj.capability_id.clone())
                    .or_default()
                    .insert(p.id.clone());
            }
        }

        let mut doc_idx = BTreeMap::new();
        let mut doc_by_id = BTreeMap::new();
        for (i, d) in src.documents.documents.iter().enumerate() {
            doc_idx.insert(d.id.as_str(), i);
            doc_by_id.insert(d.id.as_str(), d);
        }

        let mut cap_idx = BTreeMap::new();
        let mut cap_by_id = BTreeMap::new();
        for (i, c) in src.brm.capabilities.iter().enumerate() {
            cap_idx.insert(c.id.as_str(), i);
            cap_by_id.insert(c.id.as_str(), c);
        }

        let mut agent_idx = BTreeMap::new();
        for (i, a) in src.company.agents.iter().enumerate() {
            agent_idx.insert(a.id.as_str(), i);
        }

        let mut workflow_name = BTreeMap::new();
        for w in &src.brm.workflows {
            workflow_name.insert(w.id.as_str(), w.name.as_str());
        }

        Self {
            src,
            person_idx,
            person_by_id,
            person_id_by_name,
            doc_idx,
            doc_by_id,
            cap_idx,
            cap_by_id,
            agent_idx,
            workflow_name,
            cap_people,
        }
    }

    fn doc_title(&self, id: &str) -> String {
        self.doc_by_id
            .get(id)
            .map(|d| d.title.clone())
            .unwrap_or_else(|| format!("(document {id})"))
    }
}

/// Builds a provenance pointer for a source record. The components are always
/// non-empty here (ids and names come from validated fixtures), so the only
/// way this can fail is a programming error — which we surface loudly rather
/// than silently emit an unsourced claim.
fn pv(source: &str, lines: &LineIndex, record: &str, locator: impl Into<String>) -> Provenance {
    Provenance::new(source, record, locator, lines.line_of(record))
        .expect("derivation builds non-empty provenance from validated source records")
}

fn dept_slug(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Derives the full layer from `sources`, consulting `authz` read-only for
/// governed-access display and fail-closed discrepancy flagging.
pub fn derive_all(sources: &Sources, authz: &dyn GrantOracle) -> DerivedLayer {
    let ctx = Ctx::build(sources);

    let people: Vec<Page> = sources
        .people
        .people
        .iter()
        .map(|p| derive_person(&ctx, p, authz))
        .collect();

    let departments: Vec<Page> = sources
        .company
        .departments
        .iter()
        .enumerate()
        .map(|(i, d)| derive_department(&ctx, i, d))
        .collect();

    let projects: Vec<Page> = sources
        .brm
        .capabilities
        .iter()
        .map(|c| derive_project(&ctx, c))
        .collect();

    let tools: Vec<Page> = sources
        .company
        .agents
        .iter()
        .map(|a| derive_tool(&ctx, a))
        .collect();

    let index = derive_index(&ctx, &people, &departments, &projects, &tools);

    DerivedLayer {
        people,
        departments,
        projects,
        tools,
        index,
    }
}

// ---------------------------------------------------------------------------
// Person pages
// ---------------------------------------------------------------------------

fn derive_person(ctx: &Ctx, p: &RosterPerson, authz: &dyn GrantOracle) -> Page {
    let lines = &ctx.src.lines.people;
    let i = ctx.person_idx[p.id.as_str()];
    let base = format!("/people/{i}");
    let mut claims = Vec::new();
    let mut links = Vec::new();

    claims.push(Claim::new(
        format!("Name: {}", p.display_name),
        pv(SRC_PEOPLE, lines, &p.id, format!("{base}/display_name")),
    ));
    claims.push(Claim::new(
        format!("Title: {}", p.title),
        pv(SRC_PEOPLE, lines, &p.id, format!("{base}/title")),
    ));
    claims.push(Claim::new(
        format!("Department: {}", p.department_label),
        pv(SRC_PEOPLE, lines, &p.id, format!("{base}/department_label")),
    ));
    claims.push(Claim::new(
        format!("Seniority: {}", p.seniority),
        pv(SRC_PEOPLE, lines, &p.id, format!("{base}/seniority")),
    ));
    if let Some(loc) = &p.location {
        claims.push(Claim::new(
            format!("Location: {loc}"),
            pv(SRC_PEOPLE, lines, &p.id, format!("{base}/location")),
        ));
    }
    if let Some(ws) = &p.work_style {
        claims.push(Claim::new(
            format!("Work style: {ws}"),
            pv(SRC_PEOPLE, lines, &p.id, format!("{base}/work_style")),
        ));
    }
    if let Some(bio) = &p.bio {
        claims.push(Claim::new(
            format!("Bio: {bio}"),
            pv(SRC_PEOPLE, lines, &p.id, format!("{base}/bio")),
        ));
    }

    // Reporting line (reports_to is a display name; resolve to a link if known).
    if let Some(mgr) = &p.reports_to {
        claims.push(Claim::new(
            format!("Reports to: {mgr}"),
            pv(SRC_PEOPLE, lines, &p.id, format!("{base}/reports_to")),
        ));
        if let Some(mgr_id) = ctx.person_id_by_name.get(mgr.as_str()) {
            links.push(CrossLink {
                kind: PageKind::Person,
                id: (*mgr_id).to_string(),
                label: mgr.clone(),
            });
        }
    }
    if !p.manages.is_empty() {
        claims.push(Claim::new(
            format!(
                "Manages {} report(s): {}",
                p.manages.len(),
                p.manages.join(", ")
            ),
            pv(SRC_PEOPLE, lines, &p.id, format!("{base}/manages")),
        ));
        for name in &p.manages {
            if let Some(rid) = ctx.person_id_by_name.get(name.as_str()) {
                links.push(CrossLink {
                    kind: PageKind::Person,
                    id: (*rid).to_string(),
                    label: name.clone(),
                });
            }
        }
    }

    // Department cross-link.
    let dslug = dept_slug(&p.department_label);
    links.push(CrossLink {
        kind: PageKind::Department,
        id: dslug,
        label: p.department_label.clone(),
    });

    // Capabilities (projects) the roster places this person on.
    for (j, proj) in p.projects.iter().enumerate() {
        claims.push(Claim::new(
            format!(
                "Works on {} as {} ({}) — initiative “{}”, strategy “{}”, workflow “{}”",
                proj.capability_name,
                proj.role,
                proj.status,
                proj.initiative_name,
                proj.strategy_name,
                proj.workflow_name
            ),
            pv(
                SRC_PEOPLE,
                lines,
                &p.id,
                format!("{base}/projects/{j}/capability_id"),
            ),
        ));
        links.push(CrossLink {
            kind: PageKind::Project,
            id: proj.capability_id.clone(),
            label: proj.capability_name.clone(),
        });
    }

    // Governed access — verbatim from the compiled model (READ-ONLY).
    let governed_access = derive_governed_access(ctx, &p.id, authz);

    // Fail-closed: derived associations that imply ungranted access.
    let discrepancies = derive_person_discrepancies(ctx, p, authz);

    Page {
        kind: PageKind::Person,
        id: p.id.clone(),
        title: p.display_name.clone(),
        claims,
        links,
        governed_access,
        discrepancies,
    }
}

/// Reads the compiled model for this principal and packages the display. The
/// allowed list is exactly the oracle's answer; nothing derived is added.
fn derive_governed_access(
    ctx: &Ctx,
    principal_id: &str,
    authz: &dyn GrantOracle,
) -> Option<GovernedAccess> {
    if !authz.known_principal(principal_id) {
        return None;
    }
    let allowed = authz.allowed_documents(principal_id);
    let sample = allowed
        .iter()
        .take(SAMPLE_LIMIT)
        .map(|doc_id| AllowedDoc {
            document_id: doc_id.clone(),
            title: ctx.doc_title(doc_id),
            reasons: authz.why_allowed(principal_id, doc_id).unwrap_or_default(),
            superseded: authz.is_superseded(principal_id, doc_id),
        })
        .collect();
    Some(GovernedAccess {
        principal_id: principal_id.to_string(),
        snapshot_version: authz.snapshot_version().to_string(),
        allowed_total: allowed.len(),
        denied_count: authz.denied_count(principal_id),
        sample,
    })
}

/// Flags, fail-closed, every document a derived association links to this
/// person that the compiled model does NOT grant them. Two derivation bases:
/// authorship (they wrote it) and capability membership (they work on a
/// capability that owns it). Access is never widened — these are flags only.
fn derive_person_discrepancies(
    ctx: &Ctx,
    p: &RosterPerson,
    authz: &dyn GrantOracle,
) -> Vec<Discrepancy> {
    if !authz.known_principal(&p.id) {
        // A roster person absent from the compiled model cannot be assessed.
        // This never widens access: an unknown principal also gets no
        // governed-access block (see derive_governed_access), so nothing is
        // ever presented as granted for them. On the real fixtures every roster
        // id is a known principal, so this branch is inert.
        return Vec::new();
    }
    let plines = &ctx.src.lines.people;
    let dlines = &ctx.src.lines.documents;
    let i = ctx.person_idx[p.id.as_str()];

    // doc id -> (bases, provenance of the strongest derived link)
    let mut implied: BTreeMap<String, (BTreeSet<String>, Provenance)> = BTreeMap::new();

    // Basis 1: authored documents.
    for d in &ctx.src.documents.documents {
        if d.author_id == p.id {
            let di = ctx.doc_idx[d.id.as_str()];
            let prov = pv(
                SRC_DOCUMENTS,
                dlines,
                &d.id,
                format!("/documents/{di}/author_id"),
            );
            let e = implied
                .entry(d.id.clone())
                .or_insert_with(|| (BTreeSet::new(), prov.clone()));
            e.0.insert("authorship".to_string());
            e.1 = prov; // document-level cite is the strongest anchor
        }
    }

    // Basis 2: documents owned by capabilities the roster places them on.
    for (j, proj) in p.projects.iter().enumerate() {
        if let Some(cap) = ctx.cap_by_id.get(proj.capability_id.as_str()) {
            for doc_id in &cap.document_ids {
                let prov = pv(
                    SRC_PEOPLE,
                    plines,
                    &p.id,
                    format!("/people/{i}/projects/{j}/capability_id"),
                );
                let e = implied
                    .entry(doc_id.clone())
                    .or_insert_with(|| (BTreeSet::new(), prov.clone()));
                e.0.insert(format!("capability:{}", cap.id));
            }
        }
    }

    // Keep only those the model does NOT grant — and never widen.
    let mut out = Vec::new();
    for (doc_id, (bases, prov)) in implied {
        if authz.why_allowed(&p.id, &doc_id).is_none() {
            let bases: Vec<String> = bases.into_iter().collect();
            out.push(Discrepancy {
                detail: format!(
                    "Derivation links {} to {} (via {}), but the authorization model does not grant access. Flagged, not reconciled; access NOT widened.",
                    p.display_name,
                    doc_id,
                    bases.join(", ")
                ),
                principal_id: p.id.clone(),
                document_id: doc_id,
                bases,
                provenance: prov,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Department pages
// ---------------------------------------------------------------------------

fn derive_department(ctx: &Ctx, dept_index: usize, name: &str) -> Page {
    let plines = &ctx.src.lines.people;
    let mut claims = Vec::new();
    let mut links = Vec::new();

    claims.push(Claim::new(
        format!("Department: {name}"),
        // departments is a plain string array — record key is the dept name,
        // locator is the array index (no `id` anchor, so no line).
        Provenance::new(
            SRC_COMPANY,
            name,
            format!("/departments/{dept_index}"),
            None,
        )
        .expect("department name is non-empty"),
    ));

    // Members: roster people whose department_label matches.
    let mut members: Vec<&RosterPerson> = ctx
        .src
        .people
        .people
        .iter()
        .filter(|p| p.department_label == name)
        .collect();
    members.sort_by(|a, b| a.id.cmp(&b.id));
    claims.push(Claim::new(
        format!("Members: {}", members.len()),
        pv(SRC_PEOPLE, plines, "people", "/people"),
    ));
    for m in &members {
        let mi = ctx.person_idx[m.id.as_str()];
        claims.push(Claim::new(
            format!("Member: {} — {}", m.display_name, m.title),
            pv(
                SRC_PEOPLE,
                plines,
                &m.id,
                format!("/people/{mi}/display_name"),
            ),
        ));
        links.push(CrossLink {
            kind: PageKind::Person,
            id: m.id.clone(),
            label: m.display_name.clone(),
        });
    }

    // Capabilities worked on by this department's people.
    let member_ids: BTreeSet<&str> = members.iter().map(|m| m.id.as_str()).collect();
    let mut caps: BTreeSet<(String, String)> = BTreeSet::new();
    for (cap_id, people) in &ctx.cap_people {
        if people.iter().any(|pid| member_ids.contains(pid.as_str())) {
            if let Some(cap) = ctx.cap_by_id.get(cap_id.as_str()) {
                caps.insert((cap_id.clone(), cap.name.clone()));
            }
        }
    }
    claims.push(Claim::new(
        format!("Capabilities engaged by this department: {}", caps.len()),
        pv(SRC_PEOPLE, plines, "people", "/people"),
    ));
    for (cap_id, cap_name) in &caps {
        links.push(CrossLink {
            kind: PageKind::Project,
            id: cap_id.clone(),
            label: cap_name.clone(),
        });
    }

    // Documents owned by the department (corpus-derived count + sample).
    let dlines = &ctx.src.lines.documents;
    let mut owned: Vec<&DocRecord> = ctx
        .src
        .documents
        .documents
        .iter()
        .filter(|d| d.department == name)
        .collect();
    owned.sort_by(|a, b| a.id.cmp(&b.id));
    claims.push(Claim::new(
        format!("Documents owned by this department: {}", owned.len()),
        pv(SRC_DOCUMENTS, dlines, "documents", "/documents"),
    ));
    for d in owned.iter().take(SAMPLE_LIMIT) {
        let di = ctx.doc_idx[d.id.as_str()];
        claims.push(Claim::new(
            format!("Document: {} — {} ({})", d.id, d.title, d.doc_type),
            pv(
                SRC_DOCUMENTS,
                dlines,
                &d.id,
                format!("/documents/{di}/title"),
            ),
        ));
    }

    // A department page asserts independent facts ("X is a member", "this dept
    // owns document Y") — never a person→document access relation — so it makes
    // no access-implying claim and carries no fail-closed flags. Person→document
    // access implications (authorship, capability membership) are flagged on the
    // person pages, where the derivation actually links a principal to a doc.
    Page {
        kind: PageKind::Department,
        id: dept_slug(name),
        title: name.to_string(),
        claims,
        links,
        governed_access: None,
        discrepancies: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Project (capability) pages
// ---------------------------------------------------------------------------

fn derive_project(ctx: &Ctx, cap: &CapabilityRecord) -> Page {
    let blines = &ctx.src.lines.brm;
    let dlines = &ctx.src.lines.documents;
    let plines = &ctx.src.lines.people;
    let i = ctx.cap_idx[cap.id.as_str()];
    let base = format!("/capabilities/{i}");
    let mut claims = Vec::new();
    let mut links = Vec::new();

    claims.push(Claim::new(
        format!("Capability: {}", cap.name),
        pv(SRC_BRM, blines, &cap.id, format!("{base}/name")),
    ));
    if let Some(wf) = &cap.workflow_id {
        let wf_name = ctx.workflow_name.get(wf.as_str()).copied().unwrap_or(wf);
        claims.push(Claim::new(
            format!("Workflow: {wf_name} ({wf})"),
            pv(SRC_BRM, blines, &cap.id, format!("{base}/workflow_id")),
        ));
    }

    // Documents owned by this capability.
    claims.push(Claim::new(
        format!("Documents: {}", cap.document_ids.len()),
        pv(SRC_BRM, blines, &cap.id, format!("{base}/document_ids")),
    ));
    let mut dept_involved: BTreeSet<String> = BTreeSet::new();
    for (j, doc_id) in cap.document_ids.iter().enumerate() {
        let title = ctx.doc_title(doc_id);
        // Cite the brm link; the document's own dept is cited where used below.
        claims.push(Claim::new(
            format!("Owns document {doc_id} — {title}"),
            pv(SRC_BRM, blines, &cap.id, format!("{base}/document_ids/{j}")),
        ));
        if let Some(d) = ctx.doc_by_id.get(doc_id.as_str()) {
            dept_involved.insert(d.department.clone());
        }
    }

    // People on this capability (roster-derived).
    if let Some(people) = ctx.cap_people.get(&cap.id) {
        claims.push(Claim::new(
            format!("People on this capability: {}", people.len()),
            pv(SRC_PEOPLE, plines, "people", "/people"),
        ));
        for pid in people {
            if let Some(person) = ctx.person_by_id.get(pid.as_str()) {
                let pidx = ctx.person_idx[pid.as_str()];
                claims.push(Claim::new(
                    format!("Contributor: {} — {}", person.display_name, person.title),
                    pv(SRC_PEOPLE, plines, pid, format!("/people/{pidx}/projects")),
                ));
                links.push(CrossLink {
                    kind: PageKind::Person,
                    id: pid.clone(),
                    label: person.display_name.clone(),
                });
                dept_involved.insert(person.department_label.clone());
            }
        }
    }

    // Departments involved (union of contributor + document departments).
    claims.push(Claim::new(
        format!(
            "Departments involved: {}",
            dept_involved.iter().cloned().collect::<Vec<_>>().join(", ")
        ),
        pv(SRC_DOCUMENTS, dlines, "documents", "/documents"),
    ));
    for dept in &dept_involved {
        links.push(CrossLink {
            kind: PageKind::Department,
            id: dept_slug(dept),
            label: dept.clone(),
        });
    }

    Page {
        kind: PageKind::Project,
        id: cap.id.clone(),
        title: cap.name.clone(),
        claims,
        links,
        governed_access: None,
        discrepancies: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tool (agent) pages
// ---------------------------------------------------------------------------

fn derive_tool(ctx: &Ctx, agent: &AgentRecord) -> Page {
    let clines = &ctx.src.lines.company;
    let i = ctx.agent_idx[agent.id.as_str()];
    let base = format!("/agents/{i}");
    let mut claims = Vec::new();
    let mut links = Vec::new();

    claims.push(Claim::new(
        format!("Tool: {}", agent.name),
        pv(SRC_COMPANY, clines, &agent.id, format!("{base}/name")),
    ));
    claims.push(Claim::new(
        format!(
            "Type: {} software assistant",
            if agent.synthetic { "synthetic" } else { "live" }
        ),
        pv(SRC_COMPANY, clines, &agent.id, format!("{base}/synthetic")),
    ));

    // Owner (resolve owner_user_id -> roster display name + link).
    let owner_label = ctx
        .person_by_id
        .get(agent.owner_user_id.as_str())
        .map(|p| p.display_name.clone())
        .unwrap_or_else(|| agent.owner_user_id.clone());
    claims.push(Claim::new(
        format!("Owner: {} ({})", owner_label, agent.owner_user_id),
        pv(
            SRC_COMPANY,
            clines,
            &agent.id,
            format!("{base}/owner_user_id"),
        ),
    ));
    if ctx.person_by_id.contains_key(agent.owner_user_id.as_str()) {
        links.push(CrossLink {
            kind: PageKind::Person,
            id: agent.owner_user_id.clone(),
            label: owner_label,
        });
    }

    // Declared grant — the tool's OWN scope, shown from source (not computed).
    claims.push(Claim::new(
        format!(
            "Granted to act for group(s): {}",
            if agent.grant.groups.is_empty() {
                "(none)".to_string()
            } else {
                agent.grant.groups.join(", ")
            }
        ),
        pv(
            SRC_COMPANY,
            clines,
            &agent.id,
            format!("{base}/grant/groups"),
        ),
    ));
    if let Some(site) = &agent.grant.site {
        claims.push(Claim::new(
            format!("Grant scoped to site: {site}"),
            pv(SRC_COMPANY, clines, &agent.id, format!("{base}/grant/site")),
        ));
    }
    if let Some(band) = agent.grant.employment_band {
        claims.push(Claim::new(
            format!("Grant requires employment band ≥ {band}"),
            pv(
                SRC_COMPANY,
                clines,
                &agent.id,
                format!("{base}/grant/employment_band"),
            ),
        ));
    }

    Page {
        kind: PageKind::Tool,
        id: agent.id.clone(),
        title: agent.name.clone(),
        claims,
        links,
        governed_access: None,
        discrepancies: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Index page
// ---------------------------------------------------------------------------

fn derive_index(
    ctx: &Ctx,
    people: &[Page],
    departments: &[Page],
    projects: &[Page],
    tools: &[Page],
) -> Page {
    let plines = &ctx.src.lines.people;
    let clines = &ctx.src.lines.company;
    let blines = &ctx.src.lines.brm;
    let dlines = &ctx.src.lines.documents;
    let mut claims = Vec::new();

    claims.push(Claim::new(
        format!("Company: {}", ctx.src.company.company.name),
        Provenance::new(SRC_COMPANY, "company", "/company/name", None)
            .expect("company name non-empty"),
    ));
    claims.push(Claim::new(
        format!("People: {}", people.len()),
        pv(SRC_PEOPLE, plines, "people", "/people"),
    ));
    claims.push(Claim::new(
        format!("Departments: {}", departments.len()),
        pv(SRC_COMPANY, clines, "departments", "/departments"),
    ));
    claims.push(Claim::new(
        format!("Projects (capabilities): {}", projects.len()),
        pv(SRC_BRM, blines, "capabilities", "/capabilities"),
    ));
    claims.push(Claim::new(
        format!("Tools (synthetic agents): {}", tools.len()),
        pv(SRC_COMPANY, clines, "agents", "/agents"),
    ));
    claims.push(Claim::new(
        format!("Documents in corpus: {}", ctx.src.documents.documents.len()),
        pv(SRC_DOCUMENTS, dlines, "documents", "/documents"),
    ));

    Page {
        kind: PageKind::Index,
        id: "index".to_string(),
        title: format!("{} — Org Knowledge Layer", ctx.src.company.company.name),
        claims,
        links: Vec::new(),
        governed_access: None,
        discrepancies: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dept_slug_is_filename_safe() {
        assert_eq!(dept_slug("Quality & Compliance"), "quality-compliance");
        assert_eq!(dept_slug("Sales & Accounts"), "sales-accounts");
        assert_eq!(dept_slug("IT"), "it");
        assert_eq!(dept_slug("Executive"), "executive");
    }
}
