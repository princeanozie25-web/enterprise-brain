# Enterprise Brain M2 — Governed Retrieval (M2a lexical + M2b hybrid)

Retrieval that operates STRICTLY INSIDE the M1 compiled allowlists. M2a built
BM25, RRF behind the `RankSource` trait, the count-free envelope, and the
R-1..R-7 governance harness. M2b adds a vector `RankSource` (local
embeddings), an optional local LLM judge for final ordering, the metering
sidecar, and the successor-redaction hardening — all judged by the same
harness, extended to R-13.

```sh
# lexical index            # hybrid index (embeds the corpus via local Ollama)
cargo run --release -p retrieval -- index --fixtures fixtures --out idx/
cargo run --release -p retrieval -- index --fixtures fixtures --out idx/ \
    --hybrid --config retrieval/config.example.json

cargo run --release -p retrieval -- query --principal p060 \
    --artifacts compiler/artifacts --idx idx/ --q "payroll salary review" \
    [--hybrid] [--judge] [--config retrieval/config.example.json] \
    [--usage-out usage.jsonl] [--include-superseded] [--k 10]

cargo test -p retrieval --release      # R-1..R-13, fully offline
```

## The core rule

No document outside the querying principal's allowlist may appear in ANY
stage's output. The allowlist restriction is applied INSIDE the search: BM25
takes it as a `TermSetQuery` Must clause of the tantivy query; the vector
stage computes exact cosine ONLY over (allowlist ∩ partition). There is never
an unfiltered ranking to post-filter. R-1 (lexical) and R-8 (hybrid) assert
the property over 6,000 + 2,976 instrumented searches; zero tolerance.

## The network carve-out

`src/local_llm.rs` owns ALL network use in the workspace: a ~100-line
std-only HTTP/1.0 client that REFUSES, at construction, any endpoint whose
host does not resolve to loopback (127.0.0.0/8 or ::1; https refused — no TLS
stack, no cloud path). Hostnames resolve once at construction and connections
only ever go to the verified loopback addresses. Exactly two operations ride
it: embeddings (`/api/embed`) and judge chat (`/api/chat`). Zero retries;
explicit deadlines (index embed 5000ms/batch, query embed 1500ms, judge
2000ms). Tests construct it only with literal IPs (no DNS, no sockets).

## Hybrid pipeline

allowlist restriction → BM25 + exact-cosine vector (top 50/partition) as two
`RankSource`s → RRF (k=60, tie-break doc id asc) → optional judge over the
fused top-12. The judge sees ONLY (id, title, 240-char snippet) triples for
in-scope candidates — snippets are baked into the vector store at index time,
so nothing longer exists to hand it — and returns an order. Foreign ids in
its output are discarded and counted as judge faults (trace-only; never in
the envelope). Elision is deterministic: the judge runs only when fused
candidates ≥ 4 AND fused top1/top2 < 1.3.

## Degradation doctrine (R-10, each branch a test)

- Query-time embedder failure → lexical_only, byte-identical to an honest
  lexical run, exit 0. `envelope.retrieval_mode` never lies.
- Index-time embedder failure → the build FAILS (an index silently missing
  vectors is a lie).
- Judge failure/timeout/empty → fused order stands, `judge_applied=false`.
- Manifest/config model-or-dim mismatch → the query REFUSES (stale vectors
  must not rank). Less compute degrades; less governance never does.

## Index identity

`index_version` = sha256 of the canonical manifest. A hybrid build adds the
embedding model id, dimension, and the sha256 of every per-partition vector
file to the manifest, so vectors are part of the index identity (R-12);
tampered vector files refuse to load. A lexical-only build's manifest bytes
are byte-identical to M2a's.

## Effective version at serve time

Default: superseded documents are removed from the restriction itself — they
appear at no stage; the successor ranks by its own match iff allowed. With
`--include-superseded`, in-allowlist superseded docs appear carrying
`superseded: true`, and (M2b hardening, R-13) `effective_successor` is
emitted ONLY when the successor itself is in the allowlist — an
out-of-allowlist successor id is omitted entirely, not nulled.

## Metering sidecar (the Bursar seam)

With `--usage-out`, every judge / query-time embed call appends one JSONL row:
`{cost_usd: null, estimated, input_tokens, model, output_tokens, ts}` — token
counts from the local API when reported, else bytes/4 with `estimated: true`;
`ts` is a deterministic monotone ordinal (the workspace permits no wall
clock). No content, ever. Caching is M3's concern, keyed on the existing
`query_hash`; no cache exists here by design.

## Test fixtures provenance

`tests/fixtures/embeddings_{docs,queries}.json` hold vectors for the
40-document subset corpus + 12 query texts used by the offline hybrid
harness. They are SYNTHETIC: a deterministic sha256 hash-projection
(`tests/common/mod.rs::synthetic_embedding`, model id
`fixture-synthetic-256-v1`, dim 256) — NOT model output, chosen so anyone can
regenerate them byte-identically without any model:
`cargo test -p retrieval --test fixture_gen -- --ignored`.

## Governance harness

| Test | Property |
| --- | --- |
| R-1..R-7 | M2a battery (stage leaks, traps, effective version, dark counts, partition discipline, determinism, perf) — still green |
| R-8 | hybrid stage-leak: 2,976 searches, every id at every stage (embedding candidates, cosine set, fusion inputs, judge in/out) ∈ allowlist |
| R-9 | judge input seal: only allowed ids + their own snippets; foreign output ids discarded + counted, envelope unaffected beyond order |
| R-10 | all four degradation branches |
| R-11 | elision boundaries on both sides of 1.3 and 4, pure + wired |
| R-12 | hybrid byte-determinism; rebuild → identical `index_version`; vector files in the hash; tamper refusal |
| R-13 | successor redaction under `--include-superseded` (extends R-3) |

Only tests read `traps.json`; nothing reads `ground_truth.jsonl` here. M1
artifacts are consumed read-only; the compiler crate is a dev-dependency of
the harness only.
