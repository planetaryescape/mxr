#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "attachment tests unwrap and match exact fixture failures"
    )
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
}
