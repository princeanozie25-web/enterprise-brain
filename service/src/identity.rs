//! FC-A1: identity is SESSION-BOUND, never header-derived.
//!
//! The `x-demo-principal` header no longer determines identity — a caller can
//! no longer assert who it is. A caller authenticates once (`POST /auth/login`)
//! and receives a server-minted session (opaque, expiring, revocable). The
//! `require_session` middleware validates that session and inserts the resolved
//! [`SessionPrincipal`] into the request extensions; this extractor reads ONLY
//! that. No session principal in extensions -> 401 (a backstop behind the
//! middleware, so a route can never run un-authenticated even if mis-wired).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::session::SessionPrincipal;

/// The authenticated principal id, resolved from the validated session. The
/// type name is unchanged so every downstream handler is untouched — only its
/// SOURCE moved from the header to the session.
pub struct DemoPrincipal(pub String);

impl<S: Send + Sync> FromRequestParts<S> for DemoPrincipal {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<SessionPrincipal>()
            .map(|sp| DemoPrincipal(sp.0.clone()))
            .ok_or_else(unauthorized)
    }
}

/// The one 401: no valid session. Shape is constant and reveals nothing.
pub fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "application/json")],
        "{\"demo_identity_mode\":true,\"error\":\"authentication required\"}\n",
    )
        .into_response()
}

/// AUTH-4 (M1): a route with no explicit auth/scope classification is DENIED,
/// never served (default-deny). Returned as a plain 404 so an unclassified
/// route is indistinguishable from an unknown path — the deny and the router's
/// own "no such route" agree, and nothing is leaked about what exists.
pub fn route_denied() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "application/json")],
        "{\"demo_identity_mode\":true,\"error\":\"not found\"}\n",
    )
        .into_response()
}
