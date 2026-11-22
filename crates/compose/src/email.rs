pub use mxr_outbound::email::*;

#[cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]
#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, DraftId};
    use mxr_core::types::ReplyHeaders;
    use mxr_core::types::{Address, Draft, DraftContent, DraftIntent};

    fn draft() -> Draft {
        Draft {
            revision: Some(1),
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: Some(ReplyHeaders {
                in_reply_to: "<parent@example.com>".into(),
                references: vec!["<root@example.com>".into()],
                thread_id: None,
            }),
            intent: DraftIntent::Reply,
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
            content: DraftContent::markdown("hello"),
            managed_attachments: None,
            attachments: vec![],
            inline_assets: Vec::new(),
            inline_calendar_reply: None,
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
}
