//! wf-gen S4 condition: the workflow mutation ledger is tamper-evident.
//! Approval records (the `Decided` events + the approve/deny audit acts) are
//! the most tamper-sensitive rows in the system; they carry ts + prev and
//! verify via the SAME verify-ledger the core ledger uses.

use std::path::Path;
use std::sync::Arc;

use service::agent::proposals::{verify_ledger, LedgerVerification};
use service::clock::{Clock, FixedClock};
use service::proposals::{
    GroundingCounts, ProposalAnchor, ProposalBox, ProposalDraft, WorkflowProposalStore,
    STATUS_APPROVED,
};

const START_MS: u64 = 1_783_762_923_500; // 2026-07-11T09:42:03.500Z

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn draft(n: u64) -> ProposalDraft {
    ProposalDraft {
        proposer_id: "p060".to_string(),
        capability_id: "cap03".to_string(),
        approver_id: "p113".to_string(),
        title: format!("Workflow {n}"),
        goal: "ground truth".to_string(),
        boxes: vec![ProposalBox {
            box_index: 0,
            stage: "Next".to_string(),
            title: "Box".to_string(),
            description: "do the thing".to_string(),
            anchors: vec![ProposalAnchor {
                doc_id: "d0134".to_string(),
                locator: "L1".to_string(),
                quote: "verbatim".to_string(),
            }],
        }],
        grounding: GroundingCounts {
            admitted: 1,
            refused: 0,
        },
        snapshot_version: "snap-1".to_string(),
    }
}

fn chained(dir: &Path) -> WorkflowProposalStore {
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(START_MS, 1000));
    WorkflowProposalStore::open_chained(dir, clock).expect("chained store")
}

fn event_log(dir: &Path) -> std::path::PathBuf {
    dir.join("wf_proposals.jsonl")
}

// The propose->approve->materialize path writes chained rows carrying ts +
// prev, and the whole event log verifies CLEAN.
#[test]
fn workflow_event_log_is_chained_and_verifies_clean() {
    let dir = scratch("wf-chain-clean");
    let store = chained(&dir);
    let p = store.create(draft(1)).expect("create");
    store
        .decide(&p.proposal_id, STATUS_APPROVED, "p113", "snap-1")
        .expect("decide")
        .expect("decided");
    store.materialize(&p.proposal_id).expect("materialize");

    let rows: Vec<serde_json::Value> = std::fs::read_to_string(event_log(&dir))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    assert_eq!(rows.len(), 3, "created + decided + materialized");
    let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert_eq!(
        rows[0]["prev"], empty,
        "the first row anchors to sha256(\"\")"
    );
    assert!(rows
        .iter()
        .all(|r| r["ts"].is_string() && r["prev"].is_string()));
    // The Decided (approval) row carries its timestamp.
    assert!(rows
        .iter()
        .any(|r| r["event"] == "decided" && r["ts"].is_string()));

    match verify_ledger(&event_log(&dir)).expect("verify") {
        LedgerVerification::Clean { rows, chained_rows } => {
            assert_eq!((rows, chained_rows), (3, 3))
        }
        other => panic!("expected CLEAN, got {other:?}"),
    }
}

// Tamper — flip: rewrite the APPROVAL row's actor (forging who approved) ->
// the chain breaks at its successor.
#[test]
fn tampering_the_approval_row_is_detected() {
    let dir = scratch("wf-chain-flip");
    let store = chained(&dir);
    let p = store.create(draft(1)).expect("create");
    store
        .decide(&p.proposal_id, STATUS_APPROVED, "p113", "snap-1")
        .expect("decide")
        .expect("decided");
    store.materialize(&p.proposal_id).expect("materialize");
    drop(store);

    let path = event_log(&dir);
    let text = std::fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    // Row 1 is the Decided approval — forge the approver.
    assert!(lines[1].contains("\"event\":\"decided\""));
    lines[1] = lines[1].replace("\"p113\"", "\"p999\"");
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();

    match verify_ledger(&path).expect("verify") {
        LedgerVerification::Broken { ordinal, .. } => {
            assert_eq!(
                ordinal, 2,
                "the materialized row after the forged approval breaks"
            );
        }
        other => panic!("approval forgery undetected: {other:?}"),
    }
}

// Tamper — delete: removing a mid-file row breaks the chain at the successor.
#[test]
fn deleting_a_workflow_row_is_detected() {
    let dir = scratch("wf-chain-delete");
    let store = chained(&dir);
    let p = store.create(draft(1)).expect("create");
    store
        .decide(&p.proposal_id, STATUS_APPROVED, "p113", "snap-1")
        .expect("decide")
        .expect("decided");
    store.materialize(&p.proposal_id).expect("materialize");
    drop(store);

    let path = event_log(&dir);
    let text = std::fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines.remove(1); // drop the approval row
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();

    match verify_ledger(&path).expect("verify") {
        LedgerVerification::Broken { ordinal, .. } => assert_eq!(ordinal, 1),
        other => panic!("deletion undetected: {other:?}"),
    }
}

// Legacy-compat: open() (no clock) writes byte-identical pre-condition rows —
// no ts, no prev — and a legacy-only log verifies CLEAN (nothing to break).
#[test]
fn legacy_open_stays_byte_identical() {
    let dir = scratch("wf-chain-legacy");
    let store = WorkflowProposalStore::open(&dir).expect("legacy store");
    let p = store.create(draft(1)).expect("create");
    let _ = p;
    let text = std::fs::read_to_string(event_log(&dir)).unwrap();
    let row: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert!(
        row.get("ts").is_none() && row.get("prev").is_none(),
        "the legacy writer carries no chain metadata"
    );
    assert!(matches!(
        verify_ledger(&event_log(&dir)).expect("verify"),
        LedgerVerification::Clean { .. }
    ));
}

// Era boundary: legacy rows then chained rows — the first chained row anchors
// over the last legacy line; the whole file verifies CLEAN and a legacy-row
// tamper is caught by the first chained row.
#[test]
fn chain_anchors_over_pre_condition_rows() {
    let dir = scratch("wf-chain-boundary");
    // Pre-condition rows via open() (no clock).
    {
        let legacy = WorkflowProposalStore::open(&dir).expect("legacy");
        legacy.create(draft(1)).expect("legacy create");
    }
    // Reopen chained and append — the new row anchors over the legacy line.
    {
        let store = chained(&dir);
        store.create(draft(2)).expect("chained create");
    }
    let path = event_log(&dir);
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].get("prev").is_none(), "legacy row: no prev");
    assert!(rows[1]["prev"].is_string(), "chained row anchors over it");
    assert!(matches!(
        verify_ledger(&path).expect("verify"),
        LedgerVerification::Clean {
            rows: 2,
            chained_rows: 1
        }
    ));

    // Tamper the legacy row -> the chained row that anchors over it catches it.
    let text = std::fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    lines[0] = lines[0].replace("Workflow 1", "Workflow X");
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    match verify_ledger(&path).expect("verify") {
        LedgerVerification::Broken { ordinal, .. } => assert_eq!(ordinal, 1),
        other => panic!("legacy tamper undetected across the boundary: {other:?}"),
    }
}

// `verify-ledger` needs no changes to handle wf_proposals.jsonl — the same
// generic verifier the core ledger uses (this test IS that confirmation).
#[test]
fn verify_ledger_handles_the_workflow_file_unchanged() {
    let dir = scratch("wf-chain-verifier");
    let store = chained(&dir);
    store.create(draft(1)).expect("create");
    // A path-taking generic verifier — nothing workflow-specific.
    assert!(matches!(
        verify_ledger(&event_log(&dir)).expect("verify"),
        LedgerVerification::Clean { .. }
    ));
}
