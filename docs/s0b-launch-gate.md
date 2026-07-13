# S0b launch gate — blocked upstream

**S0b implementation is complete; live closure is blocked by Microsoft Entra
Agent ID preview issuing a non-faceted token despite the documented autonomous
SDK flow.**

```text
Gateway                                  GO
Standard Entra workload identity         GO
Attested autonomous Agent ID             preview-gated (upstream)
S0b                                      NOT CLOSED
```

The Enterprise Brain bridge remains deliberately fail-closed. Its live test
accepts only a cryptographically validated autonomous Agent ID token whose
signed `xms_sub_fct` and `xms_act_fct` claims both contain facet `11`.
Tokens without that evidence are denied before registration or resource
authorization.

The tenant, Agent Identity, blueprint linkage, app-role assignment, blueprint
credential, and documented `Microsoft.Identity.Web.AgentIdentities` call path
were verified. The resulting token has the expected audience, tenant, Agent
Identity `oid`/`appid`, and `Agent.Access` role, but omits both required facets
and `xms_par_app_azp`. Therefore `bridge_live` correctly denies it as
`agent_facets_missing` — the issuer did not mint agent attestation claims
(see the [runbook row](runbook-denials.md#auth-ladder-denies-wire-generic-401)).

Do not relax the bridge to close this gate. Re-run the ignored live test only
after Microsoft provides a token issued with the documented Agent Identity
facets. The support reproduction is in
[s0b-upstream-reproduction.md](s0b-upstream-reproduction.md).
