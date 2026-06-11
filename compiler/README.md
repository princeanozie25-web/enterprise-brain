# Enterprise Brain M1 — Scope Compiler

Turns `(principal, permission snapshot)` into a compiled, pinned, reason-traced
allowlist. Pure compilation: no network, no database, no retrieval, no LLM, no
wall clock. M0's materialized oracle (`/fixtures/ground_truth.jsonl`, 74,400
decisions) judges it; a single false-allow fails the build.

```sh
cargo run --release -- compile --fixtures ../fixtures --out artifacts/
cargo run --release -- verify  --fixtures ../fixtures --artifacts artifacts/
cargo test --release            # conformance suite C-1..C-6
```

## Independence invariant

The compiler implements the M1 access semantics from the spec and the raw data
fixtures (`company.json`, `documents.json`, `traps.json`) alone. It never
reads `/fixtures/ground_truth.jsonl` and is not derived from `/synth`. Only
the conformance harness (`tests/conformance.rs`) reads the oracle.

## Access semantics (`src/semantics.rs`, deny by default)

1. **ReBAC** — `acl_refs` rules of kind `public`/`group`/`role` grant read,
   OR'd. Reasons: `REBAC:<group_id>`, `REBAC:role:<role>`, `REBAC:public`.
2. **ABAC on top, AND'd where present** — `attr_site` requires principal site
   match; `attr_band_min` requires `employment_band >= min_band`;
   `special_category` requires membership of `grp_hr` OR subject identity.
   Reasons: `ABAC:site_match:<site>`, `ABAC:band_min:<n>`,
   `ABAC:special_category_hr`, `ABAC:special_category_subject`.
3. **Subject access** — a person always reads their own HR record
   (`SUBJECT:self`), unconditionally ("always" is read as bypassing ABAC; no
   fixture document distinguishes the interpretations). The manager's org edge
   grants nothing — no such rule exists.
4. **Agents** — effective access = grant ∩ owner access, per (agent, document).
   The grant side is evaluated against the same rules with only the attributes
   the grant carries; a missing site/band can never satisfy a condition.
   Reason marker: `AGENT:intersect(owner)` plus the grant-side reasons.
5. **Public sensitivity** — readable by every principal (agents included),
   short-circuiting rules 1–4. Reason: `PUBLIC:sensitivity`.
6. **Effective version** — a superseded document stays readable but its entry
   carries `superseded: true` and `effective_successor` (the terminal document
   of the supersedes chain; all fixture chains are single-hop).
7. **Mosaic tags** — passed through untouched onto entries of both documents in
   each pair; never used for decisions. The M1 spec says the metadata lives "in
   documents", but `documents.schema.json` is `additionalProperties: false`
   with no such field — the only place the tags exist is the `mosaic` section
   of `traps.json`, so the compiler reads that section as opaque pass-through
   input (flagged in the closeout).

## Artifacts (canonical JSON: sorted keys, compact, trailing newline)

One `<principal_id>.json` per principal plus `index.json`. Entries are sorted
by document id, reasons sorted and deduplicated, `compiled_at` is the fixed
epoch `2026-01-05T00:00:00Z` (no wall clock anywhere), and `snapshot_version`
is the SHA-256 of a manifest of per-file SHA-256es of the three input fixtures
(`src/snapshot.rs`). `denied_count` is a count — denials are never enumerated.
Two compiles over the same fixture bytes are byte-identical (C-4).

## Fail-closed behaviour

- Unknown principal id → empty allowlist artifact, logged, exit 0. (An id that
  cannot name an artifact file, e.g. containing `/`, refuses instead.)
- Schema/parse failure in any fixture → refuse entirely, nonzero exit
  (`deny_unknown_fields` everywhere plus structural validation: band ranges,
  payload-per-kind coherence, `fictional`/`synthetic` flags, date shapes).
- Fixture bytes changed after the snapshot → compile re-verifies before
  persisting; `verify` refuses artifacts against any other fixture bytes.
- Duplicate ids or dangling references (author, group, supersedes, subject,
  member, owner, site) → refuse. Supersedes fan-in and cycles also refuse,
  since "effective successor" would be ambiguous or undefined.

## Conformance suite (`tests/conformance.rs`)

| Test | Checks |
| --- | --- |
| C-1 | all 74,400 (principal, document) decisions vs the oracle; false_allows == 0 AND false_denies == 0 |
| C-2 | 15 confused-deputy, 8 manager-overreach, 6 cross-site DENY; 12 effective-version entries carry marker+successor; 10 mosaic pairs carry tags |
| C-3 | the four fail-closed behaviours, via the real binary |
| C-4 | two compiles produce byte-identical artifacts |
| C-5 | one flipped fixture byte changes `snapshot_version`; old artifacts refuse verification against it |
| C-6 | full 124-principal compile < 2.0 s (release; measured 0.379 s) |
