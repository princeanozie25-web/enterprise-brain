//! K1 grounded-answers governance harness G-1..G-9. FULLY OFFLINE:
//! MockGenerator over a lexical-only index (grounding needs no vectors);
//! G-9 wires FileEmbeddings/MockJudge over the SAME vectorless index to pin
//! the honest-400 preflight. The only socket any test may open is the
//! in-memory router.
//!
//! THE RULE UNDER TEST: every rendered answer sentence is verbatim-anchored
//! to a sealed-context document (Admitted), every dropped draft claim is
//! DISCLOSED (grounding counts), and no failure on the generation side is
//! ever worse than retrieval-only.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use retrieval::embed::FileEmbeddings;
use retrieval::index::build_index;
use retrieval::judge::{MockBehavior as JudgeBehavior, MockJudge};
use serde_json::Value;
use service::answer::{ask, AskError, AskOptions, AskTrace};
use service::cache::AnswerCache;
use service::generate::{parse_claims, Generator, MockBehavior as GenBehavior, MockGenerator};
use service::grounding::{ground, AnchorOnly, Claim, Grounded};
use service::{app, AppState};
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Shared, build-once lexical-only world
// ---------------------------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    for attempt in 0u64..50 {
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(&dir).is_ok()
            && std::fs::read_dir(&dir)
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
    /// doc id -> full body, straight from documents.json (the G-1 oracle).
    bodies: BTreeMap<String, String>,
}

fn world() -> &'static World {
    static WORLD: OnceLock<World> = OnceLock::new();
    WORLD.get_or_init(|| {
        let fixtures_dir = common::repo_fixtures_dir();

        let artifacts_dir = scratch("g_m1_artifacts");
        let snap = scope_compiler::snapshot::take(&fixtures_dir).expect("snapshot");
        let m1_world = scope_compiler::load_world(&fixtures_dir).expect("fixtures validate");
        let (set, unknown) =
            scope_compiler::compile::compile_set(&m1_world, &snap, None).expect("compile M1");
        assert!(unknown.is_empty());
        scope_compiler::compile::write_artifacts(&artifacts_dir, &set).expect("write artifacts");

        // LEXICAL-ONLY index: grounding anchors against full bodies, not
        // vectors — and G-9 needs manifest.vectors == None.
        let idx_dir = scratch("g_idx");
        build_index(&fixtures_dir, &idx_dir).expect("build lexical index");

        let value: Value = serde_json::from_slice(
            &std::fs::read(fixtures_dir.join("documents.json")).expect("read documents"),
        )
        .expect("parse documents");
        let bodies: BTreeMap<String, String> = value["documents"]
            .as_array()
            .expect("documents array")
            .iter()
            .map(|d| {
                (
                    d["id"].as_str().expect("id").to_string(),
                    d["body"].as_str().expect("body").to_string(),
                )
            })
            .collect();

        World {
            fixtures_dir,
            artifacts_dir,
            idx_dir,
            bodies,
        }
    })
}

fn state_with(generator: Option<Arc<dyn Generator>>, cache: Option<Arc<AnswerCache>>) -> AppState {
    let world = world();
    let mut state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
        .with_cache(cache);
    if let Some(generator) = generator {
        state = state.with_generator(generator);
    }
    state
}

fn ask_ok(
    state: &AppState,
    principal: &str,
    query: &str,
    options: &AskOptions,
) -> (Value, AskTrace) {
    let (bytes, trace) = ask(state, principal, query, options).expect("ask succeeds");
    let value: Value = serde_json::from_slice(&bytes).expect("envelope parses");
    (value, trace)
}

/// A query every demo identity resolves differently; p060 lands 10 results.
const FINANCE_QUERY: &str = "confidential financial statements";

// ---------------------------------------------------------------------------
// G-1 EVERY ADMITTED CLAIM IS VERBATIM-ANCHORED AT ITS LOCATOR
// ---------------------------------------------------------------------------

#[test]
fn g1_admitted_claims_anchor_verbatim_at_the_recorded_locator() {
    let world = world();
    let state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        None,
    );
    let queries = [
        FINANCE_QUERY,
        "payroll salary review",
        "quality procedure record",
        "warehouse stock delivery",
        "customer account review",
    ];
    let principals = ["p060", "p088", "p001", "p011", "p091"];
    let mut claims_checked = 0usize;
    for principal in principals {
        for query in queries {
            let (envelope, trace) = ask_ok(&state, principal, query, &AskOptions::default());
            let sealed: BTreeSet<&str> = trace.sealed.iter().map(|d| d.doc_id.as_str()).collect();
            let Some(answer) = envelope.get("answer") else {
                continue; // nothing sealed -> nothing generated; other tests pin that
            };
            for claim in answer["claims"].as_array().expect("claims array") {
                claims_checked += 1;
                let doc_id = claim["doc_id"].as_str().expect("doc_id");
                let locator = claim["locator"].as_str().expect("locator");
                assert!(
                    sealed.contains(doc_id),
                    "admitted claim cites outside the sealed context"
                );
                let (loc_doc, loc_off) = locator.split_once('@').expect("locator shape");
                assert_eq!(loc_doc, doc_id, "locator names the cited doc");
                let offset: usize = loc_off.parse().expect("byte offset");
                // The MockGenerator quotes the snippet's first line (<=40
                // chars); the full body must carry it verbatim AT the offset.
                let body = &world.bodies[doc_id];
                let sealed_doc = trace
                    .sealed
                    .iter()
                    .find(|d| d.doc_id == doc_id)
                    .expect("sealed doc");
                let quote: String = sealed_doc
                    .snippet
                    .chars()
                    .take_while(|c| *c != '\n')
                    .take(40)
                    .collect::<String>()
                    .trim_end()
                    .to_string();
                assert!(
                    body[offset..].starts_with(&quote),
                    "quote is not verbatim at the recorded locator ({locator})"
                );
            }
        }
    }
    assert!(
        claims_checked >= 20,
        "the battery actually exercised claims"
    );
}

// ---------------------------------------------------------------------------
// G-2 FABRICATED QUOTE -> REFUSED; ALL-REFUSED -> NO ANSWER, DISCLOSED
// ---------------------------------------------------------------------------

#[test]
fn g2_fabricated_quote_is_refused_and_the_drop_is_disclosed() {
    let state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::FabricatedQuote))),
        None,
    );
    let (envelope, trace) = ask_ok(&state, "p060", FINANCE_QUERY, &AskOptions::default());
    assert!(
        envelope.get("answer").is_none(),
        "no admitted claim -> no answer key"
    );
    assert_eq!(envelope["grounding_applied"], Value::Bool(true));
    assert_eq!(envelope["grounding"]["admitted"], Value::from(0));
    assert_eq!(envelope["grounding"]["refused"], Value::from(1));
    assert_eq!(envelope["generation_applied"], Value::Bool(false));
    assert!(!trace.generation_fault, "a refusal is not a format fault");
    assert!(
        !envelope["results"].as_array().expect("results").is_empty(),
        "retrieval still serves"
    );
}

// ---------------------------------------------------------------------------
// G-3 OUT-OF-CONTEXT CITATION -> REFUSED, EVEN WITH AN IN-SCOPE QUOTE
// ---------------------------------------------------------------------------

#[test]
fn g3_out_of_context_citation_never_anchors_even_on_a_real_in_scope_quote() {
    // The quote text verbatim-exists in the FIRST sealed doc; the claim
    // cites a doc that was never sealed. The anchor binds to the CITED doc.
    let state = state_with(
        Some(Arc::new(MockGenerator::new(
            GenBehavior::WrongSourceRealQuote,
        ))),
        None,
    );
    let (envelope, trace) = ask_ok(&state, "p060", FINANCE_QUERY, &AskOptions::default());
    assert!(envelope.get("answer").is_none());
    assert_eq!(envelope["grounding"]["admitted"], Value::from(0));
    assert_eq!(envelope["grounding"]["refused"], Value::from(1));
    assert!(!trace.generation_fault);
    let bytes = serde_json::to_string(&envelope).expect("serialize");
    assert!(
        !bytes.contains("d9999"),
        "the foreign id never reaches the envelope"
    );
}

// ---------------------------------------------------------------------------
// G-4 THE LOOKUP REFUSES UNKNOWN IDS (SEALED-ONLY, BY CONSTRUCTION)
// ---------------------------------------------------------------------------

#[test]
fn g4_a_quote_present_only_out_of_scope_can_never_anchor() {
    // ground() receives ONLY the sealed bodies — an out-of-scope document
    // does not exist as far as the gate is concerned, even when the claim
    // quotes it perfectly.
    let sealed: BTreeMap<&str, &str> = [("d_in", "the in-scope body text")].into();
    let claim = Claim {
        text: "A perfectly quoted out-of-scope fact.".to_string(),
        doc_id: "d_out".to_string(),
        quote: "the secret out-of-scope sentence".to_string(),
    };
    match ground(claim, &sealed, &AnchorOnly) {
        Grounded::Refused { reason, .. } => {
            assert_eq!(reason, "cited document is not in the sealed context");
        }
        Grounded::Admitted { .. } => panic!("an unknown id must refuse"),
    }
    // And the same quote CITED CORRECTLY but absent from the cited body
    // refuses on the verbatim rule.
    let claim = Claim {
        text: "A fact the cited source never states.".to_string(),
        doc_id: "d_in".to_string(),
        quote: "the secret out-of-scope sentence".to_string(),
    };
    match ground(claim, &sealed, &AnchorOnly) {
        Grounded::Refused { reason, .. } => {
            assert_eq!(reason, "quote not found verbatim in the cited source");
        }
        Grounded::Admitted { .. } => panic!("a non-verbatim quote must refuse"),
    }
    // Empty quote refuses; bracket smuggling refuses.
    let empty = Claim {
        text: "Empty anchor.".to_string(),
        doc_id: "d_in".to_string(),
        quote: "   ".to_string(),
    };
    assert!(matches!(
        ground(empty, &sealed, &AnchorOnly),
        Grounded::Refused {
            reason: "no extractive anchor (empty quote)",
            ..
        }
    ));
    let smuggler = Claim {
        text: "Fake cite [d_in] inside the sentence.".to_string(),
        doc_id: "d_in".to_string(),
        quote: "in-scope body".to_string(),
    };
    assert!(matches!(
        ground(smuggler, &sealed, &AnchorOnly),
        Grounded::Refused { .. }
    ));
}

// ---------------------------------------------------------------------------
// G-5 MALFORMED GENERATOR OUTPUT -> FORMAT FAULT -> RETRIEVAL-ONLY, 200
// ---------------------------------------------------------------------------

#[test]
fn g5_malformed_output_degrades_to_retrieval_only() {
    let malformed = [
        // Missing QUOTE line.
        "CLAIM: A fact.\nSOURCE: d0202",
        // Preamble prose before the block.
        "Here are my findings:\n\nCLAIM: A fact.\nSOURCE: d0202\nQUOTE: \"text\"",
        // Unquoted QUOTE.
        "CLAIM: A fact.\nSOURCE: d0202\nQUOTE: text without quotes",
        // Multi-token SOURCE.
        "CLAIM: A fact.\nSOURCE: d0202 and d0208\nQUOTE: \"text\"",
        // Empty output.
        "",
        // Seven claims (over the cap).
        &(0..7)
            .map(|i| format!("CLAIM: Fact {i}.\nSOURCE: d0202\nQUOTE: \"text\""))
            .collect::<Vec<_>>()
            .join("\n\n"),
    ];
    for output in malformed {
        assert!(parse_claims(output).is_err(), "must fault: {output:?}");
        let state = state_with(
            Some(Arc::new(MockGenerator::new(GenBehavior::Raw(
                output.to_string(),
            )))),
            None,
        );
        let (envelope, trace) = ask_ok(&state, "p060", FINANCE_QUERY, &AskOptions::default());
        assert!(envelope.get("answer").is_none());
        assert_eq!(envelope["generation_applied"], Value::Bool(false));
        assert_eq!(envelope["grounding_applied"], Value::Bool(false));
        assert!(trace.generation_fault, "format fault counted: {output:?}");
        assert!(
            !envelope["results"].as_array().expect("results").is_empty(),
            "retrieval-only response still serves results"
        );
    }
    // And the well-formed boundary parses: exactly 6 claims.
    let six = (0..6)
        .map(|i| format!("CLAIM: Fact {i}.\nSOURCE: d0202\nQUOTE: \"text\""))
        .collect::<Vec<_>>()
        .join("\n\n");
    assert_eq!(parse_claims(&six).expect("six claims parse").len(), 6);
}

// ---------------------------------------------------------------------------
// G-6 ADMITTED + REFUSED == PARSED DRAFT CLAIMS, ALWAYS
// ---------------------------------------------------------------------------

#[test]
fn g6_admitted_plus_refused_equals_the_parsed_draft() {
    let state = state_with(Some(Arc::new(MockGenerator::new(GenBehavior::Mixed))), None);
    let (envelope, trace) = ask_ok(&state, "p060", FINANCE_QUERY, &AskOptions::default());
    let admitted = envelope["grounding"]["admitted"]
        .as_u64()
        .expect("admitted");
    let refused = envelope["grounding"]["refused"].as_u64().expect("refused");
    assert_eq!(
        admitted + refused,
        u64::from(trace.draft_claims),
        "every parsed draft claim is either admitted or refused"
    );
    assert_eq!(
        refused, 2,
        "Mixed carries one fabricated + one foreign claim"
    );
    assert!(admitted >= 1, "Mixed carries groundable claims");
    // Disclosure and render agree: claims array length == admitted.
    assert_eq!(
        envelope["answer"]["claims"]
            .as_array()
            .expect("claims")
            .len() as u64,
        admitted
    );
    // The rendered text carries exactly one [citation] per admitted claim.
    let text = envelope["answer"]["text"].as_str().expect("text");
    assert_eq!(text.matches('[').count() as u64, admitted);
}

// ---------------------------------------------------------------------------
// G-7 NEW ENVELOPE KEYS ARE EXACTLY THE THREE NAMED SURFACES
// ---------------------------------------------------------------------------

#[test]
fn g7_grounded_envelope_carries_exactly_the_new_keys() {
    // A-7's whitelist gate covers "no unexpected key"; this pins PRESENCE
    // and SHAPE of the K1 additions on a grounded envelope.
    let state = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        None,
    );
    let (envelope, _) = ask_ok(&state, "p060", FINANCE_QUERY, &AskOptions::default());
    assert_eq!(envelope["grounding_applied"], Value::Bool(true));
    let grounding = envelope["grounding"].as_object().expect("grounding object");
    assert_eq!(
        grounding.keys().collect::<Vec<_>>(),
        ["admitted", "refused"],
        "grounding discloses counts and nothing else"
    );
    for claim in envelope["answer"]["claims"].as_array().expect("claims") {
        let keys: Vec<_> = claim.as_object().expect("claim object").keys().collect();
        assert_eq!(
            keys,
            ["doc_id", "locator", "text"],
            "claim carries no extras"
        );
    }
    // The no-generator envelope says false/absent — never why.
    let bare = state_with(None, None);
    let (envelope, _) = ask_ok(&bare, "p060", FINANCE_QUERY, &AskOptions::default());
    assert_eq!(envelope["grounding_applied"], Value::Bool(false));
    assert!(envelope.get("grounding").is_none());
}

// ---------------------------------------------------------------------------
// G-8 DETERMINISM: SAME ASK TWICE IS BYTE-IDENTICAL (CACHE AND COLD)
// ---------------------------------------------------------------------------

#[test]
fn g8_same_grounded_ask_twice_is_byte_identical_with_and_without_cache() {
    let cached = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        Some(Arc::new(AnswerCache::new())),
    );
    let options = AskOptions::default();
    let (bytes_one, trace_one) = ask(&cached, "p060", FINANCE_QUERY, &options).expect("ask");
    assert!(!trace_one.cache_hit);
    let (bytes_two, trace_two) = ask(&cached, "p060", FINANCE_QUERY, &options).expect("ask");
    assert!(trace_two.cache_hit, "second ask is served from the cache");
    assert_eq!(bytes_one, bytes_two);

    let uncached = state_with(
        Some(Arc::new(MockGenerator::new(GenBehavior::CiteEach))),
        None,
    );
    let (bytes_three, _) = ask(&uncached, "p060", FINANCE_QUERY, &options).expect("ask");
    let (bytes_four, _) = ask(&uncached, "p060", FINANCE_QUERY, &options).expect("ask");
    assert_eq!(bytes_three, bytes_four, "no cache: recomputed identically");
    assert_eq!(bytes_one, bytes_three, "cache on/off serve the same bytes");
}

// ---------------------------------------------------------------------------
// G-9 CONFIG WIRED + NO VECTOR ARM -> HONEST 400, NEVER 500
// ---------------------------------------------------------------------------

#[tokio::test]
async fn g9_hybrid_or_judge_without_vectors_is_an_honest_400() {
    let world = world();
    // Embedder + judge CONFIGURED, over the lexical-only index: the exact
    // deployment shape service/config.json creates before K1b's rebuild.
    let embeddings = Arc::new(
        FileEmbeddings::load(&[
            common::docs_embeddings_path().as_path(),
            common::query_embeddings_path().as_path(),
        ])
        .expect("load committed embeddings"),
    );
    let state = AppState::build(&world.fixtures_dir, &world.artifacts_dir, &world.idx_dir)
        .expect("build service state")
        .with_embedder(embeddings)
        .with_judge(Arc::new(MockJudge::new(JudgeBehavior::Identity)))
        .with_cache(None);

    // Library level: the exact refusal, as a BadRequest (never Internal).
    for (hybrid, judge) in [(true, false), (false, true), (true, true)] {
        let options = AskOptions {
            hybrid,
            judge,
            bypass_cache: false,
            granted_context: None,
        };
        match ask(&state, "p060", FINANCE_QUERY, &options) {
            Err(AskError::BadRequest(message)) => {
                assert_eq!(message, "the index carries no vector arm in this build");
            }
            Err(AskError::Internal(_)) => panic!("an absent capability must not be a 500"),
            Ok(_) => panic!("hybrid/judge without vectors must refuse"),
        }
    }

    // HTTP level: the router maps it to a 400 with the honest body.
    let router = app(Arc::new(state));
    let bearer = common::bearer(&router, "p060").await;
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ask")
                .header("content-type", "application/json")
                .header("authorization", &bearer)
                .body(Body::from(
                    serde_json::json!({ "query": FINANCE_QUERY, "hybrid": true }).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body = String::from_utf8(bytes.to_vec()).expect("utf8");
    assert!(
        body.contains("the index carries no vector arm in this build"),
        "the 400 names the missing capability: {body}"
    );

    // And a plain lexical ask over the same vectorless world still serves.
    let lexical = state_with(None, None);
    let (envelope, _) = ask_ok(&lexical, "p060", FINANCE_QUERY, &AskOptions::default());
    assert!(!envelope["results"].as_array().expect("results").is_empty());
}
