# Quickstart — three paths

[QUICKSTART.md](../QUICKSTART.md) at the repo root is the 3-command version;
this page is its grown-up sibling: Docker, native, and SDK, each executed as
written.

## 1. Docker (the packaged product)

**Needs:** Docker with Compose v2.

```sh
git clone https://github.com/princeanozie25-web/enterprise-brain.git
cd enterprise-brain
docker compose up --build -d
```

Two services run: **bootstrap** (a one-shot that mints the demo world — an
RSA key, six 24-hour agent tokens, a `DEMO`-labelled config — onto a volume)
and **gateway**, which serves once bootstrap completes and is `healthy` only
when `service doctor` passes AND the port answers. Published **host-loopback
only** (`127.0.0.1:8787`).

```sh
docker compose logs bootstrap        # the six tokens, as copy-paste curls
curl -s -H "Authorization: Bearer <TOKEN>" http://127.0.0.1:8787/v1/whoami
```

Prove the seam (same confidential object, two agents) and audit the ledger —
see [QUICKSTART.md](../QUICKSTART.md) for the exact transcript. Tear down
with `docker compose down` (add `-v` to also discard the demo world).

## 2. Native (no container runtime)

**Needs:** a Rust toolchain.

```sh
# provision the compiled artifacts + retrieval index (generated, not committed)
# (re-provisioning: the indexer refuses a non-empty retrieval/idx — delete it first)
cargo run -p scope-compiler -- compile --fixtures fixtures --out compiler/artifacts
cargo run -p retrieval     -- index   --fixtures fixtures --out retrieval/idx

# mint a demo world (prints the token curls); native worlds stay loopback-only
cargo run -p service --bin service -- bootstrap-dev --out dev-out

# serve on 127.0.0.1:8787
cargo run --release -p service --bin service -- \
  --fixtures fixtures --artifacts compiler/artifacts --idx retrieval/idx \
  --config dev-out/config.json
```

Then curl exactly as in the Docker path. `dev-out/` is gitignored — minted
credentials never enter the repo.

## 3. SDK (the real client)

**Needs:** Python ≥ 3.10 and a running gateway (either path above).

```sh
pip install enterprise-brain    # placeholder — not yet on PyPI; install from the sibling repo's wheel
```

```python
from enterprise_brain import Client

eb = Client(base_url="http://127.0.0.1:8787", token="<agent jwt from the bootstrap output>")

print(eb.whoami())                                   # Principal(principal_id='agent_...')
result = eb.retrieve("supplier audit findings")      # candidates, scoped at query construction
doc = eb.get_document(result.candidates[0].doc_id)   # full body iff authorized; NotFound otherwise
print(doc.metadata["source"])                        # "primary" or "s3"

retriever = eb.as_langchain_retriever()              # every document that enters model
docs = retriever.invoke("supplier audit findings")   # context is a ledgered allow
```

The SDK is a **dumb pipe by design** (no caching, no filtering, no authz
logic, token never logged) — enforcement lives in the gateway (EB-2). The
gateway's `404` means *nonexistent or not yours, indistinguishable*; the SDK
surfaces it as `NotFound` with exactly that sentence.
