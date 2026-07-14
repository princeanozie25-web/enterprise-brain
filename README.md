# Enterprise Brain

**An authorization gateway for AI agents: authorization is proven before retrieval — unauthorized content never reaches the model.**

[![CI](https://github.com/princeanozie25-web/enterprise-brain/actions/workflows/ci.yml/badge.svg)](https://github.com/princeanozie25-web/enterprise-brain/actions/workflows/ci.yml)
[![conformance 94,500/0/0](https://img.shields.io/badge/conformance-94%2C500%2F0%2F0-2ea44f)](#conformance)
[![tests 351 passing](https://img.shields.io/badge/tests-351%20passing-2ea44f)](#conformance)
[![license AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)
[![SDK Apache-2.0](https://img.shields.io/badge/SDK-Apache--2.0-blue)](https://github.com/princeanozie25-web/enterprise-brain-python)
<!-- tests badge is static: bump "351 passing" when the workspace suite count changes (the CI badge above is live). -->

> **Enterprise Brain** is a provisional working title — a permanent name is completing trademark clearance.

If you've ever built an internal AI assistant, you know the dirty secret: it runs on one service account that can read everything, and the "security" is a system prompt asking the model nicely not to mention the salary spreadsheet. **A system prompt is not an access control.**

<p align="center">
  <img src="docs/assets/seam-demo.gif" alt="Agent A is cleared for a confidential document and gets it (200, full body); Agent B is not and gets a 404 identical to 'does not exist'; then verify-ledger proves every row of the exchange is hash-chained." width="820">
</p>

<p align="center"><sub>Animation not playing? Static frame: <a href="docs/assets/seam-demo.png">docs/assets/seam-demo.png</a></sub></p>

*Agent A is cleared for the document and gets it. Agent B is not — and gets a 404 identical to "does not exist." Then the ledger proves the whole exchange. The document did not decide; the access model did.*

## Try it

```sh
git clone https://github.com/princeanozie25-web/enterprise-brain.git && cd enterprise-brain
docker compose up
# tokens print in the bootstrap logs — then:
curl -H "Authorization: Bearer $AGENT_A" localhost:8787/v1/documents/s3/finance-restricted/2026/q1/budget-variance-ashcombe.md   # 200
curl -H "Authorization: Bearer $AGENT_B" localhost:8787/v1/documents/s3/finance-restricted/2026/q1/budget-variance-ashcombe.md   # 404
```

*Fully offline. Synthetic 750-document company. Demo credentials minted locally, never committed. Ten minutes from clone to handing your security team a tamper-evident ledger of everything the agent saw.*

The gateway publishes **host-loopback only** (`127.0.0.1:8787`); the demo world persists across restarts (bootstrap is non-destructive by default; rotation is a deliberate `--force`). Full walkthrough with the exact tokens: [QUICKSTART.md](QUICKSTART.md) · native and SDK paths: [docs/quickstart.md](docs/quickstart.md).

## Why this exists

- Out-of-scope documents are **never even searched** — excluded at query construction, so there is no post-filter seam to leak through snippets or scores.
- Your agent authenticates as **its own identity** (Entra JWT, 10-step fail-closed ladder) — not as a god-mode service account.
- Every document that enters model context corresponds to **a ledgered allow** — hash-chained; tamper with any row in history and `verify-ledger` names it.
- An agent probing beyond its scope raises **a structured alert in milliseconds** — off the request path, so alerting can never slow or break a request.
- Misconfiguration **says its name** — `doctor` preflight and an unhealthy container with the broken field in the logs, never a silent 401.

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

## Conformance

**94,500/0/0 estate-wide decision conformance; a single false-allow blocks release.** Every (principal × document) pair is checked against an oracle that recomputes expected access independently from the raw fixture facts — the gateway's answers must match all of them, and the suite re-runs on every change.

## Architecture

```text
                        ┌─────────────────────────── Enterprise Brain gateway ───────────────────────────┐
  agent (Entra JWT) ──▶ │  validation ladder ─▶ registry (tid,oid) ─▶ compiled scope ─▶ governed retrieval │ ──▶ only authorized
      /v1 (machine)     │        │                    │                   │  (scope INSIDE the query)      │      content
                        │        └────────────────────┴───────────────────┴──▶ hash-chained decision ledger │
  human (session) ────▶ │  console surface (separate door; a session never opens /v1, a JWT never opens it) │
                        └────────────────────────────────────────────────────────────────────────────────┘
   sources: primary corpus + estate connectors (bytes only — authority NEVER travels with a document)
```

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
| [docs/without-an-agent.md](docs/without-an-agent.md) | Using it with **no LLM** — permission-aware search + audit trail on their own |
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
