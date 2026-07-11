//! S5c: the connector conformance kit, proven on real connectors.
//!
//! REALITY NOTE (reported in the S5c closeout): the estate module's prose
//! has always described "the json-corpus refactor of the primary load" as a
//! seed connector, but no `JsonCorpusConnector` TYPE exists in the engine —
//! the primary corpus load predates the seam and never went through
//! `SourceConnector`. So this suite proves the kit on (a) the REAL
//! `FsBucketConnector` against the REAL fixture estate and its REAL pinned
//! hash, and (b) a test-local `JsonCorpusConnector` — fixture code in THIS
//! file, reading `fixtures/documents.json` through the connector seam — to
//! show the kit generalizes beyond filesystem stores. The engine is
//! untouched.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use service::conformance_kit::{poison_probe, run_kit, KitExpectations, PoisonedConnector};
use service::estate::{ingest, FsBucketConnector, RawObject, SourceConnector};

fn estate_dir() -> PathBuf {
    common::repo_fixtures_dir().join("estate")
}

/// The REAL pin from the access model — the kit must round-trip to exactly
/// this value over the real store.
fn pinned_hash() -> String {
    let access = std::fs::read_to_string(estate_dir().join("s3-access.json")).expect("s3-access");
    let parsed: Value = serde_json::from_str(&access).expect("parse");
    parsed["content_sha256"].as_str().expect("pin").to_string()
}

// -- 1. The kit passes the REAL fs_bucket connector over the REAL fixture
//       estate, reproducing the access model's pinned hash exactly.
#[test]
fn kit_passes_fs_bucket_against_the_real_pin() {
    let connector = FsBucketConnector::new("s3", &estate_dir().join("s3-store"));
    let report = run_kit(
        &connector,
        &KitExpectations {
            object_count: Some(150),
            content_sha256: Some(pinned_hash()),
            doc_id_prefix: Some("s3/".to_string()),
        },
    );
    assert!(
        report.all_ok(),
        "fs_bucket certification:\n{}",
        report.to_human()
    );
    // The report names every clause (a PR pastes this).
    let human = report.to_human();
    for name in [
        "authority.bytes_only",
        "enumerate.determinism",
        "hash.round_trip",
        "object.count",
        "ingest-time-only",
    ] {
        assert!(human.contains(name), "report names {name}:\n{human}");
    }
}

// -- 2. The kit generalizes: a json-corpus connector (test-local fixture
//       code) reading fixtures/documents.json through the seam.
struct JsonCorpusConnector {
    path: PathBuf,
}

impl SourceConnector for JsonCorpusConnector {
    fn source_id(&self) -> &str {
        "json_corpus"
    }
    fn enumerate(&self) -> anyhow::Result<Vec<RawObject>> {
        let raw = std::fs::read_to_string(&self.path)?;
        let parsed: Value = serde_json::from_str(&raw)?;
        let mut objects: Vec<RawObject> = parsed["documents"]
            .as_array()
            .expect("documents array")
            .iter()
            .map(|d| {
                let mut native_meta = BTreeMap::new();
                native_meta.insert("mime".to_string(), "text/markdown".to_string());
                RawObject {
                    native_key: d["id"].as_str().expect("id").to_string(),
                    bytes: d["body"].as_str().expect("body").as_bytes().to_vec(),
                    native_meta,
                }
            })
            .collect();
        objects.sort_by(|a, b| a.native_key.cmp(&b.native_key));
        Ok(objects)
    }
}

#[test]
fn kit_passes_a_json_corpus_connector() {
    let connector = JsonCorpusConnector {
        path: common::repo_fixtures_dir().join("documents.json"),
    };
    let report = run_kit(
        &connector,
        &KitExpectations {
            object_count: Some(600),
            content_sha256: None, // the primary pin is the M1 manifest's law, not this formula's
            doc_id_prefix: None,
        },
    );
    assert!(
        report.all_ok(),
        "json_corpus certification:\n{}",
        report.to_human()
    );
}

// -- 3. The poisoned fixture FAILS, for the right reason — the kit proves
//       the engine's authority guard fires, live, in every certification.
#[test]
fn poisoned_connector_is_refused_for_the_right_reason() {
    // Through the probe (the certification path)…
    let check = poison_probe();
    assert!(check.ok, "the probe demands refusal: {}", check.detail);
    assert!(
        check.detail.contains("REFUSED"),
        "the probe reports the refusal: {}",
        check.detail
    );
    // …and straight through ingest, asserting the named reason. (No
    // `expect_err`: RawObject deliberately derives nothing.)
    let Err(refusal) = ingest(&PoisonedConnector) else {
        panic!("ingest must refuse the poisoned connector");
    };
    let msg = format!("{refusal:#}");
    assert!(
        msg.contains("acl") && msg.contains("authority lives in the access model"),
        "the refusal names the smuggled key and the law: {msg}"
    );
    // And a poisoned source run through the kit proper yields exactly one ✗.
    let report = run_kit(&PoisonedConnector, &KitExpectations::default());
    assert!(!report.all_ok(), "the kit fails a smuggling connector");
    assert!(
        report.checks.len() == 1 && !report.checks[0].ok,
        "fail-closed: one ✗, nothing else runs:\n{}",
        report.to_human()
    );
}

// -- 4. Determinism is actually enforced: a connector that changes between
//       runs fails the determinism clause.
struct FlickeringConnector {
    dir: PathBuf,
}

impl SourceConnector for FlickeringConnector {
    fn source_id(&self) -> &str {
        "flickering"
    }
    fn enumerate(&self) -> anyhow::Result<Vec<RawObject>> {
        // A counter file makes each run different — the anti-pattern.
        let counter = self.dir.join("count");
        let n: u64 = std::fs::read_to_string(&counter)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        std::fs::write(&counter, format!("{}", n + 1))?;
        Ok(vec![RawObject {
            native_key: format!("object-{n}"),
            bytes: b"drifts".to_vec(),
            native_meta: BTreeMap::new(),
        }])
    }
}

#[test]
fn a_nondeterministic_connector_fails_the_determinism_clause() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("kit-flicker");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    let report = run_kit(&FlickeringConnector { dir }, &KitExpectations::default());
    assert!(!report.all_ok());
    let determinism = report
        .checks
        .iter()
        .find(|c| c.name == "enumerate.determinism")
        .expect("determinism check present");
    assert!(
        !determinism.ok && determinism.detail.contains("DIFFER"),
        "{}",
        determinism.detail
    );
}
