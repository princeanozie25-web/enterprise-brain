# Threat model

Compact and native to this system — what we protect, where trust changes
hands, what each defense actually claims.

## Assets

1. **Document bodies** (primary corpus + estate objects) — the thing an
   unauthorized party must never read, *including through a model's context
   window*.
2. **The decision ledger** — the evidence. Its value is integrity, not
   secrecy.
3. **The access model** (compiled scopes, estate tiers, agent registry) — the
   definition of who may read what.
4. **Signing keys** (demo: locally minted; production: the tenant's) — the
   ability to speak as an agent.

## Trust boundaries

| Boundary | Trusted with | NOT trusted with |
| --- | --- | --- |
| Token issuer (Entra) | Identity claims inside a validly-signed token | Authorization (a valid token grants nothing without a registry row + compiled scope) |
| Connector bytes | Content | Authority — permission-shaped metadata is refused at ingest |
| Operator config | Wiring (which key set, which ledger dir, which bind) | Silent behavior changes — unknown fields refuse startup; doctor names faults |
| **The model / agent** | Nothing | It is the *untrusted consumer* the whole system exists to constrain: it sees only what the gateway serves, and every byte it sees is a ledgered allow |
| The SDK | Ergonomics | Enforcement (EB-2) — a hostile SDK gains nothing the token doesn't already grant |

## The ladder as attack-surface enumeration

Each validation-ladder row is a named, tested attack: `alg: none` and
HS-substitution (`algorithm_rejected`), forged/tampered signatures and
unknown `kid` (`signature_invalid`), replayed expired tokens
(`token_expired`), cross-tenant tokens (`issuer/tenant_mismatch`), tokens
minted for another API (`audience_mismatch`), delegated-user and interactive
shapes (`unsupported_token_type_*`), non-agent identities
(`agent_facets_missing`), and valid-but-unknown agents
(`agent_not_registered`). The conformance suites drive every row; the wire
answers all of them identically (generic 401), so a probe learns nothing
about which rung it died on.

## What hash-chaining claims — and does not

The ledger chains each row to the SHA-256 of the previous row's bytes.

- **Claims:** tamper-evidence. Any in-place edit, deletion, or reorder of
  committed rows breaks the chain at a named ordinal (`verify-ledger`,
  doctor, and the container healthcheck all check it).
- **Does NOT claim:** non-repudiation, nor protection against an attacker who
  can rewrite the whole file *and* every subsequent hash (i.e., full control
  of the ledger host). Signatures / external anchoring are the enterprise
  layer, deliberately out of scope here.
- Truncation of the *tail* after the last verified row is detectable only by
  row count against expectations — periodic external anchoring of the tip is
  the operational mitigation until signatures land.

## Residual risks, stated plainly

- **A compromised gateway host is game over** for both bodies and ledger —
  the design confines *callers*, not the host. Standard host hardening
  applies and is out of scope.
- **The demo world's keys are throwaway by design**; `bootstrap-dev` worlds
  must never be promoted to production (the config says DEMO loudly; the
  shipped default stays bridge-disabled).
- **Authorized exfiltration**: an agent may legitimately read everything its
  scope grants and repeat it elsewhere. The ledger makes the reads evident;
  it cannot make them un-happen. Scope minimalism is policy, not mechanism.
- **Estate authority freshness**: the access model binds at startup with a
  pinned content hash; a stale model is a stale truth until reload. Doctor
  verifies the pin; revocation latency is a certified-connector metric
  (clause 3 of the contract).
- **Denial of service** is rate-limited at the door (login and mint quotas)
  but a determined volumetric attack is an infrastructure concern, not a
  gateway one.
