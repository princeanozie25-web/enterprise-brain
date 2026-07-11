# Connector certification

A connector joins the estate only through this rubric — the four clauses of
the certified-connector contract, half mechanical (the kit), half review.

## The contract

1. **Permission semantics documented.** State, in prose, exactly how the
   source's native authority maps to the estate access model — or declare it
   carries none (the seed connectors' stance, and the strongest form of the
   clause: nothing to map).
2. **ACL inheritance tested.** Nested containers/prefixes must not silently
   widen or narrow authority; a test pins the mapping you documented in (1).
3. **Revocation latency measured.** Publish the worst-case delay between an
   upstream permission change and the estate reflecting it.
4. **Conformance passed.** The source's (principal × object) matrix is
   oracle-checked 0-false-allow / 0-false-deny before release.

And one structural law the kit enforces on every run: **connectors deliver
bytes only** — any permission-shaped `native_meta` key (`acl`, `permissions`,
`roles`, `sensitivity`, …) refuses the whole source at ingest.

## Run the kit

Implement `service::estate::SourceConnector`, then in a test:

```rust
use service::conformance_kit::{poison_probe, run_kit, KitExpectations};

let report = run_kit(&my_connector, &KitExpectations {
    object_count: Some(150),                       // if known
    content_sha256: Some("<the access model pin>".into()),  // if pinned
    doc_id_prefix: Some("s3/".into()),             // how doc ids derive from native keys
});
assert!(report.all_ok(), "{}", report.to_human());
assert!(poison_probe().ok);                        // prove the authority guard fires
println!("{}", report.to_human());                 // paste THIS into the PR
```

The kit checks: `authority.bytes_only` (your connector smuggles nothing),
`enumerate.determinism` (two runs identical), `hash.round_trip` (the estate
content-hash law, stable and matching your pin), `object.count`, and —
via `poison_probe()` — that the engine's refusal fires live against a
deliberately poisoned connector, so a green run proves the guard, not just
your good manners.

**Ingest-time-only is a review criterion** (the kit says so in its report):
`enumerate()` runs at startup/load, never on the request path — no kit can
prove that from outside, so reviewers check the call sites.

## The checklist a connector PR must satisfy

- [ ] Clause 1 prose in the PR description (or the connector's module doc)
- [ ] Clause 2 test in the PR
- [ ] Clause 3 number in the PR description
- [ ] Clause 4 conformance run described (oracle + counts)
- [ ] Kit output pasted (`report.to_human()`), all ✓
- [ ] `poison_probe()` asserted in your test
- [ ] Reviewer confirms ingest-time-only call sites
- [ ] No credentials or live-source data in fixtures

Both in-repo proofs live in `service/tests/connector_kit.rs` — the real
`fs_bucket` connector certified against the real fixture pin, a json-corpus
connector showing the kit generalizes, and the poisoned fixture failing for
the right reason.
