//! Evidence-export governance harness AE-1..AE-6 (AP-5). FULLY OFFLINE.
//!
//! THE LAW under test: THE SERVER DERIVES, NEVER RECEIVES. The request
//! schema cannot carry content (AE-1); the export is its own audited act
//! stacked on the view's act (AE-2); the PDF text carries the derived body
//! whole and its footer hash equals the hash of the live endpoint's bytes
//! (AE-3); the print register obeys every absence and redaction law (AE-4);
//! fixed-date mode renders byte-identical PDFs with the date outside the
//! attested hash (AE-5); the demo watermark is verbatim on every export
//! (AE-6).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::index::{build_index, sha256_hex};
use serde_json::{json, Value};
use service::agent::proposals::{AuditEvent, ProposalStore};
use service::export::{render_export_pdf, ExportMeta, WATERMARK};
use service::{app, AppState};
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

struct World {
    fixtures_dir: PathBuf,
    artifacts_dir: PathBuf,
    idx_dir: PathBuf,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();
        let artifacts_dir = scratch("ae_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");
        let idx_dir = scratch("ae_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build index");
        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
        }
    })
}

/// The fixed-date test value from the spec.
const FIXED_DATE: &str = "2026-01-05T00:00:00Z";

fn export_state(store_dir: Option<&Path>, fixed_date: Option<&str>) -> AppState {
    let world = world();
    let state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
        .with_export_fixed_date(fixed_date.map(str::to_string));
    match store_dir {
        Some(dir) => state.with_proposals(Arc::new(
            ProposalStore::open(dir).expect("open audit store"),
        )),
        None => state,
    }
}

async fn post_export(router: &axum::Router, actor: &str, body: &str) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/export")
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

fn read_audit(store_dir: &Path) -> Vec<AuditEvent> {
    fs::read_to_string(store_dir.join("audit.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("audit row"))
        .collect()
}

fn pdf_text(bytes: &[u8]) -> String {
    pdf_extract::extract_text_from_mem(bytes).expect("pdf text extracts")
}

/// Whitespace-squashed containment (PDF extraction breaks lines freely).
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn assert_in(haystack_squashed: &str, needle: &str, label: &str) {
    assert!(
        haystack_squashed.contains(&squash(needle)),
        "{label}: {needle:?} missing from the extracted PDF text"
    );
}

const STRICT_PARSE_BODY: &[u8] =
    b"{\"demo_identity_mode\":true,\"error\":\"export request fails strict parse (params only)\"}\n";

// ---------------------------------------------------------------------------
// AE-1 PARAMS-ONLY
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ae1_a_request_that_could_carry_content_cannot_even_parse() {
    let store_dir = scratch("ae1_store");
    let router = app(Arc::new(export_state(Some(&store_dir), Some(FIXED_DATE))));

    // Content-shaped fields refuse at the parse layer, top-level or nested.
    for body in [
        r#"{"view":"lens","lens":{"subject_id":"p060"},"docs":["d0001"]}"#,
        r#"{"view":"lens","lens":{"subject_id":"p060"},"content":"forged"}"#,
        r#"{"view":"lens","lens":{"subject_id":"p060","content":"forged"}}"#,
        r#"{"view":"diff","diff":{"left":"p060","right":"p061","rows":[]}}"#,
        r#"{"view":"ask","ask":{"query":"q","hybrid":false,"judge":false,"answer":"forged"}}"#,
        "not json at all",
    ] {
        let (status, bytes) = post_export(&router, "p060", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "refused: {body}");
        assert_eq!(bytes, STRICT_PARSE_BODY, "OUR refusal shape for: {body}");
    }

    // Structurally valid but disagreeing view/params: still a 400, and the
    // refusal names the disagreement, not axum.
    let (status, _) = post_export(
        &router,
        "p060",
        r#"{"view":"lens","diff":{"left":"p060","right":"p061"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = post_export(
        &router,
        "p060",
        r#"{"view":"lens","lens":{"subject_id":"p060"},"diff":{"left":"a","right":"b"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = post_export(&router, "p060", r#"{"view":"ledger"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert!(read_audit(&store_dir).is_empty(), "refusals are rowless");
    println!("AE-1 summary: content-bearing bodies refused=6 disagreements refused=3 rows=0");
}

// ---------------------------------------------------------------------------
// AE-2 DUAL AUDIT
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ae2_the_look_and_the_export_are_two_rows_and_refusals_are_none() {
    let store_dir = scratch("ae2_store");
    let router = app(Arc::new(export_state(Some(&store_dir), Some(FIXED_DATE))));

    // Exported diff: the look (lens_diff) then the export, adjacent ordinals.
    let (status, _) = post_export(
        &router,
        "p061",
        r#"{"view":"diff","diff":{"left":"p060","right":"agent_finance_analyst"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let audit = read_audit(&store_dir);
    assert_eq!(audit.len(), 2, "one act, two rows: the look and the export");
    assert_eq!(audit[0].action, "lens_diff");
    assert_eq!(audit[0].ordinal, 0);
    assert_eq!(audit[1].action, "evidence_export");
    assert_eq!(audit[1].ordinal, 1);
    assert_eq!(audit[1].actor_principal, "p061");
    assert_eq!(audit[1].target, "diff:p060|agent_finance_analyst");
    assert_eq!(audit[1].outcome, "allowed_demo");

    // Exported self-lens: evidence_export ONLY (a self view is not audited).
    let (status, _) = post_export(
        &router,
        "p060",
        r#"{"view":"lens","lens":{"subject_id":"p060"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let audit = read_audit(&store_dir);
    assert_eq!(audit.len(), 3);
    assert_eq!(audit[2].action, "evidence_export");
    assert_eq!(audit[2].target, "lens:p060");

    // Exported CROSS lens: the look then the export.
    let (status, _) = post_export(
        &router,
        "p061",
        r#"{"view":"lens","lens":{"subject_id":"p060"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let audit = read_audit(&store_dir);
    assert_eq!(audit.len(), 5);
    assert_eq!(audit[3].action, "lens_view");
    assert_eq!(audit[4].action, "evidence_export");
    assert_eq!((audit[3].ordinal, audit[4].ordinal), (3, 4));

    // Refusals: rowless, with the live endpoints' own bytes.
    let (status, bytes) = post_export(
        &router,
        "p061",
        r#"{"view":"diff","diff":{"left":"p060","right":"p060"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        bytes,
        b"{\"demo_identity_mode\":true,\"error\":\"a diff of a lens against itself is a category error\"}\n"
    );
    let (status, bytes) = post_export(
        &router,
        "p061",
        r#"{"view":"lens","lens":{"subject_id":"p_ghost_ae"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let lens_404 = get_raw(&router, "p061", "/lens/p_ghost_404").await;
    assert_eq!(bytes, lens_404.1, "the export refuses with THE one 404");
    let (status, _) = post_export(
        &router,
        "p060",
        r#"{"view":"atlas_capability","atlas_capability":{"capability_id":"cap_unknown"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        read_audit(&store_dir).len(),
        5,
        "every refusal left zero rows"
    );

    // No audit sink: the export cannot be recorded, so it cannot happen.
    let bare = app(Arc::new(export_state(None, Some(FIXED_DATE))));
    let (status, _) = post_export(
        &bare,
        "p060",
        r#"{"view":"lens","lens":{"subject_id":"p060"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    println!("AE-2 summary: diff=2 rows, self-lens=1, cross-lens=2, refusals=0 rows, no-store=500");
}

// ---------------------------------------------------------------------------
// AE-3 FIDELITY
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ae3_the_pdf_carries_the_derived_body_and_attests_the_live_bytes() {
    let store_dir = scratch("ae3_store");
    let router = app(Arc::new(export_state(Some(&store_dir), Some(FIXED_DATE))));

    // LENS: export p060's self view; the live endpoint derives the SAME
    // bytes, so the footer hash must equal the live hash.
    let (status, pdf) = post_export(
        &router,
        "p060",
        r#"{"view":"lens","lens":{"subject_id":"p060"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = squash(&pdf_text(&pdf));
    let (live_status, live) = get_raw(&router, "p060", "/lens/p060").await;
    assert_eq!(live_status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&live).expect("lens parses");
    let mut ids = 0usize;
    for section in body["holdings"].as_array().expect("holdings") {
        assert_in(
            &text,
            section["sentence"].as_str().expect("sentence"),
            "lens sentence",
        );
        assert_in(
            &text,
            &format!("[{}]", section["reason"].as_str().expect("reason")),
            "lens rule chip",
        );
        for doc in section["docs"].as_array().expect("docs") {
            assert_in(
                &text,
                doc["document_id"].as_str().expect("id"),
                "lens doc id",
            );
            ids += 1;
        }
    }
    assert_in(
        &text,
        &format!("content sha256: {}", sha256_hex(&live)),
        "lens attestation",
    );
    assert_in(&text, "page 1/", "the page n/N footer line");

    // DIFF: all three columns and the divergent chips survive to print.
    let (status, pdf) = post_export(
        &router,
        "p016",
        r#"{"view":"diff","diff":{"left":"p016","right":"p087"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = squash(&pdf_text(&pdf));
    let (live_status, live) = get_raw(&router, "p016", "/lens/diff?left=p016&right=p087").await;
    assert_eq!(live_status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&live).expect("diff parses");
    let mut diff_ids = 0usize;
    for column in ["left_only", "right_only"] {
        for section in body[column].as_array().expect(column) {
            assert_in(
                &text,
                section["sentence"].as_str().expect("sentence"),
                "diff sentence",
            );
            assert_in(
                &text,
                &format!("[{}]", section["reason"].as_str().expect("reason")),
                "diff rule chip",
            );
            for doc in section["docs"].as_array().expect("docs") {
                assert_in(
                    &text,
                    doc["document_id"].as_str().expect("id"),
                    "diff doc id",
                );
                diff_ids += 1;
            }
        }
    }
    for row in body["shared"].as_array().expect("shared") {
        assert_in(
            &text,
            row["doc"]["document_id"].as_str().expect("id"),
            "shared doc id",
        );
        diff_ids += 1;
    }
    // The divergent chips (page-break tolerant: a row may legally wrap
    // across pages, with the next page's footer between the fragments in
    // extraction order; AE-4's single-page render asserts the joined pair).
    assert_in(&text, "[SUBJECT:self", "the divergent left chip");
    assert_in(&text, "REBAC:grp_hr]", "the divergent right chip");
    assert_in(
        &text,
        &format!("content sha256: {}", sha256_hex(&live)),
        "diff attestation",
    );
    assert_in(&text, "page 1/", "the page n/N footer line");
    println!(
        "AE-3 summary: lens ids={ids} diff ids={diff_ids} sentences+chips+hashes all in print"
    );
}

// ---------------------------------------------------------------------------
// AE-4 LAWS IN PRINT
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ae4_absence_and_redaction_hold_in_print() {
    let store_dir = scratch("ae4_store");
    let router = app(Arc::new(export_state(Some(&store_dir), Some(FIXED_DATE))));

    // The engineered near-empty actor: p_void's whole world is one public
    // section. The print carries no count-shaped vocabulary.
    let (status, pdf) = post_export(
        &router,
        "p_void",
        r#"{"view":"lens","lens":{"subject_id":"p_void"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = pdf_text(&pdf).to_lowercase();
    for forbidden in ["count", "hidden", "total"] {
        assert!(
            !text.contains(forbidden),
            "forbidden vocabulary {forbidden:?} in the near-empty export"
        );
    }

    // One-side successor, through the SAME renderer (the body law itself is
    // AD-4's; print must not invent a second channel): the successor id
    // appears exactly once — as left's own row — and the shared row prints
    // the bare strike.
    let world = world();
    let state = export_state(None, Some(FIXED_DATE));
    let synthetic = json!({
        "actor_id": "p001",
        "left": {"id": "p_l", "kind": "human", "name": "Left Synthetic"},
        "right": {"id": "p_r", "kind": "human", "name": "Right Synthetic"},
        "left_only": [{
            "reason": "REBAC:grp_finance",
            "sentence": "You see this because you are in grp_finance.",
            "docs": [{"document_id": "d0002", "sensitivity": "internal", "title": "Successor Doc"}],
        }],
        "right_only": [],
        "shared": [{
            "divergent_route": true,
            "doc": {"document_id": "d0001", "sensitivity": "internal", "title": "Superseded Doc", "superseded": true},
            "left_reasons": ["REBAC:grp_quality_compliance"],
            "right_reasons": ["REBAC:grp_finance"],
        }],
        "snapshot_version": "synthetic",
    });
    let meta = ExportMeta {
        actor: "p001".to_string(),
        subjects: "p_l | p_r".to_string(),
        view_title: "Lens diff — p_l vs p_r".to_string(),
        snapshot_version: "synthetic".to_string(),
        index_version: "synthetic".to_string(),
        content_sha256: sha256_hex(b"synthetic"),
        audit_ordinals: vec![0, 1],
        generated: FIXED_DATE.to_string(),
    };
    let pdf = render_export_pdf("diff", &synthetic, &meta, &state.export_fonts_dir)
        .expect("synthetic diff renders");
    let text = squash(&pdf_text(&pdf));
    assert_eq!(
        text.matches("d0002").count(),
        1,
        "the successor renders exactly once — in the permitted column"
    );
    assert_in(
        &text,
        "d0001 Superseded Doc [internal] (superseded)",
        "the bare strike",
    );
    assert!(
        !text.contains("effective: d0002"),
        "the shared row never names a successor the law redacted"
    );
    // Single page, so the joined chip pair is assertable verbatim here.
    assert_in(
        &text,
        "[REBAC:grp_quality_compliance | REBAC:grp_finance]",
        "both chips, side by side",
    );
    let _ = world;
    println!("AE-4 summary: forbidden_vocab=0 one-side successor printed once, shared row bare");
}

// ---------------------------------------------------------------------------
// AE-5 DETERMINISM
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ae5_fixed_date_exports_are_byte_identical_and_the_date_is_unattested() {
    // Two worlds with identical (empty) audit histories and the same fixed
    // date: byte-identical PDFs.
    let store_a = scratch("ae5_store_a");
    let store_b = scratch("ae5_store_b");
    let router_a = app(Arc::new(export_state(Some(&store_a), Some(FIXED_DATE))));
    let router_b = app(Arc::new(export_state(Some(&store_b), Some(FIXED_DATE))));
    let request = r#"{"view":"lens","lens":{"subject_id":"p060"}}"#;
    let (status_a, pdf_a) = post_export(&router_a, "p060", request).await;
    let (status_b, pdf_b) = post_export(&router_b, "p060", request).await;
    assert_eq!(status_a, StatusCode::OK);
    assert_eq!(status_b, StatusCode::OK);
    if pdf_a != pdf_b {
        let dump = scratch("ae5_dump");
        fs::write(dump.join("a.pdf"), &pdf_a).expect("dump a");
        fs::write(dump.join("b.pdf"), &pdf_b).expect("dump b");
        let at = pdf_a
            .iter()
            .zip(pdf_b.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(pdf_a.len().min(pdf_b.len()));
        panic!(
            "fixed-date exports differ at byte {at} (lens {} vs {}); dumped to {}",
            pdf_a.len(),
            pdf_b.len(),
            dump.display()
        );
    }

    // Flip the injected date: different bytes, SAME attested hash — the
    // date participates in nothing attested.
    let store_c = scratch("ae5_store_c");
    let router_c = app(Arc::new(export_state(
        Some(&store_c),
        Some("2027-02-06T12:34:56Z"),
    )));
    let (status_c, pdf_c) = post_export(&router_c, "p060", request).await;
    assert_eq!(status_c, StatusCode::OK);
    assert_ne!(pdf_a, pdf_c, "a different date is a different artifact");

    let hash_line = |pdf: &[u8]| -> String {
        let text = squash(&pdf_text(pdf));
        let at = text.find("content sha256: ").expect("hash line");
        text[at..at + "content sha256: ".len() + 64].to_string()
    };
    assert_eq!(
        hash_line(&pdf_a),
        hash_line(&pdf_c),
        "the attested hash ignores the date"
    );
    let text_a = squash(&pdf_text(&pdf_a));
    let text_c = squash(&pdf_text(&pdf_c));
    assert_in(
        &text_a,
        &format!("Generated: {FIXED_DATE}"),
        "the dated line",
    );
    assert_in(
        &text_c,
        "Generated: 2027-02-06T12:34:56Z",
        "the flipped dated line",
    );
    println!("AE-5 summary: identical=true flipped-date hash-equal=true bytes-differ=true");
}

// ---------------------------------------------------------------------------
// AE-6 WATERMARK
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ae6_the_demo_watermark_is_verbatim_on_every_export() {
    let store_dir = scratch("ae6_store");
    let router = app(Arc::new(export_state(Some(&store_dir), Some(FIXED_DATE))));
    let exports = [
        ("p060", r#"{"view":"lens","lens":{"subject_id":"p060"}}"#),
        (
            "p016",
            r#"{"view":"diff","diff":{"left":"p016","right":"p087"}}"#,
        ),
        (
            "p060",
            r#"{"view":"atlas_capability","atlas_capability":{"capability_id":"cap01"}}"#,
        ),
        (
            "p060",
            r#"{"view":"ask","ask":{"query":"temperature range storage procedure","hybrid":false,"judge":false}}"#,
        ),
    ];
    for (actor, request) in exports {
        let (status, pdf) = post_export(&router, actor, request).await;
        assert_eq!(status, StatusCode::OK, "export succeeds: {request}");
        let text = squash(&pdf_text(&pdf));
        assert_in(&text, WATERMARK, "the watermark line");
    }
    println!("AE-6 summary: watermark verbatim on lens/diff/atlas_capability/ask exports");
}
