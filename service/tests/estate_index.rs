//! S5a: the estate retrieval index — determinism and EB-5 candidacy (the
//! authority predicate is a MUST clause on candidate formation, never a
//! post-filter). Unit-level, over the real fixture estate.

mod common;

use std::path::PathBuf;

use serde_json::Value;
use service::estate::EstateIndex;

fn estate_dir() -> PathBuf {
    common::repo_fixtures_dir().join("estate")
}

/// Build an index over the second source (deterministic, self-contained).
fn s3_entries() -> Vec<(String, String, String, String, String)> {
    let access = std::fs::read_to_string(estate_dir().join("s3-access.json")).expect("s3-access");
    let parsed: Value = serde_json::from_str(&access).expect("parse");
    parsed["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            let doc_id = o["doc_id"].as_str().unwrap().to_string();
            let rel = doc_id.strip_prefix("s3/").unwrap();
            let body =
                std::fs::read_to_string(estate_dir().join("s3-store").join(rel)).expect("body");
            let title = body
                .lines()
                .next()
                .unwrap_or("")
                .trim_start_matches("# ")
                .to_string();
            (
                doc_id,
                title,
                body,
                o["sensitivity"].as_str().unwrap().to_string(),
                "s3".to_string(),
            )
        })
        .collect()
}

#[test]
fn index_build_is_deterministic() {
    let a = EstateIndex::build(s3_entries());
    let b = EstateIndex::build(s3_entries());
    // Same corpus -> same candidacy for the same query and authority.
    let all_ok = |_: &str| true;
    let ra: Vec<&str> = a
        .retrieve("supplier audit cold chain", 50, all_ok)
        .iter()
        .map(|c| c.doc_id)
        .collect();
    let rb: Vec<&str> = b
        .retrieve("supplier audit cold chain", 50, all_ok)
        .iter()
        .map(|c| c.doc_id)
        .collect();
    assert_eq!(
        ra, rb,
        "the index build is deterministic (same candidacy + order)"
    );
    assert_eq!(a.doc_count(), 150);
}

#[test]
fn authority_is_a_must_clause_on_candidacy_not_a_post_filter() {
    let index = EstateIndex::build(s3_entries());
    // "counterparty"/"commercial" appear ONLY in the confidential objects'
    // restricted-circulation section; "supplier audit" matches all tiers.
    let query = "counterparty commercial supplier audit";

    // An internal-tier authority admits public+internal, excludes confidential.
    let internal_ok = |sensitivity: &str| matches!(sensitivity, "public" | "internal");
    let internal: Vec<&str> = index
        .retrieve(query, 100, internal_ok)
        .iter()
        .map(|c| c.doc_id)
        .collect();
    // A confidential-tier authority admits all three.
    let confidential_ok =
        |sensitivity: &str| matches!(sensitivity, "public" | "internal" | "confidential");
    let confidential: Vec<&str> = index
        .retrieve(query, 100, confidential_ok)
        .iter()
        .map(|c| c.doc_id)
        .collect();

    // Every internal-tier candidate is in a public/internal bucket — a
    // confidential object is NEVER admitted (candidacy, not post-filter).
    for id in &internal {
        assert!(
            !id.starts_with("s3/finance-restricted/"),
            "internal tier surfaced a confidential object {id}"
        );
    }
    // The confidential tier surfaces MORE (the confidential objects the
    // internal tier could never see) — proof the predicate constrains
    // candidacy, not merely the returned slice.
    assert!(
        confidential.len() > internal.len(),
        "confidential authority admits strictly more candidates"
    );
    let extra: Vec<&&str> = confidential
        .iter()
        .filter(|id| !internal.contains(id))
        .collect();
    assert!(
        extra
            .iter()
            .all(|id| id.starts_with("s3/finance-restricted/")),
        "the extra candidates are exactly the confidential objects"
    );
}

#[test]
fn gibberish_query_yields_no_candidates() {
    let index = EstateIndex::build(s3_entries());
    let out = index.retrieve("zzxqv wplkjh vrtnq", 50, |_| true);
    assert!(out.is_empty(), "no posting list -> no candidates");
}
