//! S4 Parts 1-2: ledger timestamps (clock-injected, determinism preserved)
//! and the hash-chained tamper-evidence, including anchoring across the
//! pre-S4 / S4 era boundary.

use std::path::Path;
use std::sync::Arc;

use service::agent::proposals::{
    verify_ledger, LedgerVerification, ProposalStore, TokenAuditFields,
};
use service::clock::{Clock, FixedClock};

const START_MS: u64 = 1_783_762_923_500; // 2026-07-11T09:42:03.500Z

fn scratch(name: &str) -> std::path::PathBuf {
    // Unique per invocation: Windows scanners (Search indexer / Defender) can
    // hold a just-deleted path in delete-pending state, so re-creating the
    // SAME path races them into Os error 5 "Access is denied". A fresh suffix
    // never re-opens a dying path; prior runs' dirs are swept best-effort (a
    // locked leftover is skipped now and reaped on a later run).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let base = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
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

fn read_lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect()
}

// PART 1: timestamps are present, deterministic under the injected clock,
// and monotone with the clock's step.
#[test]
fn chained_rows_carry_deterministic_timestamps() {
    let dir = scratch("ledger-ts");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(START_MS, 1000));
    let store = ProposalStore::open_chained(&dir, clock).expect("store");
    store
        .audit(
            "v1_document",
            "agent_a",
            "GET /v1/documents/d0001",
            "not_found",
        )
        .expect("row 0");
    store
        .audit(
            "v1_document",
            "agent_a",
            "GET /v1/documents/d0002",
            "authorized",
        )
        .expect("row 1");

    let rows = read_lines(&dir.join("audit.jsonl"));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["ts"], "2026-07-11T09:42:03.500Z");
    assert_eq!(rows[1]["ts"], "2026-07-11T09:42:04.500Z");
}

// PART 1 (determinism preserved): a store opened WITHOUT a clock (the legacy
// path every pre-S4 caller uses) writes NO ts and NO prev — byte-identical
// to pre-S4. This is what keeps the legacy exact-JSON pin green.
#[test]
fn legacy_open_writes_no_timestamp_and_no_chain() {
    let dir = scratch("ledger-legacy");
    let store = ProposalStore::open(&dir).expect("store");
    store
        .audit("lens_view", "p060", "p060", "allowed")
        .expect("row");
    let text = std::fs::read_to_string(dir.join("audit.jsonl")).expect("ledger");
    assert_eq!(
        text.trim(),
        r#"{"action":"lens_view","actor_principal":"p060","ordinal":0,"outcome":"allowed","target":"p060"}"#,
        "the legacy writer stays byte-identical — no ts, no prev"
    );
    // And it verifies CLEAN (a file of purely legacy rows has no chain to
    // break — every row is anchored-over).
    assert!(matches!(
        verify_ledger(&dir.join("audit.jsonl")).expect("verify"),
        LedgerVerification::Clean { .. }
    ));
}

// PART 2: a fully S4-chained ledger verifies CLEAN; the first row links to
// sha256 of the empty string.
#[test]
fn append_only_chained_ledger_verifies_clean() {
    let dir = scratch("ledger-clean");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(START_MS, 1));
    let store = ProposalStore::open_chained(&dir, clock).expect("store");
    for i in 0..10 {
        store
            .audit(
                "v1_document",
                "agent_a",
                &format!("GET /v1/documents/d{i:04}"),
                "not_found",
            )
            .expect("row");
    }
    let path = dir.join("audit.jsonl");
    let rows = read_lines(&path);
    assert_eq!(rows.len(), 10);
    // Row 0's prev is sha256("").
    let empty_sha = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert_eq!(rows[0]["prev"], empty_sha);
    // Every row carries a prev.
    assert!(rows.iter().all(|r| r["prev"].is_string()));
    match verify_ledger(&path).expect("verify") {
        LedgerVerification::Clean { rows, chained_rows } => {
            assert_eq!((rows, chained_rows), (10, 10));
        }
        other => panic!("expected CLEAN, got {other:?}"),
    }
}

// PART 2 (tamper — flipped byte): flip one byte in a mid-file row → the
// chain breaks at that row's SUCCESSOR (whose prev no longer matches).
#[test]
fn flipping_a_mid_file_byte_is_detected() {
    let dir = scratch("ledger-flip");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(START_MS, 1));
    let store = ProposalStore::open_chained(&dir, clock).expect("store");
    for i in 0..6 {
        store
            .audit(
                "v1_document",
                "agent_a",
                &format!("GET /v1/documents/d{i:04}"),
                "not_found",
            )
            .expect("row");
    }
    drop(store);
    let path = dir.join("audit.jsonl");
    let text = std::fs::read_to_string(&path).expect("read");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    // Tamper row 2: change its actor (a real forensic edit — someone
    // rewriting who did what). Row 3's prev now mismatches.
    lines[2] = lines[2].replace("\"agent_a\"", "\"agent_x\"");
    std::fs::write(&path, lines.join("\n") + "\n").expect("write");

    match verify_ledger(&path).expect("verify") {
        LedgerVerification::Broken { ordinal, .. } => {
            assert_eq!(
                ordinal, 3,
                "the break surfaces at the tampered row's successor"
            );
        }
        other => panic!("tamper undetected: {other:?}"),
    }
}

// PART 2 (tamper — deleted line): delete a mid-file line → the chain breaks
// at the successor (which now links to the wrong predecessor).
#[test]
fn deleting_a_mid_file_line_is_detected() {
    let dir = scratch("ledger-delete");
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(START_MS, 1));
    let store = ProposalStore::open_chained(&dir, clock).expect("store");
    for i in 0..6 {
        store
            .audit(
                "v1_document",
                "agent_a",
                &format!("GET /v1/documents/d{i:04}"),
                "not_found",
            )
            .expect("row");
    }
    drop(store);
    let path = dir.join("audit.jsonl");
    let text = std::fs::read_to_string(&path).expect("read");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines.remove(2); // delete row 2; row 3 becomes index 2 and mis-links
    std::fs::write(&path, lines.join("\n") + "\n").expect("write");

    match verify_ledger(&path).expect("verify") {
        LedgerVerification::Broken { ordinal, .. } => {
            assert_eq!(
                ordinal, 2,
                "the successor of the deleted line breaks the chain"
            );
        }
        other => panic!("deletion undetected: {other:?}"),
    }
}

// PART 2 (era boundary): a ledger with pre-S4 legacy rows (no prev) followed
// by S4 chained rows — the first S4 row anchors to the last LEGACY line's
// bytes, and the whole file verifies CLEAN. Tampering a legacy row is then
// caught by the first S4 row that anchors over it.
#[test]
fn chain_anchors_across_the_legacy_to_s4_boundary() {
    let dir = scratch("ledger-boundary");
    // Era 1: legacy rows via open() (no clock).
    {
        let legacy = ProposalStore::open(&dir).expect("legacy store");
        legacy
            .audit("lens_view", "p060", "p060", "allowed")
            .expect("legacy 0");
        legacy
            .audit("lens_view", "p088", "p088", "allowed")
            .expect("legacy 1");
    }
    // Era 2: reopen CHAINED and append — the first S4 row's prev = hash of
    // the last legacy line's bytes (anchoring over legacy, no rewrite).
    let token = TokenAuditFields {
        oid: Some("oid-1".into()),
        ..Default::default()
    };
    {
        let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(START_MS, 1000));
        let s4 = ProposalStore::open_chained(&dir, clock).expect("s4 store");
        s4.audit_v1(
            "v1_document",
            "agent_a",
            "GET /v1/documents/s3/finance-restricted/x.md",
            "not_found",
            &token,
            None,
            None,
            None,
            Some("s3"),
        )
        .expect("s4 0");
        s4.audit_v1(
            "v1_document",
            "agent_a",
            "GET /v1/documents/d0001",
            "not_found",
            &token,
            None,
            None,
            None,
            Some("primary"),
        )
        .expect("s4 1");
    }
    let path = dir.join("audit.jsonl");
    let rows = read_lines(&path);
    assert_eq!(rows.len(), 4, "2 legacy + 2 S4");
    assert!(
        rows[0]["prev"].is_null() && rows[1]["prev"].is_null(),
        "legacy rows have no prev"
    );
    assert!(
        rows[2]["prev"].is_string() && rows[3]["prev"].is_string(),
        "S4 rows chained"
    );
    assert!(rows[2]["ts"].is_string(), "S4 rows timestamped");

    // The whole file verifies CLEAN — the chain anchors over the legacy tail.
    assert!(matches!(
        verify_ledger(&path).expect("verify"),
        LedgerVerification::Clean {
            rows: 4,
            chained_rows: 2
        }
    ));

    // Tamper a LEGACY row (row 1): the first S4 row (index 2) anchors to its
    // bytes and catches the change.
    let text = std::fs::read_to_string(&path).expect("read");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines[1] = lines[1].replace("\"allowed\"", "\"refused\"");
    std::fs::write(&path, lines.join("\n") + "\n").expect("write");
    match verify_ledger(&path).expect("verify") {
        LedgerVerification::Broken { ordinal, .. } => {
            assert_eq!(
                ordinal, 2,
                "the first S4 row catches the legacy-row tamper it anchors over"
            );
        }
        other => panic!("era-boundary tamper undetected: {other:?}"),
    }
}
