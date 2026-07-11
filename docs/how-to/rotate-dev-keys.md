# How to: rotate the dev keys

Demo/dev worlds use a locally-minted RSA key (`bootstrap-dev`). Rotation =
regenerate the world; there is no in-place rotation for dev keys, on purpose
(fresh world, fresh evidence, no half-rotated states).

## Native

```sh
service bootstrap-dev --out dev-out --force
```

`--force` removes only the artifacts bootstrap owns (keys, tokens, config,
ledger, alerts) and mints a fresh keypair + six fresh 24-hour tokens.
**Restart the gateway** — it loads the JWKS at startup, so until restart it
still trusts the OLD key (and the new tokens 401 with `signature_invalid`).

## Docker

```sh
docker compose down && docker compose up -d     # bootstrap re-runs; gateway restarts on the new world
docker compose logs bootstrap                    # the fresh tokens
```

Every `up` that re-runs the bootstrap one-shot IS a rotation. Old tokens die
with the old key.

## Two sharp edges

- The ledger is part of the world `--force` removes: **audit evidence you
  want to keep, copy out before rotating** (`verify-ledger` it first).
- Never run bootstrap as a dependency side-effect: audit commands use
  `docker compose run --rm --no-deps …` precisely so they can't trigger an
  accidental rotation (see [verify-a-ledger](verify-a-ledger.md)).

Production key rotation is the tenant's JWKS lifecycle (the `jwks.url`
source re-fetches with single-flight caching) — not this page's mechanism.
