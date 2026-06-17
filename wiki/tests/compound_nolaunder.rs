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

const SALES: &str = "p091";
const HR: &str = "p088";

fn allowed(authz: &AuthzView, p: &str) -> BTreeSet<String> {
    authz.allowed_documents(p).into_iter().collect()
}

fn raw_claim(text: &str, doc: &str) -> CompoundClaim {
    CompoundClaim::new(
        text,
        SourceRef::RawDoc {
            doc_id: doc.to_string(),
            span: "/documents/0/body".to_string(),
        },
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

    let mut store = CompoundStore::new();
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

    let mut store = CompoundStore::new();
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
    let mut store = CompoundStore::new();
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
