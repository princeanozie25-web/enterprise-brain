//! S0 fixture tokens: locally-signed JWTs reproducing Microsoft's documented
//! Entra agent claim shape VERBATIM (S0-6) — names, `ver`-appropriate
//! issuer / `azp`-vs-`appid`, multivalue string encodings, `xms_idrel: "7"`,
//! `xms_tnt_fct`, a `uti`, `scp: ""`.
//!
//! Keys are generated per test process (2048-bit RSA, OnceLock): the good
//! key is published in a JWKS document; the rogue key signs forgeries and is
//! published NOWHERE. Signing goes through `jsonwebtoken` — the only
//! hand-assembled artifact is the deliberately-malicious `alg: none` token.

use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde_json::{json, Map, Value};

/// The configured tenant every positive fixture is minted in.
pub const TEST_TENANT: &str = "f8cdef31-a31e-4b4a-93e4-5f571e91255a";
/// A different tenant for mismatch fixtures.
pub const OTHER_TENANT: &str = "9e2b5c14-77aa-4f01-8c3d-2b9d01f7a6e4";
/// The gateway audience.
pub const TEST_AUDIENCE: &str = "api://enterprise-brain-gateway";
/// The agent identity's application id (`azp`) — ATTRIBUTION only.
pub const TEST_APP_ID: &str = "5a1f0c9d-3e4b-4d2a-9f6e-8b7c6d5e4f3a";
/// A parent application GUID (`xms_par_app_azp`) — logged, never authorized.
pub const TEST_PARENT_APP: &str = "77e0a2b3-c4d5-4e6f-8a9b-0c1d2e3f4a5b";
/// The `kid` the JWKS publishes for the good key.
pub const TEST_KID: &str = "s0-fixture-key-1";

pub struct TestIssuer {
    signing_pem: String,
    pub jwks_json: String,
}

/// The per-process fixture issuer (keygen happens once per test binary).
pub fn issuer() -> &'static TestIssuer {
    static ISSUER: OnceLock<TestIssuer> = OnceLock::new();
    ISSUER.get_or_init(|| {
        let mut rng = rand::thread_rng();
        let key = RsaPrivateKey::new(&mut rng, 2048).expect("fixture RSA keygen");
        let public = RsaPublicKey::from(&key);
        TestIssuer {
            signing_pem: key
                .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
                .expect("fixture key PEM")
                .to_string(),
            jwks_json: jwks_document(&[(TEST_KID, &public)]),
        }
    })
}

/// A second keypair published in NO JWKS — signs "key not in the set"
/// forgeries. Separate OnceLock so only binaries that forge pay the keygen.
pub fn rogue_signing_pem() -> &'static str {
    static ROGUE: OnceLock<String> = OnceLock::new();
    ROGUE.get_or_init(|| {
        let mut rng = rand::thread_rng();
        let key = RsaPrivateKey::new(&mut rng, 2048).expect("rogue RSA keygen");
        key.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .expect("rogue key PEM")
            .to_string()
    })
}

fn jwks_document(keys: &[(&str, &RsaPublicKey)]) -> String {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let entries: Vec<Value> = keys
        .iter()
        .map(|(kid, key)| {
            json!({
                "kty": "RSA",
                "use": "sig",
                "kid": kid,
                "n": b64.encode(key.n().to_bytes_be()),
                "e": b64.encode(key.e().to_bytes_be()),
            })
        })
        .collect();
    serde_json::to_string(&json!({ "keys": entries })).expect("jwks json")
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One fixture token under construction: full claim map + header knobs.
#[derive(Clone)]
pub struct TokenSpec {
    pub claims: Map<String, Value>,
    pub kid: Option<String>,
}

impl TokenSpec {
    /// The documented AUTONOMOUS AGENT (app-only, v2) shape, field for
    /// field. `oid` is the agent identity service principal — the
    /// registration key.
    pub fn autonomous(oid: &str) -> TokenSpec {
        let now = now_unix();
        let mut claims = Map::new();
        let mut put = |k: &str, v: Value| claims.insert(k.to_string(), v);
        put("aud", json!(TEST_AUDIENCE));
        put(
            "iss",
            json!(format!(
                "https://login.microsoftonline.com/{TEST_TENANT}/v2.0"
            )),
        );
        put("iat", json!(now - 60));
        put("nbf", json!(now - 60));
        put("exp", json!(now + 3600));
        put("tid", json!(TEST_TENANT));
        put("oid", json!(oid));
        put("sub", json!(oid));
        put("azp", json!(TEST_APP_ID));
        put("ver", json!("2.0"));
        put("idtyp", json!("app"));
        put("scp", json!(""));
        put("roles", json!(["Brain.Read"]));
        put("uti", json!(format!("uti-{oid}")));
        put("xms_idrel", json!("7"));
        put("xms_act_fct", json!("11"));
        put("xms_sub_fct", json!("11"));
        put("xms_tnt_fct", json!("1"));
        put("xms_par_app_azp", json!(TEST_PARENT_APP));
        TokenSpec {
            claims,
            kid: Some(TEST_KID.to_string()),
        }
    }

    /// The v1 spelling of the same shape: `sts.windows.net` issuer,
    /// `appid` instead of `azp`, `ver: "1.0"`.
    pub fn v1_autonomous(oid: &str) -> TokenSpec {
        let mut spec = TokenSpec::autonomous(oid);
        spec = spec
            .with("ver", json!("1.0"))
            .with(
                "iss",
                json!(format!("https://sts.windows.net/{TEST_TENANT}/")),
            )
            .without("azp");
        spec.with("appid", json!(TEST_APP_ID))
    }

    pub fn with(mut self, key: &str, value: Value) -> TokenSpec {
        self.claims.insert(key.to_string(), value);
        self
    }

    pub fn without(mut self, key: &str) -> TokenSpec {
        self.claims.remove(key);
        self
    }

    pub fn kid(mut self, kid: Option<&str>) -> TokenSpec {
        self.kid = kid.map(str::to_string);
        self
    }

    /// Sign RS256 with the fixture issuer's published key.
    pub fn sign(&self) -> String {
        self.sign_with(&issuer().signing_pem)
    }

    /// Sign RS256 with an arbitrary key (the rogue key for forgeries).
    pub fn sign_with(&self, pem: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = self.kid.clone();
        let key = EncodingKey::from_rsa_pem(pem.as_bytes()).expect("signing key");
        jsonwebtoken::encode(&header, &Value::Object(self.claims.clone()), &key)
            .expect("token signs")
    }

    /// Sign HS256 with an attacker secret (the symmetric-alg attack shape).
    pub fn sign_hs256(&self) -> String {
        let mut header = Header::new(Algorithm::HS256);
        header.kid = self.kid.clone();
        let key = EncodingKey::from_secret(b"attacker-chosen-secret");
        jsonwebtoken::encode(&header, &Value::Object(self.claims.clone()), &key)
            .expect("hs256 token signs")
    }

    /// Hand-assemble the `alg: none` attack token (unsigned; trailing empty
    /// signature segment). The ONE deliberately non-library assembly.
    pub fn alg_none(&self) -> String {
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let header = b64.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload = b64
            .encode(serde_json::to_vec(&Value::Object(self.claims.clone())).expect("claims json"));
        format!("{header}.{payload}.")
    }
}

/// Corrupt a valid token's signature while preserving canonical base64url.
/// Mutating the final encoded character can produce non-canonical padding
/// bits for a 256-byte RSA signature, which tests parsing rather than
/// signature verification.
pub fn tamper_signature(token: &str) -> String {
    let mut parts: Vec<String> = token.split('.').map(str::to_string).collect();
    assert_eq!(parts.len(), 3, "tamper_signature expects a compact JWS");
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut signature = b64
        .decode(&parts[2])
        .expect("valid token has a decodable signature");
    signature[0] ^= 0x01;
    parts[2] = b64.encode(signature);
    parts.join(".")
}
