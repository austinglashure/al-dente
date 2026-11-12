//! The request guard: Host, Origin and token checks, security headers, and
//! turning handler panics into 500s.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::AppState;

/// The header the UI sends the session token in.
pub const TOKEN_HEADER: &str = "x-aldente-token";

/// The Content-Security-Policy from `07-safety.md`.
pub const CSP: &str = "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";

/// Rejects requests with a foreign `Host` (DNS rebinding) or `Origin` (403)
/// and API requests without the token (401); runs the handler in its own
/// task so a panic becomes a 500; adds security headers to every response.
///
/// Example: `GET /api/v1/snapshot` with `Host: evil.example` gives 403.
pub async fn guard(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let mut res = match check(&state, req.headers(), req.uri().path()) {
        Err(status) => (status, status.canonical_reason().unwrap_or("refused")).into_response(),
        Ok(()) => match tokio::spawn(next.run(req)).await {
            Ok(res) => res,
            Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response(),
        },
    };
    let headers = res.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

/// The checks, in order: Host, Origin, then the token for `/api/` paths.
///
/// Example: a request with `Host: 127.0.0.1:8021`, no Origin and the right
/// token to `/api/v1/graph` passes.
fn check(state: &AppState, headers: &HeaderMap, path: &str) -> Result<(), StatusCode> {
    let hosts = [
        format!("127.0.0.1:{}", state.port),
        format!("localhost:{}", state.port),
        format!("[::1]:{}", state.port),
    ];
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !hosts.iter().any(|h| h == host) {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str().unwrap_or("");
        if !hosts.iter().any(|h| origin == format!("http://{h}")) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    if path.starts_with("/api/") {
        let token = headers
            .get(TOKEN_HEADER)
            .and_then(|t| t.to_str().ok())
            .unwrap_or("");
        if !constant_time_eq(token.as_bytes(), state.token.as_bytes()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(())
}

/// Compares two byte strings without stopping at the first difference.
///
/// Example: `constant_time_eq(b"ab", b"ab")` is `true`; `b"ab"` vs `b"ac"` is `false`.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
