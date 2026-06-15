//! AR-1: the humanization layer — DISPLAY IDENTITY over the FROZEN skeleton.
//!
//! THE ONE INVARIANT: names and faces are COSMETIC. Nothing in this module
//! reads, derives, or alters an authorization fact. The compiled allowlists,
//! group memberships, reporting ids, bands, sites and ownership are M1's and
//! stay M1's to the byte. This layer only decorates the principals the
//! skeleton already defined — it never widens what anyone can see.
//!
//! WHAT IT PRODUCES (`fixtures/people.json`): one record per human principal
//! with a fresh, diverse, seeded display name (the company fixture's invented
//! names are overridden on every surface — the owner's AR-1 ruling), a title
//! and department label carried straight from the frozen role/department, a
//! generated bio/location/work-style/personality tag, the reporting lines
//! DERIVED from the skeleton's `manager_id`, and 0–5 projects DERIVED from the
//! exact same Lane assignment rule the service already runs (so a project's
//! evidence is always inside the principal's compiled allowlist — access is
//! never invented).
//!
//! DETERMINISM: the whole file is a pure function of the frozen inputs plus
//! fixed seeds. Two generations are byte-identical (AR-1's determinism test),
//! and the running service REGENERATES at startup and refuses to serve a
//! `people.json` that disagrees with what the live skeleton derives — a stale
//! or hand-edited humanization layer fails closed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use retrieval::index::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::lane::BoxSeed;

/// The fixed seed that pins every deterministic choice in the layer. Changing
/// it reshuffles names/bios wholesale, so it never changes casually.
const SEED: &str = "aperture-ar-1";

/// Governance bound: a person surfaces AT MOST this many projects, and FEWER
/// when the skeleton grants them fewer capabilities. Access is never invented
/// to reach a quota — some principals legitimately show 0 (e.g. the access
/// review shadow).
pub const MAX_PROJECTS: usize = 5;

// ---------------------------------------------------------------------------
// Seeded pools — diverse, fictional, mixed-gender. None names a real person.
// ---------------------------------------------------------------------------

const FIRST_NAMES: [&str; 60] = [
    "Amara", "Wei", "Priya", "Diego", "Fatima", "Kenji", "Aisha", "Mateo", "Ling", "Omar",
    "Sofia", "Raj", "Nadia", "Hiroshi", "Zara", "Carlos", "Mei", "Ahmed", "Elena", "Kwame",
    "Yuki", "Ana", "Tariq", "Ingrid", "Jin", "Leila", "Pablo", "Sanjay", "Noor", "Hassan",
    "Camila", "Bao", "Ravi", "Yara", "Andrei", "Mariam", "Tao", "Lucia", "Idris", "Keiko",
    "Marco", "Anaya", "Dmitri", "Rania", "Hana", "Felix", "Selina", "Arjun", "Lin", "Oskar",
    "Nia", "Viktor", "Imani", "Tomas", "Chiara", "Rohan", "Asha", "Niko", "Freya", "Samir",
];

const SURNAMES: [&str; 60] = [
    "Chen", "Patel", "Okafor", "Nguyen", "Garcia", "Khan", "Kim", "Rossi", "Andersson", "Tanaka",
    "Mwangi", "Silva", "Cohen", "O'Brien", "Haddad", "Reyes", "Novak", "Adeyemi", "Petrov",
    "Santos", "Yusuf", "Lindqvist", "Park", "Dubois", "Romano", "Mensah", "Sharma", "Ali",
    "Nakamura", "Costa", "Ibrahim", "Walsh", "Kowalski", "Mendoza", "Bauer", "Haq", "Singh",
    "Moreau", "Diallo", "Vargas", "Schmidt", "Lee", "Fernandes", "Abebe", "Hoffmann", "Castillo",
    "Ortega", "Banerjee", "Kaur", "Larsen", "Marino", "Osei", "Volkov", "Nair", "Bianchi",
    "Eriksson", "Suzuki", "Flores", "Tetteh", "Rahman",
];

const MBTI: [&str; 16] = [
    "INTJ", "INTP", "ENTJ", "ENTP", "INFJ", "INFP", "ENFJ", "ENFP", "ISTJ", "ISFJ", "ESTJ", "ESFJ",
    "ISTP", "ISFP", "ESTP", "ESFP",
];

const BIO_TRAITS: [&str; 14] = [
    "Methodical and detail-driven",
    "Calm under operational pressure",
    "Pragmatic and outcomes-focused",
    "Collaborative across teams",
    "Numbers-led and precise",
    "Process-minded and thorough",
    "Curious and quick to learn",
    "Steady and dependable",
    "Direct and decisive",
    "Quietly rigorous",
    "Hands-on and practical",
    "A systems thinker by instinct",
    "Patient with ambiguity",
    "Energised by hard problems",
];

const BIO_STYLES: [&str; 14] = [
    "prefers concise, metrics-focused updates",
    "values documented decisions over meetings",
    "communicates in writing first",
    "keeps a tidy audit trail",
    "favours short feedback loops",
    "asks for evidence before opinions",
    "defaults to transparency",
    "protective of focus time",
    "happy to pair on the tricky calls",
    "reads the detail before deciding",
    "keeps stakeholders in the loop",
    "leads with questions",
    "biases toward action",
    "documents as they go",
];

// ---------------------------------------------------------------------------
// File shape (sorted keys via serde_json Value; pretty, with trailing newline)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRecord {
    pub capability_id: String,
    pub capability_name: String,
    pub initiative_name: String,
    pub role: String,
    pub status: String,
    pub strategy_name: String,
    pub workflow_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanRecord {
    pub avatar_ref: String,
    pub bio: String,
    pub department_label: String,
    pub display_name: String,
    pub id: String,
    pub location: String,
    pub manages: Vec<String>,
    pub personality_tag: String,
    pub projects: Vec<ProjectRecord>,
    pub reports_to: Option<String>,
    pub seniority: String,
    pub title: String,
    pub work_style: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeopleFile {
    pub people: Vec<HumanRecord>,
}

/// The minimal slice of a human principal the layer decorates — every field
/// here is a FROZEN structural fact read from company.json, never written.
#[derive(Debug, Clone)]
pub struct PersonInput {
    pub id: String,
    pub role: String,
    pub department: String,
    pub employment_band: u8,
    pub site: String,
    pub manager_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Deterministic helpers (sha256-seeded; no rand, no clock, no platform drift)
// ---------------------------------------------------------------------------

fn hash_u64(parts: &[&str]) -> u64 {
    let preimage = format!("{SEED}\u{1f}{}", parts.join("\u{1f}"));
    let hex = sha256_hex(preimage.as_bytes());
    u64::from_str_radix(&hex[..16], 16).expect("16 hex chars are a valid u64")
}

fn pick<'a>(items: &[&'a str], parts: &[&str]) -> &'a str {
    items[(hash_u64(parts) as usize) % items.len()]
}

/// "Capability: Cold Storage Monitoring 01" -> "Cold Storage Monitoring".
fn capability_theme(name: &str) -> String {
    let stripped = name.strip_prefix("Capability: ").unwrap_or(name);
    stripped
        .trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ')
        .to_string()
}

fn department_focus(department: &str) -> &'static str {
    match department {
        "Quality & Compliance" => "GDP compliance and batch-release quality",
        "Warehouse Operations" => "cold-chain storage, picking and despatch",
        "Pharmacy Services" => "responsible-pharmacist oversight and dispensing",
        "Finance" => "financial control, payroll and credit",
        "IT" => "the service desk, infrastructure and applications",
        "HR" => "people systems, learning and casework",
        "Sales & Accounts" => "customer accounts, pricing and contracts",
        "Executive" => "company strategy and governance",
        _ => "day-to-day operations",
    }
}

/// Seniority from the ROLE first (a band-1 COO is still leadership), band as
/// the fallback — display only, never an authorization input.
fn seniority_of(role: &str, band: u8) -> &'static str {
    let r = role.to_ascii_lowercase();
    if r.contains("chief")
        || r.contains("head of")
        || r.contains("director")
        || r.contains("company secretary")
    {
        "Leadership"
    } else if r.contains("lead")
        || r.contains("controller")
        || r.contains("manager")
        || r.contains("responsible person")
        || r.contains("responsible pharmacist")
        || r.contains("supervisor")
    {
        "Senior"
    } else if band >= 3 {
        "Mid-level"
    } else {
        "Associate"
    }
}

fn project_role(seniority: &str, top: bool) -> &'static str {
    match (seniority, top) {
        ("Leadership", _) => "Lead",
        ("Senior", true) => "Lead",
        ("Senior", false) => "Contributor",
        (_, true) => "Contributor",
        _ => "Member",
    }
}

fn project_status(id: &str, capability_id: &str) -> &'static str {
    // ~60% Active, 20% Planned, 20% Done — deterministic per (person, cap).
    match hash_u64(&["status", id, capability_id]) % 5 {
        0 | 1 | 2 => "Active",
        3 => "Planned",
        _ => "Done",
    }
}

fn work_style(id: &str, department: &str, role: &str) -> &'static str {
    // Physical roles are on-site by necessity; everyone else is hybrid unless
    // the seed says remote (~1 in 4). Deterministic, role-aware.
    let on_site_pharmacy = department == "Pharmacy Services"
        && (role.contains("Dispensary")
            || role.contains("Pharmacist")
            || role.contains("Technician"));
    if department == "Warehouse Operations" || on_site_pharmacy {
        "On-site"
    } else if hash_u64(&["workstyle", id]) % 4 == 0 {
        "Remote"
    } else {
        "Hybrid"
    }
}

fn location_of(site: &str) -> String {
    let town = match site {
        "site_keldonbury" => "Keldonbury",
        "site_withermoor" => "Withermoor",
        other => other,
    };
    format!("{town}, UK")
}

// ---------------------------------------------------------------------------
// Name assignment — diverse, deterministic, no duplicate FULL names
// ---------------------------------------------------------------------------

fn assign_names(people: &[PersonInput]) -> BTreeMap<String, String> {
    // Stable id order so collision resolution is reproducible.
    let mut ids: Vec<&str> = people.iter().map(|p| p.id.as_str()).collect();
    ids.sort_unstable();

    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    for id in ids {
        let first = pick(&FIRST_NAMES, &["first", id]);
        // Rotate the surname deterministically until the full name is unique.
        let base = hash_u64(&["surname", id]) as usize;
        let mut full = String::new();
        for offset in 0..SURNAMES.len() {
            let surname = SURNAMES[(base + offset) % SURNAMES.len()];
            let candidate = format!("{first} {surname}");
            if !used.contains(&candidate) {
                full = candidate;
                break;
            }
        }
        if full.is_empty() {
            // 60 surnames per first name vastly exceeds any first-name bucket;
            // fall back to an indexed surname so generation never wedges.
            full = format!("{first} {}", SURNAMES[base % SURNAMES.len()]);
        }
        used.insert(full.clone());
        names.insert(id.to_string(), full);
    }
    names
}

fn build_bio(
    person: &PersonInput,
    seniority: &str,
    projects: &[ProjectRecord],
    used: &mut BTreeSet<String>,
) -> String {
    let lead = format!("{} at Bryremead Distribution Ltd.", person.role);
    let focus = match projects.first() {
        Some(top) => format!("Currently working across {}.", capability_theme(&top.capability_name)),
        None => format!("Works on {}.", department_focus(&person.department)),
    };
    // Rotate trait/style deterministically until the bio is unique. The pools
    // (14 x 14) dwarf any department/seniority bucket, so this converges fast.
    let trait_base = hash_u64(&["trait", &person.id]) as usize;
    let style_base = hash_u64(&["style", &person.id]) as usize;
    let _ = seniority; // seniority shapes projects; bio stays role-anchored.
    for offset in 0..(BIO_TRAITS.len() * BIO_STYLES.len()) {
        let t = BIO_TRAITS[(trait_base + offset) % BIO_TRAITS.len()];
        let s = BIO_STYLES[(style_base + offset / BIO_TRAITS.len()) % BIO_STYLES.len()];
        let bio = format!("{lead} {focus} {t}; {s}.");
        if !used.contains(&bio) {
            used.insert(bio.clone());
            return bio;
        }
    }
    // Unreachable for 120 people, but never emit a duplicate: tag with id.
    let bio = format!("{lead} {focus} ({}).", person.id);
    used.insert(bio.clone());
    bio
}

fn build_projects(person: &PersonInput, seeds: Option<&Vec<BoxSeed>>, seniority: &str) -> Vec<ProjectRecord> {
    let Some(seeds) = seeds else {
        return Vec::new();
    };
    seeds
        .iter()
        .take(MAX_PROJECTS)
        .enumerate()
        .map(|(rank, seed)| ProjectRecord {
            capability_id: seed.capability.id.clone(),
            capability_name: seed.capability.name.clone(),
            initiative_name: seed.provenance.initiative.name.clone(),
            role: project_role(seniority, rank == 0).to_string(),
            status: project_status(&person.id, &seed.capability.id).to_string(),
            strategy_name: seed.provenance.strategy.name.clone(),
            workflow_name: seed.provenance.workflow.name.clone(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The generator — a pure function of (frozen people, derived lane seeds)
// ---------------------------------------------------------------------------

/// Generates the full humanization layer. `lane_seeds` is the service's
/// startup derivation (`AppState.lane_seeds`); projects are its top
/// `MAX_PROJECTS` per person, so a project's evidence is, by construction,
/// inside that person's compiled allowlist.
pub fn generate(people: &[PersonInput], lane_seeds: &BTreeMap<String, Vec<BoxSeed>>) -> PeopleFile {
    let names = assign_names(people);

    let mut sorted: Vec<&PersonInput> = people.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    // Direct reports per manager (derived inverse of manager_id).
    let mut reports_of: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for person in &sorted {
        if let Some(manager) = &person.manager_id {
            reports_of.entry(manager.as_str()).or_default().push(person.id.as_str());
        }
    }

    let mut used_bios: BTreeSet<String> = BTreeSet::new();
    let mut records = Vec::with_capacity(sorted.len());
    for person in &sorted {
        let display_name = names.get(&person.id).expect("every id assigned a name").clone();
        let seniority = seniority_of(&person.role, person.employment_band);
        let projects = build_projects(person, lane_seeds.get(&person.id), seniority);
        let bio = build_bio(person, seniority, &projects, &mut used_bios);

        let reports_to = person
            .manager_id
            .as_ref()
            .and_then(|m| names.get(m))
            .cloned();
        let mut manages: Vec<String> = reports_of
            .get(person.id.as_str())
            .map(|reports| reports.iter().filter_map(|r| names.get(*r).cloned()).collect())
            .unwrap_or_default();
        manages.sort();

        records.push(HumanRecord {
            avatar_ref: format!("faces/{}.jpg", person.id),
            bio,
            department_label: person.department.clone(),
            display_name,
            id: person.id.clone(),
            location: location_of(&person.site),
            manages,
            personality_tag: pick(&MBTI, &["mbti", &person.id]).to_string(),
            projects,
            reports_to,
            seniority: seniority.to_string(),
            title: person.role.clone(),
            work_style: work_style(&person.id, &person.department, &person.role).to_string(),
        });
    }

    PeopleFile { people: records }
}

/// Canonical pretty bytes: serde_json::Value is a BTreeMap when
/// `preserve_order` is off (it is — see M1), so keys sort; pretty print +
/// trailing newline matches the house fixture style.
pub fn to_pretty_bytes(file: &PeopleFile) -> Result<Vec<u8>> {
    let value = serde_json::to_value(file).context("people layer to value")?;
    let mut text = serde_json::to_string_pretty(&value).context("people layer pretty print")?;
    text.push('\n');
    Ok(text.into_bytes())
}

// ---------------------------------------------------------------------------
// Company reader (frozen, hash-verified at the call site) and validation
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CompanyForHumanize {
    people: Vec<CompanyPerson>,
}

#[derive(Debug, Deserialize)]
struct CompanyPerson {
    id: String,
    role: String,
    department: String,
    employment_band: u8,
    site: String,
    #[serde(default)]
    manager_id: Option<String>,
}

/// Reads the frozen people inputs from company.json, refusing on hash drift
/// (the same pin every governed reader uses).
pub fn read_person_inputs(fixtures_dir: &Path, company_sha256: &str) -> Result<Vec<PersonInput>> {
    let path = fixtures_dir.join("company.json");
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    if sha256_hex(&bytes) != company_sha256 {
        bail!("company.json does not match the M1-pinned hash; refusing");
    }
    let company: CompanyForHumanize =
        serde_json::from_slice(&bytes).with_context(|| format!("{} fails parse", path.display()))?;
    Ok(company
        .people
        .into_iter()
        .map(|p| PersonInput {
            id: p.id,
            role: p.role,
            department: p.department,
            employment_band: p.employment_band,
            site: p.site,
            manager_id: p.manager_id,
        })
        .collect())
}

/// The runtime humanization layer: the loaded records, indexed by id, plus the
/// pin of the bytes served.
pub struct PeopleLayer {
    pub by_id: BTreeMap<String, HumanRecord>,
    pub order: Vec<String>,
    pub sha256: String,
}

impl PeopleLayer {
    pub fn get(&self, id: &str) -> Option<&HumanRecord> {
        self.by_id.get(id)
    }

    pub fn roster(&self) -> impl Iterator<Item = &HumanRecord> {
        self.order.iter().map(move |id| &self.by_id[id])
    }
}

// ---------------------------------------------------------------------------
// Display projections — the light directory card and the masthead record the
// service surfaces. NEITHER carries a document id, a holding, or any evidence:
// a card is org-structural (name + title + department + avatar), exactly the
// internal-grade tier the Atlas BRM structure already publishes. Humanizing
// never widens visibility.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonCard {
    pub avatar_ref: String,
    pub department_label: String,
    pub display_name: String,
    pub id: String,
    pub title: String,
}

impl PersonCard {
    pub fn from_record(record: &HumanRecord) -> PersonCard {
        PersonCard {
            avatar_ref: record.avatar_ref.clone(),
            department_label: record.department_label.clone(),
            display_name: record.display_name.clone(),
            id: record.id.clone(),
            title: record.title.clone(),
        }
    }
}

/// The directory card for `id`, or `None` when this world has no humanization
/// layer or `id` is not a humanized principal (e.g. an agent).
pub fn card_for(layer: Option<&PeopleLayer>, id: &str) -> Option<PersonCard> {
    layer.and_then(|l| l.get(id)).map(PersonCard::from_record)
}

/// The full masthead record for `id` (the Lens subject's human attributes),
/// or `None` as above.
pub fn masthead_for(layer: Option<&PeopleLayer>, id: &str) -> Option<HumanRecord> {
    layer.and_then(|l| l.get(id)).cloned()
}

/// The display name to show for `id`, falling back to the supplied frozen
/// company.json name when no humanization layer is loaded — the one place the
/// override happens, so every surface shows the same name.
pub fn display_name_or<'a>(layer: Option<&'a PeopleLayer>, id: &str, fallback: &'a str) -> &'a str {
    match layer.and_then(|l| l.get(id)) {
        Some(record) => record.display_name.as_str(),
        None => fallback,
    }
}

/// Loads `fixtures/people.json` if present, then PROVES it against the live
/// skeleton: regenerate from the frozen inputs + the service's own lane seeds
/// and require byte-for-byte agreement. A stale or hand-edited layer — names
/// out of sync, an invented project, a drifted reporting line — fails closed.
/// `Ok(None)` = no humanization layer in this world (display falls back to the
/// frozen company.json names; keeps synthetic-fixture test worlds building).
pub fn load_and_verify(
    fixtures_dir: &Path,
    company_sha256: &str,
    lane_seeds: &BTreeMap<String, Vec<BoxSeed>>,
) -> Result<Option<PeopleLayer>> {
    let path = fixtures_dir.join("people.json");
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let sha256 = sha256_hex(&bytes);
    let loaded: PeopleFile =
        serde_json::from_slice(&bytes).with_context(|| format!("{} fails strict parse", path.display()))?;

    let inputs = read_person_inputs(fixtures_dir, company_sha256)?;
    let expected = generate(&inputs, lane_seeds);
    if expected != loaded {
        bail!(
            "people.json disagrees with what the live skeleton derives; refusing \
             (regenerate with `cargo run --bin gen_people`)"
        );
    }

    let mut by_id = BTreeMap::new();
    let mut order = Vec::with_capacity(loaded.people.len());
    for record in loaded.people {
        order.push(record.id.clone());
        by_id.insert(record.id.clone(), record);
    }
    Ok(Some(PeopleLayer { by_id, order, sha256 }))
}
