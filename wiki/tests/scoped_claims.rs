//! Slice-2 per-claim gates: provenance (DoD #4) and fail-closed (DoD #5).

mod common;

use std::collections::BTreeSet;

use common::{compile_artifacts, fixtures_dir, scratch, FixedSelector, RecordingSynthesizer};

use wiki::authz::{AuthzView, GrantOracle};
use wiki::scope::{ScopeContext, ScopeGate};
use wiki::scoped::{derive_scope, Topic};
use wiki::synth::RawClaim;
use wiki::Sources;

const BROAD: &str = "p060"; // Felix Osei, Head of Finance / board — broadest scope
const HR: &str = "p088";

fn allowed_set(authz: &AuthzView, pid: &str) -> BTreeSet<String> {
    authz.allowed_documents(pid).into_iter().collect()
}

#[test]
fn unsourced_or_unknown_citation_is_not_written() {
    let artifacts = scratch("scoped_prov_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();

    let gate = ScopeGate::load(&artifacts, BROAD).unwrap();
    let head: Vec<String> = gate.allowed().iter().take(3).cloned().collect();
    let in_scope = head[0].clone();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head };

    let good = in_scope.clone();
    let synth = RecordingSynthesizer::new("fake-model", move |_s| {
        vec![
            // Admitted: cites an in-scope source.
            RawClaim {
                text: "valid".into(),
                cited_doc_id: good.clone(),
                about_principal: None,
            },
            // Refused: empty citation (unsourced).
            RawClaim {
                text: "no source".into(),
                cited_doc_id: String::new(),
                about_principal: None,
            },
            // Refused: cites a document that does not exist / was not provided.
            RawClaim {
                text: "hallucinated".into(),
                cited_doc_id: "d999999".into(),
                about_principal: None,
            },
        ]
    });
    let topics = vec![Topic {
        label: "t".into(),
        query: "t".into(),
    }];
    let layer = derive_scope(&sources, &ctx, &topics, &selector, &synth, &authz).unwrap();

    // Exactly the one in-scope claim is written; every written claim cites a
    // non-empty, in-scope source.
    assert_eq!(layer.claims.len(), 1, "only the sourced claim is written");
    assert_eq!(layer.claims[0].claim.provenance().record, in_scope);
    assert!(layer
        .claims
        .iter()
        .all(|c| !c.claim.provenance().record.trim().is_empty()));
    // The unsourced and hallucinated claims were refused.
    assert_eq!(layer.rejected.len(), 2, "unsourced + unknown both refused");
    assert!(layer.rejected.iter().any(|r| r.cited_doc_id.is_empty()));
    assert!(layer.rejected.iter().any(|r| r.cited_doc_id == "d999999"));
}

#[test]
fn llm_inferred_ungranted_association_is_flagged_not_widened() {
    let artifacts = scratch("scoped_failclosed_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();

    // A document the broad scope can see but the HR principal cannot.
    let a_broad = allowed_set(&authz, BROAD);
    let a_hr = allowed_set(&authz, HR);
    let doc = a_broad
        .difference(&a_hr)
        .next()
        .cloned()
        .expect("a doc in the broad scope but outside HR");
    assert!(
        authz.why_allowed(HR, &doc).is_none(),
        "precondition: HR is not granted this doc"
    );

    let gate = ScopeGate::load(&artifacts, BROAD).unwrap();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector {
        ids: vec![doc.clone()],
    };

    // The LLM, deriving for the broad scope from an in-scope doc, infers that
    // the HR principal is associated with it — an access the model denies.
    let d = doc.clone();
    let synth = RecordingSynthesizer::new("fake-model", move |_s| {
        vec![RawClaim {
            text: "this concerns the HR administrator".into(),
            cited_doc_id: d.clone(),
            about_principal: Some(HR.to_string()),
        }]
    });
    let topics = vec![Topic {
        label: "t".into(),
        query: "t".into(),
    }];
    let layer = derive_scope(&sources, &ctx, &topics, &selector, &synth, &authz).unwrap();

    // The association is FLAGGED, fail-closed.
    let flag = layer
        .discrepancies
        .iter()
        .find(|f| f.principal_id == HR && f.document_id == doc)
        .expect("the ungranted association is flagged");
    assert!(flag.detail.contains("NOT widened"));

    // Access is verifiably NOT widened: the authz model still denies HR the doc,
    // and HR's granted set still excludes it. (AuthzView is read-only.)
    assert!(
        authz.why_allowed(HR, &doc).is_none(),
        "HR still denied the doc"
    );
    assert!(
        !allowed_set(&authz, HR).contains(&doc),
        "HR's granted set was not widened"
    );
}
