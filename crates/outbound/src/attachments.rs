use mxr_core::types::InlineAsset;
use std::path::{Path, PathBuf};

pub const DEFAULT_ATTACHMENT_LOAD_CONCURRENCY: usize = 4;

pub fn resolve_attachments(paths: &[String]) -> Result<Vec<ResolvedAttachment>, AttachmentError> {
    paths.iter().map(|path| resolve_one_str(path)).collect()
}

pub fn resolve_attachment_paths(
    paths: &[PathBuf],
) -> Result<Vec<ResolvedAttachment>, AttachmentError> {
    paths
        .iter()
        .map(PathBuf::as_path)
        .map(resolve_one_path)
        .collect()
}

#[derive(Debug, Clone)]
pub struct ResolvedAttachment {
    pub path: PathBuf,
    pub filename: String,
    pub mime_type: String,
}

#[derive(Debug, Clone)]
pub struct LoadedAttachment {
    pub filename: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

pub fn load_attachment_paths_sync(
    paths: &[PathBuf],
) -> Result<Vec<LoadedAttachment>, AttachmentLoadError> {
    resolve_attachment_paths(paths)?
        .into_iter()
        .map(load_resolved_attachment_sync)
        .collect()
}

pub async fn load_attachment_paths_async(
    paths: &[PathBuf],
) -> Result<Vec<LoadedAttachment>, AttachmentLoadError> {
    load_attachment_paths_async_with_limit(paths, DEFAULT_ATTACHMENT_LOAD_CONCURRENCY).await
}

pub async fn load_attachment_paths_async_with_limit(
    paths: &[PathBuf],
    concurrency: usize,
) -> Result<Vec<LoadedAttachment>, AttachmentLoadError> {
    use futures::{stream, StreamExt};

    let concurrency = concurrency.max(1);
    stream::iter(
        resolve_attachment_paths(paths)?
            .into_iter()
            .map(|attachment| async move { load_resolved_attachment_async(attachment).await }),
    )
    .buffered(concurrency)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect()
}

/// An inline image with its bytes loaded, ready to become a CID-referenced
/// part of a `multipart/related`.
#[derive(Debug, Clone)]
pub struct LoadedInlineAsset {
    /// The `cid:` token the HTML references, without angle brackets.
    pub cid: String,
    pub filename: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

pub fn load_inline_assets_sync(
    assets: &[InlineAsset],
) -> Result<Vec<LoadedInlineAsset>, AttachmentLoadError> {
    assets
        .iter()
        .map(|asset| {
            let resolved = resolve_one_path(&asset.path)?;
            let bytes = std::fs::read(&resolved.path)?;
            Ok(LoadedInlineAsset {
                cid: asset.cid.clone(),
                filename: resolved.filename,
                mime_type: resolved.mime_type,
                bytes,
            })
        })
        .collect()
}

pub async fn load_inline_assets_async(
    assets: &[InlineAsset],
) -> Result<Vec<LoadedInlineAsset>, AttachmentLoadError> {
    use futures::{stream, StreamExt};

    // Resolve up front so each future owns its inputs. Borrowing the caller's
    // `&InlineAsset` across the await would tie the future to that lifetime,
    // which `#[async_trait]`'s boxed return type in the send providers cannot
    // accept ("implementation of `FnOnce` is not general enough").
    let resolved = assets
        .iter()
        .map(|asset| Ok((asset.cid.clone(), resolve_one_path(&asset.path)?)))
        .collect::<Result<Vec<_>, AttachmentError>>()?;

    stream::iter(resolved.into_iter().map(|(cid, asset)| async move {
        let bytes = tokio::fs::read(&asset.path).await?;
        Ok(LoadedInlineAsset {
            cid,
            filename: asset.filename,
            mime_type: asset.mime_type,
            bytes,
        })
    }))
    .buffered(DEFAULT_ATTACHMENT_LOAD_CONCURRENCY)
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect()
}

/// A CID that is safe to place in a `Content-ID` header.
///
/// Rejects anything that could break out of the header or forge a new one —
/// CRLF above all — and keeps the token to the characters an `addr-spec`
/// tolerates.
pub fn validate_cid(cid: &str) -> Result<(), InlineAssetError> {
    if cid.is_empty() {
        return Err(InlineAssetError::InvalidCid {
            cid: cid.to_string(),
            reason: "must not be empty",
        });
    }
    if !cid
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
    {
        return Err(InlineAssetError::InvalidCid {
            cid: cid.to_string(),
            reason: "may only contain letters, digits, and . _ - +",
        });
    }
    Ok(())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InlineAssetError {
    #[error("invalid content id `{cid}`: {reason}")]
    InvalidCid { cid: String, reason: &'static str },
    #[error("duplicate content id `{0}`: each --inline cid must be unique")]
    DuplicateCid(String),
}

#[derive(Debug, thiserror::Error)]
pub enum AttachmentError {
    #[error("attachment not found: {0}")]
    NotFound(String),
}

#[derive(Debug, thiserror::Error)]
pub enum AttachmentLoadError {
    #[error(transparent)]
    Resolve(#[from] AttachmentError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn resolve_one_str(path_str: &str) -> Result<ResolvedAttachment, AttachmentError> {
    let expanded = expand_tilde(path_str);
    let path = PathBuf::from(&expanded);
    resolve_one_path(&path).map_err(|err| match err {
        AttachmentError::NotFound(_) => AttachmentError::NotFound(path_str.to_string()),
    })
}

fn resolve_one_path(path: &Path) -> Result<ResolvedAttachment, AttachmentError> {
    let path = path.to_path_buf();

    if !path.exists() {
        return Err(AttachmentError::NotFound(path.display().to_string()));
    }

    let filename = path.file_name().map_or_else(
        || "attachment".to_string(),
        |name| name.to_string_lossy().to_string(),
    );

    let mime_type = match path.extension().and_then(|extension| extension.to_str()) {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("txt") => "text/plain",
        Some("csv") => "text/csv",
        Some("html" | "htm") => "text/html",
        Some("zip") => "application/zip",
        Some("doc") => "application/msword",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xls") => "application/vnd.ms-excel",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
    .to_string();

    Ok(ResolvedAttachment {
        path,
        filename,
        mime_type,
    })
}

fn load_resolved_attachment_sync(
    attachment: ResolvedAttachment,
) -> Result<LoadedAttachment, AttachmentLoadError> {
    let bytes = std::fs::read(&attachment.path)?;
    Ok(LoadedAttachment {
        filename: attachment.filename,
        mime_type: attachment.mime_type,
        bytes,
    })
}

async fn load_resolved_attachment_async(
    attachment: ResolvedAttachment,
) -> Result<LoadedAttachment, AttachmentLoadError> {
    let bytes = tokio::fs::read(&attachment.path).await?;
    Ok(LoadedAttachment {
        filename: attachment.filename,
        mime_type: attachment.mime_type,
        bytes,
    })
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }

    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cid_that_could_end_a_header_line_is_refused() {
        // The cid is written straight into `Content-ID: <...>`. Anything that
        // can end the line can append headers of the sender's choosing.
        for hostile in [
            "logo\r\nBcc: evil@example.com",
            "logo\nBcc: evil@example.com",
            "logo\r",
            "logo\n",
            "\r\n",
            "logo\r\n\r\n<html>injected body</html>",
            "logo\u{0}",
        ] {
            assert!(
                matches!(
                    validate_cid(hostile),
                    Err(InlineAssetError::InvalidCid { .. })
                ),
                "header forgery slipped through: {hostile:?}"
            );
        }
    }

    #[test]
    fn an_empty_cid_is_refused() {
        assert!(matches!(
            validate_cid(""),
            Err(InlineAssetError::InvalidCid { .. })
        ));
    }

    #[test]
    fn the_documented_cid_charset_is_accepted() {
        // Letters, digits, and `. _ - +`.
        for cid in [
            "logo",
            "notto-logo",
            "logo.v2",
            "hero_image",
            "logo+2x",
            "LOGO",
            "0",
            "a.b_c-d+e9",
        ] {
            assert_eq!(validate_cid(cid), Ok(()), "should have accepted: {cid}");
        }
    }

    #[test]
    fn characters_outside_the_documented_charset_are_refused() {
        for cid in [
            "logo bar",
            "<logo>",
            "logo@example.com",
            "logo:1",
            "logo;name=x",
            "logo\"x",
            "logo\tx",
            "logo/1",
            "lögo",
            "🎉",
        ] {
            assert!(validate_cid(cid).is_err(), "should have refused: {cid:?}");
        }
    }

    #[test]
    fn a_refusal_names_the_offending_cid() {
        let error = validate_cid("logo bar").unwrap_err();
        let rendered = error.to_string();
        assert!(rendered.contains("logo bar"), "{rendered}");
    }
}
