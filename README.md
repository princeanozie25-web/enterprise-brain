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
