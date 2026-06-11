//! The access semantics of Enterprise Brain M1, implemented once, from the
//! milestone prompt and the raw data fixtures alone.
//!
//! INDEPENDENCE INVARIANT: nothing in this module (or this crate) reads,
//! imports, or ports `/synth/acl.py`, `/synth/oracle.py`, `/synth/traps.py`,
//! or `/fixtures/ground_truth.jsonl`. The rules below are a fresh
//! implementation of the written spec:
//!
//! 1. ReBAC: a document's `acl_refs` name groups/roles; membership in a named
//!    group or holding a named role grants read. Grants are OR'd.
//! 2. ABAC on top, AND'd where present: site match, employment-band minimum,
//!    and `special_category` requires HR-group membership OR subject identity.
//! 3. Subject access: a person always reads their own HR record; their manager
//!    gains nothing via the org edge (no rule exists for it).
//! 4. Agents: effective access = agent grant INTERSECT owner's access, per
//!    (agent, document). Nothing is inherited implicitly.
//! 5. `public` sensitivity is readable by every principal; all other
//!    sensitivities go through rules 1-4.
//!
//! Deny by default: no matching grant rule means DENY, and a malformed or
//! unknown construct refuses compilation entirely rather than guessing.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{bail, Result};

use crate::model::{
    AclKind, AclRule, Agent, CompanyFile, DocType, Document, DocumentsFile, MosaicTrap, Person,
    Sensitivity, TrapsFile,
};

/// The group whose membership satisfies the `special_category` ABAC condition
/// (access rule 2). This is the stable id of the HR department group in the
/// company fixture.
pub const HR_GROUP_ID: &str = "grp_hr";

/// One access decision with its full reason trace. `Allow` reasons are the
/// stable rule ids written into compiled artifacts; `Deny` reasons are
/// diagnostic only and never leave the compiler (denials are not enumerated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow(Vec<String>),
    Deny(Vec<String>),
}

impl Decision {
    pub fn is_allow(&self) -> bool {
        matches!(self, Decision::Allow(_))
    }

    pub fn reasons(&self) -> &[String] {
        match self {
            Decision::Allow(r) | Decision::Deny(r) => r,
        }
    }
}

/// The validated, indexed fixture model every compile runs against.
pub struct World {
    pub company: CompanyFile,
    /// Documents sorted by id; compiled entry order follows this.
    pub documents: Vec<Document>,
    pub traps: TrapsFile,
    person_idx: HashMap<String, usize>,
    agent_idx: HashMap<String, usize>,
    person_groups: HashMap<String, BTreeSet<String>>,
    /// `old document id -> the id of the document that directly supersedes it`.
    successor_of: HashMap<String, String>,
    /// `document id -> mosaic trap records naming it` (fixture order).
    mosaic_by_doc: HashMap<String, Vec<MosaicTrap>>,
}

impl World {
    /// Validates the fixtures structurally and builds lookup indexes.
    /// Any duplicate id, dangling reference, or schema constraint the serde
    /// layer cannot express refuses the compile with the full error list.
    pub fn build(
        company: CompanyFile,
        documents: DocumentsFile,
        traps: TrapsFile,
    ) -> Result<World> {
        let mut errors: Vec<String> = Vec::new();
        validate_company(&company, &mut errors);
        validate_documents(&documents.documents, &company, &mut errors);
        validate_traps(&traps, &company, &documents.documents, &mut errors);
        if !errors.is_empty() {
            bail!(
                "fixture validation failed with {} error(s):\n  - {}",
                errors.len(),
                errors.join("\n  - ")
            );
        }

        let mut documents = documents.documents;
        documents.sort_by(|a, b| a.id.cmp(&b.id));

        let person_idx = company
            .people
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.clone(), i))
            .collect();
        let agent_idx = company
            .agents
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id.clone(), i))
            .collect();

        let mut person_groups: HashMap<String, BTreeSet<String>> = HashMap::new();
        for group in &company.groups {
            for member in &group.member_ids {
                person_groups
                    .entry(member.clone())
                    .or_default()
                    .insert(group.id.clone());
            }
        }

        let successor_of = documents
            .iter()
            .filter_map(|d| d.supersedes.as_ref().map(|old| (old.clone(), d.id.clone())))
            .collect();

        let mut mosaic_by_doc: HashMap<String, Vec<MosaicTrap>> = HashMap::new();
        for tag in &traps.mosaic {
            for doc_id in [&tag.doc_a, &tag.doc_b] {
                mosaic_by_doc
                    .entry(doc_id.clone())
                    .or_default()
                    .push(tag.clone());
            }
        }

        Ok(World {
            company,
            documents,
            traps,
            person_idx,
            agent_idx,
            person_groups,
            successor_of,
            mosaic_by_doc,
        })
    }

    /// All principal ids (people then agents), sorted: the default compile set.
    pub fn principal_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .company
            .people
            .iter()
            .map(|p| p.id.clone())
            .chain(self.company.agents.iter().map(|a| a.id.clone()))
            .collect();
        ids.sort();
        ids
    }

    pub fn is_known_principal(&self, id: &str) -> bool {
        self.person_idx.contains_key(id) || self.agent_idx.contains_key(id)
    }

    pub fn person(&self, id: &str) -> Option<&Person> {
        self.person_idx.get(id).map(|&i| &self.company.people[i])
    }

    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.agent_idx.get(id).map(|&i| &self.company.agents[i])
    }

    /// True iff some other document supersedes `doc_id`.
    pub fn is_superseded(&self, doc_id: &str) -> bool {
        self.successor_of.contains_key(doc_id)
    }

    /// The effective (terminal) successor of a superseded document: the chain
    /// `old -> newer -> ... -> current` is followed to its end. Chains were
    /// validated acyclic and fan-in free at build time.
    pub fn effective_successor(&self, doc_id: &str) -> Option<&str> {
        let mut current = self.successor_of.get(doc_id)?;
        while let Some(next) = self.successor_of.get(current) {
            current = next;
        }
        Some(current)
    }

    /// Mosaic trap records naming `doc_id`, in fixture order (rule 7
    /// pass-through; never used for decisions).
    pub fn mosaic_tags(&self, doc_id: &str) -> Option<&[MosaicTrap]> {
        self.mosaic_by_doc.get(doc_id).map(Vec::as_slice)
    }

    fn groups_of(&self, person_id: &str) -> Option<&BTreeSet<String>> {
        self.person_groups.get(person_id)
    }

    /// The single decision entry point: (principal id, document) -> Decision.
    /// Unknown principals deny by default.
    pub fn decide(&self, principal_id: &str, doc: &Document) -> Decision {
        if let Some(person) = self.person(principal_id) {
            return self.decide_person(person, doc);
        }
        if let Some(agent) = self.agent(principal_id) {
            return self.decide_agent(agent, doc);
        }
        Decision::Deny(vec!["DENY:unknown_principal".to_string()])
    }

    fn decide_person(&self, person: &Person, doc: &Document) -> Decision {
        // Rule 5: public sensitivity is readable by all principals.
        if doc.sensitivity == Sensitivity::Public {
            return Decision::Allow(vec!["PUBLIC:sensitivity".to_string()]);
        }

        static EMPTY: BTreeSet<String> = BTreeSet::new();
        let groups = self.groups_of(&person.id).unwrap_or(&EMPTY);
        let view = PrincipalView {
            principal_id: &person.id,
            groups,
            role: Some(&person.role),
            site: Some(&person.site),
            band: Some(person.employment_band),
        };
        let rule_decision = eval_rules(&view, doc);

        // Rule 3: the subject always reads their own HR record, independent of
        // any ACL rule on it. The manager has no rule here, so the org edge
        // grants nothing.
        let is_subject_self = doc.doc_type == DocType::HrRecord
            && doc.subject_id.as_deref() == Some(person.id.as_str());
        if is_subject_self {
            let mut reasons = match rule_decision {
                Decision::Allow(r) => r,
                Decision::Deny(_) => Vec::new(),
            };
            reasons.push("SUBJECT:self".to_string());
            return Decision::Allow(normalize(reasons));
        }
        rule_decision
    }

    /// Rule 4: agent effective access = agent grant INTERSECT owner access,
    /// computed explicitly for this (agent, document) pair. The grant side is
    /// evaluated against the same document rules with only the attributes the
    /// grant carries; a missing attribute can never satisfy a condition.
    fn decide_agent(&self, agent: &Agent, doc: &Document) -> Decision {
        // Rule 5 applies to every principal, agents included.
        if doc.sensitivity == Sensitivity::Public {
            return Decision::Allow(vec!["PUBLIC:sensitivity".to_string()]);
        }

        let grant_groups: BTreeSet<String> = agent.grant.groups.iter().cloned().collect();
        let view = PrincipalView {
            principal_id: &agent.id,
            groups: &grant_groups,
            role: None,
            site: agent.grant.site.as_deref(),
            band: agent.grant.employment_band,
        };
        let grant_side = eval_rules(&view, doc);

        let owner = self
            .person(&agent.owner_user_id)
            .expect("validated: agent owner exists");
        let owner_side = self.decide_person(owner, doc);

        match (&grant_side, &owner_side) {
            (Decision::Allow(grant_reasons), Decision::Allow(_)) => {
                let mut reasons = grant_reasons.clone();
                reasons.push("AGENT:intersect(owner)".to_string());
                Decision::Allow(normalize(reasons))
            }
            _ => {
                let mut reasons = Vec::new();
                if let Decision::Deny(r) = &grant_side {
                    reasons.push("DENY:agent_grant".to_string());
                    reasons.extend(r.iter().cloned());
                }
                if let Decision::Deny(r) = &owner_side {
                    reasons.push("DENY:agent_owner".to_string());
                    reasons.extend(r.iter().cloned());
                }
                Decision::Deny(reasons)
            }
        }
    }
}

/// The attributes a principal brings to rule evaluation. People carry all of
/// them; agent grants carry only what they were explicitly given.
struct PrincipalView<'a> {
    principal_id: &'a str,
    groups: &'a BTreeSet<String>,
    role: Option<&'a str>,
    site: Option<&'a str>,
    band: Option<u8>,
}

/// Rules 1 + 2 for non-public sensitivities: OR over ReBAC grant rules, AND
/// over ABAC conditions where present, plus the special_category condition.
fn eval_rules(view: &PrincipalView, doc: &Document) -> Decision {
    let mut grant_reasons: Vec<String> = Vec::new();
    let mut condition_reasons: Vec<String> = Vec::new();
    let mut deny_reasons: Vec<String> = Vec::new();
    let mut conditions_hold = true;

    for rule in &doc.acl_refs {
        match rule.kind {
            AclKind::Public => grant_reasons.push("REBAC:public".to_string()),
            AclKind::Group => {
                let group = rule.group.as_deref().expect("validated: group payload");
                if view.groups.contains(group) {
                    grant_reasons.push(format!("REBAC:{group}"));
                }
            }
            AclKind::Role => {
                let role = rule.role.as_deref().expect("validated: role payload");
                if view.role == Some(role) {
                    grant_reasons.push(format!("REBAC:role:{role}"));
                }
            }
            AclKind::AttrSite => {
                let site = rule.site.as_deref().expect("validated: site payload");
                if view.site == Some(site) {
                    condition_reasons.push(format!("ABAC:site_match:{site}"));
                } else {
                    conditions_hold = false;
                    deny_reasons.push(format!("DENY:site_mismatch:{site}"));
                }
            }
            AclKind::AttrBandMin => {
                let min_band = rule.min_band.expect("validated: min_band payload");
                if view.band.is_some_and(|b| b >= min_band) {
                    condition_reasons.push(format!("ABAC:band_min:{min_band}"));
                } else {
                    conditions_hold = false;
                    deny_reasons.push(format!("DENY:band_below_min:{min_band}"));
                }
            }
        }
    }

    // Rule 2, special_category condition: HR group membership OR subject
    // identity, on top of whatever ReBAC granted.
    if doc.sensitivity == Sensitivity::SpecialCategory {
        if view.groups.contains(HR_GROUP_ID) {
            condition_reasons.push("ABAC:special_category_hr".to_string());
        } else if doc.subject_id.as_deref() == Some(view.principal_id) {
            condition_reasons.push("ABAC:special_category_subject".to_string());
        } else {
            conditions_hold = false;
            deny_reasons.push("DENY:special_category_not_hr_or_subject".to_string());
        }
    }

    if !grant_reasons.is_empty() && conditions_hold {
        grant_reasons.extend(condition_reasons);
        Decision::Allow(normalize(grant_reasons))
    } else {
        if grant_reasons.is_empty() {
            deny_reasons.push("DENY:default".to_string());
        }
        Decision::Deny(deny_reasons)
    }
}

/// Reasons are emitted sorted and deduplicated so artifacts are canonical.
fn normalize(reasons: Vec<String>) -> Vec<String> {
    let set: BTreeSet<String> = reasons.into_iter().collect();
    set.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Structural validation (fail-closed)
// ---------------------------------------------------------------------------

fn note_duplicate<'a>(
    seen: &mut HashSet<&'a str>,
    id: &'a str,
    what: &str,
    errors: &mut Vec<String>,
) {
    if !seen.insert(id) {
        errors.push(format!("duplicate {what} id: {id}"));
    }
}

fn require_nonempty(value: &str, what: &str, errors: &mut Vec<String>) {
    if value.is_empty() {
        errors.push(format!("{what} must be a non-empty string"));
    }
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

fn is_datetime_z(s: &str) -> bool {
    // YYYY-MM-DDTHH:MM:SS(.fraction)?Z
    let Some(rest) = s.strip_suffix('Z') else {
        return false;
    };
    let (head, frac) = match rest.split_once('.') {
        Some((head, frac)) => (head, Some(frac)),
        None => (rest, None),
    };
    if let Some(frac) = frac {
        if frac.is_empty() || !frac.bytes().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    let b = head.as_bytes();
    b.len() == 19
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            _ => c.is_ascii_digit(),
        })
}

fn validate_company(company: &CompanyFile, errors: &mut Vec<String>) {
    require_nonempty(&company.company.name, "company.name", errors);
    require_nonempty(
        &company.company.regulatory_context,
        "company.regulatory_context",
        errors,
    );
    if !company.company.fictional {
        errors.push("company.fictional must be true".to_string());
    }
    if company.sites.is_empty() {
        errors.push("sites must be non-empty".to_string());
    }
    if company.departments.len() != 8 {
        errors.push(format!(
            "departments must have exactly 8 entries, found {}",
            company.departments.len()
        ));
    }
    if company.sources.len() != 5 {
        errors.push(format!(
            "sources must have exactly 5 entries, found {}",
            company.sources.len()
        ));
    }

    let mut site_ids: HashSet<&str> = HashSet::new();
    for site in &company.sites {
        require_nonempty(&site.id, "site.id", errors);
        require_nonempty(&site.name, "site.name", errors);
        note_duplicate(&mut site_ids, &site.id, "site", errors);
    }
    let departments: HashSet<&str> = company.departments.iter().map(String::as_str).collect();

    let mut person_ids: HashSet<&str> = HashSet::new();
    for person in &company.people {
        require_nonempty(&person.id, "person.id", errors);
        require_nonempty(&person.name, "person.name", errors);
        require_nonempty(&person.role, "person.role", errors);
        note_duplicate(&mut person_ids, &person.id, "person", errors);
        if !(1..=5).contains(&person.employment_band) {
            errors.push(format!(
                "person {}: employment_band {} outside 1..=5",
                person.id, person.employment_band
            ));
        }
        if !site_ids.contains(person.site.as_str()) {
            errors.push(format!(
                "person {}: dangling site reference {}",
                person.id, person.site
            ));
        }
        if !departments.contains(person.department.as_str()) {
            errors.push(format!(
                "person {}: unknown department {}",
                person.id, person.department
            ));
        }
        if !is_date(&person.start_date) {
            errors.push(format!(
                "person {}: start_date {:?} is not YYYY-MM-DD",
                person.id, person.start_date
            ));
        }
        if !person.synthetic {
            errors.push(format!("person {}: synthetic must be true", person.id));
        }
    }
    for person in &company.people {
        if let Some(manager_id) = &person.manager_id {
            if !person_ids.contains(manager_id.as_str()) {
                errors.push(format!(
                    "person {}: dangling manager reference {}",
                    person.id, manager_id
                ));
            }
        }
    }

    let mut group_ids: HashSet<&str> = HashSet::new();
    for group in &company.groups {
        require_nonempty(&group.id, "group.id", errors);
        require_nonempty(&group.name, "group.name", errors);
        require_nonempty(&group.description, "group.description", errors);
        note_duplicate(&mut group_ids, &group.id, "group", errors);
        for member in &group.member_ids {
            if !person_ids.contains(member.as_str()) {
                errors.push(format!(
                    "group {}: dangling member reference {}",
                    group.id, member
                ));
            }
        }
    }

    let mut agent_ids: HashSet<&str> = HashSet::new();
    for agent in &company.agents {
        require_nonempty(&agent.id, "agent.id", errors);
        require_nonempty(&agent.name, "agent.name", errors);
        note_duplicate(&mut agent_ids, &agent.id, "agent", errors);
        if person_ids.contains(agent.id.as_str()) {
            errors.push(format!("agent id {} collides with a person id", agent.id));
        }
        if !person_ids.contains(agent.owner_user_id.as_str()) {
            errors.push(format!(
                "agent {}: dangling owner reference {}",
                agent.id, agent.owner_user_id
            ));
        }
        for group in &agent.grant.groups {
            if !group_ids.contains(group.as_str()) {
                errors.push(format!(
                    "agent {}: dangling grant group reference {}",
                    agent.id, group
                ));
            }
        }
        if let Some(site) = &agent.grant.site {
            if !site_ids.contains(site.as_str()) {
                errors.push(format!(
                    "agent {}: dangling grant site reference {}",
                    agent.id, site
                ));
            }
        }
        if let Some(band) = agent.grant.employment_band {
            if !(1..=5).contains(&band) {
                errors.push(format!(
                    "agent {}: grant employment_band {band} outside 1..=5",
                    agent.id
                ));
            }
        }
        if !agent.synthetic {
            errors.push(format!("agent {}: synthetic must be true", agent.id));
        }
    }
}

fn validate_acl_rule(
    doc: &Document,
    rule: &AclRule,
    group_ids: &HashSet<&str>,
    site_ids: &HashSet<&str>,
    errors: &mut Vec<String>,
) {
    require_nonempty(&rule.rule_id, "acl rule_id", errors);
    let where_ = format!("document {} rule {}", doc.id, rule.rule_id);

    // Exactly the payload key matching the kind must be present.
    let expect = |present: bool, field: &str, required: bool, errors: &mut Vec<String>| {
        if required && !present {
            errors.push(format!("{where_}: kind requires payload key {field}"));
        }
        if !required && present {
            errors.push(format!("{where_}: unexpected payload key {field} for kind"));
        }
    };
    let (g, r, s, m) = (
        rule.group.is_some(),
        rule.role.is_some(),
        rule.site.is_some(),
        rule.min_band.is_some(),
    );
    match rule.kind {
        AclKind::Public => {
            expect(g, "group", false, errors);
            expect(r, "role", false, errors);
            expect(s, "site", false, errors);
            expect(m, "min_band", false, errors);
        }
        AclKind::Group => {
            expect(g, "group", true, errors);
            expect(r, "role", false, errors);
            expect(s, "site", false, errors);
            expect(m, "min_band", false, errors);
            if let Some(group) = &rule.group {
                if !group_ids.contains(group.as_str()) {
                    errors.push(format!("{where_}: dangling group reference {group}"));
                }
            }
        }
        AclKind::Role => {
            expect(g, "group", false, errors);
            expect(r, "role", true, errors);
            expect(s, "site", false, errors);
            expect(m, "min_band", false, errors);
            if let Some(role) = &rule.role {
                require_nonempty(role, &format!("{where_}: role"), errors);
            }
        }
        AclKind::AttrSite => {
            expect(g, "group", false, errors);
            expect(r, "role", false, errors);
            expect(s, "site", true, errors);
            expect(m, "min_band", false, errors);
            if let Some(site) = &rule.site {
                if !site_ids.contains(site.as_str()) {
                    errors.push(format!("{where_}: dangling site reference {site}"));
                }
            }
        }
        AclKind::AttrBandMin => {
            expect(g, "group", false, errors);
            expect(r, "role", false, errors);
            expect(s, "site", false, errors);
            expect(m, "min_band", true, errors);
            if let Some(band) = rule.min_band {
                if !(1..=5).contains(&band) {
                    errors.push(format!("{where_}: min_band {band} outside 1..=5"));
                }
            }
        }
    }
}

fn validate_documents(documents: &[Document], company: &CompanyFile, errors: &mut Vec<String>) {
    let person_ids: HashSet<&str> = company.people.iter().map(|p| p.id.as_str()).collect();
    let departments: HashSet<&str> = company.departments.iter().map(String::as_str).collect();
    let group_ids: HashSet<&str> = company.groups.iter().map(|g| g.id.as_str()).collect();
    let site_ids: HashSet<&str> = company.sites.iter().map(|s| s.id.as_str()).collect();

    let mut doc_ids: HashSet<&str> = HashSet::new();
    for doc in documents {
        require_nonempty(&doc.id, "document.id", errors);
        note_duplicate(&mut doc_ids, &doc.id, "document", errors);
    }

    // old id -> ids of documents claiming to supersede it (fan-in is refused:
    // an "effective successor" must be unique to be meaningful).
    let mut superseded_by: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

    for doc in documents {
        require_nonempty(&doc.source, &format!("document {}: source", doc.id), errors);
        require_nonempty(&doc.title, &format!("document {}: title", doc.id), errors);
        require_nonempty(&doc.body, &format!("document {}: body", doc.id), errors);
        if !person_ids.contains(doc.author_id.as_str()) {
            errors.push(format!(
                "document {}: dangling author reference {}",
                doc.id, doc.author_id
            ));
        }
        if !departments.contains(doc.department.as_str()) {
            errors.push(format!(
                "document {}: unknown department {}",
                doc.id, doc.department
            ));
        }
        if !is_datetime_z(&doc.created_at) {
            errors.push(format!(
                "document {}: created_at {:?} is not an ISO-8601 Z timestamp",
                doc.id, doc.created_at
            ));
        }
        if doc.version < 1 {
            errors.push(format!("document {}: version must be >= 1", doc.id));
        }
        if let Some(subject_id) = &doc.subject_id {
            if !person_ids.contains(subject_id.as_str()) {
                errors.push(format!(
                    "document {}: dangling subject reference {subject_id}",
                    doc.id
                ));
            }
        }
        if let Some(old_id) = &doc.supersedes {
            if old_id == &doc.id {
                errors.push(format!("document {}: supersedes itself", doc.id));
            }
            if !doc_ids.contains(old_id.as_str()) {
                errors.push(format!(
                    "document {}: dangling supersedes reference {old_id}",
                    doc.id
                ));
            } else {
                superseded_by.entry(old_id).or_default().push(&doc.id);
            }
        }

        let mut rule_ids: HashSet<&str> = HashSet::new();
        for rule in &doc.acl_refs {
            if !rule_ids.insert(&rule.rule_id) {
                errors.push(format!(
                    "document {}: duplicate acl rule_id {}",
                    doc.id, rule.rule_id
                ));
            }
            validate_acl_rule(doc, rule, &group_ids, &site_ids, errors);
        }
    }

    for (old_id, successors) in &superseded_by {
        if successors.len() > 1 {
            errors.push(format!(
                "document {old_id}: superseded by more than one document ({})",
                successors.join(", ")
            ));
        }
    }

    // Supersedes chains must be acyclic for "effective successor" to exist.
    let successor_of: HashMap<&str, &str> = documents
        .iter()
        .filter_map(|d| {
            d.supersedes
                .as_ref()
                .map(|old| (old.as_str(), d.id.as_str()))
        })
        .collect();
    for start in successor_of.keys() {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut current = *start;
        while let Some(next) = successor_of.get(current) {
            if !visited.insert(current) {
                errors.push(format!("supersedes cycle involving document {start}"));
                break;
            }
            current = next;
        }
    }
}

/// The compiler consumes traps.json (mosaic pass-through), so its references
/// must resolve; anything dangling refuses the compile.
fn validate_traps(
    traps: &TrapsFile,
    company: &CompanyFile,
    documents: &[Document],
    errors: &mut Vec<String>,
) {
    let person_ids: HashSet<&str> = company.people.iter().map(|p| p.id.as_str()).collect();
    let agent_ids: HashSet<&str> = company.agents.iter().map(|a| a.id.as_str()).collect();
    let site_ids: HashSet<&str> = company.sites.iter().map(|s| s.id.as_str()).collect();
    let doc_ids: HashSet<&str> = documents.iter().map(|d| d.id.as_str()).collect();

    let check_doc = |id: &str, where_: &str, errors: &mut Vec<String>| {
        if !doc_ids.contains(id) {
            errors.push(format!("traps {where_}: dangling document reference {id}"));
        }
    };
    for (i, t) in traps.effective_version.iter().enumerate() {
        let where_ = format!("effective_version[{i}]");
        check_doc(&t.current_id, &where_, errors);
        check_doc(&t.superseded_id, &where_, errors);
    }
    for (i, t) in traps.mosaic.iter().enumerate() {
        let where_ = format!("mosaic[{i}]");
        check_doc(&t.doc_a, &where_, errors);
        check_doc(&t.doc_b, &where_, errors);
        if !person_ids.contains(t.principal_id.as_str())
            && !agent_ids.contains(t.principal_id.as_str())
        {
            errors.push(format!(
                "traps {where_}: dangling principal reference {}",
                t.principal_id
            ));
        }
    }
    for (i, t) in traps.confused_deputy.iter().enumerate() {
        let where_ = format!("confused_deputy[{i}]");
        check_doc(&t.resource_id, &where_, errors);
        if !agent_ids.contains(t.agent_id.as_str()) {
            errors.push(format!(
                "traps {where_}: dangling agent reference {}",
                t.agent_id
            ));
        }
        if !person_ids.contains(t.owner_id.as_str()) {
            errors.push(format!(
                "traps {where_}: dangling owner reference {}",
                t.owner_id
            ));
        }
    }
    for (i, t) in traps.manager_overreach.iter().enumerate() {
        let where_ = format!("manager_overreach[{i}]");
        check_doc(&t.resource_id, &where_, errors);
        for (label, id) in [("manager", &t.manager_id), ("subject", &t.subject_id)] {
            if !person_ids.contains(id.as_str()) {
                errors.push(format!("traps {where_}: dangling {label} reference {id}"));
            }
        }
    }
    for (i, t) in traps.cross_site.iter().enumerate() {
        let where_ = format!("cross_site[{i}]");
        check_doc(&t.resource_id, &where_, errors);
        if !person_ids.contains(t.principal_id.as_str()) {
            errors.push(format!(
                "traps {where_}: dangling principal reference {}",
                t.principal_id
            ));
        }
        for site in [&t.required_site, &t.principal_site] {
            if !site_ids.contains(site.as_str()) {
                errors.push(format!("traps {where_}: dangling site reference {site}"));
            }
        }
    }
}
