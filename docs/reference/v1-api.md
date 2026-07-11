# Reference — the `/v1` machine surface

Three endpoints. JWT-only (`Authorization: Bearer <Entra agent JWT>`); a
session credential here is refused without being consulted. No ledger wired ⇒
the whole namespace answers 401 (EB-6). All responses are canonical JSON.

## `GET /v1/whoami`

The handshake diagnostic: who this token resolves to — and nothing else.

```json
{"principal_id": "agent_finance_analyst", "display_name": "…"}
```

`display_name` appears only when the identity model knows one. Deliberately
no scope information: whoami is not an enumeration surface.

## `POST /v1/retrieve`

Governed retrieval, scoped at query construction (EB-5): an out-of-scope
document is never a candidate — not as an id, a title, a snippet, or a rank.

Request (unknown fields rejected):

```json
{"query": "supplier audit findings", "top_k": 8}
```

| Limit | Value | Violation |
| --- | --- | --- |
| `query` length | 1..=2,048 chars | 400 (`query_out_of_range`) |
| Request body | ≤ 16 KiB | 413 (`payload_oversize`) |
| `top_k` | 1..=50 (default 8) | 400 |

Response:

```json
{"principal": "agent_finance_analyst",
 "candidates": [
   {"doc_id": "d0134", "title": "…", "snippet": "…", "rank": 1}
 ]}
```

**`rank` is a 1-based fused rank, 1 = best, ascending in wire order.** It is
not a similarity score — do not sort descending. Raw similarity scores are
never serialized. Empty `candidates` is a normal 200, not an error.

## `GET /v1/documents/{id}`

The full authorized body. `{id}` is a catch-all — estate ids are path-like
(`s3/<bucket>/<key>`) and ride it with real slashes.

```json
{"doc_id": "…", "title": "…", "snippet": "…", "content": "<the FULL body>",
 "metadata": {"sensitivity": "internal", "source": "primary" }}
```

`metadata` also carries `superseded`/`effective_successor` (primary, when
applicable) and `bucket` (estate). Bodies over the 2 MiB response cap fail
LOUD: generic 500, ledger reason `body_exceeds_cap` — never truncated, never
a lying 404.

## The deliberate silences

- **THE 404 parity.** Out-of-scope and nonexistent return byte-identical
  404s. A caller cannot probe existence; the ledger records which it was.
- **Generic 401s.** Every auth-ladder deny is the same
  `{"error":"authentication required"}` — which rung denied is a ledger
  fact (`token_expired`, `agent_not_registered`, …), never a wire fact.
- **Unknown routes under `/v1`** 404 identically, and only for a *resolved*
  agent (auth precedes routing) — ledgered `unknown_route` (EB-7).

Error semantics from the client's seat: the Python SDK maps 401 →
`Unauthorized`, 404 → `NotFound` ("does not exist or you are not authorized —
indistinguishable by design"), 400/413 → `RequestRejected`, 5xx/transport →
`GatewayUnavailable`.
