//! S0: the Entra Agent ID token-claim bridge — a SECOND authentication path
//! into the existing authorization engine.
//!
//! An externally-issued Entra agent access token arrives as a bearer
//! credential; the [`validate::TokenValidator`] runs the cryptographic +
//! claim ladder (rows 1–9); the [`Registry`] resolves `(tid, oid)` to a
//! registered Enterprise Brain principal (row 10); the resolved principal
//! then enters the SAME principal-resolution seam a session-authenticated
//! caller uses, and the compiled scope path decides resources exactly as
//! today (row 11 — untouched by this module).
//!
//! Invariants owned here:
//!   * S0-1 identity only — nothing in this module reads a document scope;
//!   * S0-2 unregistered agent -> deny; no default principal, no fallback;
//!   * S0-3 registration keys on `(tid, oid)` — per agent identity, never
//!     `azp`/parent-app (those are attribution, logged only);
//!   * S0-4 the bridge is OFF unless config explicitly enables it;
//!   * S0-5 the session path and this path share the resolution seam and
//!     nothing else.

pub mod claims;
pub mod jwks;
pub mod validate;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub use claims::ClaimSet;
pub use validate::{DenyReason, TokenValidator};

/// The `agent_bridge` section of the service config. ABSENT (or
/// `enabled: false`) means the bridge does not exist at runtime — the
/// default-OFF posture is structural, not a flag check on a hot path.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentBridgeConfig {
    pub enabled: bool,
    /// The Entra tenant (GUID) agent identities are registered in. Tokens
    /// from any other tenant are denied (rows 4 and 7).
    pub tenant_id: String,
    /// The audience value this gateway expects (`aud`).
    pub audience: String,
    /// Where signing keys come from: a local JWKS file (tests/offline) or
    /// the tenant JWKS endpoint (live, cached).
    pub jwks: JwksSourceConfig,
    /// Allowed signature algorithms; default `["RS256"]`, RS/PS family only.
    #[serde(default = "default_algs")]
    pub allowed_algs: Vec<String>,
    /// The registration table: `(tid, oid) -> EB principal`. Fixture/config
    /// driven for S0; the type seam ([`Registry`]) is what a future
    /// admin-managed store replaces.
    #[serde(default)]
    pub agents: Vec<RegisteredAgent>,
}

fn default_algs() -> Vec<String> {
    vec!["RS256".to_string()]
}

/// One JWKS source, exactly one of `file` / `url`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JwksSourceConfig {
    #[serde(default)]
    pub file: Option<PathBuf>,
    #[serde(default)]
    pub url: Option<String>,
}

/// One registration row. `principal` is an Enterprise Brain principal id;
/// an id the identity model does not know compiles to the EMPTY scope
/// downstream (deny-by-default), so a stale registration fails closed.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredAgent {
    pub tid: String,
    pub oid: String,
    pub principal: String,
}

/// The agent registration table (ladder row 10). Keys are normalized
/// `(tid, oid)` — GUID case never distinguishes identities. Duplicate keys
/// refuse at load: two principals behind one agent identity is ambiguity,
/// and ambiguity denies at the door, not at runtime.
pub struct Registry {
    by_key: BTreeMap<(String, String), String>,
}

impl Registry {
    pub fn from_entries(entries: &[RegisteredAgent]) -> Result<Registry> {
        let mut by_key = BTreeMap::new();
        for entry in entries {
            let key = (normalize(&entry.tid), normalize(&entry.oid));
            if entry.tid.trim().is_empty() || entry.oid.trim().is_empty() {
                bail!("agent_bridge.agents entry has an empty tid/oid; refusing");
            }
            if entry.principal.trim().is_empty() {
                bail!("agent_bridge.agents entry has an empty principal; refusing");
            }
            if by_key
                .insert(key, entry.principal.trim().to_string())
                .is_some()
            {
                bail!(
                    "agent_bridge.agents registers (tid={}, oid={}) twice; refusing",
                    entry.tid,
                    entry.oid
                );
            }
        }
        Ok(Registry { by_key })
    }

    /// `(tid, oid)` to principal; `None` IS the deny (S0-2 — no default
    /// principal, no anonymous scope, no public fallback).
    pub fn resolve(&self, tid: &str, oid: &str) -> Option<&str> {
        self.by_key
            .get(&(normalize(tid), normalize(oid)))
            .map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

fn normalize(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

/// What one token-path authentication produced. Either outcome carries the
/// extractable claim evidence for the audit record — allow AND deny are
/// both signed rows in the ledger (EB-6/EB-7).
pub enum BridgeOutcome {
    /// Rows 1–10 passed: `principal` enters the existing resolution seam.
    Resolved {
        principal: String,
        claims: Box<ClaimSet>,
    },
    Denied {
        reason: DenyReason,
        /// Claims are present when the deny happened after decode (rows
        /// 4–10); rows 1–3 deny before any claim is trusted.
        claims: Option<Box<ClaimSet>>,
    },
}

/// The assembled bridge: validator + registry. Built ONLY from an
/// explicitly-enabled config; its absence on `AppState` is the disabled
/// state (S0-4).
pub struct Bridge {
    validator: TokenValidator,
    registry: Registry,
}

impl Bridge {
    pub fn from_config(config: &AgentBridgeConfig) -> Result<Bridge> {
        if !config.enabled {
            bail!("Bridge::from_config on a disabled config; the caller gates on `enabled`");
        }
        if config.tenant_id.trim().is_empty() {
            bail!("agent_bridge.tenant_id is empty; refusing");
        }
        if config.audience.trim().is_empty() {
            bail!("agent_bridge.audience is empty; refusing");
        }
        let jwks: Box<dyn jwks::JwksProvider> = match (&config.jwks.file, &config.jwks.url) {
            (Some(path), None) => Box::new(
                jwks::FileJwks::load(path)
                    .with_context(|| format!("agent_bridge.jwks.file {}", path.display()))?,
            ),
            (None, Some(url)) => Box::new(jwks::HttpJwks::new(url)),
            _ => bail!("agent_bridge.jwks needs exactly one of `file` / `url`; refusing"),
        };
        let validator = TokenValidator::new(
            &config.tenant_id,
            &config.audience,
            &config.allowed_algs,
            jwks,
        )?;
        let registry = Registry::from_entries(&config.agents)?;
        Ok(Bridge {
            validator,
            registry,
        })
    }

    /// Test/assembly seam: a bridge from already-built parts.
    pub fn from_parts(validator: TokenValidator, registry: Registry) -> Bridge {
        Bridge {
            validator,
            registry,
        }
    }

    /// The full authentication ladder for one bearer credential. Sync and
    /// CPU-bound (the HTTP edge calls it inside `spawn_blocking`).
    pub fn authenticate(&self, token: &str) -> BridgeOutcome {
        let claims = match self.validator.validate(token) {
            Ok(claims) => claims,
            Err(denied) => {
                return BridgeOutcome::Denied {
                    reason: denied.reason,
                    claims: denied.claims,
                }
            }
        };
        // Row 10: registration. Keyed on (tid, oid) ONLY — an azp or parent
        // app matching some registered agent grants nothing (S0-3).
        let (Some(tid), Some(oid)) = (claims.tid.as_deref(), claims.oid.as_deref()) else {
            return BridgeOutcome::Denied {
                reason: DenyReason::AgentNotRegistered,
                claims: Some(Box::new(claims)),
            };
        };
        match self.registry.resolve(tid, oid) {
            Some(principal) => BridgeOutcome::Resolved {
                principal: principal.to_string(),
                claims: Box::new(claims),
            },
            None => BridgeOutcome::Denied {
                reason: DenyReason::AgentNotRegistered,
                claims: Some(Box::new(claims)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(tid: &str, oid: &str, principal: &str) -> RegisteredAgent {
        RegisteredAgent {
            tid: tid.to_string(),
            oid: oid.to_string(),
            principal: principal.to_string(),
        }
    }

    #[test]
    fn registry_resolves_normalized_and_denies_unknown() {
        let registry = Registry::from_entries(&[entry("TID-A", "OID-1", "agent_qa_drafter")])
            .expect("valid table");
        assert_eq!(
            registry.resolve("tid-a", "oid-1"),
            Some("agent_qa_drafter"),
            "GUID case never distinguishes identities"
        );
        assert_eq!(registry.resolve("tid-a", "oid-2"), None);
        assert_eq!(
            registry.resolve("tid-b", "oid-1"),
            None,
            "tenant is part of the key"
        );
    }

    #[test]
    fn registry_refuses_duplicates_and_empties() {
        let dup = Registry::from_entries(&[entry("t", "o", "agent_a"), entry("T", "O", "agent_b")]);
        assert!(
            dup.is_err(),
            "one agent identity cannot map to two principals"
        );
        assert!(Registry::from_entries(&[entry("", "o", "a")]).is_err());
        assert!(Registry::from_entries(&[entry("t", "o", " ")]).is_err());
    }

    #[test]
    fn config_defaults_are_fail_closed() {
        let json = r#"{
            "enabled": true,
            "tenant_id": "f8cdef31-a31e-4b4a-93e4-5f571e91255a",
            "audience": "api://enterprise-brain-gateway",
            "jwks": {"file": "does-not-exist.json"}
        }"#;
        let config: AgentBridgeConfig = serde_json::from_str(json).expect("parses");
        assert_eq!(
            config.allowed_algs,
            vec!["RS256"],
            "default alg set is RS256 only"
        );
        assert!(config.agents.is_empty(), "no agents unless registered");
        // A missing JWKS file refuses at build — an unreachable key source
        // never becomes a silently keyless (all-deny-later) bridge.
        assert!(Bridge::from_config(&config).is_err());
    }

    #[test]
    fn disabled_config_refuses_construction() {
        let json = r#"{
            "enabled": false,
            "tenant_id": "t",
            "audience": "a",
            "jwks": {"url": "https://example.invalid/keys"}
        }"#;
        let config: AgentBridgeConfig = serde_json::from_str(json).expect("parses");
        assert!(
            Bridge::from_config(&config).is_err(),
            "S0-4: disabled builds nothing"
        );
    }
}
