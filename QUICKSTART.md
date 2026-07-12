# Quickstart — a governed gateway in one command

From a clone to a healthy gateway serving the governed fixture estate, with
demo agents whose tokens are minted **locally and never committed**. Then
prove the invariant the product exists to enforce: *a document does not decide
who may read it — the access model does.*

**Prerequisite:** Docker with Compose v2.

---

## The three commands

```sh
git clone https://github.com/princeanozie25-web/enterprise-brain.git
cd enterprise-brain
docker compose up --build -d       # build, bootstrap, serve — healthy in ~a minute
```

`compose up` runs two services: a one-shot **bootstrap** that mints the demo
world (an RSA key, 24-hour agent tokens, and a `DEMO`-labelled config) onto a
volume, and the **gateway**, which serves once bootstrap has finished. The
gateway is `healthy` only when its healthcheck — `doctor` (config sound)
**and** a port probe (serving) — both pass.

The gateway is published **host-loopback only** (`127.0.0.1:8787`): your
machine can reach it, the network cannot.

## Where the tokens are

The bootstrap service prints the tokens as copy-paste curls:

```sh
docker compose logs bootstrap
```

Copy the `Bearer …` token for the agent you want. The world **persists across
restarts** — a re-run bootstrap leaves a complete world untouched (a one-line
no-op, never a key rotation), so tokens stay valid for their 24-hour life. To
re-print a persisted world's tokens from the volume:

```sh
docker compose run --rm --no-deps --entrypoint cat gateway /data/dev-out/tokens.json
```

Expired (a day later)? Rotate deliberately —
[rotate-dev-keys](docs/how-to/rotate-dev-keys.md).

## Prove the invariant

**Who am I?** — the SDK handshake, one agent:

```sh
curl -s -H "Authorization: Bearer <TOKEN_FOR_agent_estate_confidential>" \
  http://127.0.0.1:8787/v1/whoami
# {"principal_id":"agent_estate_confidential"}
```

**The seam** — the *same* confidential estate object, two different agents:

```sh
DOC=s3/finance-restricted/2026/q1/budget-variance-ashcombe.md

# agent_estate_confidential (tier: confidential) -> 200, full body, source "s3"
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer <TOKEN_FOR_agent_estate_confidential>" \
  http://127.0.0.1:8787/v1/documents/$DOC
# 200

# agent_estate_internal (tier: internal) -> 404
curl -s -o /dev/null -w "%{http_code}\n" \
  -H "Authorization: Bearer <TOKEN_FOR_agent_estate_internal>" \
  http://127.0.0.1:8787/v1/documents/$DOC
# 404
```

Same object, same bytes, two answers. **The document did not decide. The
access model did.**

## Audit the ledger

Every one of those decisions — the 200 and the 404 — is a row in a hash-chained
ledger on the volume. Verify the chain, even against a stopped stack:

```sh
docker compose run --rm --no-deps gateway verify-ledger /data/dev-out/ledger/audit.jsonl
# CLEAN: N rows (… hash-chained) verify intact
```

(`--no-deps` is hygiene: the bootstrap one-shot is non-destructive by default
— a complete world is left untouched — so a dependency re-run can no longer
wipe the evidence; an audit command still has no reason to wake other
services.)

---

## Without Docker (native)

The same flow as plain processes — no container runtime required. Natively the
gateway keeps its **loopback-only** bind (`127.0.0.1:8787`); the config knob
that lets a container bind wider is never set here.

```sh
# 1. Provision artifacts + index, then mint the demo world.
#    (Re-provisioning: the indexer refuses a non-empty retrieval/idx — delete it first.)
cargo run -p scope-compiler -- compile --fixtures fixtures --out compiler/artifacts
cargo run -p retrieval     -- index   --fixtures fixtures --out retrieval/idx
cargo run -p service --bin service -- bootstrap-dev --out dev-out   # prints the token curls

# 2. Serve (loopback 127.0.0.1:8787).
cargo run --release -p service --bin service -- \
  --fixtures fixtures --artifacts compiler/artifacts --idx retrieval/idx \
  --config dev-out/config.json

# 3. In another shell, curl a token from step 1 against /v1/whoami or the seam doc.
```
