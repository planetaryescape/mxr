//! Portable reads of message-owned cached images, with no remote fetch or caller path.
use super::*;
use mxr_protocol::{InlineImageChunkData, MAX_INLINE_IMAGE_BYTES};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

pub(in crate::handler) async fn read_inline_image(
    state: &AppState,
    message_id: &MessageId,
    source: &str,
    offset: u64,
    length: u32,
) -> HandlerResult {
    if length == 0 || length > MAX_ATTACHMENT_CHUNK_BYTES {
        return Err("Inline image chunk length is outside the transfer limit".into());
    }
    let body = load_body_for_message(state, message_id).await?;
    let html = body
        .text_html
        .as_deref()
        .ok_or("Message has no HTML images")?;
    if !collect_html_image_sources(html)
        .iter()
        .any(|item| item == source)
    {
        return Err("Inline image source is not present in this message".into());
    }
    let asset = resolve_html_image_asset(state, message_id, &body, source, false).await;
    if asset.status != HtmlImageAssetStatus::Ready
        || matches!(
            asset.kind,
            HtmlImageSourceKind::Remote | HtmlImageSourceKind::File
        )
    {
        return Err("Inline image is unavailable; external and file sources are refused".into());
    }
    let path = asset.path.ok_or("Inline image is not cached")?;
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| error.to_string())?;
    let total_length = file
        .metadata()
        .await
        .map_err(|error| error.to_string())?
        .len();
    if total_length > MAX_INLINE_IMAGE_BYTES || offset > total_length {
        return Err("Inline image exceeds the preview limit or offset is outside the image".into());
    }
    let length = u64::from(length).min(total_length - offset) as usize;
    let mut bytes = vec![0u8; length];
    file.seek(std::io::SeekFrom::Start(offset))
        .await
        .map_err(|error| error.to_string())?;
    file.read_exact(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    Ok(ResponseData::InlineImageChunk {
        chunk: InlineImageChunkData {
            offset,
            total_length,
            mime_type: asset
                .mime_type
                .filter(|mime| mime.starts_with("image/"))
                .unwrap_or_else(|| "application/octet-stream".into()),
            data_base64: STANDARD.encode(bytes),
        },
    })
}
