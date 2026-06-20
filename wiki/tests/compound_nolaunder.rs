//! Slice-3 no-laundering + eligibility, on real scope data.
//!
//! DoD #1 transitive closure stays in scope · #1-negative laundering refused on
//! write · #2 cross-scope refusal on the VERIFIED non-nested pair · #3
//! within-scope reuse eligible · #5 acyclic. Pure store logic — no Ollama.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{compile_artifacts, scratch};

use wiki::authz::{AuthzView, GrantOracle};
use wiki::compound::{CompoundClaim, CompoundStore, CompoundedPage, ScopeStamp, SourceRef};
use wiki::{Anchor, SupportVerdict};

const SALES: &str = "p091";
const HR: &str = "p088";

fn allowed(authz: &AuthzView, p: &str) -> BTreeSet<String> {
    authz.allowed_documents(p).into_iter().collect()
}

/// Placeholder grounding for store-logic tests: the no-laundering/eligibility
/// invariants are independent of the anchor/support payload.
fn anc(src: &str) -> Anchor {
    Anchor {
        source_ref: src.to_string(),
        span_text: "span".to_string(),
        locator: format!("{src}@0"),
    }
}
fn ok() -> SupportVerdict {
    SupportVerdict {
        supported: true,
        judge_model: "fake".to_string(),
    }
}

fn raw_claim(text: &str, doc: &str) -> CompoundClaim {
    CompoundClaim::new(
        text,
        SourceRef::RawDoc {
            doc_id: doc.to_string(),
            span: "/documents/0/body".to_string(),
        },
        anc(doc),
        ok(),
    )
}

fn page(id: &str, ord: u64, scope: &str, snap: &str, claims: Vec<CompoundClaim>) -> CompoundedPage {
    CompoundedPage {
        page_id: id.to_string(),
        ordinal: ord,
        stamp: ScopeStamp {
            principal_id: scope.to_string(),
            snapshot_hash: snap.to_string(),
        },
        question: "q".to_string(),
        model: "fake".to_string(),
        claims,
        rejected: vec![],
        refused_unfounded: vec![],
        withheld: vec![],
        mention_flags: vec![],
    }
}

fn amap(pairs: &[(&str, &BTreeSet<String>)]) -> BTreeMap<String, BTreeSet<String>> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), (*v).clone()))
        .collect()
}

#[test]
fn transitive_closure_stays_in_scope_centerpiece() {
    let artifacts = scratch("compound_closure_artifacts");
    compile_artifacts(&artifacts);
    let authz = AuthzView::load(&artifacts).unwrap();
    let snap = authz.snapshot_version().to_string();
    let a_sales = allowed(&authz, SALES);
    let docs: Vec<String> = a_sales.iter().take(3).cloned().collect();

    let mut store = CompoundStore::new(snap.clone());
    // Round 1: cites two in-scope Sales docs.
    store
        .add(
            page(
                "cp0000-p091",
                0,
                SALES,
                &snap,
                vec![raw_claim("a", &docs[0]), raw_claim("b", &docs[1])],
            ),
            &amap(&[(SALES, &a_sales)]),
        )
        .unwrap();
    // Round 2: cites the round-1 page (a hop) PLUS a third in-scope doc.
    store
        .add(
            page(
                "cp0001-p091",
                1,
                SALES,
                &snap,
                vec![
                    CompoundClaim::new(
                        "c",
                        SourceRef::CompoundedPage {
                            page_id: "cp0000-p091".to_string(),
                        },
                        anc("cp0000-p091"),
                        ok(),
                    ),
                    raw_claim("d", &docs[2]),
                ],
            ),
            &amap(&[(SALES, &a_sales)]),
        )
        .unwrap();

    // CENTERPIECE: every page's transitive closure is entirely within its scope.
    for p in store.pages() {
        let closure = store.transitive_raw_docs(&p.page_id).unwrap();
        assert!(!closure.is_empty());
        assert!(
            closure.iter().all(|d| a_sales.contains(d)),
            "page {} closure has out-of-scope docs",
            p.page_id
        );
    }
    // Compounding deepened: the round-2 closure reaches the round-1 raw docs.
    let c2 = store.transitive_raw_docs("cp0001-p091").unwrap();
    assert!(c2.contains(&docs[0]) && c2.contains(&docs[1]) && c2.contains(&docs[2]));
}

#[test]
fn laundering_page_is_refused_on_write() {
    let artifacts = scratch("compound_launder_artifacts");
    compile_artifacts(&artifacts);
    let authz = AuthzView::load(&artifacts).unwrap();
    let snap = authz.snapshot_version().to_string();
    let a_sales = allowed(&authz, SALES);
    let a_hr = allowed(&authz, HR);
    let hr_only = a_hr
        .difference(&a_sales)
        .next()
        .cloned()
        .expect("an HR-exclusive doc");

    let mut store = CompoundStore::new(snap.clone());
    // A Sales-stamped page citing an HR-only doc would launder -> refused.
    let err = store
        .add(
            page(
                "cp0000-p091",
                0,
                SALES,
                &snap,
                vec![raw_claim("leak", &hr_only)],
            ),
            &amap(&[(SALES, &a_sales)]),
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("no-laundering"), "rejected: {err}");
    assert_eq!(store.len(), 0, "the laundering page was not stored");
}

#[test]
fn non_nested_sales_hr_refuse_each_other_but_reuse_within_scope() {
    let artifacts = scratch("compound_xscope_artifacts");
    compile_artifacts(&artifacts);
    let authz = AuthzView::load(&artifacts).unwrap();
    let snap = authz.snapshot_version().to_string();
    let a_sales = allowed(&authz, SALES);
    let a_hr = allowed(&authz, HR);

    // DoD numbers: assert the pair is genuinely non-nested before asserting refusal.
    assert!(!a_sales.is_subset(&a_hr), "Sales ⊄ HR");
    assert!(!a_hr.is_subset(&a_sales), "HR ⊄ Sales");

    let sales_doc = a_sales.iter().next().unwrap().clone();
    let hr_doc = a_hr.iter().next().unwrap().clone();
    let allowed_of = amap(&[(SALES, &a_sales), (HR, &a_hr)]);
    let mut store = CompoundStore::new(snap.clone());
    store
        .add(
            page(
                "cp0000-p091",
                0,
                SALES,
                &snap,
                vec![raw_claim("s", &sales_doc)],
            ),
            &allowed_of,
        )
        .unwrap();
    store
        .add(
            page("cp0001-p088", 1, HR, &snap, vec![raw_claim("h", &hr_doc)]),
            &allowed_of,
        )
        .unwrap();

    let elig_hr: Vec<String> = store
        .eligible_for(&snap, &a_hr, &allowed_of)
        .iter()
        .map(|p| p.page_id.clone())
        .collect();
    // Cross-scope refusal: the Sales page is NOT eligible for an HR question.
    assert!(
        !elig_hr.contains(&"cp0000-p091".to_string()),
        "Sales page refused for HR"
    );
    // Within-scope reuse: the HR page IS eligible for an HR question.
    assert!(
        elig_hr.contains(&"cp0001-p088".to_string()),
        "HR page reusable within HR"
    );

    let elig_sales: Vec<String> = store
        .eligible_for(&snap, &a_sales, &allowed_of)
        .iter()
        .map(|p| p.page_id.clone())
        .collect();
    assert!(
        !elig_sales.contains(&"cp0001-p088".to_string()),
        "HR page refused for Sales"
    );
    assert!(
        elig_sales.contains(&"cp0000-p091".to_string()),
        "Sales page reusable within Sales"
    );
}

/// P1-b (CENTERPIECE): the store is pinned to ONE snapshot, so the `snap_S ==
/// snap_T` conjunct is enforced at the durable WRITE, not only on the read-side
/// eligibility filter. A page stamped for a different snapshot is refused, and a
/// later page therefore cannot cite a page from another snapshot. Before this fix
/// a `CompoundStore` could mix snapshots and admit a cross-snapshot citation.
#[test]
fn cross_snapshot_page_is_refused_on_write() {
    let a = BTreeSet::from(["d1".to_string()]);
    let allowed_of = amap(&[("S", &a)]);
    let mut store = CompoundStore::new("snapX");

    // Same-snapshot page admits — no regression for the normal single-snapshot run.
    store
        .add(
            page("cp0-S", 0, "S", "snapX", vec![raw_claim("a", "d1")]),
            &allowed_of,
        )
        .unwrap();
    assert_eq!(store.len(), 1);

    // A page stamped for a DIFFERENT snapshot is refused fail-closed, not stored.
    let err = store
        .add(
            page("cp1-S", 1, "S", "snapY", vec![raw_claim("b", "d1")]),
            &allowed_of,
        )
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("snapshot") && err.contains("pinned"),
        "cross-snapshot page refused at the write: {err}"
    );
    assert_eq!(store.len(), 1, "the cross-snapshot page was not stored");

    // And so the review's repro is impossible: a later in-snapshot page cannot
    // cite the cross-snapshot page, because that page was never admitted to the
    // store (the existing unknown-cite/acyclicity gate refuses it).
    let err2 = store
        .add(
            page(
                "cp2-S",
                2,
                "S",
                "snapX",
                vec![CompoundClaim::new(
                    "c",
                    SourceRef::CompoundedPage {
                        page_id: "cp1-S".to_string(),
                    },
                    anc("cp1-S"),
                    ok(),
                )],
            ),
            &allowed_of,
        )
        .unwrap_err()
        .to_string();
    assert!(
        err2.contains("unknown page"),
        "citing a never-stored cross-snapshot page is refused: {err2}"
    );
    assert_eq!(store.len(), 1, "no cross-snapshot citation was admitted");
}
