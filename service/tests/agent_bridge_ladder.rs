//! S0 validator suite: one test per decision-ladder row (1–10), first
//! failure wins, each row its own DISTINCT reason. Fixtures reproduce
//! Microsoft's documented autonomous-agent claim shape verbatim and are
//! signed RS256 by a local per-process key (S0-6); the rogue key is
//! published in no JWKS. Fully offline — FileJwks and injected fetchers
//! only; the identical validation code the live HttpJwks path runs.

mod common;

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use common::jwt::{
    self, TokenSpec, OTHER_TENANT, TEST_APP_ID, TEST_AUDIENCE, TEST_KID, TEST_TENANT,
};
use serde_json::json;
use service::agent_bridge::jwks::{FileJwks, HttpJwks, JwksProvider};
use service::agent_bridge::{
    Bridge, BridgeOutcome, DenyReason, RegisteredAgent, Registry, TokenValidator,
};

/// The registered fixture agent (oid -> EB principal).
const QA_OID: &str = "aaaa1111-0000-4000-8000-00000000qa01";
/// A second registered agent in the OTHER tenant — exists to prove the
/// tenant rows deny BEFORE registration could ever be consulted.
const OTHER_TENANT_OID: &str = "bbbb2222-0000-4000-8000-0000000000b2";
/// An oid registered nowhere (its azp still matches a registered agent's).
const UNREGISTERED_OID: &str = "cccc3333-0000-4000-8000-0000000000c3";

fn scratch(name: &str) -> PathBuf {
    // Unique per invocation: Windows scanners (Search indexer / Defender) can
    // hold a just-deleted path in delete-pending state, so re-creating the
    // SAME path races them into Os error 5 "Access is denied". A fresh suffix
    // never re-opens a dying path; prior runs' dirs are swept best-effort (a
    // locked leftover is skipped now and reaped on a later run).
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    // The base lives in the SYSTEM temp dir, not target/tmp: the repo sits
    // under Documents\, which Windows Search indexes by default — its crawler
    // opens freshly written index segments mid-build and the write fails with
    // os error 5. AppData\Local\Temp is outside the default index scope.
    let base = std::env::temp_dir().join("enterprise-brain-test-scratch");
    std::fs::create_dir_all(&base).expect("scratch base");
    let prefix = format!("{name}-");
    if let Ok(entries) = base.read_dir() {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let dir = base.join(format!(
        "{prefix}{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn registry() -> Registry {
    Registry::from_entries(&[
        RegisteredAgent {
            tid: TEST_TENANT.to_string(),
            oid: QA_OID.to_string(),
            principal: "agent_qa_drafter".to_string(),
        },
        RegisteredAgent {
            tid: OTHER_TENANT.to_string(),
            oid: OTHER_TENANT_OID.to_string(),
            principal: "agent_exec_brief".to_string(),
        },
    ])
    .expect("fixture registry")
}

/// The shared bridge under test: FileJwks over the fixture key, RS256 only.
fn bridge() -> &'static Bridge {
    static BRIDGE: OnceLock<Bridge> = OnceLock::new();
    BRIDGE.get_or_init(|| {
        let dir = scratch("bridge-ladder-jwks");
        let jwks_path = dir.join("jwks.json");
        fs::write(&jwks_path, &jwt::issuer().jwks_json).expect("write jwks");
        let validator = TokenValidator::new(
            TEST_TENANT,
            TEST_AUDIENCE,
            &["RS256".to_string()],
            Box::new(FileJwks::load(&jwks_path).expect("fixture jwks")),
        )
        .expect("validator");
        Bridge::from_parts(validator, registry())
    })
}

fn deny_reason(outcome: BridgeOutcome) -> DenyReason {
    match outcome {
        BridgeOutcome::Denied { reason, .. } => reason,
        BridgeOutcome::Resolved { principal, .. } => {
            panic!("expected a deny, resolved as {principal}")
        }
    }
}

fn resolved_principal(outcome: BridgeOutcome) -> String {
    match outcome {
        BridgeOutcome::Resolved { principal, .. } => principal,
        BridgeOutcome::Denied { reason, .. } => {
            panic!("expected a resolve, denied {}", reason.as_str())
        }
    }
}

// Row 1: parseable JWT structure.
#[test]
fn row1_garbage_is_token_malformed() {
    for garbage in [
        "not-a-jwt.at.all",
        "ga.rb",
        "!!!.###.$$$",
        "e30.e30", // two segments only
    ] {
        assert_eq!(
            deny_reason(bridge().authenticate(garbage)),
            DenyReason::TokenMalformed,
            "{garbage:?}"
        );
    }
}

// Row 2: asymmetric algorithms only.
#[test]
fn row2_alg_none_and_hmac_are_algorithm_rejected() {
    let spec = TokenSpec::autonomous(QA_OID);
    assert_eq!(
        deny_reason(bridge().authenticate(&spec.alg_none())),
        DenyReason::AlgorithmRejected,
        "alg none is an attack, not a configuration"
    );
    assert_eq!(
        deny_reason(bridge().authenticate(&spec.sign_hs256())),
        DenyReason::AlgorithmRejected,
        "a symmetric alg on an externally-issued token is an attack"
    );
}

// Row 3: signature against the JWKS key.
#[test]
fn row3_wrong_key_unknown_kid_and_tampered_signature_are_signature_invalid() {
    // Signed by a key published in NO JWKS, claiming the good kid.
    let forged = TokenSpec::autonomous(QA_OID).sign_with(jwt::rogue_signing_pem());
    assert_eq!(
        deny_reason(bridge().authenticate(&forged)),
        DenyReason::SignatureInvalid
    );
    // A kid the JWKS does not hold.
    let unknown_kid = TokenSpec::autonomous(QA_OID).kid(Some("kid-nobody")).sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&unknown_kid)),
        DenyReason::SignatureInvalid
    );
    // One flipped signature byte on an otherwise-valid token.
    let tampered = jwt::tamper_signature(&TokenSpec::autonomous(QA_OID).sign());
    assert_eq!(
        deny_reason(bridge().authenticate(&tampered)),
        DenyReason::SignatureInvalid
    );
}

// Row 4: issuer must be the configured tenant in version-appropriate form.
#[test]
fn row4_issuer_mismatches_including_cross_version_forms() {
    // Another tenant's v2 issuer.
    let wrong_tenant_iss = TokenSpec::autonomous(QA_OID)
        .with(
            "iss",
            json!(format!(
                "https://login.microsoftonline.com/{OTHER_TENANT}/v2.0"
            )),
        )
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&wrong_tenant_iss)),
        DenyReason::IssuerMismatch
    );
    // v1 issuer form carrying a v2 `ver` claim.
    let v1_iss_v2_ver = TokenSpec::autonomous(QA_OID)
        .with(
            "iss",
            json!(format!("https://sts.windows.net/{TEST_TENANT}/")),
        )
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&v1_iss_v2_ver)),
        DenyReason::IssuerMismatch
    );
    // v2 issuer form carrying a v1 `ver` claim (the inverse).
    let v2_iss_v1_ver = TokenSpec::autonomous(QA_OID)
        .with("ver", json!("1.0"))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&v2_iss_v1_ver)),
        DenyReason::IssuerMismatch
    );
    // An undeterminable version has no valid issuer form.
    let unknown_ver = TokenSpec::autonomous(QA_OID)
        .with("ver", json!("3.0"))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&unknown_ver)),
        DenyReason::IssuerMismatch
    );
}

// Row 5: audience is this gateway.
#[test]
fn row5_wrong_audience_is_audience_mismatch() {
    let wrong_aud = TokenSpec::autonomous(QA_OID)
        .with("aud", json!("api://someone-else"))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&wrong_aud)),
        DenyReason::AudienceMismatch
    );
}

// Row 6: exp / nbf with exactly 60s skew.
#[test]
fn row6_time_bounds_with_bounded_skew() {
    let now = jwt::now_unix();
    let expired = TokenSpec::autonomous(QA_OID)
        .with("exp", json!(now - 300))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&expired)),
        DenyReason::TokenExpired,
        "5 minutes past exp is expired"
    );
    // 30s past exp sits INSIDE the 60s skew: passes time, resolves.
    let skewed = TokenSpec::autonomous(QA_OID)
        .with("exp", json!(now - 30))
        .sign();
    assert_eq!(
        resolved_principal(bridge().authenticate(&skewed)),
        "agent_qa_drafter",
        "30s past exp is within the 60s skew tolerance"
    );
    let premature = TokenSpec::autonomous(QA_OID)
        .with("nbf", json!(now + 300))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&premature)),
        DenyReason::TokenNotYetValid,
        "5 minutes before nbf is not yet valid"
    );
    // A token with NO exp never validates (a token that cannot expire is
    // denied as expired, fail closed).
    let no_exp = TokenSpec::autonomous(QA_OID).without("exp").sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&no_exp)),
        DenyReason::TokenExpired
    );
}

// Row 7: the tid claim itself — and the forgery NEVER reaches registration.
#[test]
fn row7_tenant_mismatch_denies_before_registration_is_consulted() {
    // The issuer is the CONFIGURED tenant's valid v2 form (it parses and
    // matches row 4); the tid claim inside is another tenant. Crucially,
    // (OTHER_TENANT, OTHER_TENANT_OID) IS a registered pair — if the ladder
    // ever consulted the registry for this token, it would resolve. It must
    // deny at row 7 instead.
    let forged = TokenSpec::autonomous(OTHER_TENANT_OID)
        .with("tid", json!(OTHER_TENANT))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&forged)),
        DenyReason::TenantMismatch,
        "a wrong-tenant token dies at row 7, never at registration"
    );
}

// Row 8: scenario discriminators.
#[test]
fn row8_delegated_and_agent_user_shapes_are_unsupported() {
    let now = jwt::now_unix();
    // On-behalf-of human: idtyp user, no agent subject facet, scp populated.
    let obo = TokenSpec::autonomous(QA_OID)
        .with("idtyp", json!("user"))
        .without("xms_sub_fct")
        .without("xms_act_fct")
        .with("scp", json!("Brain.Read"))
        .with("exp", json!(now + 3600))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&obo)),
        DenyReason::UnsupportedTokenTypeDelegated
    );
    // A token issued to the registered Agent Identity but without the signed
    // AgentIdentity facets is not an autonomous Agent ID token. An omitted
    // optional `idtyp` cannot turn this shape into an allow — and it is not
    // provably DELEGATED either: it is evidence-insufficient, so it denies as
    // agent_facets_missing (the S0b taxonomy: operators land on the
    // attestation runbook row, not the delegated one). This is the live
    // Microsoft preview token shape, reproduced (docs/s0b-launch-gate.md).
    let nonfaceted_agent = TokenSpec::v1_autonomous(QA_OID)
        .without("idtyp")
        .without("scp")
        .without("xms_sub_fct")
        .without("xms_act_fct")
        .without("xms_par_app_azp")
        .without("xms_idrel")
        .with("appid", json!(QA_OID))
        .with("roles", json!(["Agent.Access"]))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&nonfaceted_agent)),
        DenyReason::AgentFacetsMissing
    );
    // The agent's user account: subject facet 13.
    let agent_user = TokenSpec::autonomous(QA_OID)
        .with("idtyp", json!("user"))
        .with("xms_sub_fct", json!("13"))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&agent_user)),
        DenyReason::UnsupportedTokenTypeAgentUser
    );
    // A hybrid claiming app AND the 13 facet is the interactive shape —
    // ambiguity never classifies as autonomous.
    let hybrid = TokenSpec::autonomous(QA_OID)
        .with("xms_sub_fct", json!("11 13"))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&hybrid)),
        DenyReason::UnsupportedTokenTypeAgentUser
    );
}

// Row 9: agent facets — required values must be present, unknown ignored.
#[test]
fn row9_facets_required_and_unknown_values_ignored() {
    // idtyp app but no agent facets: a generic service-principal app token.
    let generic_app = TokenSpec::autonomous(QA_OID)
        .without("xms_sub_fct")
        .without("xms_act_fct")
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&generic_app)),
        DenyReason::AgentFacetsMissing
    );
    // Unknown EXTRA facet values must not fail classification (Microsoft:
    // ignore unknown values) — this token still resolves.
    let extra_facets = TokenSpec::autonomous(QA_OID)
        .with("xms_sub_fct", json!("11 99 3"))
        .with("xms_act_fct", json!("11 42"))
        .sign();
    assert_eq!(
        resolved_principal(bridge().authenticate(&extra_facets)),
        "agent_qa_drafter"
    );
}

// Row 10: registration keys on (tid, oid) — never on azp / blueprint (S0-3).
#[test]
fn row10_unregistered_oid_with_a_registered_agents_azp_stays_unregistered() {
    // Same tenant, same azp (application/blueprint id) as the registered
    // agent — different agent identity oid. Per-identity keying means deny.
    let sibling = TokenSpec::autonomous(UNREGISTERED_OID)
        .with("azp", json!(TEST_APP_ID))
        .sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&sibling)),
        DenyReason::AgentNotRegistered,
        "S0-3: azp matching a registered blueprint grants nothing"
    );
    // And an oid claim absent entirely cannot register.
    let no_oid = TokenSpec::autonomous(QA_OID).without("oid").sign();
    assert_eq!(
        deny_reason(bridge().authenticate(&no_oid)),
        DenyReason::AgentNotRegistered
    );
}

// The good paths: v2 and v1 shapes resolve to the registered principal.
#[test]
fn valid_v2_and_v1_agent_tokens_resolve_the_registered_principal() {
    let v2 = TokenSpec::autonomous(QA_OID).sign();
    match bridge().authenticate(&v2) {
        BridgeOutcome::Resolved { principal, claims } => {
            assert_eq!(principal, "agent_qa_drafter");
            assert_eq!(claims.azp.as_deref(), Some(TEST_APP_ID));
            assert_eq!(
                claims.parent_app_azp.as_deref(),
                Some(jwt::TEST_PARENT_APP),
                "the parent app id is carried for the audit log"
            );
        }
        BridgeOutcome::Denied { reason, .. } => panic!("v2 denied {}", reason.as_str()),
    }
    // v1: sts.windows.net issuer + appid spelling, normalized to azp.
    let v1 = TokenSpec::v1_autonomous(QA_OID).sign();
    match bridge().authenticate(&v1) {
        BridgeOutcome::Resolved { principal, claims } => {
            assert_eq!(principal, "agent_qa_drafter");
            assert_eq!(
                claims.azp.as_deref(),
                Some(TEST_APP_ID),
                "v1 appid normalizes to azp"
            );
        }
        BridgeOutcome::Denied { reason, .. } => panic!("v1 denied {}", reason.as_str()),
    }
}

// Entra's `idtyp` is optional. A real autonomous Agent ID access-token shape
// may therefore omit it while retaining both signed AgentIdentity facets.
// This must resolve, but only because the facets identify the subject AND
// actor as AgentIdentity; a delegated user remains rejected at row 8.
#[test]
fn autonomous_agent_without_optional_idtyp_resolves() {
    let agent_without_optional_idtyp = TokenSpec::autonomous(QA_OID)
        .without("idtyp")
        .with("xms_sub_fct", json!("9 3 11"))
        .with("xms_act_fct", json!("11 99"))
        .sign();

    assert_eq!(
        resolved_principal(bridge().authenticate(&agent_without_optional_idtyp)),
        "agent_qa_drafter"
    );
}

// The HttpJwks failure path through the FULL ladder: no key -> deny.
#[test]
fn http_jwks_outage_denies_through_the_ladder_never_bypasses() {
    let validator = TokenValidator::new(
        TEST_TENANT,
        TEST_AUDIENCE,
        &["RS256".to_string()],
        Box::new(HttpJwks::with_fetcher(
            "https://tenant.example/keys",
            Duration::from_secs(60),
            Box::new(|_| anyhow::bail!("simulated endpoint outage")),
        )),
    )
    .expect("validator");
    let bridge = Bridge::from_parts(validator, registry());
    let valid_shape = TokenSpec::autonomous(QA_OID).sign();
    assert_eq!(
        deny_reason(bridge.authenticate(&valid_shape)),
        DenyReason::SignatureInvalid,
        "an unreachable key source is a deny, never a bypass"
    );
}

// The S0 validation latency budget: < 5ms per token, warm JWKS cache.
#[test]
fn warm_validation_stays_inside_the_5ms_budget() {
    let token = TokenSpec::autonomous(QA_OID).sign();
    // Warm everything once (key build, first verify).
    let _ = bridge().authenticate(&token);
    const ROUNDS: u32 = 200;
    let started = Instant::now();
    for _ in 0..ROUNDS {
        let outcome = bridge().authenticate(&token);
        assert!(matches!(outcome, BridgeOutcome::Resolved { .. }));
    }
    let total = started.elapsed();
    let mean = total / ROUNDS;
    println!(
        "S0 latency: mean {:?} over {ROUNDS} warm validations (total {:?})",
        mean, total
    );
    assert!(
        mean < Duration::from_millis(5),
        "warm validation must stay under 5ms/token, measured {mean:?}"
    );
}

// The JWKS provider trait seam: FileJwks and HttpJwks answer identically
// for the same document (the validation code cannot tell them apart).
#[test]
fn file_and_http_jwks_resolve_the_same_key() {
    let dir = scratch("bridge-ladder-jwks-parity");
    let path = dir.join("jwks.json");
    fs::write(&path, &jwt::issuer().jwks_json).expect("write jwks");
    let file = FileJwks::load(&path).expect("file jwks");
    let jwks_body = jwt::issuer().jwks_json.clone();
    let http = HttpJwks::with_fetcher(
        "https://tenant.example/keys",
        Duration::from_secs(3600),
        Box::new(move |_| Ok(jwks_body.clone())),
    );
    assert_eq!(
        file.key_for(Some(TEST_KID)).is_some(),
        http.key_for(Some(TEST_KID)).is_some()
    );
    assert!(file.key_for(Some(TEST_KID)).is_some());
}
