# Contributing

## Build and test

```sh
cargo run -p scope-compiler -- compile --fixtures fixtures --out compiler/artifacts
cargo run -p retrieval     -- index   --fixtures fixtures --out retrieval/idx
cargo test --workspace          # the whole bar: every suite green
```

(Re-provisioning: the indexer refuses a non-empty `retrieval/idx` — delete it
first. The service package has two binaries; interactive runs use
`cargo run -p service --bin service -- …`.)

Docker path: `docker compose up --build -d` (see [QUICKSTART.md](QUICKSTART.md)).

## The invariant bar

The conformance suites are the contract: **the (principal × document)
matrices must stay 0 false-allows / 0 false-denies**. A PR that moves a
pinned count without an oracle-side justification — a change to the *fixture
facts* that the oracle independently recomputes — is rejected regardless of
how green the rest looks. Never post-filter for scope (EB-5); never serve
unledgered (EB-6); never put a deny reason on the wire.

`rustfmt` new leaf files; do not reformat `service/src/lib.rs` wholesale.

## Contributing a connector

The path is the [conformance kit](docs/connector-certification.md): implement
`SourceConnector`, run the kit against it, and submit the kit's output with
the four contract clauses addressed in your PR description. Connectors
deliver **bytes only** — a permission-shaped `native_meta` key fails the kit
and the PR.

## Licensing of contributions

Contributions land under the **Developer Certificate of Origin**
([developercertificate.org](https://developercertificate.org)): sign every
commit with `git commit -s`, which adds the `Signed-off-by:` trailer
asserting you have the right to submit the work. The project licence is
AGPL-3.0 (see [LICENSE](LICENSE)); your contribution is licensed the same
way. The DCO may be upgraded to a CLA at company formation — commits signed
before any such change remain governed by the DCO they were made under.
