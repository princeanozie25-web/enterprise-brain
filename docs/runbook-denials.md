# Runbook — every deny, and where its truth lives

**The wire is mute on purpose.** A denied `/v1` request answers with a generic
`401` or THE one `404` — never a reason. Reasons would teach an attacker the
ladder (which check they passed last); so reasons live where the operator
looks and the attacker cannot: the **decision ledger**, the **doctor**, and
the **startup log**. This page is where the voice lives.

Ledger rows are JSONL in the configured `ledger.dir` (`audit.jsonl`); each
deny row carries a `reason` from the tables below. `service doctor` names
misconfigurations before they become mystery denies.

## Auth-ladder denies (wire: generic 401)

| Ledger `reason` | What happened | The fix |
| --- | --- | --- |
| `credential_missing` | No `Authorization: Bearer` header | Send the agent JWT as a bearer token |
| `session_credential_on_v1` | A console session token was sent to `/v1` | `/v1` is JWT-only; sessions never cross (S1-1) |
| `bridge_disabled` | The agent bridge is not enabled in config | Set `agent_bridge.enabled: true` deliberately (S0-4: default OFF) — doctor names this |
| `bridge_unavailable` | The bridge task failed at runtime | Check the gateway log; restart; file a bug with the log line |
| `token_malformed` | Not a decodable compact JWS | Send the raw JWT, not a wrapper or fragment of one |
| `algorithm_rejected` | Header `alg` outside the allowlist (default RS256; `none`/HS rejected) | Issue RS256 tokens; never widen `allowed_algs` casually |
| `signature_invalid` | Signature fails against the JWKS (incl. unknown `kid`) | Wrong or rotated key: re-mint the token; check `agent_bridge.jwks` points at the CURRENT key set — doctor validates the file |
| `token_expired` | `exp` in the past (60 s skew allowed) | Tokens live 24 h; reissuing = rotating the world (`bootstrap-dev … --force`, the only destructive path — see [rotate-dev-keys](how-to/rotate-dev-keys.md)) |
| `token_not_yet_valid` | `nbf`/`iat` in the future | Fix clock skew at the issuer, or wait |
| `issuer_mismatch` | `iss` is not the configured tenant's issuer | Token from the wrong tenant/authority; check `agent_bridge.tenant_id` |
| `audience_mismatch` | `aud` is not this gateway | Request the token FOR this gateway (`agent_bridge.audience`) |
| `tenant_mismatch` | `tid` differs from the configured tenant | Same as issuer: the agent lives in another tenant |
| `unsupported_token_type_delegated` | A delegated (user) token (`scp`/user claims) | Only autonomous-agent (app-only) tokens; use client-credentials |
| `unsupported_token_type_agent_user` | An agent-user interactive shape (`xms_idrel` user side) | Same: app-only autonomous tokens only |
| `agent_facets_missing` | `idtyp`/agent facets absent (`xms_act_fct`/`xms_sub_fct`) | The identity is not an Entra agent identity; register a real agent identity |
| `agent_not_registered` | Ladder passed, but `(tid, oid)` has no registry row | Register the agent in `agent_bridge.agents` — a GHOST registration (row pointing at an unknown principal) is named by `doctor` at preflight |

## Resource-level denies (wire: THE 404 / generic 4xx-5xx)

| Ledger `reason` | Wire | What happened | The fix |
| --- | --- | --- | --- |
| `not_found` | 404 | Out-of-scope OR nonexistent — **indistinguishable by design** | If the principal should see it: fix the grant (primary: group/ACL fixtures; estate: `agent_tiers` in `s3-access.json`). The 404 parity is the point — do not "fix" it |
| `unknown_route` | 404 | A resolved agent probed a non-route | Nothing to fix; the probe is ledgered as a signal (EB-7) |
| `body_exceeds_cap` | 500 | Authorized document larger than the 2 MiB response cap | Deliberate fail-loud: never truncated, never a lying 404. Split the document or raise the cap consciously |
| `query_out_of_range` | 400 | Empty query or > 2,048 chars | Trim the query |
| `payload_oversize` | 413 | Request body > 16 KiB | Send less |
| `bad_request` | 400 | Body isn't the documented shape | `{"query": "...", "top_k": N}` — unknown fields are rejected |

## Denies that never reach a request

| Signal | Where it speaks | The fix |
| --- | --- | --- |
| `v1 refused: no audit ledger wired (EB-6)` | Gateway **stderr**, every `/v1` request 401s | Wire `ledger.dir` in config (no ledger ⇒ no machine surface, by design) |
| Malformed config section | **Startup fails loudly naming the field** | Read the startup error; `service doctor` reproduces it with the same field name |
| Ghost registration | `doctor` → `bridge.registry` ✗ names the principal | Remove or fix the registry row before it becomes runtime all-deny |
| Broken ledger chain | `doctor` → `ledger` ✗ names the breaking ordinal; `verify-ledger` exits 1 | Investigate tampering; the chain is evidence — do not "repair" it silently |
| Unhealthy container | `docker compose ps` (unhealthy); doctor JSON in the health log names the check | Fix the named field; health returns within one probe interval |

**Alerting (EB-7):** policy-class denies (`not_found`, `body_exceeds_cap` from a validated principal) additionally emit alerts off the request path (file sink + optional webhook) — see `alerting` in the config reference. Auth-ladder denies are fenced out of alerting by design (they are noise at the door, not policy signals).
