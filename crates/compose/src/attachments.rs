#![cfg_attr(
    test,
    allow(clippy::infallible_destructuring_match, clippy::unwrap_used)
)]

pub use mxr_outbound::attachments::*;

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_outbound::attachments::AttachmentError;

    #[test]
    fn attachment_not_found_error() {
        let result = resolve_attachments(&["/nonexistent/file.pdf".to_string()]);
        assert!(result.is_err());
        match result.unwrap_err() {
            AttachmentError::NotFound(path) => {
                assert_eq!(path, "/nonexistent/file.pdf");
            }
        }
    }

    #[test]
    fn attachment_found_with_correct_mime() {
        let tmp = tempfile::NamedTempFile::with_suffix(".pdf").unwrap();
        let result = resolve_attachments(&[tmp.path().to_str().unwrap().to_string()])
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(result.mime_type, "application/pdf");
    }

    #[test]
    fn tilde_expansion() {
        let expanded = dirs::home_dir()
            .map(|home| format!("{}/Documents/test.txt", home.display()))
            .unwrap();
        let resolved = resolve_attachments(&["~/Documents/test.txt".to_string()]);
        let missing_path = match resolved.unwrap_err() {
            AttachmentError::NotFound(path) => path,
        };
        assert!(!expanded.starts_with('~'));
        assert!(expanded.contains("Documents/test.txt"));
        assert_eq!(missing_path, "~/Documents/test.txt");
    }

    #[test]
    fn no_tilde_passthrough() {
        let missing_path =
            match resolve_attachments(&["/absolute/path/file.txt".to_string()]).unwrap_err() {
                AttachmentError::NotFound(path) => path,
            };
        assert_eq!(missing_path, "/absolute/path/file.txt");
    }
}
