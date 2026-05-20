//! Embedded SPA serving — bakes the contents of `apps/web/dist/` into the
//! binary at compile time and serves them at `/` with strict CSP.
//!
//! Only compiled when the `web-ui` cargo feature is enabled. The SPA must be
//! built first (`npm run build` in `apps/web/`) so the dist directory is
//! populated; if it isn't, the bridge serves a placeholder page that points
//! the operator at the build instructions.

use axum::{
    extract::Request,
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use include_dir::{include_dir, Dir};

#[cfg(has_spa_dist)]
static SPA_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../apps/web/dist");
#[cfg(not(has_spa_dist))]
static SPA_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/spa-empty-dist");

/// Strict Content-Security-Policy for the SPA HTML response. Forbids inline
/// scripts (defends against XSS exfiltrating the localStorage bridge token),
/// allows same-origin XHR/WS, blocks framing.
const CSP: &str = "default-src 'self'; \
    script-src 'self'; \
    style-src 'self' 'unsafe-inline'; \
    img-src 'self' data: blob:; \
    font-src 'self' data:; \
    connect-src 'self' ws: wss:; \
    frame-ancestors 'none'; \
    base-uri 'none'; \
    form-action 'self'";

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/", get(index_handler))
        .fallback(get(fallback_handler))
}

async fn index_handler() -> Response {
    serve_path("index.html").unwrap_or_else(missing_dist_response)
}

async fn fallback_handler(req: Request) -> Response {
    // /api/* and /api/v1/openapi.json are handled before this fallback runs
    // by the parent router. Anything else is either a static asset or a
    // client-side route — try the asset first, fall back to index.html.
    let path = req.uri().path().trim_start_matches('/');
    if path.is_empty() {
        return index_handler().await;
    }
    if let Some(resp) = serve_path(path) {
        return resp;
    }
    // Client-side route — let TanStack Router handle it.
    serve_path("index.html").unwrap_or_else(missing_dist_response)
}

fn serve_path(path: &str) -> Option<Response> {
    let entry = SPA_DIST.get_file(path)?;
    let content = entry.contents();
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.clone());
    if path.ends_with(".html") || mime.starts_with("text/html") {
        response = response.header(header::CONTENT_SECURITY_POLICY, CSP);
        response = response.header(header::CACHE_CONTROL, "no-cache");
    } else if path.contains("/assets/") {
        // Vite emits content-hashed asset filenames — safe to cache hard.
        response = response.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    }
    response
        .body(axum::body::Body::from(content))
        .map(IntoResponse::into_response)
        .ok()
}

fn missing_dist_response() -> Response {
    let body = include_str!("spa_placeholder.html");
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CACHE_CONTROL, "no-cache");
    if let Ok(value) = HeaderValue::from_str(CSP) {
        response = response.header(header::CONTENT_SECURITY_POLICY, value);
    }
    response
        .body(axum::body::Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        .into_response()
}

#[allow(dead_code)]
pub(crate) fn _types_used(_: Uri) {}
