# How to: verify a ledger

`verify-ledger` recomputes the hash chain over an append-only ledger file and
reports CLEAN or the first breaking ordinal. Exit `0` clean / `1` broken — it
slots into CI or a cron.

## Native

```sh
service verify-ledger dev-out/ledger/audit.jsonl
# CLEAN: 15 rows (15 hash-chained) verify intact
```

## Against the Docker volume (running OR stopped stack)

```sh
docker compose run --rm --no-deps gateway verify-ledger /data/dev-out/ledger/audit.jsonl
```

**Keep `--no-deps`.** `docker compose run` starts the service's dependencies
by default — here, the bootstrap one-shot. The footgun that used to live here
(bootstrap regenerating the world and *deleting the very ledger under audit*)
is **structurally fixed**: since S5c the one-shot is non-destructive by
default and leaves a complete world untouched. `--no-deps` stays in the
command as hygiene — an audit has no reason to wake anything.

## Reading a failure

```text
BROKEN: chain breaks at ordinal 2 (row 2's prev does not match row 1's bytes)
```

The named ordinal is where recomputation stopped matching: the row at that
ordinal (or the one before it) was edited, removed, or reordered after
writing. Treat the file as evidence — investigate, don't repair. What the
chain does and does not claim is in the
[threat model](../threat-model.md#what-hash-chaining-claims--and-does-not);
legacy pre-chain rows are anchored by the first chained row over them.
