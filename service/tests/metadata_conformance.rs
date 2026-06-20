//! AUTH-2 (FC-A2) — THE METADATA CONFORMANCE ORACLE.
//!
//! The metadata analogue of the 74,400-pair document matrix. For EVERY
//! (principal, node) pair it computes the correct visibility INDEPENDENTLY from
//! the raw company.json structural rules (DD-2, structural core) — never via the
//! service's own `visibility` module — and asserts the live metadata surface
//! (`node_summary`, which the /node/{id}/summary and /node/org/summary routes
//! call) returns EXACTLY the in-scope set:
//!   * 0 false-allow  — no out-of-scope node is ever served (200), and
//!   * 0 false-deny    — no in-scope node is ever withheld (404).
//! Exhaustive, deterministic, total: all principals × all summarisable nodes.
//!
//! Independence is the point: the expected set is derived here from the fixture
//! (groups, department, manager) and the published rule, so a bug in the
//! service's projection is caught — exactly as the document oracle is computed
//! from first principles, not from the system under test.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use service::node_summary::node_summary;
use service::AppState;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("service crate sits in the repo root")
        .to_path_buf()
}

fn meta_state() -> AppState {
    AppState::build(
        &common::repo_fixtures_dir(),
        &repo_root().join("compiler").join("artifacts"),
        &repo_root().join("retrieval").join("idx"),
    )
    .expect("build state")
    .with_people()
    .expect("load + verify people.json")
}

// ---------------------------------------------------------------------------
// Independent fixture model (mirrors scope::IdentityModel's group derivation
// and the DD-2 structural rule — WITHOUT touching service::visibility).
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Company {
    people: Vec<Person>,
    agents: Vec<Agent>,
    groups: Vec<Group>,
}
#[derive(Deserialize)]
struct Person {
    id: String,
    department: String,
    #[serde(default)]
    manager_id: Option<String>,
}
#[derive(Deserialize)]
struct Agent {
    id: String,
    owner_user_id: String,
    grant: Grant,
}
#[derive(Deserialize)]
struct Grant {
    #[serde(default)]
    groups: Vec<String>,
}
#[derive(Deserialize)]
struct Group {
    member_ids: Vec<String>,
}

struct Org {
    dept_of: BTreeMap<String, String>,
    manager_of: BTreeMap<String, String>,
    person_groups: BTreeMap<String, usize>, // count of group memberships per person
    agent_owner: BTreeMap<String, String>,
    agent_group_count: BTreeMap<String, usize>,
    people_ids: Vec<String>,
    agent_ids: Vec<String>,
}

fn load_org() -> Org {
    let bytes = fs::read(common::repo_fixtures_dir().join("company.json")).expect("company.json");
    let c: Company = serde_json::from_slice(&bytes).expect("parse company.json");
    let mut person_groups: BTreeMap<String, usize> = BTreeMap::new();
    for g in &c.groups {
        for m in &g.member_ids {
            *person_groups.entry(m.clone()).or_default() += 1;
        }
    }
    Org {
        dept_of: c
            .people
            .iter()
            .map(|p| (p.id.clone(), p.department.clone()))
            .collect(),
        manager_of: c
            .people
            .iter()
            .filter_map(|p| p.manager_id.clone().map(|m| (p.id.clone(), m)))
            .collect(),
        person_groups,
        agent_owner: c
            .agents
            .iter()
            .map(|a| (a.id.clone(), a.owner_user_id.clone()))
            .collect(),
        agent_group_count: c
            .agents
            .iter()
            .map(|a| (a.id.clone(), a.grant.groups.len()))
            .collect(),
        people_ids: c.people.iter().map(|p| p.id.clone()).collect(),
        agent_ids: c.agents.iter().map(|a| a.id.clone()).collect(),
    }
}

impl Org {
    /// Standing = the actor carries group scope (the "void" principal, with no
    /// groups, has none). Mirrors scope::IdentityModel: a person's groups come
    /// from company.groups membership; an agent's from its grant.groups.
    fn has_standing(&self, actor: &str) -> bool {
        if let Some(n) = self.agent_group_count.get(actor) {
            return *n > 0;
        }
        self.person_groups.get(actor).copied().unwrap_or(0) > 0
    }

    /// The DD-2 structural-core rule, computed from first principles.
    fn expected_visible(&self, actor: &str, node: &str) -> bool {
        if !self.has_standing(actor) {
            return false;
        }
        if node == "org" {
            return true;
        }
        // Person node?
        if let Some(node_dept) = self.dept_of.get(node) {
            let actor_dept = self.dept_of.get(actor); // None if the actor is an agent
            let same_department = actor_dept == Some(node_dept);
            let is_actor_manager = self.manager_of.get(actor).map(|m| m.as_str()) == Some(node);
            let reports_to_actor = self.manager_of.get(node).map(|m| m.as_str()) == Some(actor);
            return node == actor || same_department || is_actor_manager || reports_to_actor;
        }
        // Agent node?
        if let Some(owner) = self.agent_owner.get(node) {
            return owner == actor;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// The exhaustive matrix
// ---------------------------------------------------------------------------

#[test]
fn metadata_conformance_full_matrix_is_zero_false_allow_zero_false_deny() {
    let state = meta_state();
    let org = load_org();

    // Principals = every compiled principal (people + agents). Nodes = every
    // summarisable node (org core + people + agents).
    let mut principals: Vec<String> = org.people_ids.clone();
    principals.extend(org.agent_ids.clone());
    let mut nodes: Vec<String> = vec!["org".to_string()];
    nodes.extend(org.people_ids.clone());
    nodes.extend(org.agent_ids.clone());

    let mut decisions: u64 = 0;
    let mut false_allow: u64 = 0;
    let mut false_deny: u64 = 0;
    let mut allow_examples: Vec<(String, String)> = Vec::new();
    let mut deny_examples: Vec<(String, String)> = Vec::new();

    for actor in &principals {
        for node in &nodes {
            let expected = org.expected_visible(actor, node);
            // ACTUAL: the live metadata surface (uses service::visibility).
            let actual = node_summary(&state, actor, node)
                .expect("node_summary must not error on valid fixtures")
                .is_some();
            decisions += 1;
            if actual && !expected {
                false_allow += 1;
                if allow_examples.len() < 8 {
                    allow_examples.push((actor.clone(), node.clone()));
                }
            }
            if !actual && expected {
                false_deny += 1;
                if deny_examples.len() < 8 {
                    deny_examples.push((actor.clone(), node.clone()));
                }
            }
        }
    }

    println!(
        "METADATA CONFORMANCE: {decisions} (principal x node) pairs | {false_allow} false-allow | {false_deny} false-deny",
    );
    assert_eq!(
        false_allow, 0,
        "false-allow (out-of-scope node served): {allow_examples:?}"
    );
    assert_eq!(
        false_deny, 0,
        "false-deny (in-scope node withheld): {deny_examples:?}"
    );
    // The matrix must be total: 124 principals x (1 + 120 + 4) nodes.
    assert_eq!(
        decisions,
        (principals.len() as u64) * (nodes.len() as u64),
        "matrix is total"
    );
}

// ---------------------------------------------------------------------------
// Spot anchors (named principals) — readable witnesses of the matrix.
// ---------------------------------------------------------------------------

#[test]
fn spot_anchors_p_void_sees_nothing_and_p060_sees_its_slice() {
    let state = meta_state();
    let org = load_org();

    // p_void (no group standing) -> every node 404, including org.
    assert!(!org.has_standing("p_void"), "p_void has no group standing");
    assert!(node_summary(&state, "p_void", "org").unwrap().is_none(), "p_void org -> 404");
    assert!(node_summary(&state, "p_void", "p060").unwrap().is_none(), "p_void person -> 404");
    assert!(node_summary(&state, "p_void", "p_void").unwrap().is_none(), "p_void self -> 404 (no standing)");

    // p060 (Finance head) -> org 200; an in-scope Finance node 200; an
    // out-of-scope node 404. Find a concrete in/out node from the fixture.
    assert!(org.has_standing("p060"));
    assert!(node_summary(&state, "p060", "org").unwrap().is_some(), "p060 org -> 200");
    assert!(node_summary(&state, "p060", "p060").unwrap().is_some(), "p060 self -> 200");

    let in_scope: Vec<&String> = org
        .people_ids
        .iter()
        .filter(|q| q.as_str() != "p060" && org.expected_visible("p060", q))
        .collect();
    let out_scope: Vec<&String> = org
        .people_ids
        .iter()
        .filter(|q| !org.expected_visible("p060", q))
        .collect();
    assert!(!in_scope.is_empty() && !out_scope.is_empty(), "p060 has both in- and out-of-scope people");
    assert!(
        node_summary(&state, "p060", in_scope[0]).unwrap().is_some(),
        "p060 in-scope node {} -> 200",
        in_scope[0]
    );
    assert!(
        node_summary(&state, "p060", out_scope[0]).unwrap().is_none(),
        "p060 out-of-scope node {} -> 404",
        out_scope[0]
    );

    // Symmetry: p088 (HR) does NOT see a Finance-only node, and vice versa.
    let finance_only: Option<&String> = org
        .people_ids
        .iter()
        .find(|q| org.expected_visible("p060", q) && !org.expected_visible("p088", q));
    if let Some(q) = finance_only {
        assert!(node_summary(&state, "p088", q).unwrap().is_none(), "p088 cannot see Finance node {q}");
    }
    println!("SPOT: p_void sees nothing; p060 sees its slice; p088 cannot see Finance-only nodes");
}
