//! Regenerates the committed FULL-CORPUS embedding fixture (all 600
//! documents; provenance: the deterministic hash-projection in
//! `common::synthetic_embedding`, NOT a model). Run explicitly:
//!
//! ```sh
//! cargo test -p service --test fixture_gen -- --ignored
//! ```

mod common;

use std::collections::BTreeMap;
use std::fs;

use retrieval::index::sha256_hex;
use retrieval::vector::embedding_text;
use serde_json::{json, Value};

#[test]
#[ignore = "regenerates committed fixtures; run explicitly"]
fn regenerate_full_corpus_embeddings() {
    let full: Value = serde_json::from_slice(
        &fs::read(common::repo_fixtures_dir().join("documents.json")).expect("read documents"),
    )
    .expect("parse documents");

    let mut entries = BTreeMap::new();
    for doc in full["documents"].as_array().expect("documents array") {
        let text = embedding_text(
            doc["title"].as_str().expect("title"),
            doc["body"].as_str().expect("body"),
        );
        let hint: String = text.chars().take(40).collect();
        entries.insert(
            sha256_hex(text.as_bytes()),
            json!({ "hint": hint, "vector": common::synthetic_embedding(&text) }),
        );
    }
    assert_eq!(entries.len(), 600, "every document embedded");

    let file = json!({
        "dim": common::FIXTURE_DIM,
        "model_id": common::FIXTURE_MODEL_ID,
        "texts": entries,
    });
    fs::create_dir_all(common::service_fixture_dir()).expect("fixture dir");
    let path = common::docs_embeddings_path();
    fs::write(
        &path,
        retrieval::index::canonical_json_bytes(&file).expect("encode"),
    )
    .expect("write fixture");
    println!(
        "wrote {} ({} bytes)",
        path.display(),
        fs::metadata(&path).expect("meta").len()
    );
}
