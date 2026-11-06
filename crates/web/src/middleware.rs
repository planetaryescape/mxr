//! Bridge middleware: Host-header allowlist and CORS layer construction.
//!
//! The bridge defaults to loopback-only operation; both layers are tuned
//! for that. Non-loopback remote bridge mode is deferred product/security
//! work, not a config-only switch.

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
const LOOPBACK_HOSTS: &[&str] = &["localhost", "mxr.localhost", "127.0.0.1", "[::1]", "::1"];
const X_FRAME_OPTIONS: &str = "x-frame-options";
const X_CONTENT_TYPE_OPTIONS: &str = "x-content-type-options";
const REFERRER_POLICY: &str = "referrer-policy";

/// Strip an optional `:port` suffix from a Host header value.
fn host_only(raw: &str) -> &str {
    // IPv6 literals: `[::1]:7777` -> `[::1]`
    if raw.starts_with('[') {
        if let Some(close) = raw.find(']') {
            return &raw[..=close];
        }
    }
    // IPv4 / hostname: `localhost:7777` -> `localhost`
    raw.rsplit_once(':').map_or(raw, |(host, _)| host)
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
/// origins the operator configured. Localhost names on any port are
/// allowed (for example `http://localhost:*` and `http://mxr.localhost:*`).
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
        is_loopback_origin(origin_str) || allowed.iter().any(|v| v.as_bytes() == origin.as_bytes())
    };
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(allow_predicate))
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
        .expose_headers([header::HeaderName::from_static("x-mxr-workspace")])
}

pub async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    response
}

pub async fn require_bridge_auth(
    State(state): State<crate::AppState>,
    request: Request,
    next: Next,
) -> Response {
    if !crate::auth::token_matches(
        crate::auth::extract_token(request.headers()),
        state.config.auth_token.as_str(),
    ) {
        return crate::auth::BridgeError::Unauthorized.into_response();
    }
    reject_cross_site(&state.config, request, next).await
}

/// The `/events` WebSocket upgrade still accepts the documented
/// `?token=<token>` form for command-line clients, so it needs its own layer:
/// every other `/api/v1` route takes the token from a header only.
pub async fn require_bridge_auth_with_query_token(
    State(state): State<crate::AppState>,
    request: Request,
    next: Next,
) -> Response {
    let query_token = request.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });
    if !crate::auth::token_matches(
        crate::auth::extract_token(request.headers()).or(query_token.as_deref()),
        state.config.auth_token.as_str(),
    ) {
        return crate::auth::BridgeError::Unauthorized.into_response();
    }
    reject_cross_site(&state.config, request, next).await
}

/// A request that a browser marks as cross-site is refused even with a valid
/// credential. Without this, a page on another origin could drive the
/// bridge's mutations with the hosted-browser cookie, or with a token a
/// same-machine attacker lured into a browser. Requests with no `Origin` and
/// no `Sec-Fetch-Site` (CLI clients, native apps) pass.
async fn reject_cross_site(
    config: &crate::WebServerConfig,
    request: Request,
    next: Next,
) -> Response {
    if is_cross_site_request(request.headers(), config) {
        tracing::debug!(
            path = request.uri().path(),
            "bridge refused a cross-site request"
        );
        return (
            StatusCode::FORBIDDEN,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"error":"cross-site request refused","code":"cross_site"}"#,
        )
            .into_response();
    }
    next.run(request).await
}

fn is_cross_site_request(headers: &axum::http::HeaderMap, config: &crate::WebServerConfig) -> bool {
    if headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("cross-site"))
    {
        return true;
    }
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    if is_loopback_origin(origin)
        || config
            .cors_allowlist
            .iter()
            .any(|allowed| allowed.as_str() == origin)
    {
        return false;
    }
    // A same-origin request from the hosted browser names the bridge itself.
    let origin_host = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
        .map(host_only);
    let request_host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .map(host_only);
    match (origin_host, request_host) {
        (Some(origin), Some(host)) => !origin.eq_ignore_ascii_case(host),
        _ => true,
    }
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

    fn headers(pairs: &[(&str, &str)]) -> axum::http::HeaderMap {
        let mut headers = axum::http::HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(value).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn cross_site_requests_are_detected_from_fetch_metadata_or_origin() {
        let config =
            crate::WebServerConfig::new(std::path::PathBuf::from("/tmp/mxr.sock"), "token".into());
        let host = ("host", "127.0.0.1:42829");

        assert!(is_cross_site_request(
            &headers(&[("sec-fetch-site", "cross-site")]),
            &config
        ));
        assert!(is_cross_site_request(
            &headers(&[host, ("origin", "http://evil.example.com")]),
            &config
        ));
        assert!(is_cross_site_request(
            &headers(&[host, ("origin", "null")]),
            &config
        ));
        assert!(!is_cross_site_request(
            &headers(&[host, ("origin", "http://127.0.0.1:42829")]),
            &config
        ));
        assert!(!is_cross_site_request(
            &headers(&[host, ("origin", "http://localhost:5173")]),
            &config
        ));
        assert!(!is_cross_site_request(
            &headers(&[host, ("sec-fetch-site", "same-origin")]),
            &config
        ));
        // No browser metadata at all: a CLI or native client.
        assert!(!is_cross_site_request(&headers(&[host]), &config));

        let configured = config.with_cors_allowlist(vec!["https://app.example.com".into()]);
        assert!(!is_cross_site_request(
            &headers(&[host, ("origin", "https://app.example.com")]),
            &configured
        ));
    }

    #[test]
    fn loopback_origins_are_recognised() {
        assert!(is_loopback_origin("http://localhost"));
        assert!(is_loopback_origin("http://mxr.localhost:42829"));
        assert!(is_loopback_origin("https://localhost"));
        assert!(is_loopback_origin("http://localhost:5173"));
        assert!(is_loopback_origin("http://127.0.0.1:7777"));
        assert!(is_loopback_origin("http://[::1]:7777"));
        assert!(!is_loopback_origin("http://evil.example.com"));
        assert!(!is_loopback_origin("https://localhost.evil.com"));
    }
}
