//! Slice-2 scope gating — the centerpiece security properties.
//!
//! DoD #1 cross-scope non-leakage, DoD #2 authorization-before-retrieval at
//! derivation, DoD #6 adversarial out-of-scope citation is refused. All run
//! with a recording fake synthesizer (no Ollama) and a fixed selector (no
//! tantivy), so the gate — not a live model — is what is under test.

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::{compile_artifacts, fixtures_dir, scratch, FixedSelector, RecordingSynthesizer};

use wiki::authz::{AuthzView, GrantOracle};
use wiki::scope::{ScopeContext, ScopeGate};
use wiki::scoped::{derive_scope, Topic};
use wiki::synth::RawClaim;
use wiki::{ScopedLayer, Sources};

const SALES: &str = "p091"; // Samir Suzuki, Head of Sales & Accounts
const HR: &str = "p088"; //    Tomas Reyes, HR Systems Administrator

fn allowed_set(authz: &AuthzView, pid: &str) -> BTreeSet<String> {
    authz.allowed_documents(pid).into_iter().collect()
}

fn echo_layer(artifacts: &Path, sources: &Sources, authz: &AuthzView, pid: &str) -> ScopedLayer {
    let gate = ScopeGate::load(artifacts, pid).expect("load scope");
    let head: Vec<String> = gate.allowed().iter().take(6).cloned().collect();
    let ctx = ScopeContext::build(gate, sources);
    let selector = FixedSelector { ids: head };
    let synth = RecordingSynthesizer::echo("fake-model");
    let topics = vec![Topic {
        label: "scope material".into(),
        query: "scope material".into(),
    }];
    derive_scope(sources, &ctx, &topics, &selector, &synth, authz).expect("derive scope")
}

#[test]
fn nonleakage_sales_and_hr_layers_are_disjoint_by_source() {
    let artifacts = scratch("scoped_nonleak_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();

    let a_sales = allowed_set(&authz, SALES);
    let a_hr = allowed_set(&authz, HR);
    let hr_only: BTreeSet<_> = a_hr.difference(&a_sales).cloned().collect();
    let sales_only: BTreeSet<_> = a_sales.difference(&a_hr).cloned().collect();
    assert!(
        !hr_only.is_empty() && !sales_only.is_empty(),
        "the two scopes genuinely diverge"
    );

    let sales = echo_layer(&artifacts, &sources, &authz, SALES);
    let hr = echo_layer(&artifacts, &sources, &authz, HR);
    assert!(!sales.claims.is_empty() && !hr.claims.is_empty());

    let sales_cited = sales.cited_docs();
    let hr_cited = hr.cited_docs();

    // CENTERPIECE: every cited source is inside the deriving scope, and neither
    // layer cites the other scope's exclusive material. The Sales-derived layer
    // contains zero HR-sourced knowledge, and vice versa.
    assert!(
        sales_cited.iter().all(|d| a_sales.contains(d)),
        "every Sales claim cites an in-scope doc"
    );
    assert!(
        sales_cited.is_disjoint(&hr_only),
        "no HR-exclusive document appears in the Sales layer"
    );
    assert!(
        hr_cited.iter().all(|d| a_hr.contains(d)),
        "every HR claim cites an in-scope doc"
    );
    assert!(
        hr_cited.is_disjoint(&sales_only),
        "no Sales-exclusive document appears in the HR layer"
    );
}

#[test]
fn doc_set_handed_to_model_equals_oracle_allowed_set_and_never_exceeds_it() {
    let artifacts = scratch("scoped_auth_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();

    let gate = ScopeGate::load(&artifacts, HR).unwrap();
    let oracle = allowed_set(&authz, HR);

    // (1) The scope universe loaded by retrieval == the oracle's allowed set,
    //     exactly. Two independent loaders (PrincipalScope, AuthzView) agree.
    assert_eq!(
        *gate.allowed(),
        oracle,
        "retrieval scope universe == oracle allowed set, exactly"
    );

    // (2) After derivation, the recorder proves the model received ONLY in-scope
    //     documents — zero outside the scope ever reached it.
    let head: Vec<String> = gate.allowed().iter().take(6).cloned().collect();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head };
    let synth = RecordingSynthesizer::echo("fake-model");
    let topics = vec![Topic {
        label: "t".into(),
        query: "t".into(),
    }];
    let _layer = derive_scope(&sources, &ctx, &topics, &selector, &synth, &authz).unwrap();

    let received = synth.received_ids();
    assert!(!received.is_empty(), "the model was handed some documents");
    assert!(
        received.is_subset(&oracle),
        "every document handed to the model is in the oracle's allowed set"
    );
    assert!(
        received.difference(&oracle).next().is_none(),
        "zero out-of-scope documents reached the model"
    );
}

#[test]
fn adversarial_out_of_scope_citation_is_refused() {
    let artifacts = scratch("scoped_adversarial_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).unwrap();
    let authz = AuthzView::load(&artifacts).unwrap();

    let a_sales = allowed_set(&authz, SALES);
    let a_hr = allowed_set(&authz, HR);
    let out_of_scope = a_hr
        .difference(&a_sales)
        .next()
        .cloned()
        .expect("an HR-exclusive doc exists");

    let gate = ScopeGate::load(&artifacts, SALES).unwrap();
    let head: Vec<String> = gate.allowed().iter().take(3).cloned().collect();
    let honest = head[0].clone();
    let ctx = ScopeContext::build(gate, &sources);
    let selector = FixedSelector { ids: head };

    // A scope-S synthesizer that tries to cite an out-of-scope (HR) document,
    // plus one honest in-scope claim.
    let oos = out_of_scope.clone();
    let honest_id = honest.clone();
    let synth = RecordingSynthesizer::new("adversary", move |_sources| {
        vec![
            RawClaim {
                text: "smuggle HR knowledge".into(),
                cited_doc_id: oos.clone(),
                about_principal: None,
            },
            RawClaim {
                text: "honest in-scope fact".into(),
                cited_doc_id: honest_id.clone(),
                about_principal: None,
            },
        ]
    });
    let topics = vec![Topic {
        label: "t".into(),
        query: "t".into(),
    }];
    let layer = derive_scope(&sources, &ctx, &topics, &selector, &synth, &authz).unwrap();

    // The out-of-scope citation was REFUSED — recorded, never written.
    assert!(
        layer
            .rejected
            .iter()
            .any(|r| r.cited_doc_id == out_of_scope),
        "the out-of-scope citation was refused"
    );
    assert!(
        !layer.cited_docs().contains(&out_of_scope),
        "the out-of-scope document never appears as a source in the layer"
    );
    assert!(
        layer.cited_docs().iter().all(|d| a_sales.contains(d)),
        "every written claim cites an in-scope document"
    );
    // The honest claim still landed.
    assert!(
        layer
            .claims
            .iter()
            .any(|c| c.claim.provenance().record == honest),
        "the honest in-scope claim survived"
    );
}
