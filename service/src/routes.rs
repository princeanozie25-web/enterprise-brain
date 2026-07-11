//! AUTH-4 (threat-model M1): route-completeness, default-deny.
//!
//! Every route the service exposes carries an EXPLICIT auth/scope
//! classification. The `require_session` middleware consults [`classify`] for
//! the request's (method, path): a `Public` route runs without a session, a
//! `SessionRequired` route runs only behind a valid session, and a route that
//! [`classify`] does not recognise returns `None` — which the middleware treats
//! as DENY (fail-closed). The guarantee: a newly-added route is denied until
//! someone classifies it here, never open-by-default.
//!
//! [`REGISTERED_ROUTES`] is the authoritative enumeration of every route in
//! `app()`. The standing test `governance_routes` iterates it and fails the
//! build if any entry is unclassified — so a future route cannot be added to
//! the table without a classification, and (via the middleware) cannot be
//! served without one either.
//!
//! This is purely the migration-completeness layer. It does NOT make scope
//! decisions — the compiler/oracle/handlers (untouched) still do. It only
//! proves every surface declares which of those it stands behind.

use axum::http::Method;

/// The scope/visibility a session-required route stands behind. Declarative:
/// the actual enforcement lives in the handler / compiler / oracle. This names
/// it so every surface is on the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// The caller's own identity/state only (e.g. /scope, /me/scope, logout).
    SelfOnly,
    /// Governed by the compiled per-principal document allowlist (M1).
    DocumentScope,
    /// AUTH-2 structural-core metadata projection (graph/atlas/people/lane view).
    MetadataProjection,
    /// AUTH-3 lens: self by default, cross-principal only as audited view-as.
    LensSelfOrViewAs,
    /// AUTH-3 cross-principal diff: audited view-as, admin/demo gated.
    ViewAsAudited,
    /// M4 owner-only action (agent run, box status, proposal decision).
    OwnerGated,
    /// Approver-only decision (access-request approve/deny).
    ApprovalGated,
    /// A ledger read/append scoped to the caller (access requests/grants).
    LedgerScoped,
}

/// The explicit classification of a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteClass {
    /// No session required. ONLY `/healthz` and `/auth/login`.
    Public,
    /// A valid session is required; the named scope is what it stands behind.
    SessionRequired(ScopeKind),
    /// S1: the machine surface — a valid AGENT TOKEN (JWT bearer) is
    /// required; sessions are refused. The named scope is what the handler
    /// stands behind. `/v1` requests authenticate BEFORE routing (AUTH-4
    /// discipline extended: an unauthenticated probe cannot map the
    /// namespace), so this classification is consulted only after the
    /// token resolves.
    AgentTokenRequired(ScopeKind),
}

#[inline]
fn sr(scope: ScopeKind) -> Option<RouteClass> {
    Some(RouteClass::SessionRequired(scope))
}

#[inline]
fn at(scope: ScopeKind) -> Option<RouteClass> {
    Some(RouteClass::AgentTokenRequired(scope))
}

/// Classify a request route. `Some(class)` is the explicit classification;
/// `None` means UNCLASSIFIED — the middleware denies it (fail-closed). Accepts
/// both concrete paths (`/doc/d0001`) and the registered patterns
/// (`/doc/{id}`): a parameter segment is matched by a wildcard, so a literal
/// `{id}` is captured the same as a real id.
pub fn classify(method: &Method, path: &str) -> Option<RouteClass> {
    use ScopeKind::*;
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match (method.as_str(), segments.as_slice()) {
        // --- Public: the only two routes that run without a session ----------
        ("GET", ["healthz"]) => Some(RouteClass::Public),
        ("POST", ["auth", "login"]) => Some(RouteClass::Public),

        // --- Session: the caller's own identity/state ------------------------
        ("POST", ["auth", "logout"]) => sr(SelfOnly),
        ("GET", ["scope"]) => sr(SelfOnly),
        ("GET", ["me", "scope"]) => sr(SelfOnly),

        // --- Session: document-scope (compiled allowlist, M1) ----------------
        ("POST", ["ask"]) => sr(DocumentScope),
        ("GET", ["doc", _id]) => sr(DocumentScope),
        ("POST", ["export"]) => sr(DocumentScope),

        // --- Session: AUTH-2 metadata projection -----------------------------
        ("GET", ["atlas"]) => sr(MetadataProjection),
        ("GET", ["graph"]) => sr(MetadataProjection),
        ("GET", ["node", _id, "summary"]) => sr(MetadataProjection),
        ("GET", ["people"]) => sr(MetadataProjection),
        ("GET", ["lane"]) => sr(MetadataProjection),
        ("GET", ["lane", "rollup"]) => sr(MetadataProjection),
        ("GET", ["workflow", "project", _cap]) => sr(MetadataProjection),

        // --- Session: AUTH-3 lens / diff (audited view-as) -------------------
        // `/lens/diff` MUST precede `/lens/{id}` — the literal wins over the param.
        ("GET", ["lens", "diff"]) => sr(ViewAsAudited),
        ("GET", ["lens", _id]) => sr(LensSelfOrViewAs),

        // --- Session: lane inbox / boxes (self + owner) ----------------------
        ("GET", ["lane", "inbox"]) => sr(SelfOnly),
        ("POST", ["lane", "inbox", _id, "accept"]) => sr(SelfOnly),
        ("POST", ["lane", "inbox", _id, "dismiss"]) => sr(SelfOnly),
        ("POST", ["lane", "box", _id, "status"]) => sr(OwnerGated),

        // --- Session: M4 agent run + proposal decisions (owner-only) ---------
        ("POST", ["agent", _id, "run"]) => sr(OwnerGated),
        ("GET", ["proposals"]) => sr(LedgerScoped),
        ("POST", ["proposals", _id, "approve"]) => sr(OwnerGated),
        ("POST", ["proposals", _id, "reject"]) => sr(OwnerGated),

        // --- S1: the /v1 machine surface (agent tokens ONLY) -----------------
        ("POST", ["v1", "retrieve"]) => at(DocumentScope),
        ("GET", ["v1", "documents", _id]) => at(DocumentScope),
        ("GET", ["v1", "whoami"]) => at(SelfOnly),

        // --- Session: access requests + grants ledgers -----------------------
        // `/access-requests/inbox` precedes nothing ambiguous (2-seg literal).
        ("GET", ["access-requests"]) => sr(LedgerScoped),
        ("POST", ["access-requests"]) => sr(LedgerScoped),
        ("GET", ["access-requests", "inbox"]) => sr(ApprovalGated),
        ("POST", ["access-requests", _id, "approve"]) => sr(ApprovalGated),
        ("POST", ["access-requests", _id, "deny"]) => sr(ApprovalGated),
        ("GET", ["access-grants"]) => sr(LedgerScoped),
        ("GET", ["access-grants", _id]) => sr(LedgerScoped),
        ("POST", ["access-grants", _id, "revoke"]) => sr(LedgerScoped),

        // --- Anything else: UNCLASSIFIED -> the middleware denies it ---------
        _ => None,
    }
}

/// The authoritative enumeration of every route registered in `app()`, as
/// `(method, pattern)`. The standing test asserts each is classified; keep it
/// in lockstep with `app()`. A route added to `app()` but omitted here is still
/// denied at runtime (the middleware default-denies what `classify` returns
/// `None` for) — this list is the belt to the middleware's braces.
pub const REGISTERED_ROUTES: &[(&str, &str)] = &[
    ("POST", "/ask"),
    ("GET", "/doc/{id}"),
    ("GET", "/scope"),
    ("GET", "/me/scope"),
    ("GET", "/healthz"),
    ("POST", "/auth/login"),
    ("POST", "/auth/logout"),
    ("POST", "/agent/{id}/run"),
    ("GET", "/lens/diff"),
    ("GET", "/lens/{id}"),
    ("GET", "/atlas"),
    ("POST", "/export"),
    ("GET", "/lane"),
    ("POST", "/lane/box/{id}/status"),
    ("GET", "/lane/inbox"),
    ("POST", "/lane/inbox/{id}/accept"),
    ("POST", "/lane/inbox/{id}/dismiss"),
    ("GET", "/lane/rollup"),
    ("GET", "/workflow/project/{capability_id}"),
    ("GET", "/graph"),
    ("GET", "/node/{id}/summary"),
    ("GET", "/people"),
    ("GET", "/access-requests"),
    ("POST", "/access-requests"),
    ("GET", "/access-requests/inbox"),
    ("GET", "/access-grants"),
    ("GET", "/access-grants/{id}"),
    ("POST", "/access-grants/{id}/revoke"),
    ("POST", "/access-requests/{id}/approve"),
    ("POST", "/access-requests/{id}/deny"),
    ("GET", "/proposals"),
    ("POST", "/proposals/{id}/approve"),
    ("POST", "/proposals/{id}/reject"),
    ("POST", "/v1/retrieve"),
    ("GET", "/v1/documents/{id}"),
    ("GET", "/v1/whoami"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_route_is_classified() {
        for (method, pattern) in REGISTERED_ROUTES {
            let m = Method::from_bytes(method.as_bytes()).expect("valid method");
            assert!(
                classify(&m, pattern).is_some(),
                "registered route {method} {pattern} has no explicit classification (M1 default-deny)"
            );
        }
    }

    #[test]
    fn only_healthz_and_login_are_public() {
        let public: Vec<_> = REGISTERED_ROUTES
            .iter()
            .filter(|(method, pattern)| {
                let m = Method::from_bytes(method.as_bytes()).unwrap();
                classify(&m, pattern) == Some(RouteClass::Public)
            })
            .collect();
        assert_eq!(
            public,
            vec![&("GET", "/healthz"), &("POST", "/auth/login")],
            "exactly /healthz and /auth/login are public; everything else needs a session"
        );
    }

    #[test]
    fn unclassified_route_is_none_so_the_middleware_denies_it() {
        // The deliberately-unclassified probe: a route that exists in neither the
        // table nor `classify`. It must resolve to None — which is precisely what
        // the middleware fail-closes on. (If this were a real, served route, this
        // is the moment it would be denied instead of exposed.)
        assert_eq!(classify(&Method::GET, "/totally-new-route"), None);
        assert_eq!(classify(&Method::POST, "/admin/secrets"), None);
        // Right path, wrong method is also unclassified -> denied.
        assert_eq!(classify(&Method::DELETE, "/doc/d0001"), None);
        assert_eq!(classify(&Method::POST, "/healthz"), None);
    }

    #[test]
    fn lens_diff_literal_wins_over_the_lens_param() {
        assert_eq!(
            classify(&Method::GET, "/lens/diff"),
            sr(ScopeKind::ViewAsAudited)
        );
        assert_eq!(
            classify(&Method::GET, "/lens/p088"),
            sr(ScopeKind::LensSelfOrViewAs)
        );
    }
}
