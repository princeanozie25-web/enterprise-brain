# Enterprise Brain

**AI for your organisation's knowledge that can't leak what you're not allowed to see.**

---

## What it is

Companies want to put AI on their internal documents — policies, contracts, HR
records, customer files — so anyone can ask a question in plain English and get
an answer grounded in the company's own knowledge. The danger is obvious the
moment you say it out loud: the AI can show someone something they were never
allowed to see.

**Enterprise Brain is the layer that makes this safe.** It works out what you're
permitted to see *before* the AI is handed anything, so the AI can only ever
answer from documents you're authorised for. It isn't a chatbot — it's the
governance that makes putting a chatbot on sensitive data possible.

## The problem

Most systems do it backwards. They fetch documents, hand them to the AI, and
*then* try to filter the answer. But once unauthorised content is in front of
the model, you can't take it back — the leak has already happened. Other tools
give every employee the same view of the company's knowledge regardless of their
access.

For a regulated business — pharma, finance, healthcare — a single answer that
crosses a permission boundary is a compliance breach. So the project meant to
roll AI out across the company quietly dies at the security review.

## The key idea

**Enterprise Brain checks permission *before* retrieval, not after.** Your access
is compiled into the search itself — a hard filter applied before the model
receives a single document — so it is structurally impossible for the model to
be handed something you can't see.

And it **fails closed**: no explicit permission means the answer is *nothing*,
never a guess. Even the org chart obeys this — you see only the part of the
company inside your remit, and someone with no access sees an empty map, not a
blurred-out one.

## The proof

This isn't asserted, it's measured. Enterprise Brain is tested against a
complete synthetic company — a fictional, regulated pharmaceutical wholesaler
with **120 people** and **600 documents** — by checking *every possible access
decision* exhaustively:

| What's checked | Decisions | Wrong answers |
| --- | ---: | ---: |
| **Who can read which document** | **74,400** | **0** |
| **Who can see which part of the org map** | **15,500** | **0** |

Zero over-shared, zero wrongly hidden, in either matrix. Crucially, the
"correct" answer for each check is worked out **independently** from the raw
company data — so the tests genuinely catch mistakes instead of quietly agreeing
with the system they're testing. **One wrong answer fails the whole release.**

> Scope of the 15,500: this proves *org-map / metadata visibility* — which
> people, departments, and systems each identity may see. Visibility that flows
> from one-off access grants and project assignments is a separate, in-progress
> slice (see [Status](#status)); the number is not claimed to cover it.

## See it

> **[ screenshot placeholder ]** — the same org map for two identities, side by
> side: a finance lead sees a rich slice of their part of the company; an
> identity with no standing sees an empty map (not a blurred one). *To be added
> once the console's live render is wired to the new permission layer; the
> behaviour is already proven on the engine — see [The proof](#the-proof).*

## How it works

Five layers, each fail-closed, each refusing rather than guessing:

1. **Identity is session-bound, never asserted.** You authenticate once and the
   server mints a signed, expiring session. Identity is resolved from that
   session — never from a header the caller can set — so a request can't simply
   *claim* to be someone. No valid session, no answer (HTTP 401).

2. **Your permissions are compiled.** Each identity's access is worked out ahead
   of time into an explicit allowlist — a fixed, tamper-checked statement of
   exactly which documents that identity may read, derived from their team, role,
   site, and seniority.

3. **The allowlist is part of the search, not a filter on the results.** When you
   ask a question, your allowlist is injected into the retrieval query as a
   required clause — a *pre-retrieval* filter. Out-of-scope documents are never
   fetched in the first place, so they can never reach the model. (This is the
   opposite of fetch-everything-then-redact.)

4. **The answer is grounded and audited.** The model only ever sees a sealed set
   of in-scope documents. Every citation in its answer is checked back against
   that sealed set; an answer that cites anything outside it — or cites nothing
   at all — is refused whole. The text generator is treated as untrusted by
   construction.

5. **Metadata obeys the same rule.** The org map, the per-node summaries, and the
   "who-can-see-what" lens are scoped to *your* projection of the company. Out of
   scope is **absent** — never hidden-but-counted, never blurred, never a
   padlock. An identity with no standing gets an empty map.

Underneath all of this sits a conformance-proven decision core (the document
matrix above), consumed read-only. Nothing in the serving path can widen what it
decided.

---

## Technical detail (for engineers)

Everything below is the substrate the product story above is built on: the
synthetic company the whole system is tested against, and the independently
computed access-control ground truth that produces the 74,400/0/0 number.

### M0 — the synthetic company + ground-truth ACL oracle

A deterministic generator for **Bryremead Distribution Ltd**, a fictional UK
GDP-regulated pharmaceutical wholesaler: 120 synthetic people, 14 groups, 4 agent
principals, 600 documents across 5 mimicked sources, a 6→18→40→90 BRM (Business
Reference Model) graph, and a fully materialized access-control ground truth.
This dataset is the test substrate for a permission-aware retrieval system whose
release rule is *"a single false-allow blocks release."* Everything is fictional;
every person record carries `"synthetic": true`, and a denylist test fails
generation if any real company/distributor name appears in any output file.

#### Regenerate

```sh
python -m synth.generate --seed 42 --out fixtures/   # byte-identical to committed fixtures
python -m pytest                                     # P-1..P-9 + module tests
```

Python 3.11+, stdlib + pytest only. No network, no databases, no LLMs anywhere.

#### Oracle guarantees

The oracle is the *independent* definition of "correct" — computed from first
principles, not from the system under test. It is what makes the 74,400/0/0 a
real proof rather than a tautology.

1. `oracle.resolve(principal, resource)` is a pure function over the generated model — no caching, no randomness, no I/O.
2. It is computed from first principles (direct rule evaluation in `synth/acl.py`) and depends on nothing that will later be the system under test.
3. Deny-by-default: no matching grant rule means DENY, and unknown rule kinds fail closed.
4. Every decision carries at least one stable reason id, so any row in `ground_truth.jsonl` can be audited back to the rule that produced it.
5. ReBAC grants (public/group/role) are OR'd; ABAC constraints (site, employment-band minimum) are AND'd on top.
6. `special_category` documents additionally require explicit HR-group membership — except that a person can always read their own HR record, and their manager never can via the org edge.
7. Agent access is the explicit intersection `agent grant ∩ owner access`, computed per pair; agents never inherit owner scope implicitly, which is what the 15 confused-deputy traps verify.
8. The matrix is total — all 124 × 600 = 74,400 pairs — and regeneration with the same seed is byte-identical (no wall clocks, one seeded RNG).
9. Generation aborts rather than emitting weak fixtures: trap minimums (12/10/15/8/6), the < 35% overall allow-rate ceiling, the < 5% restricted+special ceiling, and the denylist are all enforced at build time.
10. Trap inventory is tagged in `fixtures/traps.json` and re-verified against the oracle both at generation time and in pytest (P-5..P-8).

#### The metadata conformance oracle

The same methodology, applied to the org map. For every (principal, node) pair —
124 principals × 125 nodes (the org core + 120 people + 4 agents) = **15,500
decisions** — the expected visibility is computed independently from the raw
company structure (department, manager, group standing) and checked against the
live metadata surfaces: **0 false-allow / 0 false-deny**
(`service/tests/metadata_conformance.rs`). A principal with no group standing
projects to the empty set. (Grant- and capability-reachable visibility is the
in-progress AUTH-2b slice and is intentionally outside this count.)

### Component notes

**The Lane's derivation rule (AP-6, verbatim).** Aperture's rooms are Ask / Lens
/ Atlas / **Lane** and **Export** (AP-5, server-derived attested evidence PDFs),
with Ledger still reserved (charter amendment, AP-6). The Lane is v4a — display
only — and because `/synth` is frozen, its assignments DERIVE:

> At startup, deterministically derive assignments from verified inputs
> only: for each human principal, take the capabilities whose realizing
> documents' departments match the principal's department AND where the
> principal has >=1 visible realizing doc; rank by visible-doc count
> (tie-break capability id asc); cap at 8 boxes per person. Every derived
> box carries `derived: true` and the console labels the lane "Derived
> assignments (demo)". Agents get no lane (boxes are human work).

Interpretation note (flagged in the AP-6 closeout): "realizing documents'
departments match" is read as *at least one* realizing document's department
equal to the principal's department; visibility is membership in the principal's
compiled M1 allowlist. Implemented in `service/src/lane.rs::seeds_for_human`;
held deterministic by AW-7 (two startups derive byte-identical lanes).

**Ask controls (console).** The Ask surface (`/ask`) has two toggles. Both sit on
top of the engine's permission scope — they never widen what you can see; they
only change *how* the system searches and how careful it is before answering.

- **Broad search** — when on, Ask finds documents by **meaning as well as exact
  keywords** (keyword/BM25 *and* vector/embedding search, combined with
  Reciprocal Rank Fusion, `RRF_K = 60`, in `retrieval/src/fuse.rs`). When off,
  Ask falls back to **keyword-only** (`lexical_only`) search, so close-but-not-
  exact wording can be missed. (Engine name: the "hybrid" retrieval mode.)
- **Verified answers** — when on, after an answer is drafted from your authorised
  documents the system **checks each claim against that evidence and leaves out
  anything it cannot support** (fail-closed). When off, that verification step is
  skipped. (Engine name: the "judge" grounding pass.)

**Current limitation (honest):** in this build **both toggles are DISABLED in the
UI** and only keyword-only answers run — each switch is greyed out with an
always-visible reason — because the regenerated corpus dropped its vector index
and no judge model is running, so turning them on would hit an engine path that
currently errors. Re-enabling each is a single flag in
`console/src/components/Console.tsx` once the engine supports that mode.

---

## Status

**Pre-pilot, and proven on synthetic data only — no real company's information is
involved.** The honesty here is deliberate: the system is built to be provably
bounded, and the documentation holds itself to the same standard.

Live (enforced on the engine):
- **Authorisation-before-retrieval for documents** — the allowlist compiled into
  the query, fail-closed, conformance-proven at 74,400/0/0.
- **Session-bound identity** — login mints a signed session; the header can no
  longer assert who you are.
- **Per-identity scoping of the org map** — structural visibility, proven at
  15,500/0/0.

In progress:
- **Grant/capability-reachable visibility** (AUTH-2b) — completes metadata scoping.
- **Admin "view-as"** (AUTH-3) — re-enables cross-identity views for authorised admins.
- **Spend governance** and further security hardening.
- **Console live render** — wiring the cockpit's org-map view to the new session
  + scoping layer (the engine already enforces it).

## Running it

The system is a Rust engine plus a Next.js console, over the synthetic corpus.

```sh
# 1. Provision the compiled artifacts + retrieval index from the fixtures
cargo run -p scope-compiler -- compile --fixtures fixtures --out compiler/artifacts
cargo run -p retrieval     -- index   --fixtures fixtures --out retrieval/idx

# 2. Run the governed engine (loopback only, 127.0.0.1:8787)
cargo run --release -p service -- \
    --fixtures fixtures --artifacts compiler/artifacts --idx retrieval/idx \
    --agents-config config/agents.example.json --state-dir .state/agent-store

# 3. Run the console
cd console && npm install && npm run dev   # http://localhost:3000
```

The console runs in **Demo Identity Mode**: you pick a Work Identity to sign in
as (a server-minted session — the demo stand-in for real OIDC), and a
non-dismissible banner says so on every screen. Tests: `cargo test` (Rust) and
`npm test` (console). See [`service/README.md`](service/README.md) for the full
endpoint reference.
