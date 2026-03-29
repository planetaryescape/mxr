use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use mxr_core::types::{Address, Draft};
use mxr_outbound::email::{build_message, format_message_for_gmail};

/// Build an RFC 5322 message from a Draft and return the raw bytes.
pub fn build_rfc2822(draft: &Draft, from: &Address) -> Result<Vec<u8>, GmailSendError> {
    let message =
        build_message(draft, from, true).map_err(|err| GmailSendError::Build(err.to_string()))?;
    Ok(format_message_for_gmail(&message))
}

/// Encode an RFC 5322 message as base64url for Gmail API.
pub fn encode_for_gmail(rfc2822: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(rfc2822)
}

#[derive(Debug, thiserror::Error)]
pub enum GmailSendError {
    #[error("Failed to build message: {0}")]
    Build(String),
    #[error("Gmail API error: {0}")]
    Api(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, DraftId};
    use mxr_core::types::ReplyHeaders;

    fn test_draft() -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: None,
            to: vec![Address {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test Subject".into(),
            body_markdown: "Hello **world**!".into(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn from() -> Address {
        Address {
            name: Some("Me".into()),
            email: "me@example.com".into(),
        }
    }

    #[test]
    fn rfc2822_basic_message() {
        let draft = test_draft();
        let msg = String::from_utf8(build_rfc2822(&draft, &from()).unwrap()).unwrap();
        assert!(msg.contains("From: Me <me@example.com>"));
        assert!(msg.contains("To: Alice <alice@example.com>"));
        assert!(msg.contains("Subject: Test Subject"));
        assert!(msg.contains("MIME-Version: 1.0"));
        assert!(msg.contains("Content-Type: multipart/alternative"));
        assert!(msg.contains("text/plain; charset=utf-8"));
        assert!(msg.contains("text/html; charset=utf-8"));
        assert!(msg.contains("\r\n"));
    }

    #[test]
    fn rfc2822_reply_has_full_references_chain() {
        let mut draft = test_draft();
        draft.reply_headers = Some(ReplyHeaders {
            in_reply_to: "<parent@example.com>".into(),
            references: vec!["<root@example.com>".into()],
        });
        let msg = String::from_utf8(build_rfc2822(&draft, &from()).unwrap()).unwrap();
        assert!(msg.contains("In-Reply-To: <parent@example.com>\r\n"));
        assert!(msg.contains("References: <root@example.com> <parent@example.com>\r\n"));
    }

    #[test]
    fn encode_for_gmail_base64url_round_trips_without_padding() {
        let rfc2822 = b"From: test@test.com\r\nTo: alice@test.com\r\n\r\nHello";
        let encoded = encode_for_gmail(rfc2822);
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
        assert_eq!(URL_SAFE_NO_PAD.decode(&encoded).unwrap(), rfc2822);
    }

    #[test]
    fn rfc2822_keeps_bcc_for_gmail_submission() {
        let mut draft = test_draft();
        draft.bcc = vec![Address {
            name: None,
            email: "hidden@example.com".into(),
        }];
        let msg = String::from_utf8(build_rfc2822(&draft, &from()).unwrap()).unwrap();
        assert!(msg.contains("Bcc: hidden@example.com\r\n"));
    }

    #[test]
    fn build_rfc2822_surfaces_message_build_errors() {
        let draft = test_draft();
        let invalid_from = Address {
            name: None,
            email: "not-valid".into(),
        };

        match build_rfc2822(&draft, &invalid_from) {
            Err(GmailSendError::Build(message)) => {
                assert!(message.contains("invalid address"));
            }
            other => panic!("expected GmailSendError::Build, got {other:?}"),
        }
    }
}
