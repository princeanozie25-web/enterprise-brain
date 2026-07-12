# How to: rotate the dev keys

Demo/dev worlds use a locally-minted RSA key (`bootstrap-dev`). Rotation =
regenerate the world; there is no in-place rotation for dev keys, on purpose
(fresh world, fresh evidence, no half-rotated states).

Without `--force`, `bootstrap-dev` never rotates anything: a complete world
is left untouched (a one-line no-op), a partial world is an error naming the
missing files. **`--force` is the only destructive path** — rotation is this
page, and only this page.

## Native

```sh
service bootstrap-dev --out dev-out --force
```

`--force` removes only the artifacts bootstrap owns (keys, tokens, config,
ledger, alerts) and mints a fresh keypair + six fresh 24-hour tokens.
**Restart the gateway** — it loads the JWKS at startup, so until restart it
still trusts the OLD key (and the new tokens 401 with `signature_invalid`).

## Docker

Restarts never rotate: the bootstrap one-shot is non-destructive by default,
so `docker compose up` / `compose run` cycles leave the world on the volume
untouched. Rotation is one deliberate command, then a gateway restart:

```sh
docker compose run --rm --no-deps bootstrap bootstrap-dev --out /data/dev-out --force --container
docker compose restart gateway     # loads the new JWKS; old tokens die here
```

The `run` prints the fresh token curls directly to your terminal.

## Two sharp edges

- The ledger is part of the world `--force` removes: **audit evidence you
  want to keep, copy out before rotating** (`verify-ledger` it first).
- Tokens live 24 h but the world persists indefinitely: a complete world is
  left untouched, *expired tokens included* — `token_expired` denies the day
  after a demo mean "rotate deliberately", not "bug".

Production key rotation is the tenant's JWKS lifecycle (the `jwks.url`
source re-fetches with single-flight caching) — not this page's mechanism.
