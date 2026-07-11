//! S0b seam: the IDENTICAL validation ladder against a REAL Entra-issued
//! agent token over the live tenant JWKS endpoint. Ignored by default —
//! its existence is the S0a deliverable; the owner activates it after the
//! Azure tenant setup:
//!
//! ```text
//! EB_S0B_TOKEN=<real agent access token>
//! EB_S0B_TENANT=<tenant GUID the agent identity is registered in>
//! EB_S0B_AUDIENCE=<the aud this gateway was registered as>
//! EB_S0B_JWKS_URL=<optional; defaults to the tenant v2 discovery keys URL>
//! EB_S0B_OID=<the agent identity service principal object id>
//! EB_S0B_PRINCIPAL=<the EB principal the agent is registered to>
//! cargo test -p service --test bridge_live -- --ignored --nocapture
//! ```
//!
//! Until this runs green against a live token, the bridge is NOT proven
//! against a real Entra issuance — S0a's proof is fixture-shaped only.

mod common;

use service::agent_bridge::jwks::HttpJwks;
use service::agent_bridge::{Bridge, BridgeOutcome, RegisteredAgent, Registry, TokenValidator};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

#[test]
#[ignore = "S0b: needs a real Entra tenant + token via EB_S0B_* env vars"]
fn live_entra_agent_token_walks_the_identical_ladder() {
    let token = env("EB_S0B_TOKEN").expect("EB_S0B_TOKEN (a real agent access token)");
    let tenant = env("EB_S0B_TENANT").expect("EB_S0B_TENANT (tenant GUID)");
    let audience = env("EB_S0B_AUDIENCE").expect("EB_S0B_AUDIENCE (this gateway's aud)");
    let jwks_url = env("EB_S0B_JWKS_URL").unwrap_or_else(|| {
        format!("https://login.microsoftonline.com/{tenant}/discovery/v2.0/keys")
    });
    let oid = env("EB_S0B_OID").expect("EB_S0B_OID (agent identity object id)");
    let principal = env("EB_S0B_PRINCIPAL").expect("EB_S0B_PRINCIPAL (EB principal id)");

    // THE code under proof: the same TokenValidator + Registry the gateway
    // runs, with the live HttpJwks (cached, fail-closed) instead of a file.
    let validator = TokenValidator::new(
        &tenant,
        &audience,
        &["RS256".to_string()],
        Box::new(HttpJwks::new(&jwks_url)),
    )
    .expect("validator");
    let registry = Registry::from_entries(&[RegisteredAgent {
        tid: tenant.clone(),
        oid: oid.clone(),
        principal: principal.clone(),
    }])
    .expect("registry");
    let bridge = Bridge::from_parts(validator, registry);

    match bridge.authenticate(&token) {
        BridgeOutcome::Resolved {
            principal: resolved,
            claims,
        } => {
            println!(
                "S0b LIVE: resolved {resolved} (tid={:?} oid={:?} azp={:?} parent={:?} uti={:?})",
                claims.tid, claims.oid, claims.azp, claims.parent_app_azp, claims.uti
            );
            assert_eq!(resolved, principal);
        }
        BridgeOutcome::Denied { reason, claims } => {
            panic!(
                "S0b LIVE: denied {} (claims extracted: {})",
                reason.as_str(),
                claims.is_some()
            );
        }
    }
}
