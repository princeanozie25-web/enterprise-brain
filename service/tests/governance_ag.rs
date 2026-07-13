//! Proposal-agent governance harness AG-1..AG-9. FULLY OFFLINE:
//! MockGenerator, lexical retrieval, no sockets. The agent under test is
//! agent_finance_analyst (owner p061), whose compiled allowlist IS the
//! grant∩owner intersection by M1 construction.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::index::{build_index, canonical_json_bytes, sha256_hex, tokenize};
use serde_json::{json, Value};
use service::agent::context::{
    execute_run, AgentContext, ProductionContext, ProposalDraft, ProposeOutcome,
};
use service::agent::proposals::{AuditEvent, ProposalStore};
use service::agent::standing::{AgentEntry, AgentRegistry};
use service::generate::{Generator, MockBehavior as GenBehavior, MockGenerator};
use service::{app, AppState};
use tower::ServiceExt;

const AGENT_ID: &str = "agent_finance_analyst";
const OWNER_ID: &str = "p061";
const NON_OWNER_HUMAN: &str = "p060";

const STANDING_QUERIES: [&str; 6] = [
    "payroll salary review",
    "customer account credit terms",
    "aggregate financial position",
    "site stock value report",
    "supplier invoice payment terms",
    "quarterly budget summary",
];

// ---------------------------------------------------------------------------
// Shared world (lexical index; agents run hybrid=false)
// ---------------------------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    // Unique per invocation: Windows scanners (Search indexer / Defender) can
    // hold a just-deleted path in delete-pending state, so re-creating the
    // SAME path races them into Os error 5 "Access is denied". A fresh suffix
    // never re-opens a dying path; prior runs' dirs are swept best-effort (a
    // locked leftover is skipped now and reaped on a later run).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // The base lives in the SYSTEM temp dir, not target/tmp: the repo sits
    // under Documents\, which Windows Search indexes by default — its crawler
    // opens freshly written index segments mid-build and the write fails with
    // os error 5. AppData\Local\Temp is outside the default index scope.
    let base = std::env::temp_dir().join("enterprise-brain-test-scratch");
    std::fs::create_dir_all(&base).expect("scratch base");
    let prefix = format!("{name}-");
    if let Ok(entries) = base.read_dir() {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let dir = base.join(format!(
        "{prefix}{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

struct World {
    fixtures_dir: PathBuf,
    artifacts_dir: PathBuf,
    idx_dir: PathBuf,
    agents_config: PathBuf,
    allowlists: BTreeMap<String, BTreeSet<String>>,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("ag_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");
        let allowlists: BTreeMap<String, BTreeSet<String>> = set
            .artifacts
            .iter()
            .map(|a| {
                (
                    a.principal_id.clone(),
                    a.entries.iter().map(|e| e.document_id.clone()).collect(),
                )
            })
            .collect();

        let idx_dir = scratch("ag_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        let agents_config = scratch("ag_config").join("agents.json");
        fs::write(
            &agents_config,
            serde_json::to_vec_pretty(&json!({
                "agents": [{ "agent_id": AGENT_ID, "standing_queries": STANDING_QUERIES }]
            }))
            .expect("encode"),
        )
        .expect("write agents config");

        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
            agents_config,
            allowlists,
        }
    })
}

/// A fresh service state wired for agent runs, with its own store dir.
fn agent_state(store_dir: &Path) -> (AppState, Arc<ProposalStore>) {
    let world = world();
    let base = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state");
    let registry = AgentRegistry::load(
        &world.agents_config,
        &world.fixtures_dir,
        &base.company_sha256,
    )
    .expect("load agent registry");
    let store = Arc::new(ProposalStore::open(store_dir).expect("open proposal store"));
    let generator: Arc<dyn Generator> = Arc::new(MockGenerator::new(GenBehavior::CiteFirst));
    let state = base
        .with_generator(generator)
        .with_cache(None)
        .with_agents(registry)
        .with_proposals(store.clone());
    (state, store)
}

fn entry_for(state: &AppState) -> &AgentEntry {
    state
        .agents
        .as_ref()
        .expect("registry")
        .configured(AGENT_ID)
        .expect("configured agent")
}

fn read_audit(store_dir: &Path) -> Vec<AuditEvent> {
    let text = fs::read_to_string(store_dir.join("audit.jsonl")).unwrap_or_default();
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("audit row parses"))
        .collect()
}

// ---------------------------------------------------------------------------
// AG-1 INTERSECTION
// ---------------------------------------------------------------------------

#[test]
fn ag1_every_stage_of_every_run_stays_inside_the_intersection() {
    let world = world();
    let store_dir = scratch("ag1_store");
    let (state, _store) = agent_state(&store_dir);
    let allowlist = &world.allowlists[AGENT_ID];

    // Seeded query material: corpus vocabulary.
    let vocab: Vec<String> = {
        let value: Value = serde_json::from_slice(
            &fs::read(world.fixtures_dir.join("documents.json")).expect("read documents"),
        )
        .expect("parse documents");
        let mut tokens: BTreeSet<String> = BTreeSet::new();
        for doc in value["documents"].as_array().expect("array") {
            tokens.extend(tokenize(doc["title"].as_str().expect("title")));
            tokens.extend(tokenize(doc["body"].as_str().expect("body")));
        }
        tokens.into_iter().filter(|t| t.len() >= 3).collect()
    };
    let mut rng = common::Lcg::new(0xA6_1234);

    let mut runs = 0usize;
    let mut ids_checked = 0usize;
    let mut violations: Vec<String> = Vec::new();
    for run_index in 0..50 {
        let query_count = 1 + (rng.next() as usize) % 6;
        let standing_queries: Vec<String> = (0..query_count)
            .map(|_| {
                let n = 1 + (rng.next() as usize) % 3;
                (0..n)
                    .map(|_| rng.pick(&vocab).clone())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let entry = AgentEntry {
            agent_id: AGENT_ID.to_string(),
            owner_user_id: OWNER_ID.to_string(),
            standing_queries,
            hybrid: false,
            judge: false,
        };
        let outcome = execute_run(&state, &entry, state.proposals.as_ref().expect("store"))
            .expect("agent run");
        runs += 1;

        let mut observe = |what: &str, id: &str| {
            ids_checked += 1;
            if !allowlist.contains(id) {
                violations.push(format!("run {run_index}: {what} leaked {id}"));
            }
        };
        for trace in &outcome.retrieval_traces {
            if let Some(retrieval_trace) = &trace.retrieval {
                for stage in &retrieval_trace.stages {
                    for id in &stage.doc_ids {
                        observe(&format!("stage {}", stage.stage), id);
                    }
                }
            }
            for doc in &trace.sealed {
                observe("sealed context", &doc.doc_id);
            }
        }
        for proposal in &outcome.created {
            for citation in &proposal.finding.citations {
                observe("proposal citation", citation);
            }
        }
    }

    for violation in &violations {
        println!("{violation}");
    }
    println!(
        "AG-1 summary: runs={runs} (seeded standing queries) ids_checked={ids_checked} \
         violations={}",
        violations.len()
    );
    assert!(runs >= 50);
    assert_eq!(violations.len(), 0, "intersection is zero tolerance");
}

// ---------------------------------------------------------------------------
// AG-2 NO-MUTATION
// ---------------------------------------------------------------------------

fn hash_tree(root: &Path) -> BTreeMap<String, String> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .into_owned();
                out.insert(rel, sha256_hex(&fs::read(&path).expect("read file")));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn ag2_one_hundred_runs_mutate_nothing_but_the_proposal_store() {
    let world = world();
    let store_dir = scratch("ag2_store");
    let (state, store) = agent_state(&store_dir);
    let entry = entry_for(&state).clone();

    let service_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = service_dir.parent().expect("repo root");
    let config_files = [
        world.agents_config.clone(),
        repo_root.join("config").join("agents.example.json"),
        service_dir.join("config.example.json"),
        service_dir.join("config.demo.json"),
    ];

    let before_fixtures = hash_tree(&world.fixtures_dir);
    let before_artifacts = hash_tree(&world.artifacts_dir);
    let before_idx = hash_tree(&world.idx_dir);
    let before_configs: Vec<String> = config_files
        .iter()
        .map(|p| sha256_hex(&fs::read(p).expect("read config")))
        .collect();
    let mut files_hashed =
        before_fixtures.len() + before_artifacts.len() + before_idx.len() + config_files.len();

    for _ in 0..100 {
        execute_run(&state, &entry, &store).expect("agent run");
    }

    assert_eq!(
        before_fixtures,
        hash_tree(&world.fixtures_dir),
        "fixtures mutated"
    );
    assert_eq!(
        before_artifacts,
        hash_tree(&world.artifacts_dir),
        "artifacts mutated"
    );
    assert_eq!(before_idx, hash_tree(&world.idx_dir), "index mutated");
    let after_configs: Vec<String> = config_files
        .iter()
        .map(|p| sha256_hex(&fs::read(p).expect("read config")))
        .collect();
    assert_eq!(before_configs, after_configs, "configs mutated");
    files_hashed +=
        before_fixtures.len() + before_artifacts.len() + before_idx.len() + config_files.len();

    let store_bytes = fs::metadata(store_dir.join("proposals.jsonl"))
        .expect("store exists")
        .len();
    assert!(
        store_bytes > 0,
        "the proposal store is the only thing that grew"
    );
    assert!(store.count() > 0);
    println!(
        "AG-2 summary: runs=100 files_hashed_before_and_after={files_hashed} \
         byte_identical=true proposal_store_bytes={store_bytes} proposals={}",
        store.count()
    );
}

// ---------------------------------------------------------------------------
// AG-3 APPROVAL AUTHORITY (HTTP)
// ---------------------------------------------------------------------------

async fn post_as(router: &axum::Router, principal: &str, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("authorization", common::bearer(router, principal).await)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, serde_json::from_slice(&bytes).expect("json"))
}

#[tokio::test]
async fn ag3_only_the_owning_human_decides_and_every_attempt_is_audited() {
    let store_dir = scratch("ag3_store");
    let (state, store) = agent_state(&store_dir);
    let router = app(Arc::new(state));

    // Seed proposals through the authorized path.
    let (status, run_body) = post_as(&router, OWNER_ID, &format!("/agent/{AGENT_ID}/run")).await;
    assert_eq!(status, StatusCode::OK);
    let created: Vec<&str> = run_body["created_proposal_ids"]
        .as_array()
        .expect("ids")
        .iter()
        .map(|v| v.as_str().expect("id"))
        .collect();
    assert!(
        created.len() >= 2,
        "need proposals to decide on: {created:?}"
    );

    // The agent principal is STRUCTURALLY refused.
    let (status, _) = post_as(
        &router,
        AGENT_ID,
        &format!("/proposals/{}/approve", created[0]),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let audit = read_audit(&store_dir);
    let agent_attempt = audit
        .iter()
        .find(|a| a.actor_principal == AGENT_ID && a.action == "proposal_approved")
        .expect("agent attempt audited");
    assert_eq!(agent_attempt.outcome, "refused_agent_principal");
    assert_eq!(agent_attempt.target, created[0]);

    // A human non-owner is refused.
    let (status, _) = post_as(
        &router,
        NON_OWNER_HUMAN,
        &format!("/proposals/{}/approve", created[0]),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(read_audit(&store_dir)
        .iter()
        .any(|a| a.actor_principal == NON_OWNER_HUMAN && a.outcome == "refused_not_owner"));

    // The owner approves: audit-before-effect, decision carries the actor,
    // and approval changes STATUS and nothing else.
    let before = store.get(created[0]).expect("proposal");
    let (status, approved) = post_as(
        &router,
        OWNER_ID,
        &format!("/proposals/{}/approve", created[0]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["decision"]["actor_principal"], OWNER_ID);
    assert!(approved["decision"]["decided_ordinal"].is_u64());
    let after = store.get(created[0]).expect("proposal");
    assert_eq!(before.finding.rationale, after.finding.rationale);
    assert_eq!(before.finding.citations, after.finding.citations);
    assert_eq!(before.proposal_key, after.proposal_key);
    assert_eq!(before.created_ordinal, after.created_ordinal);
    let allowed = read_audit(&store_dir)
        .iter()
        .filter(|a| {
            a.actor_principal == OWNER_ID
                && a.action == "proposal_approved"
                && a.outcome == "allowed"
        })
        .count();
    assert_eq!(allowed, 1, "exactly one allowed audit row for the approval");

    // Already decided -> refused; reject works on a different proposal.
    let (status, _) = post_as(
        &router,
        OWNER_ID,
        &format!("/proposals/{}/approve", created[0]),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, rejected) = post_as(
        &router,
        OWNER_ID,
        &format!("/proposals/{}/reject", created[1]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rejected["status"], "rejected");
}

// ---------------------------------------------------------------------------
// AG-4 CITATION SEAL
// ---------------------------------------------------------------------------

#[test]
fn ag4_drafts_violating_the_citation_seal_are_refused_whole() {
    let world = world();
    let store_dir = scratch("ag4_store");
    let (state, store) = agent_state(&store_dir);
    let entry = entry_for(&state).clone();
    let allowlist = &world.allowlists[AGENT_ID];
    let in_scope: Vec<String> = allowlist.iter().take(5).cloned().collect();
    assert!(
        !allowlist.contains("d0091"),
        "the HR record must be outside the agent's intersection"
    );

    let mut context = ProductionContext::new(&state, &entry, &store).expect("capability context");
    let draft = |rationale: &str, citations: &[&str]| ProposalDraft {
        standing_query: "seal test".to_string(),
        rationale: rationale.to_string(),
        citations: citations.iter().map(|s| s.to_string()).collect(),
    };

    // Out-of-scope citation: the WHOLE proposal refuses.
    let outcome = context
        .propose(draft("claim [d0091].", &["d0091"]))
        .expect("propose");
    assert!(matches!(outcome, ProposeOutcome::Refused { .. }));

    // Zero citations: refused.
    let outcome = context
        .propose(draft("uncited claim.", &[]))
        .expect("propose");
    assert!(matches!(outcome, ProposeOutcome::Refused { .. }));

    // Five citations: refused (1..=4).
    let five: Vec<&str> = in_scope.iter().take(5).map(String::as_str).collect();
    let outcome = context.propose(draft("too many.", &five)).expect("propose");
    assert!(matches!(outcome, ProposeOutcome::Refused { .. }));

    // Rationale citing outside its own evidence list: refused.
    let outcome = context
        .propose(draft(
            &format!("claim [{}].", in_scope[1]),
            &[in_scope[0].as_str()],
        ))
        .expect("propose");
    assert!(matches!(outcome, ProposeOutcome::Refused { .. }));

    // Oversized rationale: refused.
    let outcome = context
        .propose(draft(&"x".repeat(601), &[in_scope[0].as_str()]))
        .expect("propose");
    assert!(matches!(outcome, ProposeOutcome::Refused { .. }));
    assert_eq!(
        context.proposal_faults, 5,
        "every refusal counted as a fault"
    );
    assert_eq!(store.count(), 0, "nothing reached the store");

    // Valid 1..=4 in-scope citations pass.
    let four: Vec<&str> = in_scope.iter().take(4).map(String::as_str).collect();
    let outcome = context
        .propose(draft(
            &format!("grounded [{}] and [{}].", four[0], four[1]),
            &four,
        ))
        .expect("propose");
    assert!(matches!(outcome, ProposeOutcome::Created { .. }));
    assert_eq!(store.count(), 1);
}

// ---------------------------------------------------------------------------
// AG-5 IDEMPOTENCY
// ---------------------------------------------------------------------------

#[test]
fn ag5_running_twice_changes_nothing() {
    let store_dir = scratch("ag5_store");
    let (state, store) = agent_state(&store_dir);
    let entry = entry_for(&state).clone();

    let first = execute_run(&state, &entry, &store).expect("first run");
    assert!(
        !first.created.is_empty(),
        "the standing queries propose something"
    );
    let count_after_first = store.count();

    let second = execute_run(&state, &entry, &store).expect("second run");
    assert_eq!(second.created.len(), 0, "no new proposals on re-run");
    assert!(second.deduplicated > 0, "the dedupe path was exercised");
    assert_eq!(store.count(), count_after_first, "proposal count unchanged");
}

// ---------------------------------------------------------------------------
// AG-6 STALENESS (fixture byte-flip)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ag6_snapshot_flip_withholds_findings_and_refuses_decisions() {
    let world = world();
    let store_dir = scratch("ag6_store");
    let (state_a, store) = agent_state(&store_dir);
    let entry = entry_for(&state_a).clone();
    let first = execute_run(&state_a, &entry, &store).expect("run under snapshot A");
    assert!(!first.created.is_empty());
    let count_a = store.count();
    let snapshot_a = state_a.snapshot_version.clone();

    // The flipped world: one byte in documents.json, recompiled + reindexed.
    let flipped_fixtures = scratch("ag6_flipped_fixtures");
    for name in ["company.json", "documents.json", "traps.json"] {
        fs::copy(world.fixtures_dir.join(name), flipped_fixtures.join(name)).expect("copy");
    }
    let documents_path = flipped_fixtures.join("documents.json");
    let text = fs::read_to_string(&documents_path).expect("read");
    assert!(text.contains("controlled"));
    fs::write(
        &documents_path,
        text.replacen("controlled", "cantrolled", 1),
    )
    .expect("flip");
    let flipped_artifacts = scratch("ag6_flipped_artifacts");
    let snap = scope_compiler::snapshot::take(&flipped_fixtures).expect("snapshot");
    let m1_world = scope_compiler::load_world(&flipped_fixtures).expect("validate");
    let (set, unknown) =
        scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile flipped");
    assert!(unknown.is_empty());
    scope_compiler::compile::write_artifacts(&flipped_artifacts, &set).expect("write");
    let flipped_idx = scratch("ag6_flipped_idx");
    build_index(&flipped_fixtures, &flipped_idx).expect("build flipped index");

    let base_b = AppState::build(&flipped_fixtures, &flipped_artifacts, &flipped_idx)
        .expect("build flipped state");
    assert_ne!(base_b.snapshot_version, snapshot_a);
    let registry_b = AgentRegistry::load(
        &world.agents_config,
        &flipped_fixtures,
        &base_b.company_sha256,
    )
    .expect("registry over flipped world");
    let generator: Arc<dyn Generator> = Arc::new(MockGenerator::new(GenBehavior::CiteFirst));
    let state_b = base_b
        .with_generator(generator)
        .with_cache(None)
        .with_agents(registry_b)
        .with_proposals(store.clone());
    let router_b = app(Arc::new(state_b));

    // Render under the new snapshot: stale, findings WITHHELD.
    let response = router_b
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/proposals")
                .header("authorization", common::bearer(&router_b, OWNER_ID).await)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let listing: Value = serde_json::from_slice(&bytes).expect("json");
    let proposals = listing["proposals"].as_array().expect("array");
    assert_eq!(proposals.len(), count_a);
    let body_text = String::from_utf8(bytes.to_vec()).expect("utf8");
    for proposal in proposals {
        assert_eq!(proposal["stale"], Value::Bool(true));
        assert_eq!(proposal["refresh"], "re-run to refresh");
        assert!(proposal.get("finding").is_none(), "finding withheld");
        assert!(proposal["status"].is_string());
        assert!(proposal["standing_query"].is_string());
    }
    assert!(
        !body_text.contains("rationale"),
        "no rationale text renders stale"
    );
    assert!(
        !body_text.contains("citations"),
        "no citations render stale"
    );

    // Approve/reject on a stale proposal is refused, with audit.
    let stale_id = first.created[0].proposal_id.clone();
    let (status, _) = post_as(
        &router_b,
        OWNER_ID,
        &format!("/proposals/{stale_id}/approve"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(read_audit(&store_dir)
        .iter()
        .any(|a| a.outcome == "refused_stale" && a.target == stale_id));

    // A fresh run under the new snapshot creates new proposals.
    let (status, run_body) = post_as(&router_b, OWNER_ID, &format!("/agent/{AGENT_ID}/run")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !run_body["created_proposal_ids"]
            .as_array()
            .expect("ids")
            .is_empty(),
        "the new snapshot re-proposes"
    );
    assert!(store.count() > count_a);
}

// ---------------------------------------------------------------------------
// AG-7 RUN AUTHORITY (HTTP)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ag7_only_the_owner_invokes_runs() {
    let store_dir = scratch("ag7_store");
    let (state, _store) = agent_state(&store_dir);
    let router = app(Arc::new(state));

    // Owner: allowed.
    let (status, body) = post_as(&router, OWNER_ID, &format!("/agent/{AGENT_ID}/run")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["agent_id"], AGENT_ID);

    // The agent invoking itself: structurally refused + audited.
    let (status, _) = post_as(&router, AGENT_ID, &format!("/agent/{AGENT_ID}/run")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(read_audit(&store_dir).iter().any(|a| {
        a.action == "agent_run"
            && a.actor_principal == AGENT_ID
            && a.outcome == "refused_agent_principal"
    }));

    // A human non-owner: refused + audited.
    let (status, _) = post_as(&router, NON_OWNER_HUMAN, &format!("/agent/{AGENT_ID}/run")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(read_audit(&store_dir).iter().any(|a| {
        a.action == "agent_run"
            && a.actor_principal == NON_OWNER_HUMAN
            && a.outcome == "refused_not_owner"
    }));

    // Unknown and unconfigured agents: 404 (agent_qa_drafter exists in the
    // fixtures but is not configured).
    let (status, _) = post_as(&router, OWNER_ID, "/agent/agent_nope/run").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post_as(&router, "p093", "/agent/agent_qa_drafter/run").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // No identity: 401.
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/agent/{AGENT_ID}/run"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// AG-8 DETERMINISM
// ---------------------------------------------------------------------------

#[test]
fn ag8_identical_runs_produce_byte_identical_proposal_sets() {
    let dir_one = scratch("ag8_store_one");
    let dir_two = scratch("ag8_store_two");
    let (state_one, store_one) = agent_state(&dir_one);
    let (state_two, store_two) = agent_state(&dir_two);
    let entry = entry_for(&state_one).clone();

    let first = execute_run(&state_one, &entry, &store_one).expect("run one");
    let second = execute_run(&state_two, &entry, &store_two).expect("run two");
    assert!(!first.created.is_empty());

    let bytes_one = canonical_json_bytes(&first.created).expect("encode");
    let bytes_two = canonical_json_bytes(&second.created).expect("encode");
    assert_eq!(bytes_one, bytes_two, "proposal sets must be byte-identical");

    // The stored logs are byte-identical too.
    assert_eq!(
        fs::read(dir_one.join("proposals.jsonl")).expect("read one"),
        fs::read(dir_two.join("proposals.jsonl")).expect("read two"),
    );
}

// ---------------------------------------------------------------------------
// AG-9 SIDECAR
// ---------------------------------------------------------------------------

#[test]
fn ag9_agent_run_generation_rows_are_content_free() {
    let store_dir = scratch("ag9_store");
    let (state, store) = agent_state(&store_dir);
    let entry = entry_for(&state).clone();

    let outcome = execute_run(&state, &entry, &store).expect("run");
    assert!(
        !outcome.usage_events.is_empty(),
        "agent-run generation rows appear"
    );
    for event in &outcome.usage_events {
        assert_eq!(event.model, "mock-generator");
        assert!(event.cost_usd.is_none());
        assert!(event.estimated);
        let row = serde_json::to_value(event).expect("row");
        let keys: Vec<&str> = row
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "cost_usd",
                "estimated",
                "input_tokens",
                "model",
                "output_tokens",
                "ts"
            ],
            "rows carry numbers and a model id, nothing else"
        );
    }
    // No standing-query text or rationale ever lands in a usage row.
    let serialized = serde_json::to_string(&outcome.usage_events).expect("encode");
    assert!(!serialized.contains("payroll"));
    assert!(!serialized.contains("rationale"));
}
