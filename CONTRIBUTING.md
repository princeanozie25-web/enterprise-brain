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

DCO vs CLA is **deliberately undecided** until the launch ceremony (the
licence itself is still being finalised). Until then, contributions are
accepted on the understanding they will be licensed under the project's
eventual OSS licence; if that is unacceptable, wait for the decision.
