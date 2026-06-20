//! Lane governance harness AW-1..AW-8 (AP-6, v4a DISPLAY ONLY). OFFLINE.
//!
//! THE INVARIANTS under test: the amber class is unconstructable; the lane
//! is self-only by construction; display-only renders mutate nothing
//! outside the stores; SOP fail-closed blocks without hinting; the rollup
//! floor makes sub-5 capabilities ABSENT; inbox authority is M4's, with
//! both audits before both effects; derivation is deterministic; every box
//! carries its honesty line and illegal transitions refuse rowless.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::envelope::ScopeStatement;
use retrieval::index::{build_index, canonical_json_bytes, sha256_hex};
use serde_json::Value;
use service::agent::context::ProposalDraft;
use service::agent::proposals::{AuditEvent, ProposalStore};
use service::agent::standing::AgentRegistry;
use service::lane::{
    seeds_for_human, transition_is_legal, BoxStore, EffectClass, LaneBox, LaneBoxParts,
    LaneCapability, LaneEntryFacts, LaneGraph, LaneInitiative, LaneStrategy, LaneWorkflow,
    Provenance, ProvenanceNode, ROLLUP_HONESTY,
};
use service::{app, AppState, DocMeta};
use tower::ServiceExt;

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    for attempt in 0u64..50 {
        let _ = fs::remove_dir_all(&dir);
        if fs::create_dir_all(&dir).is_ok()
            && fs::read_dir(&dir)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(false)
        {
            return dir;
        }
        std::thread::sleep(std::time::Duration::from_millis(20 * (attempt.min(5) + 1)));
    }
    panic!("scratch dir {name} could not be reset");
}

/// doc -> (superseded, effective_successor)
type DocFacts = BTreeMap<String, (Option<bool>, Option<String>)>;

struct World {
    fixtures_dir: PathBuf,
    artifacts_dir: PathBuf,
    idx_dir: PathBuf,
    allowlists: BTreeMap<String, BTreeSet<String>>,
    facts: BTreeMap<String, DocFacts>,
    person_department: BTreeMap<String, String>,
    humans: Vec<String>,
    agents: Vec<String>,
    brm: Value,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("aw_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let mut allowlists = BTreeMap::new();
        let mut facts = BTreeMap::new();
        for artifact in &set.artifacts {
            allowlists.insert(
                artifact.principal_id.clone(),
                artifact
                    .entries
                    .iter()
                    .map(|e| e.document_id.clone())
                    .collect::<BTreeSet<_>>(),
            );
            facts.insert(
                artifact.principal_id.clone(),
                artifact
                    .entries
                    .iter()
                    .map(|e| {
                        (
                            e.document_id.clone(),
                            (e.superseded, e.effective_successor.clone()),
                        )
                    })
                    .collect::<BTreeMap<_, _>>(),
            );
        }

        let idx_dir = scratch("aw_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        let company: Value = serde_json::from_slice(
            &fs::read(fixtures_dir.join("company.json")).expect("read company"),
        )
        .expect("company parses");
        let mut person_department = BTreeMap::new();
        let mut humans = Vec::new();
        for person in company["people"].as_array().expect("people") {
            let id = person["id"].as_str().expect("id").to_string();
            person_department.insert(
                id.clone(),
                person["department"]
                    .as_str()
                    .expect("department")
                    .to_string(),
            );
            humans.push(id);
        }
        let agents: Vec<String> = company["agents"]
            .as_array()
            .expect("agents")
            .iter()
            .map(|a| a["id"].as_str().expect("id").to_string())
            .collect();

        let brm: Value =
            serde_json::from_slice(&fs::read(fixtures_dir.join("brm.json")).expect("read brm"))
                .expect("brm parses");

        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
            allowlists,
            facts,
            person_department,
            humans,
            agents,
            brm,
        }
    })
}

/// An independent LaneGraph built from the raw brm fixture (NOT through
/// the service's converter) for recompute properties.
fn graph_from_fixture(brm: &Value) -> LaneGraph {
    let nodes = |key: &str| brm[key].as_array().expect(key);
    LaneGraph {
        capabilities: nodes("capabilities")
            .iter()
            .map(|c| LaneCapability {
                document_ids: c["document_ids"]
                    .as_array()
                    .expect("document_ids")
                    .iter()
                    .map(|v| v.as_str().expect("doc id").to_string())
                    .collect(),
                id: c["id"].as_str().expect("id").to_string(),
                name: c["name"].as_str().expect("name").to_string(),
                workflow_id: c["workflow_id"].as_str().expect("workflow_id").to_string(),
            })
            .collect(),
        initiatives: nodes("initiatives")
            .iter()
            .map(|i| LaneInitiative {
                id: i["id"].as_str().expect("id").to_string(),
                name: i["name"].as_str().expect("name").to_string(),
                strategy_id: i["strategy_id"].as_str().expect("strategy_id").to_string(),
                workflow_ids: i["workflow_ids"]
                    .as_array()
                    .expect("workflow_ids")
                    .iter()
                    .map(|v| v.as_str().expect("wf id").to_string())
                    .collect(),
            })
            .collect(),
        strategies: nodes("strategies")
            .iter()
            .map(|s| LaneStrategy {
                id: s["id"].as_str().expect("id").to_string(),
                name: s["name"].as_str().expect("name").to_string(),
            })
            .collect(),
        workflows: nodes("workflows")
            .iter()
            .map(|w| LaneWorkflow {
                id: w["id"].as_str().expect("id").to_string(),
                initiative_id: w["initiative_id"]
                    .as_str()
                    .expect("initiative_id")
                    .to_string(),
                name: w["name"].as_str().expect("name").to_string(),
            })
            .collect(),
    }
}

fn facts_of(world: &World, principal: &str) -> Vec<LaneEntryFacts> {
    world.facts[principal]
        .iter()
        .map(|(doc, (sup, succ))| LaneEntryFacts {
            document_id: doc.clone(),
            superseded: *sup,
            effective_successor: succ.clone(),
        })
        .collect()
}

fn docs_meta(world: &World) -> BTreeMap<String, DocMeta> {
    static DOCS: OnceLock<BTreeMap<String, DocMeta>> = OnceLock::new();
    DOCS.get_or_init(|| {
        let parsed: Value = serde_json::from_slice(
            &fs::read(world.fixtures_dir.join("documents.json")).expect("read documents"),
        )
        .expect("documents parse");
        parsed["documents"]
            .as_array()
            .expect("documents")
            .iter()
            .map(|d| {
                (
                    d["id"].as_str().expect("id").to_string(),
                    DocMeta {
                        title: d["title"].as_str().expect("title").to_string(),
                        body: String::new(),
                        sensitivity: d["sensitivity"].as_str().expect("sensitivity").to_string(),
                        department: d["department"].as_str().expect("department").to_string(),
                    },
                )
            })
            .collect()
    })
    .clone()
}

fn lane_state(store_dir: Option<&Path>) -> AppState {
    let world = world();
    let state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state");
    match store_dir {
        Some(dir) => {
            let registry = AgentRegistry::load(
                &world
                    .fixtures_dir
                    .parent()
                    .expect("repo root")
                    .join("config")
                    .join("agents.example.json"),
                &world.fixtures_dir,
                &state.company_sha256,
            )
            .expect("agent registry");
            state
                .with_agents(registry)
                .with_proposals(Arc::new(ProposalStore::open(dir).expect("audit store")))
                .with_lane_boxes(Arc::new(BoxStore::open(dir).expect("box store")))
        }
        None => state,
    }
}

async fn get_raw(router: &axum::Router, actor: &str, uri: &str) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("authorization", common::bearer(router, actor).await)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, bytes.to_vec())
}

async fn post_raw(
    router: &axum::Router,
    actor: &str,
    uri: &str,
    body: &str,
) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("authorization", common::bearer(router, actor).await)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, bytes.to_vec())
}

fn read_audit(store_dir: &Path) -> Vec<AuditEvent> {
    fs::read_to_string(store_dir.join("audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("audit row"))
        .collect()
}

fn sample_humans(world: &World, count: usize, seed: u64) -> Vec<String> {
    let mut rng = common::Lcg::new(seed);
    let mut sampled: Vec<String> = Vec::new();
    while sampled.len() < count {
        let pick = rng.pick(&world.humans).clone();
        if !sampled.contains(&pick) {
            sampled.push(pick);
        }
    }
    sampled
}

// ---------------------------------------------------------------------------
// AW-1 LANE SCOPE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aw1_every_evidence_id_is_inside_the_actors_allowlist() {
    let world = world();
    let router = app(Arc::new(lane_state(None)));

    let mut boxes_checked = 0usize;
    let mut evidence_checked = 0usize;
    for actor in sample_humans(world, 20, 0xAA_2026) {
        let (status, bytes) = get_raw(&router, &actor, "/lane").await;
        assert_eq!(status, StatusCode::OK);
        let body: Value = serde_json::from_slice(&bytes).expect("lane parses");
        assert_eq!(body["actor_id"], actor.as_str());
        let allowlist = &world.allowlists[&actor];
        for lane_box in body["boxes"].as_array().expect("boxes") {
            boxes_checked += 1;
            assert_eq!(lane_box["derived"], Value::Bool(true));
            assert_eq!(lane_box["effect_class"], "read_only");
            for row in lane_box["evidence"].as_array().expect("evidence") {
                evidence_checked += 1;
                let id = row["document_id"].as_str().expect("id");
                assert!(
                    allowlist.contains(id),
                    "{id} rendered in {actor}'s lane outside their allowlist"
                );
            }
            assert!(
                !lane_box["evidence"]
                    .as_array()
                    .expect("evidence")
                    .is_empty(),
                "a box without visible evidence cannot derive"
            );
        }
    }

    // Agents get the empty shape — boxes are human work.
    for agent in &world.agents {
        let (status, bytes) = get_raw(&router, agent, "/lane").await;
        assert_eq!(status, StatusCode::OK);
        let body: Value = serde_json::from_slice(&bytes).expect("parses");
        assert_eq!(body["boxes"].as_array().expect("boxes").len(), 0);
        assert_eq!(body["actor_id"], agent.as_str());
    }
    println!(
        "AW-1 summary: humans=20 boxes_checked={boxes_checked} \
         evidence_ids_checked={evidence_checked} out_of_scope=0 agents_empty=4"
    );
}

// ---------------------------------------------------------------------------
// AW-2 SELF-ONLY
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aw2_the_lane_has_no_subject_shape() {
    let router = app(Arc::new(lane_state(None)));

    // A crafted subject parameter changes nothing: there is nothing for it
    // to name.
    let plain = get_raw(&router, "p060", "/lane").await;
    let crafted = get_raw(&router, "p060", "/lane?subject=p016").await;
    assert_eq!(plain.0, StatusCode::OK);
    assert_eq!(plain, crafted, "a ?subject= parameter is inert");

    // A path variant is not a lane: there is no such endpoint shape.
    let (status, _) = get_raw(&router, "p060", "/lane/p016").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Lane content varies ONLY with the header actor.
    let other = get_raw(&router, "p061", "/lane").await;
    assert_eq!(other.0, StatusCode::OK);
    assert_ne!(plain.1, other.1, "different actors, different lanes");
    println!("AW-2 summary: inert-query=identical path-variant=404 header-actor=authoritative");
}

// ---------------------------------------------------------------------------
// AW-3 DISPLAY-ONLY
// ---------------------------------------------------------------------------

fn hash_tree(dirs: &[&Path]) -> BTreeMap<String, String> {
    let mut hashes = BTreeMap::new();
    for dir in dirs {
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in fs::read_dir(&current).expect("read dir") {
                let entry = entry.expect("dir entry");
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    hashes.insert(
                        path.display().to_string(),
                        sha256_hex(&fs::read(&path).expect("read file")),
                    );
                }
            }
        }
    }
    hashes
}

#[tokio::test]
async fn aw3_the_amber_class_is_unconstructable_and_renders_mutate_nothing() {
    let world = world();

    // Invariant 1 at the constructor: side_effecting REFUSES.
    let parts = || LaneBoxParts {
        blocked: false,
        blocked_by: Vec::new(),
        blocks: Vec::new(),
        box_id: "boxtest".to_string(),
        capability: ProvenanceNode {
            id: "cap_x".to_string(),
            name: "Capability X".to_string(),
        },
        derived: true,
        evidence: Vec::new(),
        honesty: ScopeStatement {
            band: None,
            groups: Vec::new(),
            sites: Vec::new(),
        },
        provenance: Provenance {
            initiative: ProvenanceNode {
                id: "i".into(),
                name: "I".into(),
            },
            strategy: ProvenanceNode {
                id: "s".into(),
                name: "S".into(),
            },
            workflow: ProvenanceNode {
                id: "w".into(),
                name: "W".into(),
            },
        },
        snapshot_version: "snap".to_string(),
        status: "candidate".to_string(),
        why: "test".to_string(),
    };
    assert!(LaneBox::try_new(parts(), EffectClass::ReadOnly).is_ok());
    let refused = LaneBox::try_new(parts(), EffectClass::SideEffecting);
    assert!(refused.is_err(), "the amber class must be unconstructable");
    assert!(format!("{:#}", refused.err().unwrap()).contains("unconstructable"));

    // 100 renders + real status flows: every verified input byte-identical
    // afterwards; the stores are the only growth.
    let store_dir = scratch("aw3_store");
    let state = lane_state(Some(&store_dir));
    let router = app(Arc::new(state));
    let before = hash_tree(&[&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir]);

    let actors = sample_humans(world, 10, 0xA3_2026);
    for i in 0..100 {
        let actor = &actors[i % actors.len()];
        let (status, _) = get_raw(&router, actor, "/lane").await;
        assert_eq!(status, StatusCode::OK);
    }
    // A legal flow: p060's first candidate box -> active -> done.
    let (_, bytes) = get_raw(&router, "p060", "/lane").await;
    let lane: Value = serde_json::from_slice(&bytes).expect("parses");
    let candidate = lane["boxes"]
        .as_array()
        .expect("boxes")
        .iter()
        .find(|b| b["status"] == "candidate")
        .expect("p060 has a candidate box");
    let box_id = candidate["box_id"].as_str().expect("box_id");
    let (status, _) = post_raw(
        &router,
        "p060",
        &format!("/lane/box/{box_id}/status"),
        r#"{"to":"active"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post_raw(
        &router,
        "p060",
        &format!("/lane/box/{box_id}/status"),
        r#"{"to":"done"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get_raw(&router, "p060", "/lane/rollup").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = get_raw(&router, "p061", "/lane/inbox").await;
    assert_eq!(status, StatusCode::OK);

    let after = hash_tree(&[&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir]);
    assert_eq!(before, after, "display only: no verified input moved");
    assert!(
        store_dir.join("boxes.jsonl").exists(),
        "the box store is the growth"
    );
    println!(
        "AW-3 summary: amber=unconstructable renders=100 flows=2 inputs_hashed={} drift=0",
        before.len()
    );
}

// ---------------------------------------------------------------------------
// AW-4 SOP FAIL-CLOSED (constructed inputs — the live corpus co-grants
// every successor, so the blocked state is proven here; flagged)
// ---------------------------------------------------------------------------

#[test]
fn aw4_superseded_sop_blocks_and_never_hints() {
    let mut docs: BTreeMap<String, DocMeta> = BTreeMap::new();
    for (id, dept) in [
        ("d1", "DeptX"),
        ("d2", "DeptX"),
        ("d3", "DeptX"),
        ("d9", "DeptX"),
    ] {
        docs.insert(
            id.to_string(),
            DocMeta {
                title: format!("Doc {id}"),
                body: String::new(),
                sensitivity: "internal".to_string(),
                department: dept.to_string(),
            },
        );
    }
    let graph = LaneGraph {
        capabilities: vec![
            LaneCapability {
                document_ids: vec!["d1".into(), "d2".into()],
                id: "cap_blocked".into(),
                name: "Capability Blocked".into(),
                workflow_id: "wf1".into(),
            },
            LaneCapability {
                document_ids: vec!["d3".into(), "d2".into()],
                id: "cap_current".into(),
                name: "Capability Current".into(),
                workflow_id: "wf1".into(),
            },
        ],
        workflows: vec![LaneWorkflow {
            id: "wf1".into(),
            initiative_id: "init1".into(),
            name: "Workflow One".into(),
        }],
        initiatives: vec![LaneInitiative {
            id: "init1".into(),
            name: "Initiative One".into(),
            strategy_id: "strat1".into(),
            workflow_ids: vec!["wf1".into()],
        }],
        strategies: vec![LaneStrategy {
            id: "strat1".into(),
            name: "Strategy One".into(),
        }],
    };
    // d1 superseded by d9 which the worker CANNOT see -> blocked, no hint.
    // d3 superseded by d2 which the worker CAN see -> current, link shown.
    let entries = vec![
        LaneEntryFacts {
            document_id: "d1".into(),
            superseded: Some(true),
            effective_successor: Some("d9".into()),
        },
        LaneEntryFacts {
            document_id: "d2".into(),
            superseded: None,
            effective_successor: None,
        },
        LaneEntryFacts {
            document_id: "d3".into(),
            superseded: Some(true),
            effective_successor: Some("d2".into()),
        },
    ];
    let seeds =
        seeds_for_human("p_test", "DeptX", &entries, &docs, &graph, "snap").expect("derive");
    assert_eq!(seeds.len(), 2);
    let blocked = seeds
        .iter()
        .find(|s| s.capability.id == "cap_blocked")
        .unwrap();
    assert!(blocked.blocked, "out-of-scope successor blocks the box");
    let d1 = blocked
        .evidence
        .iter()
        .find(|r| r.document_id == "d1")
        .unwrap();
    assert_eq!(d1.superseded, Some(true));
    assert_eq!(
        d1.effective_successor, None,
        "the successor is never hinted"
    );
    let serialized =
        String::from_utf8(canonical_json_bytes(&blocked).expect("serializes")).expect("utf8");
    assert!(
        !serialized.contains("d9"),
        "d9 appears NOWHERE in the blocked box"
    );

    let current = seeds
        .iter()
        .find(|s| s.capability.id == "cap_current")
        .unwrap();
    assert!(!current.blocked, "an in-scope successor does not block");
    let d3 = current
        .evidence
        .iter()
        .find(|r| r.document_id == "d3")
        .unwrap();
    assert_eq!(d3.effective_successor.as_deref(), Some("d2"));

    // The rendered box states the deviation and refuses every transition.
    let rendered = LaneBox::try_new(
        LaneBoxParts {
            blocked: blocked.blocked,
            blocked_by: blocked.blocked_by.clone(),
            blocks: blocked.blocks.clone(),
            box_id: blocked.box_id.clone(),
            capability: blocked.capability.clone(),
            derived: true,
            evidence: blocked.evidence.clone(),
            honesty: ScopeStatement {
                band: Some(3),
                groups: vec!["grp_x".into()],
                sites: vec!["site_x".into()],
            },
            provenance: blocked.provenance.clone(),
            snapshot_version: "snap".into(),
            status: "blocked".into(),
            why: blocked.why.clone(),
        },
        EffectClass::ReadOnly,
    )
    .expect("renders");
    let body: Value = serde_json::from_slice(&canonical_json_bytes(&rendered).unwrap()).unwrap();
    assert_eq!(body["sop_state"], "blocked_superseded");
    assert_eq!(body["deviation"]["kind"], "superseded_sop");
    for to in ["active", "done", "dismissed"] {
        assert!(
            !transition_is_legal("blocked", to),
            "no transition leaves blocked"
        );
    }
    println!("AW-4 summary: blocked=1 hint=none current-with-successor=1 transitions=refused (constructed inputs; the live corpus co-grants every successor — flagged)");
}

// ---------------------------------------------------------------------------
// AW-5 ROLLUP FLOOR + SHAPE
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aw5_the_floor_makes_small_capabilities_absent_and_the_shape_is_anonymous() {
    let world = world();
    let router = app(Arc::new(lane_state(None)));
    let (status, bytes) = get_raw(&router, "p060", "/lane/rollup").await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&bytes).expect("rollup parses");

    // Independent recompute of assignment counts via the same public rule.
    let graph = graph_from_fixture(&world.brm);
    let docs = docs_meta(world);
    let mut assigned: BTreeMap<String, usize> = BTreeMap::new();
    for human in &world.humans {
        let seeds = seeds_for_human(
            human,
            &world.person_department[human],
            &facts_of(world, human),
            &docs,
            &graph,
            "ignored",
        )
        .expect("derive");
        for seed in seeds {
            *assigned.entry(seed.capability.id.clone()).or_insert(0) += 1;
        }
    }
    let expected_present: BTreeSet<&String> = assigned
        .iter()
        .filter(|(_, n)| **n >= 5)
        .map(|(c, _)| c)
        .collect();
    let expected_absent: BTreeSet<&String> = assigned
        .iter()
        .filter(|(_, n)| **n < 5)
        .map(|(c, _)| c)
        .collect();
    assert!(!expected_absent.is_empty(), "the floor is exercised");

    let rows = body["capabilities"].as_array().expect("capabilities");
    let present: BTreeSet<String> = rows
        .iter()
        .map(|r| r["capability_id"].as_str().expect("id").to_string())
        .collect();
    for capability in &expected_present {
        assert!(
            present.contains(capability.as_str()),
            "{capability} above the floor present"
        );
    }
    for capability in &expected_absent {
        assert!(
            !present.contains(capability.as_str()),
            "{capability} below the floor must be ABSENT"
        );
    }

    // Anonymous shape: exactly these keys, nothing person-shaped anywhere.
    let top: BTreeSet<&str> = body
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        top,
        ["capabilities", "honesty", "snapshot_version"]
            .into_iter()
            .collect()
    );
    for row in rows {
        let keys: BTreeSet<&str> = row
            .as_object()
            .expect("row")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["capability_id", "status_counts"].into_iter().collect()
        );
        let counts: BTreeSet<&str> = row["status_counts"]
            .as_object()
            .expect("counts")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            counts,
            ["active", "blocked", "candidate", "dismissed", "done"]
                .into_iter()
                .collect()
        );
    }
    let text = String::from_utf8(bytes.clone()).expect("utf8");
    for human in world.humans.iter().take(20) {
        assert!(
            !text.contains(human.as_str()),
            "no principal id renders in the rollup"
        );
    }
    assert_eq!(body["honesty"].as_str().expect("honesty"), ROLLUP_HONESTY);
    assert_eq!(
        ROLLUP_HONESTY,
        "This view shows assignment status by capability. It cannot see activity, time, load, or any individual.",
        "the honesty statement is byte-exact"
    );
    println!(
        "AW-5 summary: present={} absent={} shape=anonymous honesty=byte-exact",
        expected_present.len(),
        expected_absent.len()
    );
}

// ---------------------------------------------------------------------------
// AW-6 INBOX AUTHORITY
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aw6_inbox_authority_is_m4s_and_both_audits_precede_both_effects() {
    let world = world();
    let store_dir = scratch("aw6_store");
    let state = lane_state(Some(&store_dir));
    let proposals = state.proposals.clone().expect("store");
    let router = app(Arc::new(state));

    // A pending proposal for p061's agent, citing capability-mapped docs
    // inside p061's allowlist (d0134 -> cap18, d0135 -> cap25; the binding
    // ties on overlap 1 and breaks to the ascending capability id, cap18).
    let citations: Vec<String> = ["d0134", "d0135"].iter().map(|d| d.to_string()).collect();
    for citation in &citations {
        assert!(world.allowlists["p061"].contains(citation.as_str()));
    }
    let snapshot = {
        let (_, bytes) = get_raw(&router, "p061", "/lane").await;
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        v["snapshot_version"].as_str().unwrap().to_string()
    };
    let created = proposals
        .create(
            "agent_finance_analyst",
            "p061",
            &snapshot,
            "idx",
            &ProposalDraft {
                standing_query: "customer account credit terms".to_string(),
                citations: citations.clone(),
                rationale: "test rationale".to_string(),
            },
        )
        .expect("create proposal");
    let proposal_id = match created {
        service::agent::proposals::CreateOutcome::Created(p) => p.proposal_id.clone(),
        service::agent::proposals::CreateOutcome::Deduplicated => panic!("fresh store"),
    };

    // The inbox shows it to the owner only.
    let (status, bytes) = get_raw(&router, "p061", "/lane/inbox").await;
    assert_eq!(status, StatusCode::OK);
    let inbox: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(inbox["proposals"].as_array().unwrap().len(), 1);
    let (_, bytes) = get_raw(&router, "p060", "/lane/inbox").await;
    let other: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(other["proposals"].as_array().unwrap().len(), 0);

    // Agent principals are STRUCTURALLY refused, with the audit row.
    let rows_before = read_audit(&store_dir).len();
    let (status, _) = post_raw(
        &router,
        "agent_finance_analyst",
        &format!("/lane/inbox/{proposal_id}/accept"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let audit = read_audit(&store_dir);
    assert_eq!(audit.len(), rows_before + 1);
    assert_eq!(audit.last().unwrap().action, "box_accept");
    assert_eq!(audit.last().unwrap().outcome, "refused_agent_principal");

    // Non-owner humans are refused, with the audit row.
    let (status, _) = post_raw(
        &router,
        "p060",
        &format!("/lane/inbox/{proposal_id}/accept"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        read_audit(&store_dir).last().unwrap().outcome,
        "refused_not_owner"
    );

    // The owner accepts: both audits, in order, then both effects.
    let (status, bytes) = post_raw(
        &router,
        "p061",
        &format!("/lane/inbox/{proposal_id}/accept"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let accept: Value = serde_json::from_slice(&bytes).unwrap();
    let box_id = accept["box_id"].as_str().unwrap().to_string();
    assert_eq!(accept["proposal_status"], "approved");
    assert_eq!(accept["status"], "candidate");

    let audit = read_audit(&store_dir);
    let n = audit.len();
    assert_eq!(audit[n - 2].action, "box_accept");
    assert_eq!(audit[n - 2].outcome, "allowed");
    assert_eq!(audit[n - 1].action, "proposal_approve");
    assert_eq!(audit[n - 1].outcome, "allowed");
    assert_eq!(audit[n - 2].ordinal + 1, audit[n - 1].ordinal);

    // Effects: the M4 proposal is approved; the candidate box renders in
    // the owner's lane, scope-checked, derived: false.
    assert_eq!(
        proposals.get(&proposal_id).unwrap().status,
        service::agent::proposals::STATUS_APPROVED
    );
    let (_, bytes) = get_raw(&router, "p061", "/lane").await;
    let lane: Value = serde_json::from_slice(&bytes).unwrap();
    let accepted_box = lane["boxes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["box_id"] == box_id.as_str())
        .expect("the accepted box renders");
    assert_eq!(accepted_box["derived"], Value::Bool(false));
    assert_eq!(accepted_box["status"], "candidate");
    for row in accepted_box["evidence"].as_array().unwrap() {
        assert!(world.allowlists["p061"].contains(row["document_id"].as_str().unwrap()));
    }

    // Re-accept refuses (already decided), audited.
    let (status, _) = post_raw(
        &router,
        "p061",
        &format!("/lane/inbox/{proposal_id}/accept"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        read_audit(&store_dir).last().unwrap().outcome,
        "refused_already_decided"
    );

    // Dismiss flow on a second proposal: rejected, no box.
    let second = proposals
        .create(
            "agent_finance_analyst",
            "p061",
            &snapshot,
            "idx",
            &ProposalDraft {
                standing_query: "site stock value report".to_string(),
                citations: citations.clone(),
                rationale: "second".to_string(),
            },
        )
        .expect("create");
    let second_id = match second {
        service::agent::proposals::CreateOutcome::Created(p) => p.proposal_id.clone(),
        service::agent::proposals::CreateOutcome::Deduplicated => panic!("fresh key"),
    };
    let boxes_before = {
        let (_, bytes) = get_raw(&router, "p061", "/lane").await;
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        v["boxes"].as_array().unwrap().len()
    };
    let (status, _) = post_raw(
        &router,
        "p061",
        &format!("/lane/inbox/{second_id}/dismiss"),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        proposals.get(&second_id).unwrap().status,
        service::agent::proposals::STATUS_REJECTED
    );
    let (_, bytes) = get_raw(&router, "p061", "/lane").await;
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        v["boxes"].as_array().unwrap().len(),
        boxes_before,
        "dismiss makes no box"
    );
    println!(
        "AW-6 summary: agent=403+row non-owner=403+row accept=2 rows->2 effects dismiss=reject"
    );
}

// ---------------------------------------------------------------------------
// AW-7 DERIVATION DETERMINISM
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aw7_two_startups_derive_byte_identical_lanes() {
    let world = world();
    let state_a = lane_state(None);
    let state_b = lane_state(None);
    let lanes_a = canonical_json_bytes(&state_a.lane_seeds).expect("serializes");
    let lanes_b = canonical_json_bytes(&state_b.lane_seeds).expect("serializes");
    assert_eq!(lanes_a, lanes_b, "two startups, byte-identical lanes");

    // The 8-cap and ranking law, recomputed independently per sampled human.
    let graph = graph_from_fixture(&world.brm);
    let docs = docs_meta(world);
    let mut humans_with_lanes = 0usize;
    for human in sample_humans(world, 10, 0xA7_2027) {
        let expected = seeds_for_human(
            human.as_str(),
            &world.person_department[&human],
            &facts_of(world, &human),
            &docs,
            &graph,
            &state_a.snapshot_version,
        )
        .expect("derive");
        assert!(expected.len() <= 8, "the 8-box cap holds");
        let counts: Vec<usize> = expected.iter().map(|s| s.evidence.len()).collect();
        for window in counts.windows(2) {
            assert!(
                window[0] >= window[1],
                "ranked by visible-doc count, descending"
            );
        }
        for pair in expected.windows(2) {
            if pair[0].evidence.len() == pair[1].evidence.len() {
                assert!(
                    pair[0].capability.id < pair[1].capability.id,
                    "ties break by capability id ascending"
                );
            }
        }
        let got = state_a.lane_seeds.get(&human).cloned().unwrap_or_default();
        assert_eq!(
            canonical_json_bytes(&got).unwrap(),
            canonical_json_bytes(&expected).unwrap(),
            "startup derivation equals the independent recompute for {human}"
        );
        if !expected.is_empty() {
            humans_with_lanes += 1;
        }
    }
    println!("AW-7 summary: startups=2 identical=true sampled=10 with_lanes={humans_with_lanes}");
}

// ---------------------------------------------------------------------------
// AW-8 HONESTY + TRANSITIONS
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aw8_every_box_carries_honesty_and_illegal_transitions_are_rowless() {
    let world = world();
    let store_dir = scratch("aw8_store");
    let router = app(Arc::new(lane_state(Some(&store_dir))));

    for actor in sample_humans(world, 8, 0xA8_2026) {
        let (_, scope_bytes) = get_raw(&router, &actor, "/scope").await;
        let scope: Value = serde_json::from_slice(&scope_bytes).unwrap();
        let (_, bytes) = get_raw(&router, &actor, "/lane").await;
        let lane: Value = serde_json::from_slice(&bytes).unwrap();
        for lane_box in lane["boxes"].as_array().unwrap() {
            assert_eq!(
                lane_box["honesty"], scope["scope_statement"],
                "the honesty line IS the actor's scope statement"
            );
        }
    }

    // Illegal transitions refuse ROWLESS; the legal path writes its row.
    let (_, bytes) = get_raw(&router, "p060", "/lane").await;
    let lane: Value = serde_json::from_slice(&bytes).unwrap();
    let candidate = lane["boxes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["status"] == "candidate")
        .expect("a candidate box");
    let box_id = candidate["box_id"].as_str().unwrap();

    let rows_before = read_audit(&store_dir).len();
    let (status, _) = post_raw(
        &router,
        "p060",
        &format!("/lane/box/{box_id}/status"),
        r#"{"to":"done"}"#,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "candidate cannot jump to done"
    );
    let (status, _) = post_raw(
        &router,
        "p060",
        &format!("/lane/box/{box_id}/status"),
        r#"{"to":"blocked"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "no human sets blocked");
    assert_eq!(
        read_audit(&store_dir).len(),
        rows_before,
        "refusals are rowless"
    );

    // Someone else's box id is indistinguishable from a nonexistent one.
    let (status, _) = post_raw(
        &router,
        "p061",
        &format!("/lane/box/{box_id}/status"),
        r#"{"to":"active"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(read_audit(&store_dir).len(), rows_before);

    let (status, _) = post_raw(
        &router,
        "p060",
        &format!("/lane/box/{box_id}/status"),
        r#"{"to":"active"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let audit = read_audit(&store_dir);
    assert_eq!(audit.len(), rows_before + 1);
    assert_eq!(audit.last().unwrap().action, "box_status");
    assert_eq!(audit.last().unwrap().outcome, "allowed");
    let (status, _) = post_raw(
        &router,
        "p060",
        &format!("/lane/box/{box_id}/status"),
        r#"{"to":"dismissed"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "active does not dismiss");
    assert_eq!(read_audit(&store_dir).len(), rows_before + 1);
    println!("AW-8 summary: honesty=scope-statement illegal=rowless legal=1 row foreign-box=404");
}
