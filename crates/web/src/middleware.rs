//! Bridge middleware: Host-header allowlist and CORS layer construction.
//!
//! The bridge defaults to loopback-only operation; both layers are tuned
//! for that. When the daemon is intentionally bound to a non-loopback
//! address, the operator must opt-in via `[bridge]` config.

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Loopback hosts always allowed regardless of operator config. These are
/// the only hosts that don't expose us to DNS-rebinding attacks because
/// they cannot be poisoned by a malicious authoritative DNS server.
const LOOPBACK_HOSTS: &[&str] = &["localhost", "127.0.0.1", "[::1]", "::1"];

/// Strip an optional `:port` suffix from a Host header value.
fn host_only(raw: &str) -> &str {
    // IPv6 literals: `[::1]:7777` -> `[::1]`
    if raw.starts_with('[') {
        if let Some(close) = raw.find(']') {
            return &raw[..=close];
        }
    }
    // IPv4 / hostname: `localhost:7777` -> `localhost`
    raw.rsplit_once(':').map(|(host, _)| host).unwrap_or(raw)
}

/// Configurable Host-header allowlist applied to every request. Loopback
/// hosts are always accepted; additional hostnames are passed in via the
/// `[bridge].host_allowlist` config.
///
/// Defends against DNS rebinding: a malicious page in a user's browser
/// could resolve `attacker.com` to `127.0.0.1` and make requests carrying
/// the user's bridge token, but the Host header would say `attacker.com`
/// and we would reject it here.
pub async fn host_allowlist(
    State(allowlist): State<Arc<Vec<String>>>,
    request: Request,
    next: Next,
) -> Response {
    let host_header = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(host_only);

    let Some(host) = host_header else {
        return reject_host("missing Host header");
    };

    if LOOPBACK_HOSTS.iter().any(|h| h.eq_ignore_ascii_case(host)) {
        return next.run(request).await;
    }
    if allowlist.iter().any(|h| h.eq_ignore_ascii_case(host)) {
        return next.run(request).await;
    }

    reject_host("Host header not in allowlist")
}

fn reject_host(reason: &'static str) -> Response {
    (
        StatusCode::FORBIDDEN,
        [(header::CONTENT_TYPE, "application/json")],
        format!(r#"{{"error":"{reason}"}}"#),
    )
        .into_response()
}

/// Build a CORS layer that allows the loopback defaults plus any extra
/// origins the operator configured. Localhost on any port is allowed
/// (matches the source-doc behaviour `http://localhost:*` /
/// `https://localhost:*`).
pub fn cors_layer(extra_origins: &[String]) -> CorsLayer {
    let mut allowed: Vec<HeaderValue> = Vec::new();
    for origin in extra_origins {
        if let Ok(value) = HeaderValue::from_str(origin) {
            allowed.push(value);
        }
    }
    let allow_predicate = move |origin: &HeaderValue, _req: &_| {
        let Ok(origin_str) = origin.to_str() else {
            return false;
        };
        is_loopback_origin(origin_str)
            || allowed.iter().any(|v| v.as_bytes() == origin.as_bytes())
    };
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(allow_predicate))
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}

/// `http(s)://(localhost|127.0.0.1|[::1])(:port)?`
fn is_loopback_origin(origin: &str) -> bool {
    let rest = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    let Some(rest) = rest else {
        return false;
    };
    let host = host_only(rest);
    LOOPBACK_HOSTS.iter().any(|h| h.eq_ignore_ascii_case(host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_only_strips_ports_and_keeps_ipv6_brackets() {
        assert_eq!(host_only("localhost:7777"), "localhost");
        assert_eq!(host_only("127.0.0.1:7777"), "127.0.0.1");
        assert_eq!(host_only("[::1]:7777"), "[::1]");
        assert_eq!(host_only("[::1]"), "[::1]");
        assert_eq!(host_only("evil.example.com"), "evil.example.com");
    }

    #[test]
    fn loopback_origins_are_recognised() {
        assert!(is_loopback_origin("http://localhost"));
        assert!(is_loopback_origin("https://localhost"));
        assert!(is_loopback_origin("http://localhost:5173"));
        assert!(is_loopback_origin("http://127.0.0.1:7777"));
        assert!(is_loopback_origin("http://[::1]:7777"));
        assert!(!is_loopback_origin("http://evil.example.com"));
        assert!(!is_loopback_origin("https://localhost.evil.com"));
    }
}
