//! Regenerates the committed embedding fixtures (provenance: the
//! deterministic hash-projection in `common::synthetic_embedding`, NOT a
//! model — see README). Run explicitly:
//!
//! ```sh
//! cargo test -p retrieval --test fixture_gen -- --ignored
//! ```

mod common;

use std::collections::BTreeMap;
use std::fs;

use retrieval::index::sha256_hex;
use retrieval::vector::embedding_text;
use serde_json::{json, Value};

fn fixture_file(entries: BTreeMap<String, Value>) -> Value {
    json!({
        "dim": common::FIXTURE_DIM,
        "model_id": common::FIXTURE_MODEL_ID,
        "texts": entries,
    })
}

fn entry(text: &str) -> (String, Value) {
    let hint: String = text.chars().take(40).collect();
    (
        sha256_hex(text.as_bytes()),
        json!({ "hint": hint, "vector": common::synthetic_embedding(text) }),
    )
}

#[test]
#[ignore = "regenerates committed fixtures; run explicitly"]
fn regenerate_embedding_fixtures() {
    let full: Value = serde_json::from_slice(
        &fs::read(common::repo_fixtures_dir().join("documents.json")).expect("read documents"),
    )
    .expect("parse documents");
    let mut by_id: BTreeMap<&str, &Value> = BTreeMap::new();
    for doc in full["documents"].as_array().expect("documents array") {
        by_id.insert(doc["id"].as_str().expect("id"), doc);
    }

    let mut doc_entries = BTreeMap::new();
    for id in common::SUBSET_DOC_IDS {
        let doc = by_id.get(id).unwrap_or_else(|| panic!("{id} missing"));
        let text = embedding_text(
            doc["title"].as_str().expect("title"),
            doc["body"].as_str().expect("body"),
        );
        let (sha, value) = entry(&text);
        doc_entries.insert(sha, value);
    }
    let mut query_entries = BTreeMap::new();
    for query in common::QUERY_TEXTS {
        let (sha, value) = entry(query);
        query_entries.insert(sha, value);
    }

    fs::create_dir_all(common::embedding_fixture_dir()).expect("fixture dir");
    let (docs_path, queries_path) = common::embedding_fixture_paths();
    fs::write(
        &docs_path,
        retrieval::index::canonical_json_bytes(&fixture_file(doc_entries)).expect("encode"),
    )
    .expect("write docs fixture");
    fs::write(
        &queries_path,
        retrieval::index::canonical_json_bytes(&fixture_file(query_entries)).expect("encode"),
    )
    .expect("write queries fixture");
    println!(
        "wrote {} ({} bytes) and {} ({} bytes)",
        docs_path.display(),
        fs::metadata(&docs_path).expect("meta").len(),
        queries_path.display(),
        fs::metadata(&queries_path).expect("meta").len()
    );
}
