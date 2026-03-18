use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use mxr_compose::render::render_markdown;
use mxr_core::types::{Address, Draft};

/// Build an RFC 2822 message from a Draft and return the raw string.
pub fn build_rfc2822(draft: &Draft, from: &str) -> Result<String, GmailSendError> {
    let rendered = render_markdown(&draft.body_markdown);

    let mut headers = Vec::new();
    headers.push(format!("From: {from}"));
    headers.push(format!(
        "To: {}",
        draft
            .to
            .iter()
            .map(format_address)
            .collect::<Vec<_>>()
            .join(", ")
    ));

    if !draft.cc.is_empty() {
        headers.push(format!(
            "Cc: {}",
            draft
                .cc
                .iter()
                .map(format_address)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    headers.push(format!("Subject: {}", draft.subject));
    headers.push(format!("Date: {}", chrono::Utc::now().to_rfc2822()));
    headers.push(format!(
        "Message-ID: <{}.mxr@localhost>",
        uuid::Uuid::now_v7()
    ));
    headers.push("MIME-Version: 1.0".to_string());

    if let Some(ref reply_to) = draft.in_reply_to {
        headers.push(format!("In-Reply-To: {reply_to}"));
        headers.push(format!("References: {reply_to}"));
    }

    let boundary = format!("mxr-{}", uuid::Uuid::now_v7());
    headers.push(format!(
        "Content-Type: multipart/alternative; boundary=\"{boundary}\""
    ));

    let mut message = headers.join("\r\n");
    message.push_str("\r\n\r\n");

    // text/plain part
    message.push_str(&format!("--{boundary}\r\n"));
    message.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    message.push_str("Content-Transfer-Encoding: quoted-printable\r\n\r\n");
    message.push_str(&rendered.plain);
    message.push_str("\r\n");

    // text/html part
    message.push_str(&format!("--{boundary}\r\n"));
    message.push_str("Content-Type: text/html; charset=utf-8\r\n");
    message.push_str("Content-Transfer-Encoding: quoted-printable\r\n\r\n");
    message.push_str(&rendered.html);
    message.push_str("\r\n");

    message.push_str(&format!("--{boundary}--\r\n"));

    Ok(message)
}

/// Encode an RFC 2822 message as base64url for Gmail API.
pub fn encode_for_gmail(rfc2822: &str) -> String {
    URL_SAFE_NO_PAD.encode(rfc2822.as_bytes())
}

fn format_address(addr: &Address) -> String {
    match &addr.name {
        Some(name) => format!("{name} <{}>", addr.email),
        None => addr.email.clone(),
    }
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

    fn test_draft() -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            in_reply_to: None,
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

    #[test]
    fn rfc2822_basic_message() {
        let draft = test_draft();
        let msg = build_rfc2822(&draft, "me@example.com").unwrap();
        assert!(msg.contains("From: me@example.com"));
        assert!(msg.contains("To: Alice <alice@example.com>"));
        assert!(msg.contains("Subject: Test Subject"));
        assert!(msg.contains("MIME-Version: 1.0"));
        assert!(msg.contains("Content-Type: multipart/alternative"));
        assert!(msg.contains("text/plain"));
        assert!(msg.contains("text/html"));
    }

    #[test]
    fn rfc2822_reply_has_in_reply_to() {
        let mut draft = test_draft();
        draft.in_reply_to = Some(mxr_core::id::MessageId::new());
        let msg = build_rfc2822(&draft, "me@example.com").unwrap();
        assert!(msg.contains("In-Reply-To:"));
        assert!(msg.contains("References:"));
    }

    #[test]
    fn encode_for_gmail_base64url() {
        let rfc2822 = "From: test@test.com\r\nTo: alice@test.com\r\n\r\nHello";
        let encoded = encode_for_gmail(rfc2822);
        // Should be valid base64url (no +, /, or = padding)
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }

    #[test]
    fn rfc2822_with_cc() {
        let mut draft = test_draft();
        draft.cc = vec![Address {
            name: None,
            email: "bob@example.com".into(),
        }];
        let msg = build_rfc2822(&draft, "me@example.com").unwrap();
        assert!(msg.contains("Cc: bob@example.com"));
    }
}
