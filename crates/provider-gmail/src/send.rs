use crate::mxr_compose::email::{build_message, format_message_for_gmail};
use crate::mxr_core::types::{Address, Draft};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

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
    use crate::mxr_core::id::{AccountId, DraftId};
    use crate::mxr_core::types::ReplyHeaders;

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
    fn encode_for_gmail_base64url() {
        let rfc2822 = b"From: test@test.com\r\nTo: alice@test.com\r\n\r\nHello";
        let encoded = encode_for_gmail(rfc2822);
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
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
}
