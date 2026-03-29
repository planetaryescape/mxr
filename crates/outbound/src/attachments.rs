use std::path::{Path, PathBuf};

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

#[derive(Debug)]
pub struct ResolvedAttachment {
    pub path: PathBuf,
    pub filename: String,
    pub mime_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AttachmentError {
    #[error("attachment not found: {0}")]
    NotFound(String),
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

    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());

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

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }

    path.to_string()
}
