//! Assemble a bounded message image from path-free daemon chunks.
use crate::{AppState, BridgeError};
use axum::{
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use mxr_core::MessageId;
use mxr_protocol::{Request, ResponseData, MAX_ATTACHMENT_CHUNK_BYTES, MAX_INLINE_IMAGE_BYTES};

/// Raster types the reader will serve. `text/html`, `image/svg+xml` and every
/// other mail-authored markup type are refused: the bridge origin carries the
/// hosted browser's credential, so an inline part must never be a top-level
/// document there. This is an allowlist, not a `image/` prefix check.
const ALLOWED_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/avif",
    "image/bmp",
];

/// The one builder for inline-image responses, shared by the local bridge
/// route and the hosted-browser route so their MIME policy and headers cannot
/// drift apart.
///
/// `415 Unsupported Media Type` (empty body) for anything outside
/// [`ALLOWED_TYPES`] — the daemon's `mime_type` is attacker-declared.
pub(crate) fn response(
    mime: Option<&str>,
    bytes: Vec<u8>,
    cache_control: &'static str,
) -> Response {
    let Some(mime) = mime
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .filter(|mime| ALLOWED_TYPES.contains(&mime.as_str()))
    else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache_control)
        .header(header::CONTENT_DISPOSITION, "inline")
        .header("x-content-type-options", "nosniff")
        .header("content-security-policy", "default-src 'none'; sandbox")
        .body(axum::body::Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

pub(crate) async fn read(
    state: &AppState,
    headers: &HeaderMap,
    message_id: MessageId,
    source: String,
) -> Result<Response, BridgeError> {
    crate::ensure_authorized(headers, None, &state.config.auth_token)?;
    let mut bytes = Vec::new();
    let mut expected = None;
    let mut mime = None;
    loop {
        let offset = bytes.len() as u64;
        let response = crate::ipc_request(
            &state.config.socket_path,
            Request::ReadInlineImage {
                message_id: message_id.clone(),
                source: source.clone(),
                offset,
                length: MAX_ATTACHMENT_CHUNK_BYTES,
            },
        )
        .await?;
        let ResponseData::InlineImageChunk { chunk } = response else {
            return Err(BridgeError::UnexpectedResponse);
        };
        if chunk.offset != offset
            || chunk.total_length > MAX_INLINE_IMAGE_BYTES
            || expected.is_some_and(|size| size != chunk.total_length)
            || mime.as_ref().is_some_and(|mime| mime != &chunk.mime_type)
        {
            return Err(BridgeError::UnexpectedResponse);
        }
        let decoded = STANDARD
            .decode(chunk.data_base64)
            .map_err(|_| BridgeError::UnexpectedResponse)?;
        let remaining = chunk
            .total_length
            .checked_sub(offset)
            .ok_or(BridgeError::UnexpectedResponse)?;
        if decoded.len() as u64 != remaining.min(u64::from(MAX_ATTACHMENT_CHUNK_BYTES)) {
            return Err(BridgeError::UnexpectedResponse);
        }
        expected = Some(chunk.total_length);
        mime = Some(chunk.mime_type);
        bytes.extend_from_slice(&decoded);
        if bytes.len() as u64 == chunk.total_length {
            break;
        }
    }
    Ok(response(mime.as_deref(), bytes, "no-store"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_types_are_served_with_locked_down_headers() {
        for mime in ALLOWED_TYPES {
            let response = response(Some(mime), b"img".to_vec(), "no-store");
            assert_eq!(response.status(), StatusCode::OK, "{mime}");
            assert_eq!(response.headers()[header::CONTENT_TYPE], *mime);
            assert_eq!(response.headers()[header::CONTENT_DISPOSITION], "inline");
            assert_eq!(response.headers()["x-content-type-options"], "nosniff");
            assert_eq!(
                response.headers()["content-security-policy"],
                "default-src 'none'; sandbox"
            );
        }
    }

    #[test]
    fn markup_and_unknown_types_are_refused() {
        for mime in [
            "text/html",
            "image/svg+xml",
            "application/xhtml+xml",
            "text/xml",
            "application/octet-stream",
            "image/png; charset=utf-8",
            "",
        ] {
            let response = response(
                Some(mime),
                b"<script>alert(1)</script>".to_vec(),
                "no-store",
            );
            assert_eq!(
                response.status(),
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "{mime:?}"
            );
            assert!(response.headers().get(header::CONTENT_TYPE).is_none());
        }
        let missing = response(None, b"img".to_vec(), "no-store");
        assert_eq!(missing.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
}
