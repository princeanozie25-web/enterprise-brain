//! Hand-rolled CORS for the console: ~80 auditable lines instead of a new
//! dependency. ONLY loopback origins may ever be allowed — constructing the
//! layer from any non-loopback origin is refused, mirroring the bind-refusal
//! pattern (A-9). Disallowed origins simply receive no CORS headers (the
//! browser blocks); the service neither errors nor hints.

use std::net::IpAddr;

use anyhow::{bail, Context, Result};
use axum::body::Body;
use axum::http::{header, HeaderValue, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// The only origins the console service will ever allow.
pub const ALLOWED_ORIGINS: [&str; 2] = ["http://localhost:3000", "http://127.0.0.1:3000"];

#[derive(Debug, Clone)]
pub struct CorsConfig {
    allowed: Vec<String>,
}

fn origin_is_loopback(origin: &str) -> Result<()> {
    let rest = origin
        .strip_prefix("http://")
        .with_context(|| format!("origin {origin:?} must be plain http to loopback"))?;
    let host_port = rest.split('/').next().unwrap_or(rest);
    let host = if let Some(v6) = host_port.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        host_port.rsplit_once(':').map_or(host_port, |(h, _)| h)
    };
    if host == "localhost" {
        return Ok(());
    }
    let ip: IpAddr = host
        .parse()
        .with_context(|| format!("origin host {host:?} is neither localhost nor an IP literal"))?;
    let loopback = match ip {
        IpAddr::V4(v4) => v4.octets()[0] == 127,
        IpAddr::V6(v6) => v6.is_loopback(),
    };
    if !loopback {
        bail!("origin {origin} is not loopback; the console CORS layer refuses it");
    }
    Ok(())
}

/// Builds the CORS allowlist, REFUSING any non-loopback origin at
/// construction.
pub fn cors_layer(origins: &[&str]) -> Result<CorsConfig> {
    if origins.is_empty() {
        bail!("CORS layer needs at least one allowed origin");
    }
    for origin in origins {
        origin_is_loopback(origin)?;
    }
    Ok(CorsConfig {
        allowed: origins.iter().map(|o| o.to_string()).collect(),
    })
}

impl CorsConfig {
    fn allow(&self, origin: &HeaderValue) -> bool {
        origin
            .to_str()
            .map(|o| self.allowed.iter().any(|a| a == o))
            .unwrap_or(false)
    }
}

/// Middleware: answers preflights for allowed origins and stamps the CORS
/// headers onto responses. Disallowed origins pass through untouched.
///
/// S1-5: the `/v1` namespace is NOT a browser surface — no preflight is
/// answered and no CORS header is ever stamped there, whatever the origin.
/// A browser cannot be granted what the layer never offers.
pub async fn apply(cors: CorsConfig, request: Request<Body>, next: Next) -> Response {
    if is_v1_path(request.uri().path()) {
        return next.run(request).await;
    }
    let origin = request.headers().get(header::ORIGIN).cloned();
    let allowed_origin = origin.filter(|o| cors.allow(o));

    if request.method() == Method::OPTIONS {
        if let Some(origin) = &allowed_origin {
            return (
                StatusCode::NO_CONTENT,
                [
                    (header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.clone()),
                    (
                        header::ACCESS_CONTROL_ALLOW_METHODS,
                        HeaderValue::from_static("GET, POST, OPTIONS"),
                    ),
                    (
                        header::ACCESS_CONTROL_ALLOW_HEADERS,
                        HeaderValue::from_static("content-type, authorization"),
                    ),
                    (header::VARY, HeaderValue::from_static("Origin")),
                ],
            )
                .into_response();
        }
    }

    let mut response = next.run(request).await;
    if let Some(origin) = allowed_origin {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    response
}

/// The machine namespace: `/v1` and everything under it.
pub fn is_v1_path(path: &str) -> bool {
    path == "/v1" || path.starts_with("/v1/")
}
