# How to: run doctor

`doctor` is the read-only preflight: the same launch flags as the server,
zero mutation, zero network calls, exit `0` all-green / `1` otherwise.

```sh
service doctor --fixtures fixtures --artifacts compiler/artifacts \
  --idx retrieval/idx --config dev-out/config.json          # human ✓/✗
service doctor --json --fixtures … --config …               # {ok, checks:[{name,ok,detail}]}
```

## What it checks

| Check | ✓ means | ✗ names |
| --- | --- | --- |
| `config` | Parses; every section schema-valid | The offending field (same message a failing startup prints) |
| `ledger` | Dir writable; `audit.jsonl` chain verifies | Permissions, or the breaking ordinal |
| `bridge.jwks` / `.url` | Key file loads / URL well-formed (no live fetch) | The missing/unparseable path |
| `bridge.registry` | Every registered principal resolvable | The **ghost** principal by name |
| `estate` | Objects verify against the pinned hash; index builds | The mismatch or missing piece |
| `alerting.sink` / `.webhook_url` | Sink dir writable / URL well-formed | The unwritable path |
| `workflow_store` | `wf_proposals.jsonl` chain verifies (when a state dir is given) | The breaking ordinal |

Secrets never print — writability is probed with a create/remove of
`.doctor-write-probe`, never by touching real files.

## Where it runs for you already

The **container healthcheck** is doctor plus a port probe: a misconfigured
container flips `(unhealthy)` in `docker compose ps`, and the doctor JSON in
`docker inspect <container> --format '{{json .State.Health.Log}}'` names the
failing check — the [runbook](../runbook-denials.md) maps it to the fix.

Run it before every deploy; it exists so the first symptom of a misconfig is
a named line, not a silent all-deny.
