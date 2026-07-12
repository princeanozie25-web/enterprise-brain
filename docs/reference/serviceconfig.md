# Reference — ServiceConfig

One JSON file (`--config path`). **Unknown fields refuse startup naming the
field** — a typo is a loud failure, not a silently-ignored setting. Every
section maps to a doctor check; `service doctor` validates the file with the
same schema and messages.

| Field | Type / default | Meaning | Doctor check |
| --- | --- | --- | --- |
| `profile` | string, optional | Free-text label ("production", "DEMO …") — informational, and the file's comment channel | `config` |
| `endpoint` | string, optional | Local LLM endpoint (Ollama) for the console's model capabilities | `config` |
| `embed_model` + `embed_dim` | strings/int, optional | Embeddings; `embed_model` without `embed_dim` refuses (never guess) | `config` |
| `judge_model` / `generate_model` | string, optional | Judge / generator models; absent = capability off | `config` |
| `judge_timeout_ms` | int, default 2000 | Demo-profile knob | `config` |
| `bind` | string, **default `127.0.0.1:8787`** | Explicit listen address. Absent = the loopback invariant, byte-for-byte. Present = the operator's recorded choice; a non-loopback bind logs a loud warning. The containerized demo sets `0.0.0.0:8787` behind the compose host-loopback mapping | `config` |
| `ledger.dir` | path | The decision ledger (append-only, hash-chained, timestamped). **No ledger ⇒ no `/v1`** | `ledger` |
| `agent_bridge` | section, default OFF | The Entra token path. `enabled: false` or absent wires nothing (S0-4) | `bridge.*` |
| `agent_bridge.tenant_id` / `audience` | strings | The tenant tokens must come from / the audience they must name | `config` |
| `agent_bridge.jwks.file` \| `.url` | exactly one | Verification keys: local file (dev/offline) or tenant endpoint (cached, single-flight) | `bridge.jwks` / `.url` |
| `agent_bridge.allowed_algs` | list, default `["RS256"]` | Signature algorithms (RS/PS family only; `none`/HS never) | `config` |
| `agent_bridge.agents[]` | `{tid, oid, principal}` | The registry. Duplicates and empties refuse at load; ghosts are named by doctor | `bridge.registry` |
| `estate_dir` | path, optional | The multi-source estate (`s3-access.json` + `s3-store/`). Absent = single-source | `estate` |
| `alerting.enabled` | bool | Policy-deny alerting (EB-7), off the request path | `config` |
| `alerting.alerts_path` | path, required when enabled | The durable file sink (append, fsync) | `alerting.sink` |
| `alerting.webhook_url` | string, optional | Best-effort webhook (3 attempts, 3 s timeout) | `alerting.webhook_url` |

The **shipped default config is bridge-disabled**; only the generated,
DEMO-labelled `bootstrap-dev` config enables it — deliberately.
