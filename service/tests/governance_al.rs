//! Lens-room governance harness AL-1..AL-5 (AP-2). FULLY OFFLINE.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::index::build_index;
use serde_json::Value;
use service::agent::proposals::{AuditEvent, ProposalStore};
use service::lens::{build_holdings, sentence_for, LensEntry};
use service::{app, AppState};
use tower::ServiceExt;

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
    /// principal -> allowlist; and principal -> (doc -> reasons).
    allowlists: BTreeMap<String, BTreeSet<String>>,
    reasons: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    principal_ids: Vec<String>,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("al_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let mut allowlists = BTreeMap::new();
        let mut reasons = BTreeMap::new();
        for artifact in &set.artifacts {
            allowlists.insert(
                artifact.principal_id.clone(),
                artifact
                    .entries
                    .iter()
                    .map(|e| e.document_id.clone())
                    .collect::<BTreeSet<_>>(),
            );
            reasons.insert(
                artifact.principal_id.clone(),
                artifact
                    .entries
                    .iter()
                    .map(|e| (e.document_id.clone(), e.reasons.clone()))
                    .collect::<BTreeMap<_, _>>(),
            );
        }
        let principal_ids: Vec<String> = allowlists.keys().cloned().collect();

        let idx_dir = scratch("al_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
            allowlists,
            reasons,
            principal_ids,
        }
    })
}

fn lens_state(store_dir: Option<&Path>) -> AppState {
    let world = world();
    let state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state");
    match store_dir {
        Some(dir) => state.with_proposals(Arc::new(
            ProposalStore::open(dir).expect("open audit store"),
        )),
        None => state,
    }
}

async fn get_lens(
    router: &axum::Router,
    actor: &str,
    subject: &str,
) -> (StatusCode, Vec<(String, String)>, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/lens/{subject}"))
                .header("authorization", common::bearer(router, actor).await)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let mut headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    headers.sort();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, headers, bytes.to_vec())
}

/// The reason priority, reimplemented independently for the property test.
fn class_of(reason: &str) -> u8 {
    if reason == "SUBJECT:self" {
        0
    } else if reason.starts_with("REBAC:") {
        1
    } else if reason.starts_with("ABAC:") {
        2
    } else if reason.starts_with("AGENT:") {
        3
    } else {
        4 // PUBLIC
    }
}

fn normalize(reason: &str) -> String {
    if reason == "PUBLIC:sensitivity" {
        "PUBLIC:all".to_string()
    } else {
        reason.to_string()
    }
}

// ---------------------------------------------------------------------------
// AL-1 COMPLETENESS + GROUPING
// ---------------------------------------------------------------------------

#[tokio::test]
async fn al1_holdings_equal_the_artifact_exactly_with_priority_grouping() {
    let world = world();
    let router = app(Arc::new(lens_state(None)));

    let mut sampled: Vec<String> = world
        .principal_ids
        .iter()
        .filter(|p| p.starts_with("agent_"))
        .cloned()
        .collect();
    assert_eq!(sampled.len(), 4, "all four agents sampled");
    let mut rng = common::Lcg::new(0xA1_2026);
    while sampled.len() < 20 {
        let pick = rng.pick(&world.principal_ids).clone();
        if !sampled.contains(&pick) {
            sampled.push(pick);
        }
    }

    let mut docs_checked = 0usize;
    let mut multi_reason_checked = 0usize;
    for subject in &sampled {
        let (status, _, bytes) = get_lens(&router, subject, subject).await;
        assert_eq!(status, StatusCode::OK);
        let body: Value = serde_json::from_slice(&bytes).expect("lens parses");
        assert_eq!(body["cross_lens"], Value::Bool(false));

        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for section in body["holdings"].as_array().expect("holdings") {
            let section_reason = section["reason"].as_str().expect("reason");
            for doc in section["docs"].as_array().expect("docs") {
                let id = doc["document_id"].as_str().expect("id");
                *seen.entry(id).or_insert(0) += 1;
                docs_checked += 1;

                // Priority property: the section reason is the minimum of
                // the doc's full reason set by (class, lexicographic).
                let original = &world.reasons[subject][id];
                let mut all: Vec<String> = original.iter().map(|r| normalize(r)).collect();
                all.sort_by_key(|r| (class_of(r), r.clone()));
                all.dedup();
                assert_eq!(
                    section_reason, all[0],
                    "primary reason for {id} of {subject}"
                );
                let chips: Vec<&str> = doc["also_via"]
                    .as_array()
                    .expect("also_via")
                    .iter()
                    .map(|v| v.as_str().expect("chip"))
                    .collect();
                assert_eq!(
                    chips,
                    all[1..].iter().map(String::as_str).collect::<Vec<_>>()
                );
                if !chips.is_empty() {
                    multi_reason_checked += 1;
                }
            }
        }

        // Completeness law: exactly the artifact, each doc exactly once.
        let allowlist = &world.allowlists[subject];
        assert_eq!(
            seen.keys().cloned().collect::<BTreeSet<_>>(),
            allowlist
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            "holdings union equals the artifact for {subject}"
        );
        assert!(
            seen.values().all(|count| *count == 1),
            "no doc repeats as primary for {subject}"
        );

        // PUBLIC:all is last when present.
        let section_reasons: Vec<&str> = body["holdings"]
            .as_array()
            .expect("holdings")
            .iter()
            .map(|s| s["reason"].as_str().expect("reason"))
            .collect();
        if let Some(position) = section_reasons.iter().position(|r| *r == "PUBLIC:all") {
            assert_eq!(
                position,
                section_reasons.len() - 1,
                "PUBLIC:all renders last"
            );
        }
    }
    println!(
        "AL-1 summary: subjects=20 (incl 4 agents) primary_docs_checked={docs_checked} \
         multi_reason_docs={multi_reason_checked} violations=0"
    );
    assert!(
        multi_reason_checked > 0,
        "the priority property was exercised"
    );
}

// ---------------------------------------------------------------------------
// AL-2 CROSS-LENS AUDIT
// ---------------------------------------------------------------------------

#[tokio::test]
async fn al2_cross_lens_views_audit_before_render_self_views_do_not() {
    let store_dir = scratch("al2_store");
    let router = app(Arc::new(lens_state(Some(&store_dir))));
    let read_audit = || -> Vec<AuditEvent> {
        fs::read_to_string(store_dir.join("audit.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("audit row"))
            .collect()
    };

    // Self view: no audit row.
    let (status, _, _) = get_lens(&router, "p060", "p060").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        read_audit().is_empty(),
        "actor == subject writes no audit row"
    );

    // Cross view: exactly one row, fields exact, ordinal 0.
    let (status, _, bytes) = get_lens(&router, "p061", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&bytes).expect("parses");
    assert_eq!(body["cross_lens"], Value::Bool(true));
    assert_eq!(body["actor_id"], "p061");
    let audit = read_audit();
    assert_eq!(audit.len(), 1, "exactly one lens_view row per cross view");
    assert_eq!(audit[0].action, "lens_view");
    assert_eq!(audit[0].actor_principal, "p061");
    assert_eq!(audit[0].target, "p060");
    assert_eq!(audit[0].outcome, "allowed_demo");
    assert_eq!(audit[0].ordinal, 0);

    // Ordinals increase per audited act — the row is written at request
    // time (before the response renders), not batched after.
    let (status, _, _) = get_lens(&router, "p060", "agent_finance_analyst").await;
    assert_eq!(status, StatusCode::OK);
    let audit = read_audit();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[1].ordinal, 1);
    assert!(audit[0].ordinal < audit[1].ordinal);

    // Another self view: still no new row.
    let (status, _, _) = get_lens(&router, "p061", "p061").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read_audit().len(), 2);
}

// ---------------------------------------------------------------------------
// AL-3 NAMES + OWNED AGENTS
// ---------------------------------------------------------------------------

#[tokio::test]
async fn al3_names_match_company_and_agent_lists_are_exact() {
    let world = world();
    let router = app(Arc::new(lens_state(None)));
    let company: Value = serde_json::from_slice(
        &fs::read(world.fixtures_dir.join("company.json")).expect("read company"),
    )
    .expect("parse company");

    let person_name = |id: &str| -> String {
        company["people"]
            .as_array()
            .expect("people")
            .iter()
            .find(|p| p["id"] == id)
            .expect("person")["name"]
            .as_str()
            .expect("name")
            .to_string()
    };
    let owned_agents = |id: &str| -> Vec<String> {
        let mut owned: Vec<String> = company["agents"]
            .as_array()
            .expect("agents")
            .iter()
            .filter(|a| a["owner_user_id"] == id)
            .map(|a| a["id"].as_str().expect("id").to_string())
            .collect();
        owned.sort();
        owned
    };

    for subject in ["p060", "p061", "p017", "p093", "p116"] {
        let (status, _, bytes) = get_lens(&router, subject, subject).await;
        assert_eq!(status, StatusCode::OK);
        let body: Value = serde_json::from_slice(&bytes).expect("parses");
        assert_eq!(body["subject"]["kind"], "human");
        assert_eq!(body["subject"]["name"], person_name(subject).as_str());
        let listed: Vec<String> = body["agents"]
            .as_array()
            .expect("agents")
            .iter()
            .map(|a| a["agent_id"].as_str().expect("id").to_string())
            .collect();
        assert_eq!(listed, owned_agents(subject), "owned agents for {subject}");
    }

    // Agent subjects: kind, owner, and NO owned-agents list.
    let (status, _, bytes) =
        get_lens(&router, "agent_finance_analyst", "agent_finance_analyst").await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&bytes).expect("parses");
    assert_eq!(body["subject"]["kind"], "agent");
    assert_eq!(body["subject"]["owner_user_id"], "p061");
    assert_eq!(body["agents"].as_array().expect("agents").len(), 0);
}

// ---------------------------------------------------------------------------
// AL-4 SENTENCE TOTALITY
// ---------------------------------------------------------------------------

#[test]
fn al4_every_compiled_reason_has_a_sentence_and_unknowns_fail_closed() {
    let world = world();

    // Totality over the real world: every reason id in every artifact.
    let mut distinct: BTreeSet<&str> = BTreeSet::new();
    for per_doc in world.reasons.values() {
        for reasons in per_doc.values() {
            for reason in reasons {
                distinct.insert(reason);
            }
        }
    }
    assert!(
        distinct.len() >= 8,
        "the corpus exercises many reason kinds"
    );
    for reason in &distinct {
        sentence_for(reason).unwrap_or_else(|_| panic!("no sentence for {reason}"));
    }
    println!(
        "AL-4: {} distinct reason ids, all sentenced",
        distinct.len()
    );

    // An injected unknown reason refuses the WHOLE build — no partial body.
    let state = lens_state(None);
    let entries = vec![
        LensEntry {
            document_id: "d0001".to_string(),
            reasons: vec!["REBAC:grp_quality_compliance".to_string()],
            superseded: None,
            effective_successor: None,
        },
        LensEntry {
            document_id: "d0002".to_string(),
            reasons: vec!["MYSTERY:unknowable".to_string()],
            superseded: None,
            effective_successor: None,
        },
    ];
    let allowlist: BTreeSet<String> = entries.iter().map(|e| e.document_id.clone()).collect();
    assert!(
        build_holdings(&entries, &state.docs, &allowlist).is_err(),
        "an unknown reason fails the whole response, never a partial render"
    );
}

// ---------------------------------------------------------------------------
// AL-5 404 IDENTITY
// ---------------------------------------------------------------------------

#[tokio::test]
async fn al5_unknown_and_malformed_subjects_share_one_404() {
    let router = app(Arc::new(lens_state(None)));
    let unknown = get_lens(&router, "p060", "p_ghost_404").await;
    let malformed = get_lens(&router, "p060", "%21%21not--a--principal%21%21").await;
    assert_eq!(unknown.0, StatusCode::NOT_FOUND);
    assert_eq!(
        unknown, malformed,
        "unknown and malformed are byte-identical"
    );
}
