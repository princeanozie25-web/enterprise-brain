//! Slice-3 answer flow: typed provenance, refusal, and scoped-derivation
//! preserved (the model sees only in-scope material). Uses the slice-2 fakes.

mod common;

use std::collections::BTreeSet;

use common::{compile_artifacts, fixtures_dir, scratch, FixedSelector, RecordingSynthesizer};

use wiki::authz::{AuthzView, GrantOracle};
use wiki::compound::{compound_answer, CompoundClaim, CompoundedPage, ScopeStamp, SourceRef};
use wiki::scope::{ScopeContext, ScopeGate};
use wiki::synth::RawClaim;
use wiki::Sources;

const SALES: &str = "p091";

#[test]
fn answer_types_cites_refuses_out_of_scope_and_only_sees_in_scope_material() {
    let artifacts = scratch("compound_answer_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();
    let a_sales: BTreeSet<String> = authz.allowed_documents(SALES).into_iter().collect();

    let gate = ScopeGate::load(&artifacts, SALES).unwrap();
    let head: Vec<String> = gate.allowed().iter().take(3).cloned().collect();
    let good = head[0].clone();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head.clone() };

    // An eligible prior compounded page (scope Sales) offered as a source.
    let prior = CompoundedPage {
        page_id: "cp0000-p091".to_string(),
        ordinal: 0,
        stamp: ScopeStamp {
            principal_id: SALES.to_string(),
            snapshot_hash: authz.snapshot_version().to_string(),
        },
        question: "q1".to_string(),
        model: "fake".to_string(),
        claims: vec![CompoundClaim::new(
            "prior",
            SourceRef::RawDoc {
                doc_id: good.clone(),
                span: "/documents/0/body".to_string(),
            },
        )],
        rejected: vec![],
    };
    let eligible: Vec<&CompoundedPage> = vec![&prior];

    // Fake cites: an in-scope raw doc, the prior page, and a bogus id.
    let good_c = good.clone();
    let synth = RecordingSynthesizer::new("fake", move |_s| {
        vec![
            RawClaim {
                text: "from raw".into(),
                cited_doc_id: good_c.clone(),
                about_principal: None,
            },
            RawClaim {
                text: "from prior".into(),
                cited_doc_id: "cp0000-p091".into(),
                about_principal: None,
            },
            RawClaim {
                text: "bogus".into(),
                cited_doc_id: "d999999".into(),
                about_principal: None,
            },
        ]
    });

    let mut allowed_of = std::collections::BTreeMap::new();
    allowed_of.insert(SALES.to_string(), a_sales.clone());
    let page = compound_answer(
        &sources,
        &ctx,
        "q2",
        "q2",
        &selector,
        &synth,
        &eligible,
        &allowed_of,
        1,
    )
    .unwrap();

    // Typed provenance: the raw cite -> RawDoc, the page cite -> CompoundedPage.
    assert_eq!(page.claims.len(), 2, "two valid cites admitted");
    assert!(page
        .claims
        .iter()
        .any(|c| matches!(c.source(), SourceRef::RawDoc { doc_id, .. } if doc_id == &good)));
    assert!(page.claims.iter().any(
        |c| matches!(c.source(), SourceRef::CompoundedPage { page_id } if page_id == "cp0000-p091")
    ));
    // The bogus (out-of-provided) cite was refused.
    assert_eq!(page.rejected, vec!["d999999".to_string()]);
    // Scope stamp carries the asking scope + snapshot.
    assert_eq!(page.stamp.principal_id, SALES);
    assert_eq!(page.stamp.snapshot_hash, authz.snapshot_version());

    // DoD #7: every RAW document the model saw is in-scope; the only non-raw
    // source offered was the eligible (in-scope-derived) compounded page.
    let received = synth.received_ids();
    let raw_received: BTreeSet<String> = received
        .iter()
        .filter(|id| id.starts_with('d'))
        .cloned()
        .collect();
    assert!(
        raw_received.is_subset(&a_sales),
        "every raw doc handed to the model is within the Sales scope"
    );
    assert!(
        received.contains("cp0000-p091"),
        "the eligible compounded page was the only non-raw source"
    );
}

#[test]
fn ineligible_page_is_never_fed_to_the_model() {
    let artifacts = scratch("compound_selfgate_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();
    let a_sales: BTreeSet<String> = authz.allowed_documents(SALES).into_iter().collect();

    let gate = ScopeGate::load(&artifacts, SALES).unwrap();
    let head: Vec<String> = gate.allowed().iter().take(2).cloned().collect();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head };

    // An INELIGIBLE prior page: a FOREIGN snapshot (so is_eligible is false even
    // though its scope would be a subset). The caller wrongly offers it anyway.
    let prior = CompoundedPage {
        page_id: "cp0000-p091".to_string(),
        ordinal: 0,
        stamp: ScopeStamp {
            principal_id: SALES.to_string(),
            snapshot_hash: "OLD-SNAPSHOT".to_string(),
        },
        question: "q1".to_string(),
        model: "fake".to_string(),
        claims: vec![CompoundClaim::new(
            "stale",
            SourceRef::RawDoc {
                doc_id: "d0001".to_string(),
                span: "/documents/0/body".to_string(),
            },
        )],
        rejected: vec![],
    };
    let eligible: Vec<&CompoundedPage> = vec![&prior];
    let mut allowed_of = std::collections::BTreeMap::new();
    allowed_of.insert(SALES.to_string(), a_sales);

    // The fake tries to cite the (ineligible) prior page.
    let synth = RecordingSynthesizer::new("fake", move |_s| {
        vec![RawClaim {
            text: "from stale prior".into(),
            cited_doc_id: "cp0000-p091".into(),
            about_principal: None,
        }]
    });
    let page = compound_answer(
        &sources,
        &ctx,
        "q2",
        "q2",
        &selector,
        &synth,
        &eligible,
        &allowed_of,
        1,
    )
    .unwrap();

    // The self-gate filtered the ineligible page: it was NEVER fed to the model,
    // so the model could not read its summary text...
    assert!(
        !synth.received_ids().contains("cp0000-p091"),
        "an ineligible page is never handed to the model"
    );
    // ...and the attempt to cite it is refused (it was never a provided source).
    assert!(
        page.claims
            .iter()
            .all(|c| !matches!(c.source(), SourceRef::CompoundedPage { .. })),
        "no compounded-page cite admitted from an ineligible page"
    );
    assert!(
        page.rejected.contains(&"cp0000-p091".to_string()),
        "the stale-page cite was refused"
    );
}
