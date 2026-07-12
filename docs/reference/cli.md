# Reference — CLI

One binary, three subcommands plus the server. Common launch flags:
`--fixtures <dir> --artifacts <dir> --idx <dir> [--config <file>]`
(plus `--usage-out`, `--no-cache`, `--agents-config` + `--state-dir` for the
console/M4 layers).

## `service …` (the server)

Serves on the config's `bind` (default loopback `127.0.0.1:8787`; the
default refuses any non-loopback address). Startup fails loudly on any
malformed config section, naming the field.

## `service bootstrap-dev --out <dir> [--force] [--container]`

Mints a complete demo world into `<dir>`: RSA-2048 keypair → `jwks.json` +
`private_key.pem`; six 24-hour agent JWTs (four primary + two estate) →
`tokens.json` and copy-paste curls on stdout; a DEMO-labelled ServiceConfig
(bridge enabled, ledger, alert sink).

- **Non-destructive by default**: no world in `<dir>` → mints one; a COMPLETE
  world (keys, tokens, config) → exit 0, one line, nothing touched — repeated
  `docker compose up` cycles cannot rotate keys or reset ledgers; a PARTIAL
  world → an error naming the missing files (never a silent partial overwrite).
- `--force` is the only destructive path: keys rotated, tokens reissued,
  ledger/alerts reset (removes only the artifacts bootstrap owns, then
  regenerates) — see [rotate-dev-keys](../how-to/rotate-dev-keys.md).
- `--container`: the config binds `0.0.0.0:8787` and its profile states why
  that is safe only under the compose host-loopback mapping. Native worlds
  never set `bind`.
- Nothing it mints may be committed: `dev-out/`, `*.pem`, `tokens.json` are
  gitignored and a standing test sweeps tracked files.

## `service doctor [--json] <launch flags>`

Read-only preflight: config schema, ledger chain + writability, JWKS,
registry (ghosts named), estate hash + index, alert sink, workflow store.
Human ✓/✗ or `--json {ok, checks:[{name, ok, detail}]}`. Exit 0 all-green /
1 otherwise. Never mutates, never fetches, never prints a secret. Also the
container healthcheck. Details: [run-doctor](../how-to/run-doctor.md).

## `service verify-ledger <path>`

Recomputes a ledger's hash chain. `CLEAN: N rows (M hash-chained) verify
intact` (exit 0) or `BROKEN: chain breaks at ordinal K (…)` (exit 1).
Container form keeps `--no-deps` (hygiene — an audit has no reason to wake
other services; the old wipe-the-evidence footgun is structurally fixed):
`docker compose run --rm --no-deps gateway verify-ledger /data/dev-out/ledger/audit.jsonl`
— see [verify-a-ledger](../how-to/verify-a-ledger.md).
