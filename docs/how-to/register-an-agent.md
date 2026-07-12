# How to: register an agent

An agent is registered when its Entra identity — the `(tid, oid)` pair from
its token — maps to an Enterprise Brain principal in the gateway config.
Registration grants **identity resolution only**; what the principal may read
still comes from the compiled scope (primary) or the estate tiers.

## 1. Add the registry row

In the ServiceConfig (`agent_bridge.agents`):

```json
{
  "agent_bridge": {
    "enabled": true,
    "tenant_id": "<your tenant GUID>",
    "audience": "api://enterprise-brain-gateway",
    "jwks": { "file": "path/to/jwks.json" },
    "agents": [
      { "tid": "<tenant GUID>", "oid": "<agent service-principal object id>",
        "principal": "agent_finance_analyst" }
    ]
  }
}
```

Rules the gateway enforces at load: no empty `tid`/`oid`/`principal`; no
duplicate `(tid, oid)` (one agent identity cannot be two principals); GUID
case never matters.

## 2. Preflight it

```sh
service doctor --fixtures fixtures --artifacts compiler/artifacts \
  --idx retrieval/idx --config <your config>
```

`bridge.registry` must be ✓. A **ghost registration** — a `principal` the
identity model does not know — is named here at preflight instead of
becoming a mystery all-deny at runtime (it would compile to the empty scope:
fail-closed, but unfriendly).

## 3. Restart and verify

Config is read at startup: restart the gateway, then:

```sh
curl -s -H "Authorization: Bearer <that agent's JWT>" http://127.0.0.1:8787/v1/whoami
# {"principal_id":"agent_finance_analyst"}
```

A valid token whose `(tid, oid)` has no row denies with ledger reason
`agent_not_registered` — see the [denial runbook](../runbook-denials.md).
