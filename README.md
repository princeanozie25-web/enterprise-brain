# Enterprise Brain

**An authorization gateway for AI agents: authorization is proven before retrieval — unauthorized content never reaches the model.**

> **Enterprise Brain** is a provisional working title — a permanent name is completing trademark clearance.

```text
                        ┌─────────────────────────── Enterprise Brain gateway ───────────────────────────┐
  agent (Entra JWT) ──▶ │  validation ladder ─▶ registry (tid,oid) ─▶ compiled scope ─▶ governed retrieval │ ──▶ only authorized
      /v1 (machine)     │        │                    │                   │  (scope INSIDE the query)      │      content
                        │        └────────────────────┴───────────────────┴──▶ hash-chained decision ledger │
  human (session) ────▶ │  console surface (separate door; a session never opens /v1, a JWT never opens it) │
                        └────────────────────────────────────────────────────────────────────────────────┘
   sources: primary corpus + estate connectors (bytes only — authority NEVER travels with a document)
```

## Ten lines to governed retrieval

```python
from enterprise_brain import Client

eb = Client(base_url="http://127.0.0.1:8787", token="<agent jwt>")

me = eb.whoami()                                  # who this token resolves to
result = eb.retrieve("supplier audit findings")   # candidates scoped AT QUERY CONSTRUCTION
doc = eb.get_document(result.candidates[0].doc_id)  # full body IFF authorized; else THE 404

retriever = eb.as_langchain_retriever()           # LangChain: every document that enters
docs = retriever.invoke("supplier audit findings")  # model context is a ledgered allow
```

**94,500/0/0 estate-wide decision conformance; a single false-allow blocks release.** Every (principal × document) pair is checked against an oracle that recomputes expected access independently from the raw fixture facts — the gateway's answers must match all of them, and the suite re-runs on every change.

## Three commands to a running gateway

```sh
git clone https://github.com/princeanozie25-web/enterprise-brain.git
cd enterprise-brain
docker compose up --build -d     # healthy in ~a minute; tokens: docker compose logs bootstrap
```

The gateway publishes **host-loopback only** (`127.0.0.1:8787`). Demo agent tokens are minted locally on a volume — never committed, and the world persists across restarts (bootstrap is non-destructive by default; rotation is a deliberate `--force`). See [QUICKSTART.md](QUICKSTART.md) for the two curls that prove the invariant, and [docs/quickstart.md](docs/quickstart.md) for the native and SDK paths.

---

## The invariants

The whole system is these seven statements; every feature since the first commit exists to keep them true:

- **EB-1 — Authorization before retrieval.** The caller's authority is established before any retrieval work begins; there is no "fetch then filter."
- **EB-2 — Enforcement is server-side.** No client, SDK, or prompt is trusted to enforce anything; the SDK is a dumb pipe by design.
- **EB-3 — Decisions are deterministic compiled lookups.** Authority compiles ahead of time into artifacts; a request-time decision is a lookup, not an inference.
- **EB-4 — Fail closed.** Missing key, unknown principal, unverifiable state, unledgerable surface — every uncertainty denies.
- **EB-5 — Exclusion at query construction.** Out-of-scope documents are never candidates: scope is a MUST clause inside the query, never a post-filter over a wider result.
- **EB-6 — Every decision is ledgered.** Allow and deny both write hash-chained ledger rows; a surface that cannot ledger does not serve.
- **EB-7 — Denies are monitoring signals.** A policy deny is security telemetry, alertable off the request path — not just a refusal.

## The seam, demonstrated

The same confidential estate object, two agents (from the executed [QUICKSTART](QUICKSTART.md)):

```text
whoami(agent_estate_confidential)                       -> 200 {"principal_id":"agent_estate_confidential"}
GET /v1/documents/s3/finance-restricted/.../budget-variance-ashcombe.md
  as agent_estate_confidential (tier: confidential)     -> 200, full body, metadata.source = "s3"
  as agent_estate_internal     (tier: internal)         -> 404
verify-ledger audit.jsonl                               -> CLEAN: 15 rows (15 hash-chained) verify intact
```

**The document did not decide. The access model did.** Objects carry bytes; authority lives in a separate access model the objects know nothing about.

## Documentation

| Where | What |
| --- | --- |
| [docs/quickstart.md](docs/quickstart.md) | Docker, native, and SDK paths from zero |
| [docs/concepts.md](docs/concepts.md) | The invariants, the two surfaces, authority-vs-bytes |
| [docs/how-to/](docs/how-to/) | Register an agent, add a source, rotate dev keys, read the ledger, run doctor, verify a ledger |
| [docs/reference/](docs/reference/) | `/v1` API, ServiceConfig schema, CLI (`bootstrap-dev` / `doctor` / `verify-ledger`) |
| [docs/runbook-denials.md](docs/runbook-denials.md) | **Every deny class → where the truth lives → the fix** (the wire is mute on purpose) |
| [docs/threat-model.md](docs/threat-model.md) | Assets, trust boundaries, the ladder as attack surface, what hash-chaining does and does not claim |
| [docs/connector-certification.md](docs/connector-certification.md) | The certified-connector rubric + the conformance kit |

## Posture

- **Synthetic corpus.** Every document, person, department, and organization in `fixtures/` is generated fiction — no real company data, no real people, no real records.
- **No telemetry.** The gateway and the SDK make no calls except the ones you configure; nothing phones home.
- **Entra status.** Supports Entra workload identities today; autonomous Agent ID attestation is integrated fail-closed and pending live validation against Microsoft's preview token issuance ([detail](docs/s0b-launch-gate.md)).
- **Solo maintainer.** Best-effort responses; security reports get priority — see [SECURITY.md](SECURITY.md). No SLA is implied.
- **Licensing.** The gateway, policy engine, and conformance suite are AGPL-3.0. The Python SDK (separate repository) is Apache-2.0. A commercial licence for the enterprise control plane will be offered separately. See [LICENSE](LICENSE).
