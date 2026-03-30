pub use mxr_outbound::email::*;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, DraftId};
    use mxr_core::types::ReplyHeaders;
    use mxr_core::types::{Address, Draft};
    use mxr_test_support::redact_rfc822;

    fn draft() -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: Some(ReplyHeaders {
                in_reply_to: "<parent@example.com>".into(),
                references: vec!["<root@example.com>".into()],
            }),
            to: vec![Address {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![Address {
                name: None,
                email: "hidden@example.com".into(),
            }],
            subject: "Hello".into(),
            body_markdown: "hello".into(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn build_message_keeps_bcc_for_gmail() {
        let message = build_message(
            &draft(),
            &Address {
                name: Some("Me".into()),
                email: "me@example.com".into(),
            },
            true,
        )
        .unwrap();
        let formatted = String::from_utf8(format_message_for_gmail(&message)).unwrap();
        assert!(formatted.contains("Bcc: hidden@example.com\r\n"));
        assert!(formatted.contains("References: <root@example.com> <parent@example.com>\r\n"));
    }

    #[test]
    fn snapshot_reply_message_rfc822() {
        let message = build_message(
            &draft(),
            &Address {
                name: Some("Me".into()),
                email: "me@example.com".into(),
            },
            true,
        )
        .unwrap();
        let formatted = String::from_utf8(format_message_for_gmail(&message)).unwrap();
        insta::assert_snapshot!("reply_message_rfc822", redact_rfc822(&formatted));
    }

    #[test]
    fn snapshot_multipart_message_with_attachment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hello.txt");
        std::fs::write(&path, "hello attachment").unwrap();

        let mut draft = draft();
        draft.subject = "Unicode café".into();
        draft.reply_headers = None;
        draft.attachments = vec![path];

        let message = build_message(
            &draft,
            &Address {
                name: Some("Më Sender".into()),
                email: "sender@example.com".into(),
            },
            false,
        )
        .unwrap();
        let formatted = String::from_utf8(message.formatted()).unwrap();
        insta::assert_snapshot!("multipart_attachment_rfc822", redact_rfc822(&formatted));
    }
}
