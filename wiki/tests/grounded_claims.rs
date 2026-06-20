//! Slice-4 grounding — extractive anchoring + judge support-verification.
//!
//! These prove the claims the brief makes, deterministically (no Ollama):
//!   * CENTERPIECE — a claim whose quote is NOT verbatim in its cited in-scope
//!     source is REFUSED as unfounded, even when the judge WOULD confirm it.
//!     Anchoring is a hard deterministic gate, independent of the model.
//!   * The anchor is extractive: its locator is the REAL byte offset of the span
//!     in the cited source, and re-running yields byte-identical anchors.
//!   * A span that exists only in an OUT-OF-SCOPE document cannot anchor a claim
//!     citing an in-scope doc — the span must be in the CITED in-scope source.
//!   * Support is FAIL-CLOSED: an anchored claim the judge does not confirm is
//!     WITHHELD, never written. Same inputs, only the judge flips the outcome.
//!   * Grounding COMPOSES with compounding: the same gate runs in the slice-3
//!     answer flow (unfounded -> refused, unsupported -> withheld).

mod common;

use std::collections::BTreeSet;

use common::{
    compile_artifacts, fixtures_dir, scratch, verbatim_prefix, FakeVerifier, FixedSelector,
    RecordingSynthesizer,
};

use wiki::authz::{AuthzView, GrantOracle};
use wiki::compound::compound_answer;
use wiki::scope::{ScopeContext, ScopeGate};
use wiki::scoped::{derive_scope, Topic};
use wiki::synth::RawClaim;
use wiki::Sources;

const SALES: &str = "p091";
const HR: &str = "p088";

/// A span that is not in the corpus (used to simulate a hallucinated quote).
const FABRICATED: &str = "ZZZ_FABRICATED_SPAN_NOT_PRESENT_IN_ANY_SOURCE_ZZZ";

fn allowed_set(authz: &AuthzView, pid: &str) -> BTreeSet<String> {
    authz.allowed_documents(pid).into_iter().collect()
}

fn one_topic() -> Vec<Topic> {
    vec![Topic {
        label: "scope material".into(),
        query: "scope material".into(),
    }]
}

fn doc_body(sources: &Sources, id: &str) -> String {
    sources
        .documents
        .documents
        .iter()
        .find(|d| d.id == id)
        .map(|d| d.body.clone())
        .unwrap_or_default()
}

#[test]
fn hallucinated_quote_is_refused_unfounded_even_if_the_judge_would_confirm() {
    let artifacts = scratch("ground_hallucination_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();

    let gate = ScopeGate::load(&artifacts, SALES).unwrap();
    let head: Vec<String> = gate.allowed().iter().take(3).cloned().collect();
    let good = head[0].clone();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head };

    // Both claims cite an IN-SCOPE doc (so neither is stopped by the scope gate).
    // One carries a verbatim quote; the other a fabricated span.
    let good_c = good.clone();
    let synth = RecordingSynthesizer::new("adversary", move |s| {
        let real = s
            .iter()
            .find(|d| d.doc_id == good_c)
            .map(|d| verbatim_prefix(&d.text, 48))
            .unwrap_or_default();
        vec![
            RawClaim {
                text: "honest, grounded fact".into(),
                cited_doc_id: good_c.clone(),
                quote: real,
                about_principal: None,
            },
            RawClaim {
                text: "hallucinated fact".into(),
                cited_doc_id: good_c.clone(),
                quote: FABRICATED.into(),
                about_principal: None,
            },
        ]
    });

    // The judge would CONFIRM everything — proving the refusal is from anchoring,
    // not the model.
    let verifier = FakeVerifier::always();
    let layer = derive_scope(
        &sources,
        &ctx,
        &one_topic(),
        &selector,
        &synth,
        &verifier,
        &authz,
    )
    .unwrap();

    // The hallucinated claim is refused as UNFOUNDED and never written...
    assert_eq!(
        layer.refused_unfounded.len(),
        1,
        "the fabricated-quote claim is refused unfounded"
    );
    assert_eq!(layer.refused_unfounded[0].cited_doc_id, good);
    assert!(
        layer.refused_unfounded[0].reason.contains("verbatim"),
        "reason names the missing verbatim anchor: {}",
        layer.refused_unfounded[0].reason
    );
    assert!(
        !layer
            .claims
            .iter()
            .any(|c| c.claim.text().contains("hallucinated")),
        "no hallucinated claim is admitted"
    );
    // ...while the honestly-anchored claim lands, anchored to a real span.
    assert_eq!(layer.claims.len(), 1, "only the grounded claim is admitted");
    assert!(layer.claims[0].claim.text().contains("honest"));
    assert!(layer.claims[0].support.supported);
    assert!(!layer.claims[0].anchor.span_text.is_empty());
}

#[test]
fn extractive_anchor_locator_is_the_real_offset_and_is_deterministic() {
    let artifacts = scratch("ground_determinism_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();

    let build = || {
        let gate = ScopeGate::load(&artifacts, SALES).unwrap();
        let head: Vec<String> = gate.allowed().iter().take(4).cloned().collect();
        let ctx = ScopeContext::build(gate, &sources);
        let selector = FixedSelector { ids: head };
        let synth = RecordingSynthesizer::echo("fake-model");
        let verifier = FakeVerifier::always();
        derive_scope(
            &sources,
            &ctx,
            &one_topic(),
            &selector,
            &synth,
            &verifier,
            &authz,
        )
        .unwrap()
    };

    let layer = build();
    assert!(!layer.claims.is_empty(), "echo produces grounded claims");

    // Every anchor's locator carries the REAL byte offset of its span in the
    // cited source body — extractive, provable without any model.
    for c in &layer.claims {
        let cited = &c.claim.provenance().record;
        let body = doc_body(&sources, cited);
        let span = &c.anchor.span_text;
        let (src_ref, off_str) = c
            .anchor
            .locator
            .rsplit_once('@')
            .expect("locator is `<source_ref>@<offset>`");
        assert_eq!(src_ref, cited, "locator names the cited source");
        let offset: usize = off_str.parse().expect("offset is a number");
        assert_eq!(
            body.find(span.as_str()),
            Some(offset),
            "the locator offset is where the span actually occurs"
        );
        assert_eq!(
            &body[offset..offset + span.len()],
            span,
            "the source bytes at the offset ARE the anchored span"
        );
    }

    // Re-running yields byte-identical anchors — deterministic, no model.
    let again = build();
    let fp = |l: &wiki::ScopedLayer| -> Vec<(String, String, String)> {
        l.claims
            .iter()
            .map(|c| {
                (
                    c.claim.text().to_string(),
                    c.anchor.span_text.clone(),
                    c.anchor.locator.clone(),
                )
            })
            .collect()
    };
    assert_eq!(fp(&layer), fp(&again), "anchoring is deterministic");
}

#[test]
fn a_span_present_only_out_of_scope_cannot_anchor_an_in_scope_cite() {
    let artifacts = scratch("ground_oos_span_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();

    let a_sales = allowed_set(&authz, SALES);
    let a_hr = allowed_set(&authz, HR);
    let hr_only = a_hr
        .difference(&a_sales)
        .next()
        .cloned()
        .expect("an HR-exclusive doc exists");

    // A distinctive span lifted from the OUT-OF-SCOPE (HR-only) document.
    let hr_body = doc_body(&sources, &hr_only);
    let span: String = hr_body
        .chars()
        .skip(24)
        .take(64)
        .collect::<String>()
        .trim()
        .to_string();
    assert!(span.len() > 16, "the out-of-scope span is substantial");

    let gate = ScopeGate::load(&artifacts, SALES).unwrap();
    let head: Vec<String> = gate.allowed().iter().take(3).cloned().collect();
    let good = head[0].clone();
    let ctx = ScopeContext::build(gate, &sources);
    // Precondition: the out-of-scope span is NOT verbatim in the in-scope doc.
    assert!(
        !doc_body(&sources, &good).contains(&span),
        "precondition: the HR span is not coincidentally in the Sales doc"
    );
    let selector = FixedSelector { ids: head };

    // The model cites an IN-SCOPE doc but quotes the OUT-OF-SCOPE span — an
    // attempt to smuggle out-of-scope text through an in-scope citation.
    let good_c = good.clone();
    let span_c = span.clone();
    let synth = RecordingSynthesizer::new("adversary", move |_s| {
        vec![RawClaim {
            text: "smuggled via an out-of-scope span".into(),
            cited_doc_id: good_c.clone(),
            quote: span_c.clone(),
            about_principal: None,
        }]
    });
    let verifier = FakeVerifier::always();
    let layer = derive_scope(
        &sources,
        &ctx,
        &one_topic(),
        &selector,
        &synth,
        &verifier,
        &authz,
    )
    .unwrap();

    // The anchor must exist in the CITED in-scope source — it does not, so the
    // claim is refused unfounded and nothing is written.
    assert!(
        layer.claims.is_empty(),
        "no claim anchored to a foreign span"
    );
    assert_eq!(layer.refused_unfounded.len(), 1);
    assert_eq!(layer.refused_unfounded[0].cited_doc_id, good);
}

#[test]
fn anchored_but_judge_unconfirmed_is_withheld_fail_closed() {
    let artifacts = scratch("ground_withheld_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();

    let run = |verifier: &dyn wiki::Verifier| {
        let gate = ScopeGate::load(&artifacts, SALES).unwrap();
        let head: Vec<String> = gate.allowed().iter().take(3).cloned().collect();
        let ctx = ScopeContext::build(gate, &sources);
        let selector = FixedSelector { ids: head };
        let synth = RecordingSynthesizer::echo("fake-model");
        derive_scope(
            &sources,
            &ctx,
            &one_topic(),
            &selector,
            &synth,
            verifier,
            &authz,
        )
        .unwrap()
    };

    // Echo's quotes anchor verbatim, so the ONLY variable is the judge. With a
    // confirming judge the claims are admitted...
    let admitted = run(&FakeVerifier::always());
    assert!(
        !admitted.claims.is_empty(),
        "anchored + supported -> admitted"
    );
    let anchored = admitted.claims.len();

    // ...and with a denying judge the SAME anchored claims are withheld — none
    // written, all surfaced as withheld (fail-closed).
    let withheld = run(&FakeVerifier::never());
    assert!(
        withheld.claims.is_empty(),
        "fail-closed: an unconfirmed claim is never admitted"
    );
    assert_eq!(
        withheld.withheld.len(),
        anchored,
        "every anchored-but-unsupported claim is withheld"
    );
}

#[test]
fn grounding_composes_with_compounding() {
    let artifacts = scratch("ground_compound_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();
    let a_sales = allowed_set(&authz, SALES);

    let gate = ScopeGate::load(&artifacts, SALES).unwrap();
    let head: Vec<String> = gate.allowed().iter().take(3).cloned().collect();
    let good = head[0].clone();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head };

    // One grounded claim + one hallucinated claim, both citing an in-scope doc.
    let good_c = good.clone();
    let synth = RecordingSynthesizer::new("fake", move |s| {
        let real = s
            .iter()
            .find(|d| d.doc_id == good_c)
            .map(|d| verbatim_prefix(&d.text, 48))
            .unwrap_or_default();
        vec![
            RawClaim {
                text: "grounded compounded fact".into(),
                cited_doc_id: good_c.clone(),
                quote: real,
                about_principal: None,
            },
            RawClaim {
                text: "hallucinated compounded fact".into(),
                cited_doc_id: good_c.clone(),
                quote: FABRICATED.into(),
                about_principal: None,
            },
        ]
    });

    let mut allowed_of = std::collections::BTreeMap::new();
    allowed_of.insert(SALES.to_string(), a_sales);

    // With a confirming judge: the grounded claim lands, the hallucinated one is
    // refused unfounded — grounding runs INSIDE the compounding answer flow.
    let page = compound_answer(
        &sources,
        &ctx,
        "q",
        "q",
        &selector,
        &synth,
        &FakeVerifier::always(),
        &[],
        &allowed_of,
        0,
    )
    .unwrap();
    assert_eq!(page.claims.len(), 1, "only the grounded claim is admitted");
    assert!(page.claims[0].text().contains("grounded"));
    assert_eq!(
        page.refused_unfounded,
        vec![good.clone()],
        "the hallucinated compounded claim is refused unfounded"
    );

    // With a denying judge: the anchored claim is withheld too (fail-closed).
    let page2 = compound_answer(
        &sources,
        &ctx,
        "q",
        "q",
        &selector,
        &synth,
        &FakeVerifier::never(),
        &[],
        &allowed_of,
        1,
    )
    .unwrap();
    assert!(page2.claims.is_empty(), "fail-closed under a denying judge");
    assert_eq!(
        page2.withheld,
        vec![good.clone()],
        "the anchored-but-unsupported compounded claim is withheld"
    );
    assert_eq!(
        page2.refused_unfounded,
        vec![good],
        "the hallucinated claim is still refused unfounded under any judge"
    );
}
