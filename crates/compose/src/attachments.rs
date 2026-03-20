use crate::frontmatter::ComposeError;
use std::path::PathBuf;

/// Resolve and validate attachment paths from frontmatter.
/// Supports tilde expansion.
pub fn resolve_attachments(paths: &[String]) -> Result<Vec<ResolvedAttachment>, ComposeError> {
    paths.iter().map(|p| resolve_one_str(p)).collect()
}

pub fn resolve_attachment_paths(paths: &[PathBuf]) -> Result<Vec<ResolvedAttachment>, ComposeError> {
    paths.iter().map(|p| resolve_one_path(p)).collect()
}

#[derive(Debug)]
pub struct ResolvedAttachment {
    pub path: PathBuf,
    pub filename: String,
    pub mime_type: String,
}

fn resolve_one_str(path_str: &str) -> Result<ResolvedAttachment, ComposeError> {
    let expanded = expand_tilde(path_str);
    let path = PathBuf::from(&expanded);
    resolve_one_path(&path).map_err(|err| match err {
        ComposeError::AttachmentNotFound(_) => ComposeError::AttachmentNotFound(path_str.to_string()),
        other => other,
    })
}

fn resolve_one_path(path: &PathBuf) -> Result<ResolvedAttachment, ComposeError> {
    let path = path.clone();

    if !path.exists() {
        return Err(ComposeError::AttachmentNotFound(path.display().to_string()));
    }

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());

    let mime_type = match path.extension().and_then(|e| e.to_str()) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_not_found_error() {
        let result = resolve_one_str("/nonexistent/file.pdf");
        assert!(result.is_err());
        match result.unwrap_err() {
            ComposeError::AttachmentNotFound(path) => {
                assert_eq!(path, "/nonexistent/file.pdf");
            }
            e => panic!("Expected AttachmentNotFound, got: {e}"),
        }
    }

    #[test]
    fn attachment_found_with_correct_mime() {
        let tmp = tempfile::NamedTempFile::with_suffix(".pdf").unwrap();
        let result = resolve_one_str(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(result.mime_type, "application/pdf");
    }

    #[test]
    fn tilde_expansion() {
        let expanded = expand_tilde("~/Documents/test.txt");
        assert!(!expanded.starts_with('~'));
        assert!(expanded.contains("Documents/test.txt"));
    }

    #[test]
    fn no_tilde_passthrough() {
        let path = "/absolute/path/file.txt";
        assert_eq!(expand_tilde(path), path);
    }
}
