//! S0: typed extraction + scenario classification for Entra agent tokens.
//!
//! A [`ClaimSet`] is a LOSSY, typed view of a validated token's payload —
//! extraction never fails (absent claims become `None` / empty sets) so the
//! decision ladder, not the parser, decides which absences deny. Multivalue
//! claims (`xms_sub_fct`, `xms_act_fct`, `xms_idrel`) follow Microsoft's
//! documented semantics: space-separated integer strings, treated as an
//! unordered set, UNKNOWN values ignored (their presence must not fail a
//! parse; the absence of a REQUIRED value still denies at its ladder row).
//!
//! Classification here is identity-shape only (which Entra scenario minted
//! this token) — it makes no resource decision (S0-1).

use std::collections::BTreeSet;

use serde_json::Value;

/// Agent-identity facet value: `11` = AgentIdentity in `xms_act_fct` /
/// `xms_sub_fct` (Microsoft claim reference, 2026-06-11).
pub const FACET_AGENT_IDENTITY: u64 = 11;
/// Subject facet value `13` = the agent's USER ACCOUNT — an interactive
/// identity S0 explicitly does not support.
pub const FACET_AGENT_USER_ACCOUNT: u64 = 13;

/// The typed view of a token payload. All fields are as-extracted; nothing
/// here has been authorized. `azp` is NORMALIZED: v2 `azp`, falling back to
/// the v1 `appid` spelling — attribution only, never an authorization key
/// (S0-3).
#[derive(Debug, Clone, Default)]
pub struct ClaimSet {
    pub iss: Option<String>,
    pub aud: Vec<String>,
    pub tid: Option<String>,
    pub oid: Option<String>,
    pub sub: Option<String>,
    pub idtyp: Option<String>,
    pub ver: Option<String>,
    /// Normalized client attribution: `azp` (v2) else `appid` (v1).
    pub azp: Option<String>,
    /// Parent application GUID (`xms_par_app_azp`) — LOG ONLY. Authorizing on
    /// it "would result in widespread access by many agents" (Microsoft).
    pub parent_app_azp: Option<String>,
    /// Token unique id, when present.
    pub uti: Option<String>,
    pub sub_fct: BTreeSet<u64>,
    pub act_fct: BTreeSet<u64>,
    pub idrel: BTreeSet<u64>,
}

/// The Entra token scenario, per the discriminator table. Only
/// `AutonomousAgent` proceeds to registration; every other shape denies at
/// ladder row 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    /// An autonomous Agent ID. `idtyp: app` is the documented shape; the
    /// `idtyp` claim is optional in Entra access tokens, so its absence is
    /// accepted only when both signed AgentIdentity facets are present.
    AutonomousAgent,
    /// A human-subject (on-behalf-of / delegated) token.
    Delegated,
    /// The agent's user account (`xms_sub_fct` contains 13).
    AgentUserAccount,
}

impl ClaimSet {
    /// Extracts the typed view from a decoded payload. Never fails: the
    /// ladder rows own every deny decision, including required-claim absence.
    pub fn from_value(claims: &Value) -> ClaimSet {
        ClaimSet {
            iss: str_claim(claims, "iss"),
            aud: aud_claim(claims),
            tid: str_claim(claims, "tid"),
            oid: str_claim(claims, "oid"),
            sub: str_claim(claims, "sub"),
            idtyp: str_claim(claims, "idtyp"),
            ver: str_claim(claims, "ver"),
            azp: str_claim(claims, "azp").or_else(|| str_claim(claims, "appid")),
            parent_app_azp: str_claim(claims, "xms_par_app_azp"),
            uti: str_claim(claims, "uti"),
            sub_fct: multivalue_set(claims.get("xms_sub_fct")),
            act_fct: multivalue_set(claims.get("xms_act_fct")),
            idrel: multivalue_set(claims.get("xms_idrel")),
        }
    }

    /// Ladder row 8: which Entra scenario minted this token. The subject
    /// facet participates because `idtyp` alone cannot distinguish a human
    /// from an agent's user account; a `13` facet classifies as the agent's
    /// user account REGARDLESS of `idtyp` (ambiguous hybrids deny as the
    /// unsupported interactive shape, never as autonomous — EB-4).
    ///
    /// `idtyp` is an optional Entra access-token claim. Its absence must not
    /// turn a signed autonomous Agent ID token into a delegated token, but it
    /// is NEVER an allow by itself: the absent-`idtyp` path requires both
    /// AgentIdentity facets (`11`). An explicit non-`app` value, especially
    /// `user`, remains delegated.
    pub fn scenario(&self) -> Scenario {
        if self.sub_fct.contains(&FACET_AGENT_USER_ACCOUNT) {
            return Scenario::AgentUserAccount;
        }
        match self.idtyp.as_deref() {
            Some("app") => Scenario::AutonomousAgent,
            None if self.has_agent_facets() => Scenario::AutonomousAgent,
            _ => Scenario::Delegated,
        }
    }

    /// Ladder row 9: both agent-identity facets present. Unknown extra
    /// values in either set are ignored by construction (`multivalue_set`).
    pub fn has_agent_facets(&self) -> bool {
        self.sub_fct.contains(&FACET_AGENT_IDENTITY) && self.act_fct.contains(&FACET_AGENT_IDENTITY)
    }
}

/// The issuer string a token of version `ver` MUST carry for the configured
/// tenant. Unknown / absent `ver` yields `None` — the row denies (a token
/// whose version-appropriate form cannot be determined has no valid issuer).
pub fn expected_issuer(ver: Option<&str>, tenant_id: &str) -> Option<String> {
    match ver {
        Some("1.0") => Some(format!("https://sts.windows.net/{tenant_id}/")),
        Some("2.0") => Some(format!(
            "https://login.microsoftonline.com/{tenant_id}/v2.0"
        )),
        _ => None,
    }
}

/// Case-insensitive GUID equality (Entra GUIDs are canonically lowercase;
/// a case-variant of the SAME tenant/object must not bypass a match, and a
/// different value still mismatches).
pub fn guid_eq(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn str_claim(claims: &Value, name: &str) -> Option<String> {
    claims
        .get(name)
        .and_then(Value::as_str)
        .map(|s| s.to_string())
}

/// `aud` may be a single string or an array of strings (RFC 7519 §4.1.3).
fn aud_claim(claims: &Value) -> Vec<String> {
    match claims.get("aud") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|s| s.to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// Microsoft multivalue semantics: a space-separated integer string (or,
/// defensively, an array of such strings / numbers), parsed as an unordered
/// set. Unparseable and unknown members are IGNORED — never a parse failure.
pub fn multivalue_set(value: Option<&Value>) -> BTreeSet<u64> {
    let mut set = BTreeSet::new();
    match value {
        Some(Value::String(s)) => collect_ints(s, &mut set),
        Some(Value::Array(items)) => {
            for item in items {
                match item {
                    Value::String(s) => collect_ints(s, &mut set),
                    Value::Number(n) => {
                        if let Some(v) = n.as_u64() {
                            set.insert(v);
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    set
}

fn collect_ints(s: &str, into: &mut BTreeSet<u64>) {
    for part in s.split_whitespace() {
        if let Ok(v) = part.parse::<u64>() {
            into.insert(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn multivalue_parses_sets_and_ignores_unknowns() {
        let v = json!("11 99 3");
        let set = multivalue_set(Some(&v));
        assert!(set.contains(&11) && set.contains(&99) && set.contains(&3));
        // Garbage members are ignored, not fatal.
        let v = json!("11 banana 7");
        let set = multivalue_set(Some(&v));
        assert_eq!(set, BTreeSet::from([7, 11]));
        // Array forms parse defensively too.
        let v = json!(["11", 7]);
        assert_eq!(multivalue_set(Some(&v)), BTreeSet::from([7, 11]));
        // Absent -> empty (the REQUIRING row denies, not the parser).
        assert!(multivalue_set(None).is_empty());
    }

    #[test]
    fn scenario_discriminators_match_the_reference_table() {
        let agent = ClaimSet::from_value(&json!({
            "idtyp": "app", "xms_sub_fct": "11", "xms_act_fct": "11"
        }));
        assert_eq!(agent.scenario(), Scenario::AutonomousAgent);
        assert!(agent.has_agent_facets());

        let obo = ClaimSet::from_value(&json!({ "idtyp": "user" }));
        assert_eq!(obo.scenario(), Scenario::Delegated);

        let agent_user = ClaimSet::from_value(&json!({
            "idtyp": "user", "xms_sub_fct": "13"
        }));
        assert_eq!(agent_user.scenario(), Scenario::AgentUserAccount);

        // A hybrid carrying 13 denies as the interactive shape even with
        // idtyp app — ambiguity never classifies as autonomous.
        let hybrid = ClaimSet::from_value(&json!({
            "idtyp": "app", "xms_sub_fct": "11 13", "xms_act_fct": "11"
        }));
        assert_eq!(hybrid.scenario(), Scenario::AgentUserAccount);

        // Missing idtyp is not an autonomous agent.
        let bare = ClaimSet::from_value(&json!({}));
        assert_eq!(bare.scenario(), Scenario::Delegated);

        // `idtyp` is optional on Entra access tokens. Its omission is not a
        // delegated-user signal when the signed agent facets unambiguously
        // identify an autonomous Agent ID.
        let agent_without_optional_idtyp = ClaimSet::from_value(&json!({
            "xms_sub_fct": "9 3 11", "xms_act_fct": "11 99"
        }));
        assert_eq!(
            agent_without_optional_idtyp.scenario(),
            Scenario::AutonomousAgent
        );

        // An explicit user subject cannot use agent facets to become app-only.
        let user_with_agent_facets = ClaimSet::from_value(&json!({
            "idtyp": "user", "xms_sub_fct": "11", "xms_act_fct": "11"
        }));
        assert_eq!(user_with_agent_facets.scenario(), Scenario::Delegated);
    }

    #[test]
    fn issuer_forms_are_version_appropriate() {
        let tid = "f8cdef31-a31e-4b4a-93e4-5f571e91255a";
        assert_eq!(
            expected_issuer(Some("1.0"), tid).unwrap(),
            format!("https://sts.windows.net/{tid}/")
        );
        assert_eq!(
            expected_issuer(Some("2.0"), tid).unwrap(),
            format!("https://login.microsoftonline.com/{tid}/v2.0")
        );
        assert_eq!(expected_issuer(Some("3.0"), tid), None);
        assert_eq!(expected_issuer(None, tid), None);
    }

    #[test]
    fn azp_normalizes_v1_appid() {
        let v1 = ClaimSet::from_value(&json!({ "appid": "app-guid-1" }));
        assert_eq!(v1.azp.as_deref(), Some("app-guid-1"));
        let v2 = ClaimSet::from_value(&json!({ "azp": "app-guid-2", "appid": "shadow" }));
        assert_eq!(v2.azp.as_deref(), Some("app-guid-2"), "v2 azp wins");
    }
}
