# Enterprise Brain M0 — synthetic company + ground-truth ACL oracle

A deterministic generator for **Bryremead Distribution Ltd**, a fictional UK
GDP-regulated pharmaceutical wholesaler: 120 synthetic people, 14 groups, 4 agent
principals, 600 documents across 5 mimicked sources, a 6→18→40→90 BRM graph, and a
fully materialized access-control ground truth. This dataset is the test substrate for
a permission-aware retrieval system whose release rule is *"a single false-allow
blocks release."* Everything is fictional; every person record carries
`"synthetic": true`, and a denylist test fails generation if any real
company/distributor name appears in any output file.

## Regenerate

```sh
python -m synth.generate --seed 42 --out fixtures/   # byte-identical to committed fixtures
python -m pytest                                     # P-1..P-9 + module tests
```

Python 3.11+, stdlib + pytest only. No network, no databases, no LLMs anywhere.

## Oracle guarantees

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

## The Lane's derivation rule (AP-6, verbatim)

Aperture's rooms are Ask / Lens / Atlas / **Lane**, with Ledger still
reserved (charter amendment, AP-6). The Lane is v4a — display only — and
because `/synth` is frozen, its assignments DERIVE:

> At startup, deterministically derive assignments from verified inputs
> only: for each human principal, take the capabilities whose realizing
> documents' departments match the principal's department AND where the
> principal has >=1 visible realizing doc; rank by visible-doc count
> (tie-break capability id asc); cap at 8 boxes per person. Every derived
> box carries `derived: true` and the console labels the lane "Derived
> assignments (demo)". Agents get no lane (boxes are human work).

Interpretation note (flagged in the AP-6 closeout): "realizing documents'
departments match" is read as *at least one* realizing document's
department equal to the principal's department; visibility is membership
in the principal's compiled M1 allowlist. Implemented in
`service/src/lane.rs::seeds_for_human`; held deterministic by AW-7 (two
startups derive byte-identical lanes).

## Ask controls (console)

The Ask surface (`/ask`) has two toggles. Both sit on top of the engine's
permission scope — they never widen what you can see; they only change *how*
the system searches and how careful it is before answering.

- **Broad search** — when on, Ask finds documents by **meaning as well as exact
  keywords** (keyword/BM25 *and* vector/embedding search, combined with
  Reciprocal Rank Fusion, `RRF_K = 60`, in `retrieval/src/fuse.rs`). When off,
  Ask falls back to **keyword-only** (`lexical_only`) search, so close-but-not-
  exact wording can be missed. (Engine names: this is the "hybrid" retrieval
  mode in `service/src/answer.rs`.)
- **Verified answers** — when on, after an answer is drafted from your
  authorised documents the system **checks each claim against that evidence and
  leaves out anything it cannot support** (fail-closed). When off, that
  verification step is skipped. (Engine name: the "judge" grounding pass.)

**Current limitation (honest):** in this build **both toggles are DISABLED in
the UI** and only keyword-only answers run. Each switch is greyed out, cannot be
flipped, and shows an always-visible reason — "semantic index not loaded"
(Broad search) and "verification model not loaded" (Verified answers) — because
turning them on would hit an engine path that currently errors: the regenerated
corpus dropped its vector index (AR-1b), and no judge model is running. They are
disabled rather than left flippable so no one can trigger a 500 from the UI.
Re-enabling each is a single flag in `console/src/components/Console.tsx`
(`BROAD_SEARCH_AVAILABLE`, `VERIFIED_ANSWERS_AVAILABLE`); flip it back to `true`
once the engine supports that mode. Rebuilding the vector index, running a judge
model, and adopting RRF end-to-end are separate, sequenced engine tasks.
