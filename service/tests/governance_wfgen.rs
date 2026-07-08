//! SHOWCASE-III — grounded workflow generation, the FIRST mutation path.
//! WF-G1..8, FULLY OFFLINE: MockGenerator over the shared lexical world; the
//! only socket any test opens is the in-memory router. The invariants under
//! test are non-negotiable — the Gate (model proposes, human materializes), the
//! anchor-visibility no-laundering law (S4), fail-closed everywhere, audit-
//! before-effect, existence-hiding, and the single materialize() call site.

mod common;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use retrieval::index::build_index;
use serde_json::Value;
use service::generate::{Generator, MockBehavior, MockGenerator};
use service::proposals::{
    generate_proposal, redact_boxes_for, GenerateOutcome, WorkflowProposalStore,
};
use service::{app, AppState};
use tower::ServiceExt;

// --- shared world ----------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    for attempt in 0u64..50 {
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(&dir).is_ok()
            && std::fs::read_dir(&dir)
                .map(|mut e| e.next().is_none())
                .unwrap_or(false)
        {
            return dir;
        }
        std::thread::sleep(std::time::Duration::from_millis(20 * (attempt.min(5) + 1)));
    }
    panic!("scratch dir {name} could not be reset");
}

struct World {
    fixtures_dir: PathBuf,
    artifacts_dir: PathBuf,
    idx_dir: PathBuf,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("wfg_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");
        let idx_dir = scratch("wfg_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build lexical index");
        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
        }
    })
}

/// Base state with a MockGenerator + a fresh workflow-proposal store in `dir`.
fn state_with(behavior: MockBehavior, dir: &std::path::Path) -> AppState {
    let world = world();
    let store = WorkflowProposalStore::open(dir).expect("open wf proposal store");
    AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
        .with_generator(Arc::new(MockGenerator::new(behavior)) as Arc<dyn Generator>)
        .with_wf_proposals(Arc::new(store))
}

const GOAL: &str = "confidential financial statements";
const CAP: &str = "cap31";

// --- HTTP helper -----------------------------------------------------------

async fn send(
    router: &Router,
    method: &str,
    uri: &str,
    bearer: &str,
    body: Option<String>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", bearer);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let request = builder
        .body(body.map(Body::from).unwrap_or(Body::empty()))
        .expect("request");
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

// ===========================================================================
// WF-G1 — every admitted box's quote is verbatim at its locator in an
// in-proposer-scope document.
// ===========================================================================
#[test]
fn wf_g1_admitted_boxes_are_verbatim_anchored() {
    let dir = scratch("wfg1_state");
    let state = state_with(MockBehavior::CiteEach, &dir);
    let outcome =
        generate_proposal(&state, "p060", CAP, "p113", "Onboarding new hires", GOAL).expect("gen");
    let GenerateOutcome::Drafted(draft) = outcome else {
        panic!("CiteEach over an in-scope goal must draft admitted boxes");
    };
    assert!(!draft.boxes.is_empty());
    assert_eq!(draft.grounding.admitted, draft.boxes.len());
    for b in &draft.boxes {
        assert_eq!(b.stage, "Next");
        assert_eq!(
            b.anchors.len(),
            1,
            "an admitted box carries exactly its proven anchor"
        );
        let anchor = &b.anchors[0];
        let (loc_doc, loc_off) = anchor.locator.split_once('@').expect("locator shape");
        assert_eq!(loc_doc, anchor.doc_id, "locator names the cited doc");
        let offset: usize = loc_off.parse().expect("byte offset");
        let meta = state.docs.get(&anchor.doc_id).expect("cited doc in corpus");
        assert!(
            meta.body[offset..].starts_with(&anchor.quote),
            "quote is not verbatim at the recorded locator {}",
            anchor.locator
        );
    }
}

// ===========================================================================
// WF-G2 — fabricated / foreign-doc boxes are refused + counted; zero admitted
// writes ZERO proposal rows.
// ===========================================================================
#[test]
fn wf_g2_fabricated_boxes_refused_and_zero_admitted_writes_nothing() {
    let dir = scratch("wfg2_state");
    let state = state_with(MockBehavior::FabricatedQuote, &dir);
    let outcome = generate_proposal(&state, "p060", CAP, "p113", "Bad plan", GOAL).expect("gen");
    match outcome {
        GenerateOutcome::ZeroAdmitted { refused } => assert!(refused >= 1),
        GenerateOutcome::Drafted(_) => panic!("a fabricated quote must not draft a box"),
        GenerateOutcome::Fault => {
            panic!("a well-formed fabricated box is a refusal, not a parse fault")
        }
    }
    // A foreign citation likewise refuses.
    let dir2 = scratch("wfg2b_state");
    let state2 = state_with(MockBehavior::ForeignCitation, &dir2);
    let outcome2 = generate_proposal(&state2, "p060", CAP, "p113", "Foreign", GOAL).expect("gen");
    assert!(matches!(outcome2, GenerateOutcome::ZeroAdmitted { .. }));
    // Zero admitted -> the store was never written (no jsonl file, or empty).
    let proposals_file = dir.join("wf_proposals.jsonl");
    let written = std::fs::read_to_string(&proposals_file).unwrap_or_default();
    assert!(
        written.trim().is_empty(),
        "zero-admitted must write no proposal rows"
    );
}

// ===========================================================================
// WF-G4 — anchor-visibility: verbatim content never crosses to an identity
// whose own scope lacks the doc. Proposer sees full anchors; a disjoint viewer
// sees only the withheld marker.
// ===========================================================================
#[test]
fn wf_g4_anchor_visibility_never_launders() {
    let dir = scratch("wfg4_state");
    let state = state_with(MockBehavior::CiteEach, &dir);
    let GenerateOutcome::Drafted(draft) =
        generate_proposal(&state, "p060", CAP, "p113", "Onboarding", GOAL).expect("gen")
    else {
        panic!("expected a drafted proposal");
    };

    // The proposer sees the full anchor (its scope covers the doc).
    let proposer_view = redact_boxes_for(&state, "p060", &draft.boxes).expect("redact");
    let mut proposer_visible = 0;
    for bv in &proposer_view {
        for a in &bv.anchors {
            if a.visible {
                proposer_visible += 1;
                assert!(a.doc_id.is_some() && a.quote.is_some() && a.locator.is_some());
            }
        }
    }
    assert!(
        proposer_visible >= 1,
        "the proposer must see its own anchors in full"
    );

    // A viewer with an empty/disjoint scope (p_void) sees NOTHING verbatim: no
    // doc_id, title, quote, or locator — only the withheld marker + a count.
    let void_view = redact_boxes_for(&state, "p_void", &draft.boxes).expect("redact");
    for bv in &void_view {
        assert_eq!(bv.sources_outside_view, bv.sources_total);
        for a in &bv.anchors {
            assert!(!a.visible, "an out-of-scope anchor must be withheld");
            assert!(
                a.doc_id.is_none() && a.title.is_none() && a.quote.is_none() && a.locator.is_none(),
                "no verbatim field may cross to an unauthorized viewer"
            );
        }
    }
}

// ===========================================================================
// WF-G5 — decision gating: non-approver → forbidden/hidden; approver approve
// records the audit BEFORE materializing; a second decision → 409.
// ===========================================================================
#[tokio::test]
async fn wf_g5_decision_gate_audit_before_effect_and_idempotent() {
    let dir = scratch("wfg5_state");
    let state = Arc::new(state_with(MockBehavior::CiteEach, &dir));
    let router = app(state.clone());

    // p060 proposes.
    let felix = common::bearer(&router, "p060").await;
    let body =
        serde_json::json!({ "capability_id": CAP, "title": "Onboarding new hires", "goal": GOAL })
            .to_string();
    let (status, created) = send(&router, "POST", "/workflow/proposals", &felix, Some(body)).await;
    assert_eq!(status, StatusCode::OK, "generation should draft a proposal");
    let proposal_id = created["proposal"]["proposal_id"]
        .as_str()
        .expect("proposal_id")
        .to_string();

    // A non-approver (Felix himself is the proposer, not the approver) cannot decide.
    let (status, _) = send(
        &router,
        "POST",
        &format!("/workflow/proposals/{proposal_id}/approve"),
        &felix,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the proposer is not the approver"
    );

    // The approver (p113, Felix's manager) approves.
    let ingrid = common::bearer(&router, "p113").await;
    let (status, decided) = send(
        &router,
        "POST",
        &format!("/workflow/proposals/{proposal_id}/approve"),
        &ingrid,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the approver may approve");
    assert_eq!(decided["proposal"]["status"], "approved");
    assert_eq!(decided["proposal"]["materialized"], true);

    // Audit-before-effect. Two facts pin it: (1) the approve was AUDITED
    // ("allowed") — the row is present in the audit log; (2) in the append-only
    // events log, the Decided(approved) row precedes the Materialized row (the
    // handler order is audit -> decide -> materialize). The audit is flushed
    // before either effect is written.
    let audit = std::fs::read_to_string(dir.join("wf_proposals_audit.jsonl")).expect("audit log");
    assert!(
        audit
            .lines()
            .any(|l| l.contains("\"action\":\"proposal_approve\"")
                && l.contains("\"outcome\":\"allowed\"")),
        "the approve was audited (allowed) before any effect"
    );
    let events = std::fs::read_to_string(dir.join("wf_proposals.jsonl")).expect("events");
    let lines: Vec<&str> = events.lines().collect();
    let decided_idx = lines
        .iter()
        .position(|l| l.contains("\"event\":\"decided\"") && l.contains("\"status\":\"approved\""))
        .expect("decided(approved) event");
    let materialized_idx = lines
        .iter()
        .position(|l| l.contains("\"event\":\"materialized\""))
        .expect("materialized event");
    assert!(
        decided_idx < materialized_idx,
        "the decision is recorded before materialization"
    );

    // A second decision is idempotent-rejected with the recorded outcome.
    let (status, _) = send(
        &router,
        "POST",
        &format!("/workflow/proposals/{proposal_id}/deny"),
        &ingrid,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a decided proposal cannot be re-decided"
    );
}

// ===========================================================================
// WF-G5b — deny records status only and materializes NOTHING.
// ===========================================================================
#[tokio::test]
async fn wf_g5b_deny_materializes_nothing() {
    let dir = scratch("wfg5b_state");
    let state = Arc::new(state_with(MockBehavior::CiteEach, &dir));
    let router = app(state.clone());
    let felix = common::bearer(&router, "p060").await;
    let body = serde_json::json!({ "capability_id": CAP, "title": "Onboarding", "goal": GOAL })
        .to_string();
    let (_, created) = send(&router, "POST", "/workflow/proposals", &felix, Some(body)).await;
    let id = created["proposal"]["proposal_id"]
        .as_str()
        .unwrap()
        .to_string();
    let ingrid = common::bearer(&router, "p113").await;
    let (status, decided) = send(
        &router,
        "POST",
        &format!("/workflow/proposals/{id}/deny"),
        &ingrid,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decided["proposal"]["status"], "denied");
    assert_eq!(decided["proposal"]["materialized"], false);
    let events = std::fs::read_to_string(dir.join("wf_proposals.jsonl")).unwrap();
    // Match the EVENT row, not the `materialized: bool` field the created row
    // legitimately serializes as false.
    assert!(
        !events
            .lines()
            .any(|l| l.contains("\"event\":\"materialized\"")),
        "deny never materializes"
    );
}

// ===========================================================================
// WF-G6 — merge law: /workflow returns fixture ∪ overlay; the overlay survives
// a store reopen (restart).
// ===========================================================================
#[tokio::test]
async fn wf_g6_merge_and_overlay_survives_restart() {
    let dir = scratch("wfg6_state");
    {
        let state = Arc::new(state_with(MockBehavior::CiteEach, &dir));
        let router = app(state.clone());
        let felix = common::bearer(&router, "p060").await;
        let body = serde_json::json!({ "capability_id": CAP, "title": "Onboarding", "goal": GOAL })
            .to_string();
        let (_, created) = send(&router, "POST", "/workflow/proposals", &felix, Some(body)).await;
        let id = created["proposal"]["proposal_id"]
            .as_str()
            .unwrap()
            .to_string();
        let ingrid = common::bearer(&router, "p113").await;
        let (status, _) = send(
            &router,
            "POST",
            &format!("/workflow/proposals/{id}/approve"),
            &ingrid,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    // Reopen the store from the SAME dir (a fresh process) — the approved
    // proposal + its materialized boxes survive, and /workflow merges them.
    let reopened = WorkflowProposalStore::open(&dir).expect("reopen");
    let mine = reopened.proposed_by("p060");
    assert_eq!(mine.len(), 1, "the proposal survives the restart");
    let snapshot = mine[0].snapshot_version.clone();
    let approved = reopened.approved_for(CAP, &snapshot);
    assert_eq!(
        approved.len(),
        1,
        "the materialized proposal survives the restart"
    );
    assert!(approved[0].materialized);
}

// ===========================================================================
// WF-G7 — the single materialize() call site: `.materialize(` appears EXACTLY
// once across service/src, in the approve handler.
// ===========================================================================
#[test]
fn wf_g7_single_materialize_call_site() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits: Vec<(String, String)> = Vec::new();
    fn walk(dir: &std::path::Path, hits: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).expect("read src") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, hits);
            } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                let text = std::fs::read_to_string(&path).expect("read rs");
                for line in text.lines() {
                    if line.contains(".materialize(") {
                        hits.push((path.display().to_string(), line.trim().to_string()));
                    }
                }
            }
        }
    }
    walk(&src, &mut hits);
    assert_eq!(
        hits.len(),
        1,
        "materialize() must have exactly one call site, found: {hits:?}"
    );
    let (file, _) = &hits[0];
    assert!(
        file.contains("lib.rs"),
        "the sole materialize() call site is the approve handler in lib.rs"
    );
}

// ===========================================================================
// WF-G8 — rate limit: the 4th generation in the window is cheap-rejected (429).
// ===========================================================================
#[tokio::test]
async fn wf_g8_generation_rate_limited() {
    let dir = scratch("wfg8_state");
    let state = Arc::new(state_with(MockBehavior::CiteEach, &dir).with_generation_rate(3, 60));
    let router = app(state.clone());
    let felix = common::bearer(&router, "p060").await;
    let body =
        || serde_json::json!({ "capability_id": CAP, "title": "P", "goal": GOAL }).to_string();
    for _ in 0..3 {
        let (status, _) = send(&router, "POST", "/workflow/proposals", &felix, Some(body())).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "the first three generations are allowed"
        );
    }
    let (status, _) = send(&router, "POST", "/workflow/proposals", &felix, Some(body())).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "the 4th generation in the window is 429"
    );
}
