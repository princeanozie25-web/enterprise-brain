# Concepts

## The problem in one sentence

An AI agent with retrieval reads whatever its retriever can see; if authority
is applied after retrieval — or trusted to the model — unauthorized content
has already reached the context window. Enterprise Brain inverts that:
**authorization is proven before retrieval**, structurally.

## The invariants (EB-1..7)

- **EB-1 — Authorization before retrieval.** Identity resolves and authority
  is established before any retrieval work begins.
- **EB-2 — Enforcement is server-side.** No client, SDK, or prompt is trusted
  with enforcement; the Python SDK is deliberately a dumb pipe (no filtering,
  no caching, no authz logic).
- **EB-3 — Deterministic compiled lookup.** Authority compiles ahead of time
  (the scope compiler) into pinned artifacts; a request-time decision is a
  lookup, never an inference.
- **EB-4 — Fail closed.** Unknown principal, missing key, unverifiable hash,
  unledgerable surface, unknown sensitivity — every uncertainty denies.
- **EB-5 — Exclusion at query construction.** Scope is a MUST clause inside
  candidate formation (a `TermSetQuery` over the allowlist on the primary
  index; the authority predicate inside the estate index's candidacy). An
  out-of-scope document is never a candidate — never scored, never ranked,
  never snippeted. Post-filtering a wider result would be a violation, and
  probes test for exactly that.
- **EB-6 — Every decision is ledgered.** Allow AND deny write append-only,
  hash-chained, timestamped rows. A surface that cannot ledger does not serve
  (no ledger ⇒ no `/v1`).
- **EB-7 — Denies are monitoring signals.** Policy-class denies emit alerts
  off the request path; the ledger is projectable by ordinal into telemetry.

## The two surfaces

| | Console (human) | `/v1` (machine) |
| --- | --- | --- |
| Credential | Server-minted session (cookie) | Entra agent JWT (bearer) |
| Crossing | A session on `/v1` is refused (`session_credential_on_v1`) | A JWT never opens a console room |
| Authorization | Same compiled scopes, per-identity projections | Same compiled scopes via the agent registry |
| Deny shape | Room-appropriate UI states | Generic 401 / THE one byte-identical 404 |

One authorization engine, two doors, no shared credentials. The bridge maps a
validated `(tid, oid)` to a registered principal and *nothing else* — an
unregistered agent is denied, an `azp`/parent-app match grants nothing.

## Authority vs bytes (the connector sentence)

**Permissions do not live with the document.** A connector delivers bytes at
ingest time; the access model — a file the objects know nothing about —
delivers authority. A connector that emits anything permission-shaped in its
metadata is refused whole (ingest fail-closed), because a source that can
smuggle authority past the access model is a second, ungoverned decision
maker. This is the certified-connector contract's first clause, and the
conformance kit probes it with a deliberately poisoned connector.

## Fail closed, explain loudly

The wire is generic; the operator's surfaces are precise:

- a malformed config **fails startup naming the field**;
- `service doctor` runs the same checks as a read-only preflight (and as the
  container healthcheck), each ✓/✗ with the exact fix;
- every deny's reason is a ledger row (see the
  [denial runbook](runbook-denials.md));
- the ledger's hash chain makes tampering evident (`verify-ledger` names the
  breaking ordinal).

Silence toward the caller and loudness toward the operator are the same
design decision viewed from two sides.
