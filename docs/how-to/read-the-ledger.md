# How to: read the ledger

The decision ledger is append-only JSONL at `<ledger.dir>/audit.jsonl` —
one row per decision, allow AND deny, hash-chained and timestamped.

## A row

```json
{"action":"v1_whoami","actor_principal":"agent_qa_drafter","ordinal":15,
 "outcome":"authorized","prev":"bf7ddbe4…","target":"GET /v1/whoami",
 "token_tid":"f8cdef31-…","token_oid":"aaaa0001-…","ts":"2026-07-11T22:41:03.117Z"}
```

The fields that matter when reading:

| Field | Meaning |
| --- | --- |
| `action` | The surface (`v1_whoami`, `v1_document`, `v1_retrieve`, …) |
| `actor_principal` | Who (resolved principal; absent on pre-resolution denies) |
| `target` | What was asked (`GET /v1/documents/<id>`, capped query text on retrieves) |
| `outcome` | `authorized` or the deny reason — the wire never says this, the ledger always does |
| `ordinal` | Row number; `verify-ledger` names breaks by it |
| `prev` | SHA-256 of the previous row's bytes (the tamper-evidence chain) |
| `ts` | RFC3339 UTC milliseconds |
| `token_*` | Claim evidence from the presented token (`tid`/`oid`/`azp`) |
| `source` | `primary` / `s3` on document decisions |

## Useful reads

```sh
# every deny, with reasons (the runbook translates them)
grep -v '"outcome":"authorized"' dev-out/ledger/audit.jsonl

# one principal's trail
grep '"actor_principal":"agent_estate_internal"' dev-out/ledger/audit.jsonl

# the last decision
tail -1 dev-out/ledger/audit.jsonl
```

(With `jq`: `jq -r 'select(.outcome != "authorized") | "\(.ts) \(.actor_principal) \(.target) -> \(.outcome)"'`.)

**Read it, never edit it.** Any in-place change breaks the chain at that
ordinal — which is the point. Verify before trusting:
[verify-a-ledger](verify-a-ledger.md). Policy-class denies additionally land
in the alert sink (`alerting.alerts_path`) as one-line JSON alerts carrying
the ledger ordinal they project.
