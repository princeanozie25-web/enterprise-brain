//! S5b: `bootstrap-dev` — mint a COMPLETE local demo world so a stranger with
//! the repo goes from clone to a healthy gateway serving the governed fixture
//! estate in one command. It writes four files into an out-dir:
//!
//!   * `jwks.json`        — the public verification key (kid `dev-key-1`);
//!   * `private_key.pem`  — the throwaway signing key (DEMO — never commit);
//!   * `tokens.json`      — four 24 h agent JWTs, `principal -> token`;
//!   * `config.json`      — a DEMO/DEV ServiceConfig (bridge ENABLED here).
//!
//! INVARIANTS THIS RESPECTS:
//!   * S5b-1 nothing minted here is ever committed — the out-dir is gitignored
//!     and a standing test sweeps tracked files for `*.pem` / `tokens.json`.
//!   * S5b-2 the SHIPPED default config stays bridge-DISABLED (S0-4); the
//!     bridge is enabled ONLY in this generated, DEMO-labelled config.
//!   * S5b-4 zero request-path changes — this is a CLI that writes files and
//!     exits; it never runs inside the server, never adds a route.
//!
//! The Entra claim shape is the S0/S2 fixture shape VERBATIM (`idtyp: app`,
//! `xms_idrel: "7"`, facets `11`, v2 issuer/tid/aud) so the minted tokens
//! validate through the real [`crate::agent_bridge`] ladder unchanged — the
//! demo proves the product, it does not simulate it.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::Engine;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use rsa::pkcs1::{EncodeRsaPrivateKey, LineEnding};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde_json::{json, Map, Value};

/// The demo tenant/audience — VERBATIM the S0/S2 fixture identity so tokens
/// minted here pass the real validation ladder without any test-only relaxation.
const DEMO_TENANT: &str = "f8cdef31-a31e-4b4a-93e4-5f571e91255a";
const DEMO_AUDIENCE: &str = "api://enterprise-brain-gateway";
/// The agent application id (`azp`) — ATTRIBUTION only, never authorization.
const DEMO_APP_ID: &str = "5a1f0c9d-3e4b-4d2a-9f6e-8b7c6d5e4f3a";
/// A parent application GUID (`xms_par_app_azp`) — logged, never authorized.
const DEMO_PARENT_APP: &str = "77e0a2b3-c4d5-4e6f-8a9b-0c1d2e3f4a5b";
/// The key id the demo JWKS publishes and the demo tokens carry.
const DEMO_KID: &str = "dev-key-1";
/// Demo tokens live 24 hours (long enough for a session at a desk).
const TOKEN_TTL_SECS: u64 = 24 * 60 * 60;

/// The four demo agents. Two primary-corpus agents (for `whoami` and the
/// primary machine surface) and the TWO estate agents — the only principals
/// with any estate grant, so the tier-seam demo (a confidential estate object
/// one may read and the other may not) has agents to turn on. Object ids
/// follow the S0 fixture convention (`aaaa…` primary, `bbbb…` estate).
const DEMO_AGENTS: [(&str, &str); 4] = [
    (
        "agent_finance_analyst",
        "aaaa0003-5c1e-4a2b-9d3e-000000000a03",
    ),
    ("agent_exec_brief", "aaaa0004-5c1e-4a2b-9d3e-000000000a04"),
    (
        "agent_estate_confidential",
        "bbbb0001-5c1e-4a2b-9d3e-000000000b01",
    ),
    (
        "agent_estate_internal",
        "bbbb0002-5c1e-4a2b-9d3e-000000000b02",
    ),
];

/// A confidential estate object — the seam-demo target: the confidential
/// agent reads it (200), the internal agent cannot (404).
const SEAM_CONFIDENTIAL_DOC: &str = "s3/finance-restricted/2026/q1/budget-variance-ashcombe.md";

/// The result of a bootstrap run: where everything landed, plus the minted
/// `(principal, token)` pairs (so a caller — or a test — can drive the
/// gateway without re-reading the files).
pub struct BootstrapOutput {
    pub out_dir: PathBuf,
    pub config_path: PathBuf,
    pub jwks_path: PathBuf,
    pub private_key_path: PathBuf,
    pub tokens_path: PathBuf,
    pub tokens: Vec<(String, String)>,
}

/// Mint the demo world into `out`. Refuses a non-empty out-dir without
/// `force`; under `force` it removes the artifacts it owns and regenerates a
/// clean world (freshly-minted keys each time — "idempotent" means a clean,
/// complete result, not byte-identical keys).
///
/// `container` (the compose bootstrap passes `--container`): the generated
/// config sets `bind: 0.0.0.0:8787` — loopback inside a container is
/// fail-useless — and the profile states why that is safe ONLY under the
/// compose host-loopback port mapping. The native demo world (default) keeps
/// the loopback invariant untouched: no `bind` key at all.
pub fn bootstrap_dev(out: &Path, force: bool, container: bool) -> Result<BootstrapOutput> {
    if out.exists() {
        let non_empty = out
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false);
        if non_empty && !force {
            bail!(
                "out-dir {} exists and is not empty; pass --force to regenerate \
                 (this deletes the demo keys/tokens/config it owns)",
                out.display()
            );
        }
        if force {
            remove_owned_artifacts(out);
        }
    }
    std::fs::create_dir_all(out)
        .with_context(|| format!("cannot create out-dir {}", out.display()))?;

    // Absolute paths WITHOUT the Windows `\\?\` verbatim prefix, forward-slashed
    // so the generated config reads cleanly on every platform (and identically
    // inside a container that mounts the out-dir at a fixed absolute path).
    let abs_out = if out.is_absolute() {
        out.to_path_buf()
    } else {
        std::env::current_dir()
            .context("cwd for absolute out-dir")?
            .join(out)
    };
    let ledger_dir = abs_out.join("ledger");
    let alerts_dir = abs_out.join("alerts");
    std::fs::create_dir_all(&ledger_dir).context("ledger dir")?;
    std::fs::create_dir_all(&alerts_dir).context("alerts dir")?;

    // 1. RSA-2048 keypair -> public JWKS + private PEM.
    let mut rng = rand::thread_rng();
    let key = RsaPrivateKey::new(&mut rng, 2048).context("RSA-2048 keygen")?;
    let public = RsaPublicKey::from(&key);
    let private_pem = key
        .to_pkcs1_pem(LineEnding::LF)
        .context("private key PEM")?
        .to_string();
    let jwks_json = jwks_document(DEMO_KID, &public);

    let jwks_path = abs_out.join("jwks.json");
    let private_key_path = abs_out.join("private_key.pem");
    std::fs::write(&jwks_path, &jwks_json).context("write jwks.json")?;
    std::fs::write(&private_key_path, &private_pem).context("write private_key.pem")?;

    // 2. Four 24 h agent JWTs, signed with the throwaway key.
    let now = now_unix();
    let mut tokens = Vec::new();
    for (principal, oid) in DEMO_AGENTS {
        let claims = autonomous_claims(oid, now);
        let token = sign_rs256(&claims, DEMO_KID, &private_pem)
            .with_context(|| format!("sign token for {principal}"))?;
        tokens.push((principal.to_string(), token));
    }
    let tokens_json = {
        let mut map = Map::new();
        for (principal, token) in &tokens {
            map.insert(principal.clone(), Value::String(token.clone()));
        }
        serde_json::to_string_pretty(&Value::Object(map)).context("tokens.json")?
    };
    let tokens_path = abs_out.join("tokens.json");
    std::fs::write(&tokens_path, tokens_json).context("write tokens.json")?;

    // 3. The DEMO/DEV ServiceConfig: bridge ENABLED (demo only), FileJwks ->
    //    the generated jwks, the four registrations, a chained ledger, an
    //    always-on alert sink. The `profile` field is the file's DEMO header
    //    AND its comment channel (JSON has no comments, and the config schema
    //    is deny_unknown_fields — prose lives in `profile`).
    let mut profile = "DEMO/DEV — enterprise-brain bootstrap-dev world. DO NOT DEPLOY. \
                       The agent bridge is ENABLED here with a throwaway key minted locally; \
                       the shipped default config remains bridge-DISABLED (S0-4)."
        .to_string();
    if container {
        // The 1.2 comment line: why 0.0.0.0 is safe HERE and only here.
        profile.push_str(
            " CONTAINER PROFILE: bind 0.0.0.0:8787 is safe ONLY because the compose \
             mapping publishes it host-loopback (127.0.0.1:8787:8787) — the no-external-\
             exposure intent moves to the host boundary; never map this port unqualified.",
        );
    }
    let config = json!({
        "profile": profile,
        "agent_bridge": {
            "enabled": true,
            "tenant_id": DEMO_TENANT,
            "audience": DEMO_AUDIENCE,
            "jwks": { "file": slashed(&jwks_path) },
            "agents": DEMO_AGENTS
                .iter()
                .map(|(principal, oid)| json!({
                    "tid": DEMO_TENANT, "oid": oid, "principal": principal
                }))
                .collect::<Vec<Value>>(),
        },
        "ledger": { "dir": slashed(&ledger_dir) },
        "alerting": {
            "enabled": true,
            "alerts_path": slashed(&alerts_dir.join("alerts.jsonl")),
        },
    });
    let mut config = config;
    if container {
        // Explicit bind (ServiceConfig.bind) — the native default stays
        // loopback by OMITTING the key entirely.
        config["bind"] = json!("0.0.0.0:8787");
    }
    let config_path = abs_out.join("config.json");
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&config).context("config.json")?,
    )
    .context("write config.json")?;

    Ok(BootstrapOutput {
        out_dir: abs_out,
        config_path,
        jwks_path,
        private_key_path,
        tokens_path,
        tokens,
    })
}

impl BootstrapOutput {
    /// Print the launch guide + copy-paste curls to stdout. Keys are on disk;
    /// this only echoes the (already-written) tokens and where things live.
    pub fn print_launch_guide(&self) {
        let out = slashed(&self.out_dir);
        println!("bootstrap-dev: wrote a DEMO/DEV world to {out}");
        println!("  jwks:        {}", slashed(&self.jwks_path));
        println!(
            "  private key: {}   (DEMO — never commit)",
            slashed(&self.private_key_path)
        );
        println!(
            "  config:      {}       (agent bridge ENABLED — demo only)",
            slashed(&self.config_path)
        );
        println!("  tokens:      {}", slashed(&self.tokens_path));
        println!();
        println!("Launch the gateway (loopback 127.0.0.1:8787):");
        println!(
            "  service --fixtures fixtures --artifacts compiler/artifacts \
             --idx retrieval/idx --config {}",
            slashed(&self.config_path)
        );
        println!();
        println!("Four demo agents (24 h tokens). WHO resolved:");
        for (principal, token) in &self.tokens {
            println!("  # {principal}");
            println!(
                "  curl -s -H \"Authorization: Bearer {token}\" \
                 http://127.0.0.1:8787/v1/whoami"
            );
        }
        println!();
        println!("The seam (same confidential object, two agents — the access model decides):");
        if let Some((_, conf)) = self
            .tokens
            .iter()
            .find(|(p, _)| p == "agent_estate_confidential")
        {
            println!("  # agent_estate_confidential -> 200");
            println!(
                "  curl -s -H \"Authorization: Bearer {conf}\" \
                 http://127.0.0.1:8787/v1/documents/{SEAM_CONFIDENTIAL_DOC}"
            );
        }
        if let Some((_, intl)) = self
            .tokens
            .iter()
            .find(|(p, _)| p == "agent_estate_internal")
        {
            println!("  # agent_estate_internal -> 404 (the document did not decide; the access model did)");
            println!(
                "  curl -s -H \"Authorization: Bearer {intl}\" \
                 http://127.0.0.1:8787/v1/documents/{SEAM_CONFIDENTIAL_DOC}"
            );
        }
    }
}

/// Remove ONLY the artifacts this command owns (never the whole out-dir — it
/// may be a mounted volume the operator put other things in).
fn remove_owned_artifacts(out: &Path) {
    for file in ["config.json", "jwks.json", "private_key.pem", "tokens.json"] {
        let _ = std::fs::remove_file(out.join(file));
    }
    for dir in ["ledger", "alerts"] {
        let _ = std::fs::remove_dir_all(out.join(dir));
    }
}

/// A single-key JWKS document (RS256 verification key), pretty-printed.
fn jwks_document(kid: &str, key: &RsaPublicKey) -> String {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let doc = json!({
        "keys": [{
            "kty": "RSA",
            "use": "sig",
            "kid": kid,
            "n": b64.encode(key.n().to_bytes_be()),
            "e": b64.encode(key.e().to_bytes_be()),
        }]
    });
    serde_json::to_string_pretty(&doc).expect("jwks json is serializable")
}

/// The documented autonomous-agent (app-only, v2) claim set — S0/S2 fixture
/// shape verbatim, with a 24 h `exp`.
fn autonomous_claims(oid: &str, now: u64) -> Map<String, Value> {
    let mut claims = Map::new();
    let mut put = |k: &str, v: Value| claims.insert(k.to_string(), v);
    put("aud", json!(DEMO_AUDIENCE));
    put(
        "iss",
        json!(format!(
            "https://login.microsoftonline.com/{DEMO_TENANT}/v2.0"
        )),
    );
    put("iat", json!(now - 60));
    put("nbf", json!(now - 60));
    put("exp", json!(now + TOKEN_TTL_SECS));
    put("tid", json!(DEMO_TENANT));
    put("oid", json!(oid));
    put("sub", json!(oid));
    put("azp", json!(DEMO_APP_ID));
    put("ver", json!("2.0"));
    put("idtyp", json!("app"));
    put("scp", json!(""));
    put("roles", json!(["Brain.Read"]));
    put("uti", json!(format!("uti-{oid}")));
    put("xms_idrel", json!("7"));
    put("xms_act_fct", json!("11"));
    put("xms_sub_fct", json!("11"));
    put("xms_tnt_fct", json!("1"));
    put("xms_par_app_azp", json!(DEMO_PARENT_APP));
    claims
}

/// Sign the claims RS256 with the given PEM and key id.
fn sign_rs256(claims: &Map<String, Value>, kid: &str, pem: &str) -> Result<String> {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kid.to_string());
    let key = EncodingKey::from_rsa_pem(pem.as_bytes()).context("encoding key from PEM")?;
    jsonwebtoken::encode(&header, &Value::Object(claims.clone()), &key).context("JWT encode")
}

/// Absolute-ish path as a forward-slashed string (drops the Windows `\\?\`).
fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn now_unix() -> u64 {
    // Startup/CLI wall time (24 h expiry is real time — this is not a
    // conformance oracle, so the wall clock is correct here).
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
