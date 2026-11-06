//! Assemble a bounded message image from path-free daemon chunks.
use crate::{AppState, BridgeError};
use axum::{http::HeaderMap, response::Response};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use mxr_core::MessageId;
use mxr_protocol::{Request, ResponseData, MAX_ATTACHMENT_CHUNK_BYTES, MAX_INLINE_IMAGE_BYTES};

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
    Response::builder()
        .header(
            axum::http::header::CONTENT_TYPE,
            mime.unwrap_or_else(|| "application/octet-stream".into()),
        )
        .header(axum::http::header::CACHE_CONTROL, "no-store")
        .header("x-content-type-options", "nosniff")
        .body(axum::body::Body::from(bytes))
        .map_err(|_| BridgeError::UnexpectedResponse)
}
