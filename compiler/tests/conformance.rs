//! Conformance suite C-1..C-6 for the M1 scope compiler.
//!
//! C-1 judges the compiler against /fixtures/ground_truth.jsonl — the
//! materialized M0 oracle. The harness (this file) is the ONLY code in the
//! crate allowed to read that file; the compiler itself never does.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use scope_compiler::compile::{self, Artifact, CompiledEntry, CompiledSet};
use scope_compiler::semantics::{Decision, World};
use scope_compiler::{load_world, snapshot};
use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Harness plumbing
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler crate sits in the repo root")
        .join("fixtures")
}

/// A fresh per-test scratch directory under the cargo-managed tmp dir.
fn scratch(name: &str) -> PathBuf {
    // Unique per invocation: Windows scanners (Search indexer / Defender) can
    // hold a just-deleted path in delete-pending state, so re-creating the
    // SAME path races them into Os error 5 "Access is denied". A fresh suffix
    // never re-opens a dying path; prior runs' dirs are swept best-effort (a
    // locked leftover is skipped now and reaped on a later run).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // The base lives in the SYSTEM temp dir, not target/tmp: the repo sits
    // under Documents\, which Windows Search indexes by default — its crawler
    // opens freshly written index segments mid-build and the write fails with
    // os error 5. AppData\Local\Temp is outside the default index scope.
    let base = std::env::temp_dir().join("enterprise-brain-test-scratch");
    std::fs::create_dir_all(&base).expect("scratch base");
    let prefix = format!("{name}-");
    if let Ok(entries) = base.read_dir() {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let dir = base.join(format!(
        "{prefix}{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Copies the compiler's three input fixtures into `dest`.
fn copy_fixtures(dest: &Path) {
    let src = fixtures_dir();
    for name in snapshot::INPUT_FILES {
        fs::copy(src.join(name), dest.join(name)).expect("copy fixture");
    }
}

/// Loads a fixture as JSON, applies `mutate`, writes it back (valid JSON,
/// arbitrary formatting — used only for refusal tests).
fn mutate_json(dir: &Path, file: &str, mutate: impl FnOnce(&mut Value)) {
    let path = dir.join(file);
    let mut value: Value =
        serde_json::from_slice(&fs::read(&path).expect("read fixture copy")).expect("parse");
    mutate(&mut value);
    fs::write(&path, serde_json::to_vec(&value).expect("encode")).expect("write fixture copy");
}

fn run_binary(args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_scope-compiler"))
        .args(args)
        .output()
        .expect("run scope-compiler binary")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// One full in-memory compile over the real fixtures.
fn compile_real_fixtures() -> (World, CompiledSet) {
    let dir = fixtures_dir();
    let snap = snapshot::take(&dir).expect("snapshot fixtures");
    let world = load_world(&dir).expect("fixtures load and validate");
    let (set, unknown) = compile::compile_set(&world, &snap, None).expect("compile all");
    assert!(unknown.is_empty(), "all fixture principals are known");
    (world, set)
}

fn artifacts_by_principal(set: &CompiledSet) -> HashMap<&str, &Artifact> {
    set.artifacts
        .iter()
        .map(|a| (a.principal_id.as_str(), a))
        .collect()
}

fn entries_by_doc(artifact: &Artifact) -> BTreeMap<&str, &CompiledEntry> {
    artifact
        .entries
        .iter()
        .map(|e| (e.document_id.as_str(), e))
        .collect()
}

/// Mirror of /test/schemas/ground_truth_row.schema.json. Read by the harness
/// only — never by the compiler.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GroundTruthRow {
    principal_id: String,
    resource_id: String,
    decision: String,
    reasons: Vec<String>,
}

fn load_ground_truth() -> Vec<GroundTruthRow> {
    let path = fixtures_dir().join("ground_truth.jsonl");
    let text = fs::read_to_string(&path).expect("read ground_truth.jsonl");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("parse ground truth row"))
        .collect()
}

// ---------------------------------------------------------------------------
// C-1 FULL MATRIX: every (principal, document) decision vs the oracle.
// ---------------------------------------------------------------------------

#[test]
fn c1_full_matrix() {
    let (world, set) = compile_real_fixtures();
    let artifacts = artifacts_by_principal(&set);

    let doc_ids: BTreeSet<&str> = world.documents.iter().map(|d| d.id.as_str()).collect();
    let docs_by_id: HashMap<&str, _> = world.documents.iter().map(|d| (d.id.as_str(), d)).collect();
    let principal_ids = world.principal_ids();
    assert_eq!(principal_ids.len(), 124, "fixture principal count");
    assert_eq!(doc_ids.len(), 600, "fixture document count");

    // The compiled allow sets, with reasons for the trace report.
    let mut compiled_allow: HashMap<(&str, &str), &CompiledEntry> = HashMap::new();
    for artifact in &set.artifacts {
        assert_eq!(
            artifact.entries.len() + artifact.denied_count,
            world.documents.len(),
            "principal {}: entries + denied_count must cover every document",
            artifact.principal_id
        );
        for entry in &artifact.entries {
            assert!(
                !entry.reasons.is_empty(),
                "allow without a reason: ({}, {})",
                artifact.principal_id,
                entry.document_id
            );
            compiled_allow.insert(
                (artifact.principal_id.as_str(), entry.document_id.as_str()),
                entry,
            );
        }
    }

    let rows = load_ground_truth();
    assert_eq!(
        rows.len(),
        74_400,
        "oracle matrix must be total (124 x 600)"
    );
    let mut seen_pairs: BTreeSet<(&str, &str)> = BTreeSet::new();

    let mut false_allows: Vec<String> = Vec::new();
    let mut false_denies: Vec<String> = Vec::new();
    let mut oracle_allow_total = 0usize;

    for row in &rows {
        assert!(
            artifacts.contains_key(row.principal_id.as_str()),
            "oracle names unknown principal {}",
            row.principal_id
        );
        assert!(
            doc_ids.contains(row.resource_id.as_str()),
            "oracle names unknown document {}",
            row.resource_id
        );
        assert!(
            seen_pairs.insert((row.principal_id.as_str(), row.resource_id.as_str())),
            "oracle repeats pair ({}, {})",
            row.principal_id,
            row.resource_id
        );

        let oracle_allows = row.decision == "ALLOW";
        if oracle_allows {
            oracle_allow_total += 1;
        }
        let compiled = compiled_allow.get(&(row.principal_id.as_str(), row.resource_id.as_str()));

        match (compiled, oracle_allows) {
            (Some(entry), false) => false_allows.push(format!(
                "FALSE ALLOW ({}, {}): compiler reasons {:?}; oracle reasons {:?}",
                row.principal_id, row.resource_id, entry.reasons, row.reasons
            )),
            (None, true) => {
                let doc = docs_by_id[row.resource_id.as_str()];
                let trace = match world.decide(&row.principal_id, doc) {
                    Decision::Deny(reasons) => reasons,
                    Decision::Allow(reasons) => reasons,
                };
                false_denies.push(format!(
                    "FALSE DENY ({}, {}): compiler deny trace {:?}; oracle reasons {:?}",
                    row.principal_id, row.resource_id, trace, row.reasons
                ));
            }
            _ => {}
        }
    }
    assert_eq!(
        seen_pairs.len(),
        74_400,
        "oracle must cover the full matrix exactly once"
    );

    for line in false_allows.iter().chain(false_denies.iter()) {
        println!("{line}");
    }
    println!(
        "C-1 summary: pairs=74400 false_allows={} false_denies={} compiled_allow_total={} oracle_allow_total={}",
        false_allows.len(),
        false_denies.len(),
        compiled_allow.len(),
        oracle_allow_total
    );

    assert_eq!(false_allows.len(), 0, "false allows fail the build");
    assert_eq!(false_denies.len(), 0, "false denies fail the build");
    assert_eq!(compiled_allow.len(), oracle_allow_total);
}

// ---------------------------------------------------------------------------
// C-2 TRAP BATTERY
// ---------------------------------------------------------------------------

#[test]
fn c2_trap_battery() {
    let (world, set) = compile_real_fixtures();
    let artifacts = artifacts_by_principal(&set);
    let traps = &world.traps;

    assert_eq!(
        traps.confused_deputy.len(),
        15,
        "confused-deputy trap count"
    );
    assert_eq!(
        traps.manager_overreach.len(),
        8,
        "manager-overreach trap count"
    );
    assert_eq!(traps.cross_site.len(), 6, "cross-site trap count");
    assert_eq!(
        traps.effective_version.len(),
        12,
        "effective-version trap count"
    );
    assert_eq!(traps.mosaic.len(), 10, "mosaic trap count");

    // Confused deputy: the agent's effective access (grant INTERSECT owner)
    // must DENY even though one side alone would allow.
    for trap in &traps.confused_deputy {
        let agent = world.agent(&trap.agent_id).expect("trap agent exists");
        assert_eq!(
            agent.owner_user_id, trap.owner_id,
            "trap owner matches fixture"
        );
        let entries = entries_by_doc(artifacts[trap.agent_id.as_str()]);
        assert!(
            !entries.contains_key(trap.resource_id.as_str()),
            "confused deputy must DENY: ({}, {})",
            trap.agent_id,
            trap.resource_id
        );
    }

    // Manager overreach: the org edge grants nothing — manager DENY, while
    // the subject still reads their own record (positive control).
    for trap in &traps.manager_overreach {
        let manager_entries = entries_by_doc(artifacts[trap.manager_id.as_str()]);
        assert!(
            !manager_entries.contains_key(trap.resource_id.as_str()),
            "manager overreach must DENY: ({}, {})",
            trap.manager_id,
            trap.resource_id
        );
        let subject_entries = entries_by_doc(artifacts[trap.subject_id.as_str()]);
        let entry = subject_entries
            .get(trap.resource_id.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "subject must read own HR record: ({}, {})",
                    trap.subject_id, trap.resource_id
                )
            });
        assert!(
            entry.reasons.iter().any(|r| r == "SUBJECT:self"),
            "subject access must trace to SUBJECT:self"
        );
    }

    // Cross-site: ABAC site condition must DENY despite a matching group.
    for trap in &traps.cross_site {
        let person = world
            .person(&trap.principal_id)
            .expect("trap person exists");
        assert_eq!(
            person.site, trap.principal_site,
            "trap site matches fixture"
        );
        let entries = entries_by_doc(artifacts[trap.principal_id.as_str()]);
        assert!(
            !entries.contains_key(trap.resource_id.as_str()),
            "cross-site must DENY: ({}, {})",
            trap.principal_id,
            trap.resource_id
        );
    }

    // Effective version: superseded entries stay readable but must carry the
    // marker + successor on EVERY artifact that allows them.
    for trap in &traps.effective_version {
        let mut carriers = 0usize;
        for artifact in &set.artifacts {
            let entries = entries_by_doc(artifact);
            if let Some(entry) = entries.get(trap.superseded_id.as_str()) {
                carriers += 1;
                assert_eq!(
                    entry.superseded,
                    Some(true),
                    "superseded marker missing: ({}, {})",
                    artifact.principal_id,
                    trap.superseded_id
                );
                assert_eq!(
                    entry.effective_successor.as_deref(),
                    Some(trap.current_id.as_str()),
                    "effective successor wrong: ({}, {})",
                    artifact.principal_id,
                    trap.superseded_id
                );
            }
            if let Some(entry) = entries.get(trap.current_id.as_str()) {
                assert_eq!(
                    entry.superseded, None,
                    "current version must not be marked superseded: {}",
                    trap.current_id
                );
            }
        }
        assert!(
            carriers > 0,
            "no principal carries superseded entry {} — trap is vacuous",
            trap.superseded_id
        );
    }

    // Mosaic: both halves of each pair are individually readable by the trap
    // principal and carry the fixture tag record untouched.
    for trap in &traps.mosaic {
        let entries = entries_by_doc(artifacts[trap.principal_id.as_str()]);
        for doc_id in [&trap.doc_a, &trap.doc_b] {
            let entry = entries.get(doc_id.as_str()).unwrap_or_else(|| {
                panic!(
                    "mosaic half must be individually allowed: ({}, {doc_id})",
                    trap.principal_id
                )
            });
            let tags = entry.mosaic_tags.as_ref().unwrap_or_else(|| {
                panic!("mosaic tags missing on ({}, {doc_id})", trap.principal_id)
            });
            assert!(
                tags.contains(trap),
                "mosaic tag not passed through untouched on ({}, {doc_id})",
                trap.principal_id
            );
        }
    }
}

// ---------------------------------------------------------------------------
// C-3 FAIL-CLOSED
// ---------------------------------------------------------------------------

#[test]
fn c3_unknown_principal_compiles_empty_and_exits_zero() {
    let out = scratch("c3_unknown_principal_out");
    let fixtures = fixtures_dir();
    let output = run_binary(&[
        "compile",
        "--fixtures",
        fixtures.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--principal",
        "p_ghost_404",
    ]);
    assert!(
        output.status.success(),
        "unknown principal is not a refusal: {}",
        stderr_of(&output)
    );
    assert!(
        stderr_of(&output).contains("unknown principal"),
        "the unknown principal must be logged"
    );

    let artifact: Artifact =
        serde_json::from_slice(&fs::read(out.join("p_ghost_404.json")).expect("artifact written"))
            .expect("artifact parses");
    assert!(artifact.entries.is_empty(), "empty allowlist");
    assert_eq!(artifact.denied_count, 600, "every document denied");

    let index: compile::IndexFile =
        serde_json::from_slice(&fs::read(out.join("index.json")).expect("index written"))
            .expect("index parses");
    assert_eq!(index.principals.len(), 1);
    assert_eq!(index.principals[0].unknown_principal, Some(true));
}

#[test]
fn c3_schema_or_parse_failure_refuses_entirely() {
    // (a) malformed JSON
    let dir = scratch("c3_parse_malformed");
    copy_fixtures(&dir);
    fs::write(dir.join("documents.json"), b"{ this is not json").unwrap();
    let out = dir.join("out");
    let output = run_binary(&[
        "compile",
        "--fixtures",
        dir.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(!output.status.success(), "malformed JSON must refuse");
    assert!(!out.join("index.json").exists(), "no artifacts on refusal");

    // (b) well-formed JSON violating a schema constraint serde types catch
    // (unknown key under deny_unknown_fields)
    let dir = scratch("c3_parse_unknown_key");
    copy_fixtures(&dir);
    mutate_json(&dir, "documents.json", |v| {
        v["documents"][0]["mystery_field"] = Value::Bool(true);
    });
    let output = run_binary(&[
        "compile",
        "--fixtures",
        dir.to_str().unwrap(),
        "--out",
        dir.join("out").to_str().unwrap(),
    ]);
    assert!(!output.status.success(), "unknown key must refuse");

    // (c) well-formed JSON violating a value constraint (band outside 1..=5)
    let dir = scratch("c3_parse_band_range");
    copy_fixtures(&dir);
    mutate_json(&dir, "company.json", |v| {
        v["people"][0]["employment_band"] = Value::from(9);
    });
    let output = run_binary(&[
        "compile",
        "--fixtures",
        dir.to_str().unwrap(),
        "--out",
        dir.join("out").to_str().unwrap(),
    ]);
    assert!(!output.status.success(), "band outside 1..=5 must refuse");
    assert!(
        stderr_of(&output).contains("employment_band"),
        "refusal names the violated constraint"
    );
}

#[test]
fn c3_fixture_change_after_snapshot_refuses_verification() {
    let dir = scratch("c3_reverify");
    copy_fixtures(&dir);
    let out = dir.join("out");
    let output = run_binary(&[
        "compile",
        "--fixtures",
        dir.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr_of(&output));

    // Positive control: unchanged fixtures verify.
    let output = run_binary(&[
        "verify",
        "--fixtures",
        dir.to_str().unwrap(),
        "--artifacts",
        out.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr_of(&output));

    // Change fixture bytes after the snapshot was taken -> refuse.
    let traps_path = dir.join("traps.json");
    let mut bytes = fs::read(&traps_path).unwrap();
    bytes.push(b' ');
    fs::write(&traps_path, bytes).unwrap();
    let output = run_binary(&[
        "verify",
        "--fixtures",
        dir.to_str().unwrap(),
        "--artifacts",
        out.to_str().unwrap(),
    ]);
    assert!(!output.status.success(), "hash mismatch must refuse");
    assert!(
        stderr_of(&output).contains("snapshot"),
        "refusal names the snapshot mismatch"
    );
}

#[test]
fn c3_duplicate_or_dangling_references_refuse() {
    // Duplicate document id.
    let dir = scratch("c3_duplicate_doc_id");
    copy_fixtures(&dir);
    mutate_json(&dir, "documents.json", |v| {
        let first = v["documents"][0]["id"].clone();
        v["documents"][1]["id"] = first;
    });
    let output = run_binary(&[
        "compile",
        "--fixtures",
        dir.to_str().unwrap(),
        "--out",
        dir.join("out").to_str().unwrap(),
    ]);
    assert!(
        !output.status.success(),
        "duplicate document id must refuse"
    );
    assert!(stderr_of(&output).contains("duplicate document id"));

    // Dangling author reference.
    let dir = scratch("c3_dangling_author");
    copy_fixtures(&dir);
    mutate_json(&dir, "documents.json", |v| {
        v["documents"][0]["author_id"] = Value::from("p_nobody");
    });
    let output = run_binary(&[
        "compile",
        "--fixtures",
        dir.to_str().unwrap(),
        "--out",
        dir.join("out").to_str().unwrap(),
    ]);
    assert!(!output.status.success(), "dangling author must refuse");
    assert!(stderr_of(&output).contains("dangling author reference"));

    // Dangling group reference in an ACL rule.
    let dir = scratch("c3_dangling_group");
    copy_fixtures(&dir);
    mutate_json(&dir, "documents.json", |v| {
        let docs = v["documents"].as_array_mut().unwrap();
        let doc = docs
            .iter_mut()
            .find(|d| d["acl_refs"][0]["kind"] == "group")
            .expect("a group-ruled document exists");
        doc["acl_refs"][0]["group"] = Value::from("grp_does_not_exist");
    });
    let output = run_binary(&[
        "compile",
        "--fixtures",
        dir.to_str().unwrap(),
        "--out",
        dir.join("out").to_str().unwrap(),
    ]);
    assert!(!output.status.success(), "dangling group must refuse");
    assert!(stderr_of(&output).contains("dangling group reference"));

    // Dangling supersedes reference.
    let dir = scratch("c3_dangling_supersedes");
    copy_fixtures(&dir);
    mutate_json(&dir, "documents.json", |v| {
        v["documents"][0]["supersedes"] = Value::from("d_nothing");
    });
    let output = run_binary(&[
        "compile",
        "--fixtures",
        dir.to_str().unwrap(),
        "--out",
        dir.join("out").to_str().unwrap(),
    ]);
    assert!(!output.status.success(), "dangling supersedes must refuse");
    assert!(stderr_of(&output).contains("dangling supersedes reference"));
}

// ---------------------------------------------------------------------------
// C-4 DETERMINISM
// ---------------------------------------------------------------------------

#[test]
fn c4_two_compiles_are_byte_identical() {
    let out_a = scratch("c4_out_a");
    let out_b = scratch("c4_out_b");
    let fixtures = fixtures_dir();
    for out in [&out_a, &out_b] {
        let output = run_binary(&[
            "compile",
            "--fixtures",
            fixtures.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        assert!(output.status.success(), "{}", stderr_of(&output));
    }

    let names = |dir: &Path| -> BTreeSet<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    };
    let names_a = names(&out_a);
    assert_eq!(names_a, names(&out_b), "same artifact set");
    assert_eq!(names_a.len(), 125, "124 principals + index.json");

    for name in &names_a {
        let a = fs::read(out_a.join(name)).unwrap();
        let b = fs::read(out_b.join(name)).unwrap();
        assert_eq!(a, b, "artifact {name} differs between identical compiles");
    }
}

// ---------------------------------------------------------------------------
// C-5 SNAPSHOT PINNING
// ---------------------------------------------------------------------------

#[test]
fn c5_one_flipped_byte_changes_snapshot_and_refuses_old_artifacts() {
    let dir_a = scratch("c5_fixtures_a");
    let dir_b = scratch("c5_fixtures_b");
    copy_fixtures(&dir_a);
    copy_fixtures(&dir_b);

    // Flip exactly one byte inside free text: schema-valid, parse-valid.
    let path = dir_b.join("documents.json");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("controlled"), "flip target exists");
    let flipped = text.replacen("controlled", "cantrolled", 1);
    assert_eq!(text.len(), flipped.len(), "exactly one byte flipped");
    fs::write(&path, flipped).unwrap();

    let out_a = scratch("c5_out_a");
    let out_b = scratch("c5_out_b");
    for (fixtures, out) in [(&dir_a, &out_a), (&dir_b, &out_b)] {
        let output = run_binary(&[
            "compile",
            "--fixtures",
            fixtures.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        assert!(output.status.success(), "{}", stderr_of(&output));
    }

    let index = |out: &Path| -> compile::IndexFile {
        serde_json::from_slice(&fs::read(out.join("index.json")).unwrap()).unwrap()
    };
    let version_a = index(&out_a).snapshot_version;
    let version_b = index(&out_b).snapshot_version;
    assert_ne!(
        version_a, version_b,
        "one flipped byte must change snapshot_version"
    );

    // Artifacts built from snapshot A refuse verification against fixtures B.
    let output = run_binary(&[
        "verify",
        "--fixtures",
        dir_b.to_str().unwrap(),
        "--artifacts",
        out_a.to_str().unwrap(),
    ]);
    assert!(
        !output.status.success(),
        "old artifacts must refuse verification against the new snapshot"
    );

    // And still verify against the fixtures they pinned.
    let output = run_binary(&[
        "verify",
        "--fixtures",
        dir_a.to_str().unwrap(),
        "--artifacts",
        out_a.to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{}", stderr_of(&output));
}

// ---------------------------------------------------------------------------
// C-6 PERFORMANCE
// ---------------------------------------------------------------------------

#[test]
fn c6_full_compile_under_two_seconds() {
    let out = scratch("c6_out");
    let fixtures = fixtures_dir();

    let started = std::time::Instant::now();
    let snap = snapshot::take(&fixtures).expect("snapshot");
    let world = load_world(&fixtures).expect("load");
    let (set, _) = compile::compile_set(&world, &snap, None).expect("compile");
    snapshot::verify_unchanged(&fixtures, &snap).expect("re-verify");
    compile::write_artifacts(&out, &set).expect("write");
    let elapsed = started.elapsed();

    println!(
        "C-6: full {}-principal compile (snapshot + load + compile + re-verify + write) took {:.3}s ({} profile)",
        set.index.totals.principals,
        elapsed.as_secs_f64(),
        if cfg!(debug_assertions) { "debug" } else { "release" }
    );
    assert_eq!(set.index.totals.principals, 124);

    // The 2.0s bound is specified for the release build; debug runs still
    // report their timing above.
    if !cfg!(debug_assertions) {
        assert!(
            elapsed.as_secs_f64() < 2.0,
            "C-6 bound exceeded: {:.3}s",
            elapsed.as_secs_f64()
        );
    }
}
