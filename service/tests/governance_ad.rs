//! Lens-diff governance harness AD-1..AD-5 (AP-4). FULLY OFFLINE.
//!
//! THE LAW under test: SET EXACTNESS — (left_only ∪ shared) == left's
//! compiled artifact and (right_only ∪ shared) == right's, exactly, for
//! every sampled pair; every difference attributed to the rule verbatim
//! from the owning artifact; ONE lens_diff audit row per act, written
//! before render; refusals (self-diff 400, unknown 404) leave no row.

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
use service::diff::build_diff_columns;
use service::lens::LensEntry;
use service::{app, AppState};
use tower::ServiceExt;

fn scratch(name: &str) -> PathBuf {
    // Unique per invocation: Windows scanners (Search indexer / Defender) can
    // hold a just-deleted path in delete-pending state, so re-creating the
    // SAME path races them into Os error 5 "Access is denied". A fresh suffix
    // never re-opens a dying path.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // The base lives in the SYSTEM temp dir, not target/tmp: the repo sits
    // under Documents\, which Windows Search indexes by default — its crawler
    // opens freshly written index segments mid-build and the write fails with
    // os error 5. AppData\Local\Temp is outside the default index scope.
    let base = std::env::temp_dir().join("enterprise-brain-test-scratch");
    std::fs::create_dir_all(&base).expect("scratch base");
    // CREATE-ONLY: the unique pid+seq suffix already guarantees no collision,
    // so this helper never deletes a sibling. Reaping by shared name-prefix
    // raced a concurrently running test in the same binary that used the same
    // name and deleted its live dir (failed estate_probes on Linux CI). Stale
    // dirs from old runs are harmless; the OS temp cleaner reaps them.
    let dir = base.join(format!(
        "{name}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[derive(Clone)]
struct EntryFacts {
    reasons: Vec<String>,
    superseded: Option<bool>,
    effective_successor: Option<String>,
}

struct World {
    fixtures_dir: PathBuf,
    artifacts_dir: PathBuf,
    idx_dir: PathBuf,
    /// principal -> doc -> the artifact entry facts (the ground truth).
    facts: BTreeMap<String, BTreeMap<String, EntryFacts>>,
    allowlists: BTreeMap<String, BTreeSet<String>>,
    principal_ids: Vec<String>,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("ad_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        let mut facts = BTreeMap::new();
        let mut allowlists = BTreeMap::new();
        for artifact in &set.artifacts {
            let mut per_doc = BTreeMap::new();
            let mut allow = BTreeSet::new();
            for entry in &artifact.entries {
                allow.insert(entry.document_id.clone());
                per_doc.insert(
                    entry.document_id.clone(),
                    EntryFacts {
                        reasons: entry.reasons.clone(),
                        superseded: entry.superseded,
                        effective_successor: entry.effective_successor.clone(),
                    },
                );
            }
            facts.insert(artifact.principal_id.clone(), per_doc);
            allowlists.insert(artifact.principal_id.clone(), allow);
        }
        let principal_ids: Vec<String> = allowlists.keys().cloned().collect();

        let idx_dir = scratch("ad_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");

        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
            facts,
            allowlists,
            principal_ids,
        }
    })
}

fn diff_state(store_dir: Option<&Path>) -> AppState {
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

async fn get_diff(
    router: &axum::Router,
    actor: &str,
    left: &str,
    right: &str,
) -> (StatusCode, Vec<u8>) {
    get_raw(
        router,
        actor,
        &format!("/lens/diff?left={left}&right={right}"),
    )
    .await
}

/// 10 deterministic pairs: 4 human-human, 2 agent-agent, 4 mixed.
fn sample_pairs(world: &World) -> Vec<(String, String)> {
    let agents: Vec<String> = world
        .principal_ids
        .iter()
        .filter(|p| p.starts_with("agent_"))
        .cloned()
        .collect();
    assert_eq!(agents.len(), 4, "all four agents in play");
    let humans_pool: Vec<String> = world
        .principal_ids
        .iter()
        .filter(|p| !p.starts_with("agent_"))
        .cloned()
        .collect();
    let mut rng = common::Lcg::new(0xAD_2026);
    let mut humans: Vec<String> = Vec::new();
    while humans.len() < 8 {
        let pick = rng.pick(&humans_pool).clone();
        if !humans.contains(&pick) {
            humans.push(pick);
        }
    }
    vec![
        (humans[0].clone(), humans[1].clone()),
        (humans[2].clone(), humans[3].clone()),
        (humans[4].clone(), humans[5].clone()),
        (humans[6].clone(), humans[7].clone()),
        (agents[0].clone(), agents[1].clone()),
        (agents[2].clone(), agents[3].clone()),
        (agents[0].clone(), humans[0].clone()),
        (agents[1].clone(), humans[2].clone()),
        (agents[2].clone(), humans[4].clone()),
        (agents[3].clone(), humans[6].clone()),
    ]
}

/// Independent reimplementation of the priority law for the property tests.
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

fn primary_of(reasons: &[String]) -> String {
    let mut all: Vec<String> = reasons.iter().map(|r| normalize(r)).collect();
    all.sort_by_key(|r| (class_of(r), r.clone()));
    all.dedup();
    all[0].clone()
}

fn column_doc_ids(sections: &Value) -> Vec<String> {
    sections
        .as_array()
        .expect("sections")
        .iter()
        .flat_map(|s| {
            s["docs"]
                .as_array()
                .expect("docs")
                .iter()
                .map(|d| d["document_id"].as_str().expect("id").to_string())
        })
        .collect()
}

fn shared_doc_ids(shared: &Value) -> Vec<String> {
    shared
        .as_array()
        .expect("shared")
        .iter()
        .map(|r| r["doc"]["document_id"].as_str().expect("id").to_string())
        .collect()
}

/// The AT-suite doc-id sweep, reused: `d` + exactly four digits, bounded by
/// non-alphanumerics (hex hashes cannot false-match).
fn extract_doc_ids(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut found = BTreeSet::new();
    for i in 0..bytes.len() {
        if bytes[i] != b'd' {
            continue;
        }
        if i > 0 && bytes[i - 1].is_ascii_alphanumeric() {
            continue;
        }
        let digits = &bytes[i + 1..];
        if digits.len() < 4 || !digits[..4].iter().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if digits.len() > 4 && digits[4].is_ascii_digit() {
            continue;
        }
        found.insert(text[i..i + 5].to_string());
    }
    found
}

// ---------------------------------------------------------------------------
// AD-1 SET EXACTNESS
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ad1_columns_partition_both_artifacts_exactly() {
    let world = world();
    let store_dir = scratch("ad1_store");
    let router = app(Arc::new(diff_state(Some(&store_dir))));
    let pairs = sample_pairs(world);

    let mut docs_checked = 0usize;
    for (left, right) in &pairs {
        let (status, bytes) = get_diff(&router, "p001", left, right).await;
        assert_eq!(status, StatusCode::OK, "diff {left} vs {right}");
        let body: Value = serde_json::from_slice(&bytes).expect("diff parses");

        let l = column_doc_ids(&body["left_only"]);
        let r = column_doc_ids(&body["right_only"]);
        let s = shared_doc_ids(&body["shared"]);

        // Zero duplicates anywhere.
        let lset: BTreeSet<String> = l.iter().cloned().collect();
        let rset: BTreeSet<String> = r.iter().cloned().collect();
        let sset: BTreeSet<String> = s.iter().cloned().collect();
        assert_eq!(l.len(), lset.len(), "left_only has no duplicates");
        assert_eq!(r.len(), rset.len(), "right_only has no duplicates");
        assert_eq!(s.len(), sset.len(), "shared has no duplicates");

        // Pairwise disjoint.
        assert!(
            lset.is_disjoint(&sset),
            "{left}|{right}: left_only ∩ shared"
        );
        assert!(
            rset.is_disjoint(&sset),
            "{left}|{right}: right_only ∩ shared"
        );
        assert!(
            lset.is_disjoint(&rset),
            "{left}|{right}: left_only ∩ right_only"
        );

        // SET EXACTNESS: unions equal the artifacts, zero misses.
        let left_union: BTreeSet<String> = lset.union(&sset).cloned().collect();
        let right_union: BTreeSet<String> = rset.union(&sset).cloned().collect();
        assert_eq!(left_union, world.allowlists[left], "left union for {left}");
        assert_eq!(
            right_union, world.allowlists[right],
            "right union for {right}"
        );

        // Shared sorted by document_id ascending.
        assert!(
            s.windows(2).all(|w| w[0] < w[1]),
            "shared sorted by document_id"
        );

        // Zero out-of-scope ids anywhere in the raw body.
        let in_scope: BTreeSet<&String> = world.allowlists[left]
            .iter()
            .chain(world.allowlists[right].iter())
            .collect();
        let text = String::from_utf8(bytes).expect("utf8");
        for id in extract_doc_ids(&text) {
            assert!(
                in_scope.contains(&id),
                "{id} serialized in diff {left}|{right} but belongs to neither artifact"
            );
        }
        docs_checked += l.len() + r.len() + s.len();
    }
    println!(
        "AD-1 summary: pairs=10 (4 human-human, 2 agent-agent, 4 mixed) \
         docs_checked={docs_checked} misses=0 duplicates=0 out_of_scope=0"
    );
}

// ---------------------------------------------------------------------------
// AD-2 AUDIT + GUARDS
// ---------------------------------------------------------------------------

const INTERNAL_ERROR_BODY: &[u8] = b"{\"demo_identity_mode\":true,\"error\":\"internal error\"}\n";

#[tokio::test]
async fn ad2_one_audit_row_per_diff_and_refusals_leave_none() {
    let store_dir = scratch("ad2_store");
    let router = app(Arc::new(diff_state(Some(&store_dir))));
    let read_audit = || -> Vec<AuditEvent> {
        fs::read_to_string(store_dir.join("audit.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("audit row"))
            .collect()
    };

    // Refusals FIRST — none of them may write a row.
    // Self-diff: a category error, same answer for known and unknown ids.
    let (status, bytes) = get_diff(&router, "p061", "p060", "p060").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        bytes,
        b"{\"demo_identity_mode\":true,\"error\":\"a diff of a lens against itself is a category error\"}\n"
    );
    let (status, _) = get_diff(&router, "p061", "p_ghost_ad", "p_ghost_ad").await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "unknown self-diff is still the category error"
    );

    // Unknown ids on either side: THE one 404, byte-identical to /lens's.
    let lens_404 = get_raw(&router, "p061", "/lens/p_ghost_404").await;
    assert_eq!(lens_404.0, StatusCode::NOT_FOUND);
    for (left, right) in [
        ("p_ghost_ad", "p060"),
        ("p060", "p_ghost_ad"),
        ("p_ghost_ad", "p_ghost_bd"),
    ] {
        let response = get_diff(&router, "p061", left, right).await;
        assert_eq!(response, lens_404, "diff {left}|{right} shares THE one 404");
    }

    // Malformed queries: our 400 shape, not axum's.
    let (status, bytes) = get_raw(&router, "p061", "/lens/diff?left=p060").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        bytes,
        b"{\"demo_identity_mode\":true,\"error\":\"left and right are required\"}\n"
    );
    let (status, bytes) = get_raw(&router, "p061", "/lens/diff?left=p060&right=p061&count=1").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        bytes,
        b"{\"demo_identity_mode\":true,\"error\":\"only left and right are accepted\"}\n"
    );

    assert!(
        read_audit().is_empty(),
        "no refusal of any kind wrote a lens_diff row"
    );

    // The allowed act: exactly ONE row, both sides on it, ordinal 0.
    let (status, _) = get_diff(&router, "p061", "p060", "agent_finance_analyst").await;
    assert_eq!(status, StatusCode::OK);
    let audit = read_audit();
    assert_eq!(audit.len(), 1, "one act, one row — never two lens_views");
    assert_eq!(audit[0].action, "lens_diff");
    assert_eq!(audit[0].actor_principal, "p061");
    assert_eq!(audit[0].left.as_deref(), Some("p060"));
    assert_eq!(audit[0].right.as_deref(), Some("agent_finance_analyst"));
    assert_eq!(audit[0].outcome, "allowed_demo");
    assert_eq!(audit[0].ordinal, 0);

    // Ordinals advance per act.
    let (status, _) = get_diff(&router, "p016", "p016", "p087").await;
    assert_eq!(status, StatusCode::OK);
    let audit = read_audit();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[1].ordinal, 1);

    // No audit sink configured: the act cannot be recorded, so it cannot
    // happen. Fail closed, explain nothing.
    let bare = app(Arc::new(diff_state(None)));
    let (status, bytes) = get_diff(&bare, "p001", "p060", "p061").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(bytes, INTERNAL_ERROR_BODY);
    println!("AD-2 summary: refusals(400/404/malformed)=7 rows=0; allowed=2 rows=2 ordinals=0,1; no-store=500");
}

// ---------------------------------------------------------------------------
// AD-3 ATTRIBUTION
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ad3_reasons_verbatim_and_divergence_iff_primaries_differ() {
    let world = world();
    let store_dir = scratch("ad3_store");
    let router = app(Arc::new(diff_state(Some(&store_dir))));

    // The sampled pairs, plus the charter's own example (the HR record route
    // split: SUBJECT:self vs REBAC:grp_hr).
    let mut pairs = sample_pairs(world);
    pairs.push(("p016".to_string(), "p087".to_string()));

    let (mut shared_checked, mut divergent_seen, mut exclusive_checked) = (0usize, 0usize, 0usize);
    for (left, right) in &pairs {
        let (status, bytes) = get_diff(&router, "p001", left, right).await;
        assert_eq!(status, StatusCode::OK);
        let body: Value = serde_json::from_slice(&bytes).expect("parses");

        for row in body["shared"].as_array().expect("shared") {
            let id = row["doc"]["document_id"].as_str().expect("id");
            let left_reasons: Vec<String> = row["left_reasons"]
                .as_array()
                .expect("left_reasons")
                .iter()
                .map(|v| v.as_str().expect("reason").to_string())
                .collect();
            let right_reasons: Vec<String> = row["right_reasons"]
                .as_array()
                .expect("right_reasons")
                .iter()
                .map(|v| v.as_str().expect("reason").to_string())
                .collect();
            assert_eq!(
                left_reasons, world.facts[left][id].reasons,
                "left reasons verbatim for {id} of {left}"
            );
            assert_eq!(
                right_reasons, world.facts[right][id].reasons,
                "right reasons verbatim for {id} of {right}"
            );
            let expected = primary_of(&left_reasons) != primary_of(&right_reasons);
            assert_eq!(
                row["divergent_route"].as_bool(),
                Some(expected),
                "divergent_route for {id} of {left}|{right}"
            );
            shared_checked += 1;
            if expected {
                divergent_seen += 1;
            }
        }

        for (column, owner) in [("left_only", left), ("right_only", right)] {
            for section in body[column].as_array().expect(column) {
                let reason = section["reason"].as_str().expect("reason");
                for doc in section["docs"].as_array().expect("docs") {
                    let id = doc["document_id"].as_str().expect("id");
                    assert_eq!(
                        reason,
                        primary_of(&world.facts[owner.as_str()][id].reasons),
                        "section attribution for {id} of {owner}"
                    );
                    exclusive_checked += 1;
                }
            }
        }
    }
    assert!(divergent_seen > 0, "the divergence property was exercised");

    // The charter example, explicitly: d0093 routes SUBJECT:self for its
    // subject and REBAC:grp_hr for the HR group.
    let (_, bytes) = get_diff(&router, "p001", "p016", "p087").await;
    let body: Value = serde_json::from_slice(&bytes).expect("parses");
    let d0093 = body["shared"]
        .as_array()
        .expect("shared")
        .iter()
        .find(|r| r["doc"]["document_id"] == "d0093")
        .expect("d0093 is shared between p016 and p087");
    assert_eq!(d0093["divergent_route"], Value::Bool(true));
    assert_eq!(d0093["left_reasons"][0], "SUBJECT:self");
    println!(
        "AD-3 summary: pairs=11 shared_checked={shared_checked} \
         divergent_seen={divergent_seen} exclusive_checked={exclusive_checked} violations=0"
    );
}

// ---------------------------------------------------------------------------
// AD-4 REDACTION
// ---------------------------------------------------------------------------

fn synth_entry(id: &str, reasons: &[&str], superseded: bool, successor: Option<&str>) -> LensEntry {
    LensEntry {
        document_id: id.to_string(),
        reasons: reasons.iter().map(|r| r.to_string()).collect(),
        superseded: superseded.then_some(true),
        effective_successor: successor.map(|s| s.to_string()),
    }
}

#[test]
fn ad4_successor_redaction_is_per_side_and_intersection_on_shared() {
    let world = world();
    let state = diff_state(None);

    // LEFT holds: d0001 (superseded -> d0002), d0002 itself, and d0003
    // (superseded -> d0004, which left does NOT hold).
    let left = vec![
        synth_entry(
            "d0001",
            &["REBAC:grp_quality_compliance"],
            true,
            Some("d0002"),
        ),
        synth_entry("d0002", &["REBAC:grp_quality_compliance"], false, None),
        synth_entry(
            "d0003",
            &["REBAC:grp_quality_compliance"],
            true,
            Some("d0004"),
        ),
    ];
    // RIGHT holds d0001 by a different route (same corpus supersedence
    // facts), plus its own superseded pair d0005 -> d0006, both held.
    let right = vec![
        synth_entry("d0001", &["REBAC:grp_finance"], true, Some("d0002")),
        synth_entry("d0005", &["REBAC:grp_finance"], true, Some("d0006")),
        synth_entry("d0006", &["REBAC:grp_finance"], false, None),
    ];
    let (left_only, right_only, shared) =
        build_diff_columns(&left, &right, &state.docs).expect("columns build");

    // The shared row serves two worlds: its successor scope is the
    // INTERSECTION, and d0002 is left's alone — so the row carries NO
    // successor, while d0002 renders as its own row in the one column
    // whose side may see it.
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].doc.document_id, "d0001");
    assert_eq!(shared[0].doc.superseded, Some(true));
    assert_eq!(shared[0].doc.effective_successor, None);
    assert!(shared[0].divergent_route, "different REBAC routes diverge");

    let left_ids: Vec<&str> = left_only
        .iter()
        .flat_map(|s| s.docs.iter().map(|d| d.document_id.as_str()))
        .collect();
    assert!(
        left_ids.contains(&"d0002"),
        "the successor renders in left's column"
    );
    let d0003 = left_only
        .iter()
        .flat_map(|s| s.docs.iter())
        .find(|d| d.document_id == "d0003")
        .expect("d0003 in left_only");
    assert_eq!(
        d0003.effective_successor, None,
        "an out-of-scope successor is redacted in the owning column too"
    );

    let d0005 = right_only
        .iter()
        .flat_map(|s| s.docs.iter())
        .find(|d| d.document_id == "d0005")
        .expect("d0005 in right_only");
    assert_eq!(
        d0005.effective_successor.as_deref(),
        Some("d0006"),
        "an in-scope successor renders in the owning column"
    );

    // Worlds that disagree about supersedence refuse outright.
    let l2 = vec![synth_entry(
        "d0010",
        &["REBAC:grp_finance"],
        true,
        Some("d0011"),
    )];
    let r2 = vec![synth_entry("d0010", &["REBAC:grp_finance"], false, None)];
    assert!(
        build_diff_columns(&l2, &r2, &state.docs).is_err(),
        "supersedence is corpus fact; disagreement refuses"
    );

    // Opportunistic corpus scan: does the real world contain an asymmetric
    // successor pair? Reported either way (the synthetic cases above carry
    // the property regardless).
    let mut real_case: Option<(String, String, String, String)> = None;
    'outer: for (p, docs) in &world.facts {
        for (doc, entry_facts) in docs {
            let (Some(true), Some(successor)) = (
                entry_facts.superseded,
                entry_facts.effective_successor.as_ref(),
            ) else {
                continue;
            };
            if !world.allowlists[p].contains(successor) {
                continue;
            }
            for (q, q_allow) in &world.allowlists {
                if q != p && q_allow.contains(doc) && !q_allow.contains(successor) {
                    real_case = Some((p.clone(), q.clone(), doc.clone(), successor.clone()));
                    break 'outer;
                }
            }
        }
    }
    match &real_case {
        Some((p, q, doc, successor)) => println!(
            "AD-4 summary: synthetic cases proven; real corpus case found: \
             {doc} shared by {p}|{q}, successor {successor} visible to {p} only"
        ),
        None => println!(
            "AD-4 summary: synthetic cases proven; the live corpus has no \
             asymmetric successor pair (reported, not assumed)"
        ),
    }
}

// ---------------------------------------------------------------------------
// AD-5 SHAPE
// ---------------------------------------------------------------------------

fn check_keys(value: &Value, allowed: &[&str], required: &[&str], path: &str) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("object at {path}"));
    for key in object.keys() {
        assert!(
            allowed.contains(&key.as_str()),
            "unexpected key {key:?} at {path}"
        );
    }
    for key in required {
        assert!(object.contains_key(*key), "missing key {key:?} at {path}");
    }
}

fn check_doc_row(row: &Value, path: &str) {
    check_keys(
        row,
        &[
            "document_id",
            "effective_successor",
            "sensitivity",
            "superseded",
            "title",
        ],
        &["document_id", "sensitivity", "title"],
        path,
    );
}

fn check_diff_shape(body: &Value) {
    check_keys(
        body,
        &[
            "actor_id",
            "demo_identity_mode",
            "left",
            "left_only",
            "right",
            "right_only",
            "shared",
            "snapshot_version",
        ],
        &[
            "actor_id",
            "demo_identity_mode",
            "left",
            "left_only",
            "right",
            "right_only",
            "shared",
            "snapshot_version",
        ],
        "$",
    );
    for side in ["left", "right"] {
        check_keys(
            &body[side],
            &["id", "kind", "name"],
            &["id", "kind", "name"],
            side,
        );
    }
    for column in ["left_only", "right_only"] {
        for (index, section) in body[column].as_array().expect(column).iter().enumerate() {
            let path = format!("{column}[{index}]");
            check_keys(
                section,
                &["docs", "reason", "sentence"],
                &["docs", "reason", "sentence"],
                &path,
            );
            for (di, doc) in section["docs"].as_array().expect("docs").iter().enumerate() {
                check_doc_row(doc, &format!("{path}.docs[{di}]"));
            }
        }
    }
    for (index, row) in body["shared"]
        .as_array()
        .expect("shared")
        .iter()
        .enumerate()
    {
        let path = format!("shared[{index}]");
        check_keys(
            row,
            &["divergent_route", "doc", "left_reasons", "right_reasons"],
            &["divergent_route", "doc", "left_reasons", "right_reasons"],
            &path,
        );
        check_doc_row(&row["doc"], &format!("{path}.doc"));
        for side in ["left_reasons", "right_reasons"] {
            for reason in row[side].as_array().expect(side) {
                assert!(reason.is_string(), "{path}.{side} carries strings only");
            }
        }
    }
}

#[tokio::test]
async fn ad5_nothing_beyond_the_schema_and_scarcity_is_only_shorter_arrays() {
    let world = world();
    let store_dir = scratch("ad5_store");
    let router = app(Arc::new(diff_state(Some(&store_dir))));

    let (status, bytes) = get_diff(&router, "p001", "p060", "p061").await;
    assert_eq!(status, StatusCode::OK);
    let rich: Value = serde_json::from_slice(&bytes).expect("parses");
    check_diff_shape(&rich);

    // The engineered near-empty side: p_void (public-only) against p060 is
    // a STRICT SUBSET — its exclusive column is the empty array, a finding,
    // and the only thing scarcity changed is array lengths.
    let (status, bytes) = get_diff(&router, "p001", "p_void", "p060").await;
    assert_eq!(status, StatusCode::OK);
    let scarce: Value = serde_json::from_slice(&bytes).expect("parses");
    check_diff_shape(&scarce);

    let void_allow = &world.allowlists["p_void"];
    let p060_allow = &world.allowlists["p060"];
    assert!(
        void_allow.is_subset(p060_allow),
        "p_void ⊂ p060 in this corpus"
    );
    assert_eq!(
        scarce["left_only"].as_array().expect("left_only").len(),
        0,
        "a strict subset has an EMPTY exclusive column — whitespace, not prose"
    );
    assert_eq!(
        shared_doc_ids(&scarce["shared"]).len(),
        void_allow.len(),
        "everything p_void holds is shared"
    );
    assert_eq!(
        column_doc_ids(&scarce["right_only"]).len(),
        p060_allow.len() - void_allow.len(),
        "the remainder is exactly p060's exclusive holding"
    );
    println!(
        "AD-5 summary: schema whitelist holds for rich and scarce bodies; \
         strict-subset column empty; arrays are the only variance"
    );
}
