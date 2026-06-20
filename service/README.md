# Enterprise Brain M3a — Ask Brain answer service

The first long-running process in Enterprise Brain and the first place an LLM
generates text from retrieved context. A loopback-only axum service over the
conformance-proven substrate (M1 compiler, M2a/M2b retrieval, consumed
read-only): deny by default, fail closed, no dark counts, no out-of-scope
ids — and the generator is untrusted by construction.

```sh
cargo run --release -p service -- \
    --fixtures fixtures --artifacts compiler/artifacts --idx retrieval/idx \
    [--config service/config.example.json] [--usage-out usage.jsonl] [--no-cache]
```

Binds `127.0.0.1:8787` ONLY (`loopback_listener` refuses anything else, A-9).

## Endpoints

Every endpoint except `GET /healthz` requires `X-Demo-Principal` (the demo
stand-in for OIDC); a missing header → 401, and there is no default principal.
An unknown principal is NOT rejected at the door — it flows in and is denied by
default downstream (empty envelope / the one 404), indistinguishable from a
principal granted nothing (A-5). Every response carries `demo_identity_mode:
true`.

### Ask & documents

| Endpoint | Behavior |
| --- | --- |
| `POST /ask` `{query, hybrid?, judge?}` | the answer pipeline below; results carry `{title, sensitivity}` copied from the same scope-checked source as `/doc` |
| `GET /doc/{id}` | scope-checked document card (title, sensitivity, ≤480-char snippet, superseded notice + in-allowlist successor); out-of-scope and nonexistent ids return a BYTE-IDENTICAL 404 (A-10) — never full bodies |
| `GET /scope` | the caller's REAL scope statement (groups/sites/band from company.json) — their own only |
| `GET /me/scope` | the caller's own scope, as the console's identity probe reads it |
| `GET /healthz` | `{"status":"ok"}`, no identity, reveals nothing |

### Aperture rooms

| Endpoint | Room | Behavior |
| --- | --- | --- |
| `GET /lens/{id}` | Lens (AP-2) | one principal's access, reason-grouped, audited (actor + subject) |
| `GET /lens/diff` | Lens diff (AP-4) | set-exact two-principal comparison in a single audited act |
| `GET /atlas` | Atlas (AP-3) | the capability surface, from the service-pinned trust root |
| `POST /export` | Export (AP-5) | server-derived, attested evidence PDFs (dual-audited) |
| `GET /lane` · `GET /lane/inbox` · `GET /lane/rollup` | Lane (AP-6) | display-only derived assignments, inbox, and rollup (Ledger still reserved) |
| `POST /lane/box/{id}/status` · `POST /lane/inbox/{id}/accept` · `POST /lane/inbox/{id}/dismiss` | Lane (AP-6) | box-status + inbox accept/dismiss (dual-audited) |

### Org graph & directory

| Endpoint | Behavior |
| --- | --- |
| `GET /graph` | the scope-filtered org graph — structure only, no holdings (GR-3) |
| `GET /node/{id}/summary` | org-graph inspector: metadata-only counts (GR-7); **404 to unknown callers** (`is_known` gate, GR-8) and to non-principal ids |
| `GET /people` | the org-structural roster (AR-1) |
| `GET /workflow/project/{capability_id}` | a capability's project/workflow view |

### Agents & proposals (M4)

| Endpoint | Behavior |
| --- | --- |
| `POST /agent/{id}/run` | owner-only agent run; proposal-only, mutates nothing (the agent itself is refused) |
| `GET /proposals` | owner-scoped proposal list |
| `POST /proposals/{id}/approve` · `POST /proposals/{id}/reject` | owner-only and HUMAN-only; changes STATUS only |

### Access control (requests & grants)

| Endpoint | Behavior |
| --- | --- |
| `GET /access-requests` · `POST /access-requests` · `GET /access-requests/inbox` | list / create / inbox of access requests |
| `POST /access-requests/{id}/approve` · `POST /access-requests/{id}/deny` | decide a request — audited before any effect |
| `GET /access-grants` · `GET /access-grants/{id}` · `POST /access-grants/{id}/revoke` | list / read / revoke effective grants |

CORS allows exactly `http://localhost:3000` and `http://127.0.0.1:3000` (the
console); constructing the layer from any non-loopback origin is refused at
startup, mirroring the bind-refusal pattern (A-9x).

## M4: the proposal-only agent

An agent principal runs standing queries through the EXISTING governed
pipeline at its intersection scope and emits PROPOSALS — structured
suggestions with validated evidence. It executes nothing, mutates nothing,
approves nothing (AG-2 proves 100 runs leave every fixture, artifact, index,
and config byte-identical; the proposal store is the only thing that grows).

- **Capability rule:** agent code (`src/agent/runner.rs`) imports ONLY the
  context trait — `retrieve(query)` (the M3a pipeline AS the agent principal)
  and `propose(draft)`. Nothing else is reachable by construction.
- **Proposals** (`--agents-config config/agents.example.json --state-dir
  <dir>`): append-only JSONL event log, ordinal time, canonical JSON.
  `proposal_key = sha256(agent_id + query + sorted evidence ids)`;
  deduplication is scoped per snapshot. Drafts are validated like answers:
  citations 1..=4 inside the agent's compiled allowlist, rationale <= 600
  chars citing only its own evidence — any failure refuses the WHOLE
  proposal.
- **Authority:** `POST /agent/{id}/run` is owner-only (the agent itself is
  refused); `GET /proposals` is owner-scoped; approve/reject are owner-only
  and HUMAN-only (agent principals structurally refused). Every attempt —
  allowed or refused — lands in the append-only audit log BEFORE any effect.
  Approval changes STATUS and nothing else; execution is deliberately out of
  scope (a later, separately-gated program).
- **Snapshot pinning:** proposals pin the snapshot they were created under.
  Under any other snapshot they render `{status, standing_query, stale:
  true, refresh: "re-run to refresh"}` with the finding WITHHELD, and cannot
  be approved or rejected — stale evidence never renders, because the scope
  that justified it may no longer exist.
- No scheduler, no daemon, no background anything: runs are explicit,
  audited invocations only.

## Demo profile

`config.demo.json` is a clearly-labeled variant that raises ONLY the judge
timeout to 8000ms so slow hardware can demonstrate the judge path end to end.
**2000ms remains the production default** (`config.example.json` states it
explicitly), and no code path selects the demo value implicitly — only an
operator passing `--config service/config.demo.json` does.

**Demo identity caveat:** `X-Demo-Principal` is a stand-in for real OIDC and
every response says so (`demo_identity_mode: true`). Missing header → 401. No
default principal. An unknown principal gets the empty envelope — same shape
as a principal granted nothing, indistinguishable by construction (A-5).

## The answer pipeline (order is law)

identity → retrieval (M2b hybrid library path; its degradation doctrine
passes through, `retrieval_mode` says what actually ranked) → **mosaic
bound** (if both members of a tagged pair co-appear in this principal's
results, the lower-ranked member leaves the generation context;
`aggregation_bounded: true` discloses that the rule fired — never what it
hid; plain retrieval results keep both members) → **sealed context** (top 6
survivors as id + title + ≤480-char deterministic snippet; nothing else ever
reaches the generator) → **generate** (15s timeout; failure degrades to a
retrieval-only response) → **citation validation** (every bracketed segment
must exactly match a sealed-context id; any foreign citation refuses the
WHOLE answer; zero citations likewise — an uncited answer over a private
corpus is an unauditable claim) → canonical envelope.

The mosaic bound is applied conservatively: any tagged pair, for any
principal authorised to see both members. Pairs are harvested at startup from
the M1 compiled artifacts (hash-verified), the pass-through M1 built for
exactly this layer.

## Cache semantics

LRU, 256 entries, storing final canonical envelope bytes. Keyed on the
existing `query_hash` — which already pins the normalized query, principal,
`snapshot_version`, and `index_version`, so scope isolation is inherited and
any fixture change invalidates by construction (A-4) — plus the request's
mode flags, so a cached lexical envelope is never served for a hybrid ask.
Only clean envelopes are cached: transient degradations (embedder, judge, or
generator failures; citation faults) and unknown-principal envelopes are
recomputed, never pinned. `--no-cache` disables caching entirely.

## Offline governance (A-1..A-9)

MockGenerator + FileEmbeddings (full 600-doc committed fixture,
`tests/fixtures/embeddings_docs_full.json`, same synthetic hash-projection
provenance as M2b — regenerate with
`cargo test -p service --test fixture_gen -- --ignored`) + MockJudge. A-6
re-runs the stage-leak property over the FULL corpus and all 124 principals
through the whole service pipeline (closing the M2b review gap). The only
sockets tests open are loopback listeners in A-9's bind-refusal checks.
