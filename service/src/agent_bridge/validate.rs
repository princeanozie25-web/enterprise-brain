//! S0: the token validation ladder (rows 1–9 of the decision ladder; row 10,
//! registration, lives in [`super::Bridge`]). Ordered, first failure wins,
//! every row a DISTINCT reason code. This layer establishes IDENTITY only —
//! no resource decision is made here (S0-1); authorization stays in the
//! compiled scope path it hands the resolved principal to.
//!
//! Cryptography is the `jsonwebtoken` crate's: signature verification,
//! base64url of the signed input, and exp/nbf time comparison (leeway 60s)
//! are never hand-rolled. The ONE local read of the raw header (segment
//! count + `alg`/`kid` fields, via the `base64`/`serde_json` crates) exists
//! because the ladder must distinguish `algorithm_rejected` (`none`, HS*)
//! from `token_malformed`, and the library's typed header conflates unknown
//! algorithms with parse failure. Policy classification, not crypto.

use std::collections::HashSet;

use base64::Engine;
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::Value;

use super::claims::{expected_issuer, guid_eq, ClaimSet, Scenario};
use super::jwks::JwksProvider;

/// Clock skew tolerance on `exp` / `nbf`: 60 seconds. No other leniency.
pub const CLOCK_SKEW_SECS: u64 = 60;

/// Every way the bridge says no (ladder rows, plus the two operational
/// denies), and the one way it says yes. `as_str` values are the audit
/// reason codes — distinct per row, stable, greppable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    TokenMalformed,
    AlgorithmRejected,
    SignatureInvalid,
    IssuerMismatch,
    AudienceMismatch,
    TokenExpired,
    TokenNotYetValid,
    TenantMismatch,
    UnsupportedTokenTypeDelegated,
    UnsupportedTokenTypeAgentUser,
    AgentFacetsMissing,
    AgentNotRegistered,
    /// S0-4: the bridge is not enabled; a JWT-shaped bearer credential is
    /// denied without validation and nothing else changes behaviour.
    BridgeDisabled,
    /// EB-4 x EB-6: the bridge cannot operate to discipline (no audit sink
    /// for an allow) — deny rather than proceed unrecorded.
    BridgeUnavailable,
}

impl DenyReason {
    /// Every deny reason the bridge can produce — the standing wire test
    /// sweeps this to prove NO reason string ever reaches an HTTP response
    /// (reasons are ledger-only). Extend this when extending the enum; the
    /// sweep inherits the new value automatically.
    pub const ALL: &'static [DenyReason] = &[
        DenyReason::TokenMalformed,
        DenyReason::AlgorithmRejected,
        DenyReason::SignatureInvalid,
        DenyReason::IssuerMismatch,
        DenyReason::AudienceMismatch,
        DenyReason::TokenExpired,
        DenyReason::TokenNotYetValid,
        DenyReason::TenantMismatch,
        DenyReason::UnsupportedTokenTypeDelegated,
        DenyReason::UnsupportedTokenTypeAgentUser,
        DenyReason::AgentFacetsMissing,
        DenyReason::AgentNotRegistered,
        DenyReason::BridgeDisabled,
        DenyReason::BridgeUnavailable,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            DenyReason::TokenMalformed => "token_malformed",
            DenyReason::AlgorithmRejected => "algorithm_rejected",
            DenyReason::SignatureInvalid => "signature_invalid",
            DenyReason::IssuerMismatch => "issuer_mismatch",
            DenyReason::AudienceMismatch => "audience_mismatch",
            DenyReason::TokenExpired => "token_expired",
            DenyReason::TokenNotYetValid => "token_not_yet_valid",
            DenyReason::TenantMismatch => "tenant_mismatch",
            DenyReason::UnsupportedTokenTypeDelegated => "unsupported_token_type_delegated",
            DenyReason::UnsupportedTokenTypeAgentUser => "unsupported_token_type_agent_user",
            DenyReason::AgentFacetsMissing => "agent_facets_missing",
            DenyReason::AgentNotRegistered => "agent_not_registered",
            DenyReason::BridgeDisabled => "bridge_disabled",
            DenyReason::BridgeUnavailable => "bridge_unavailable",
        }
    }
}

/// A deny with whatever claims were extractable when it happened — rows 1–3
/// deny before any claim is trusted (`claims: None`); later rows carry the
/// decoded set so the audit record can attribute the attempt. Boxed: the
/// deny is the COMMON return on a hostile edge, and it stays pointer-sized.
pub struct Denied {
    pub reason: DenyReason,
    pub claims: Option<Box<ClaimSet>>,
}

/// The lenient header pre-read for the alg-policy gate. Read with standard
/// decoders only; never trusted for anything but rows 1–2.
#[derive(Deserialize)]
struct RawHeader {
    alg: Option<String>,
    #[serde(default)]
    kid: Option<String>,
}

pub struct TokenValidator {
    tenant_id: String,
    audience: String,
    /// Allowed signature algorithms (asymmetric only, RS/PS family). `none`
    /// and every HMAC algorithm are rejected UNCONDITIONALLY — a symmetric
    /// alg on an externally-issued token is an attack, not a configuration.
    allowed_algs: Vec<Algorithm>,
    allowed_names: Vec<String>,
    jwks: Box<dyn JwksProvider>,
}

impl TokenValidator {
    /// `allowed_algs` names come from config (default `["RS256"]`); anything
    /// outside the RS/PS family — or unparseable — is refused at build time,
    /// so a config typo cannot widen the accepted set.
    pub fn new(
        tenant_id: &str,
        audience: &str,
        allowed_algs: &[String],
        jwks: Box<dyn JwksProvider>,
    ) -> anyhow::Result<TokenValidator> {
        let mut algs = Vec::new();
        let mut names = Vec::new();
        for name in allowed_algs {
            let alg = match name.as_str() {
                "RS256" => Algorithm::RS256,
                "RS384" => Algorithm::RS384,
                "RS512" => Algorithm::RS512,
                "PS256" => Algorithm::PS256,
                "PS384" => Algorithm::PS384,
                "PS512" => Algorithm::PS512,
                other => anyhow::bail!(
                    "agent_bridge.allowed_algs {other:?} is not in the RS/PS family; refusing"
                ),
            };
            algs.push(alg);
            names.push(name.clone());
        }
        if algs.is_empty() {
            anyhow::bail!("agent_bridge.allowed_algs is empty; refusing");
        }
        Ok(TokenValidator {
            tenant_id: tenant_id.to_string(),
            audience: audience.to_string(),
            allowed_algs: algs,
            allowed_names: names,
            jwks,
        })
    }

    /// Rows 1–9. First failure wins; success yields the validated claim set
    /// for row 10 (registration) in the bridge.
    pub fn validate(&self, token: &str) -> Result<ClaimSet, Denied> {
        let deny = |reason: DenyReason| Denied {
            reason,
            claims: None,
        };

        // Row 1: parseable JWT structure (three dot-separated segments, a
        // base64url-JSON header).
        let segments: Vec<&str> = token.split('.').collect();
        if segments.len() != 3 {
            return Err(deny(DenyReason::TokenMalformed));
        }
        let header_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(segments[0])
            .map_err(|_| deny(DenyReason::TokenMalformed))?;
        let header: RawHeader =
            serde_json::from_slice(&header_bytes).map_err(|_| deny(DenyReason::TokenMalformed))?;

        // Row 2: alg in the allowed asymmetric set. `none` (any casing) and
        // HS* are named attacks; anything not explicitly allowed is out.
        let alg_name = header.alg.ok_or_else(|| deny(DenyReason::TokenMalformed))?;
        if alg_name.eq_ignore_ascii_case("none")
            || alg_name.to_ascii_uppercase().starts_with("HS")
            || !self
                .allowed_names
                .iter()
                .any(|allowed| allowed == &alg_name)
        {
            return Err(deny(DenyReason::AlgorithmRejected));
        }

        // Row 3: signature against the JWKS key for `kid`. No key — unknown
        // kid, unreachable/failed key source — is a deny, never a bypass.
        let key = self
            .jwks
            .key_for(header.kid.as_deref())
            .ok_or_else(|| deny(DenyReason::SignatureInvalid))?;
        let claims_value = self.decode_signature_only(token, &key)?;
        let claims = ClaimSet::from_value(&claims_value);
        let deny_with = |reason: DenyReason| Denied {
            reason,
            claims: Some(Box::new(claims.clone())),
        };

        // Row 4: issuer matches the CONFIGURED tenant in the form its `ver`
        // demands (v1 sts.windows.net / v2 login.microsoftonline.com). An
        // undeterminable version has no valid issuer form.
        let expected = expected_issuer(claims.ver.as_deref(), &self.tenant_id)
            .ok_or_else(|| deny_with(DenyReason::IssuerMismatch))?;
        if claims.iss.as_deref() != Some(expected.as_str()) {
            return Err(deny_with(DenyReason::IssuerMismatch));
        }

        // Row 5: audience is this gateway.
        if !claims.aud.iter().any(|aud| aud == &self.audience) {
            return Err(deny_with(DenyReason::AudienceMismatch));
        }

        // Row 6: exp / nbf, ±60s skew — the library's time comparison, run
        // at THIS row so an expired wrong-issuer token still reports the
        // issuer (ladder order).
        self.decode_with_time(token, &key).map_err(deny_with)?;

        // Row 7: the tenant claim itself.
        let tid_ok = claims
            .tid
            .as_deref()
            .is_some_and(|tid| guid_eq(tid, &self.tenant_id));
        if !tid_ok {
            return Err(deny_with(DenyReason::TenantMismatch));
        }

        // Row 8: scenario — only the autonomous agent shape proceeds.
        match claims.scenario() {
            Scenario::AutonomousAgent => {}
            Scenario::Delegated => {
                return Err(deny_with(DenyReason::UnsupportedTokenTypeDelegated))
            }
            Scenario::AgentUserAccount => {
                return Err(deny_with(DenyReason::UnsupportedTokenTypeAgentUser))
            }
        }

        // Row 9: both agent-identity facets present (unknown extras ignored).
        if !claims.has_agent_facets() {
            return Err(deny_with(DenyReason::AgentFacetsMissing));
        }

        Ok(claims)
    }

    /// Pass 1: signature only (claim-time and audience checks belong to
    /// their own ladder rows). Signature errors are row 3; a payload that
    /// verifies but fails to parse is row 1.
    fn decode_signature_only(&self, token: &str, key: &DecodingKey) -> Result<Value, Denied> {
        let mut validation = Validation::new(self.allowed_algs[0]);
        validation.algorithms = self.allowed_algs.clone();
        validation.validate_exp = false;
        validation.validate_nbf = false;
        validation.validate_aud = false;
        validation.required_spec_claims = HashSet::new();
        match jsonwebtoken::decode::<Value>(token, key, &validation) {
            Ok(data) => Ok(data.claims),
            Err(err) => Err(Denied {
                reason: match err.kind() {
                    ErrorKind::InvalidSignature => DenyReason::SignatureInvalid,
                    ErrorKind::Base64(_) | ErrorKind::Json(_) | ErrorKind::Utf8(_) => {
                        DenyReason::TokenMalformed
                    }
                    // Anything unanticipated fails closed as unverifiable.
                    _ => DenyReason::SignatureInvalid,
                },
                claims: None,
            }),
        }
    }

    /// Pass 2 (row 6): exp/nbf via the library, leeway [`CLOCK_SKEW_SECS`].
    /// `exp` is REQUIRED — a token that never expires is denied as expired.
    fn decode_with_time(&self, token: &str, key: &DecodingKey) -> Result<(), DenyReason> {
        let mut validation = Validation::new(self.allowed_algs[0]);
        validation.algorithms = self.allowed_algs.clone();
        validation.leeway = CLOCK_SKEW_SECS;
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.validate_aud = false;
        validation.required_spec_claims = HashSet::from(["exp".to_string()]);
        match jsonwebtoken::decode::<Value>(token, key, &validation) {
            Ok(_) => Ok(()),
            Err(err) => Err(match err.kind() {
                ErrorKind::ExpiredSignature => DenyReason::TokenExpired,
                ErrorKind::ImmatureSignature => DenyReason::TokenNotYetValid,
                ErrorKind::MissingRequiredClaim(_) => DenyReason::TokenExpired,
                // The signature already verified in pass 1; anything else
                // here is unanticipated and fails closed.
                _ => DenyReason::SignatureInvalid,
            }),
        }
    }
}
