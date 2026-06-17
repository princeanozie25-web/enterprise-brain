//! DoD #3 — provenance is required and present.
//!
//! Every claim on every page carries a non-empty source pointer. A claim
//! authored without provenance is not representable: `Claim::new` demands a
//! `Provenance`, and `Provenance::new` refuses blank components at run time.

mod common;

use common::{compile_artifacts, fixtures_dir, scratch};

use wiki::authz::AuthzView;
use wiki::provenance::{Provenance, ProvenanceError};
use wiki::{derive_all, Sources};

#[test]
fn provenance_new_refuses_unsourced_claims() {
    // The "author a claim without a source -> fails" path, at run time.
    assert_eq!(
        Provenance::new("", "p001", "/people/0", None).unwrap_err(),
        ProvenanceError::EmptySource
    );
    assert_eq!(
        Provenance::new("fixtures/people.json", "", "/people/0", None).unwrap_err(),
        ProvenanceError::EmptyRecord
    );
    assert_eq!(
        Provenance::new("fixtures/people.json", "p001", "   ", None).unwrap_err(),
        ProvenanceError::EmptyLocator
    );
}

#[test]
fn every_claim_in_the_layer_cites_a_source() {
    let artifacts = scratch("provenance_artifacts");
    compile_artifacts(&artifacts);
    let sources = Sources::load(&fixtures_dir()).expect("load sources");
    let authz = AuthzView::load(&artifacts).expect("load authz");
    let layer = derive_all(&sources, &authz);

    let mut claims_checked = 0usize;
    for page in layer.entity_pages().chain(std::iter::once(&layer.index)) {
        assert!(!page.claims.is_empty(), "page {} has no claims", page.id);
        for claim in &page.claims {
            let p = claim.provenance();
            assert!(
                !p.source.trim().is_empty(),
                "claim on {} has empty source",
                page.id
            );
            assert!(
                !p.record.trim().is_empty(),
                "claim on {} has empty record",
                page.id
            );
            assert!(
                !p.locator.trim().is_empty(),
                "claim on {} has empty locator",
                page.id
            );
            assert!(!p.cite().is_empty());
            claims_checked += 1;
        }
        // Every fail-closed flag is itself provenance-anchored.
        for d in &page.discrepancies {
            assert!(!d.provenance.cite().is_empty());
        }
    }
    assert!(
        claims_checked > 120,
        "the layer carries many sourced claims"
    );
}

#[test]
fn rendered_facts_each_show_their_cite() {
    let artifacts = scratch("provenance_render_artifacts");
    compile_artifacts(&artifacts);
    let out = scratch("provenance_render_out");
    wiki::generate(&fixtures_dir(), &artifacts, &out).expect("generate");

    // In every rendered page, each Facts bullet ends with a `src:` cite — so
    // the count of `src:` markers is at least the count of fact bullets.
    for rel in [
        "people/p001.md",
        "departments/finance.md",
        "projects/cap01.md",
    ] {
        let md = std::fs::read_to_string(out.join(rel)).unwrap();
        let facts = md
            .split("## Facts")
            .nth(1)
            .expect("a Facts section")
            .split("\n## ")
            .next()
            .unwrap();
        let bullets = facts.lines().filter(|l| l.starts_with("- ")).count();
        let cites = facts.matches("`src: ").count();
        assert!(bullets > 0, "{rel} has fact bullets");
        assert_eq!(bullets, cites, "{rel}: every fact bullet carries a cite");
    }
}

#[test]
fn governed_access_block_is_cited_to_the_compiled_model() {
    let artifacts = scratch("provenance_ga_artifacts");
    compile_artifacts(&artifacts);
    let out = scratch("provenance_ga_out");
    wiki::generate(&fixtures_dir(), &artifacts, &out).expect("generate");

    let md = std::fs::read_to_string(out.join("people/p001.md")).unwrap();
    // Isolate just the Governed-access block (up to the next heading), so the
    // fail-closed flags section below it is not counted.
    let ga = md
        .split("## Governed access")
        .nth(1)
        .expect("a governed-access section")
        .split("\n## ")
        .next()
        .unwrap();

    // The summary line cites the model, and every granted-doc bullet does too.
    assert!(
        ga.contains("`src: compiled-model#p001 (snapshot "),
        "summary line cites the compiled model"
    );
    let doc_bullets = ga.lines().filter(|l| l.starts_with("- `")).count();
    let doc_cites = ga.matches("`src: compiled-model#p001 /entries/").count();
    assert!(doc_bullets > 0, "p001 has granted-doc lines");
    assert_eq!(
        doc_bullets, doc_cites,
        "every granted-doc line cites the model entry"
    );
}
