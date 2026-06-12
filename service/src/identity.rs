//! DEMO identity layer — honest about being a stand-in for real OIDC.
//!
//! Requests carry `X-Demo-Principal`. No header -> 401. No default
//! principal, ever. An unknown principal is NOT rejected here: it flows to
//! the answer layer, which serves the empty scope (deny by default), with a
//! response shape indistinguishable from a principal granted nothing.
//! Every response in the service carries `demo_identity_mode: true`.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

pub const DEMO_PRINCIPAL_HEADER: &str = "x-demo-principal";

/// The authenticated (demo) principal id.
pub struct DemoPrincipal(pub String);

impl<S: Send + Sync> FromRequestParts<S> for DemoPrincipal {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let value = parts
            .headers
            .get(DEMO_PRINCIPAL_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|v| !v.is_empty());
        match value {
            Some(principal) => Ok(DemoPrincipal(principal.to_string())),
            None => Err((
                StatusCode::UNAUTHORIZED,
                [(header::CONTENT_TYPE, "application/json")],
                "{\"demo_identity_mode\":true,\"error\":\"missing X-Demo-Principal header\"}\n",
            )
                .into_response()),
        }
    }
}
