#![cfg_attr(test, allow(clippy::panic, clippy::unwrap_used))]

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use mail_builder::MessageBuilder;
use mxr_core::types::{Address, Draft};
use mxr_outbound::attachments::{
    load_attachment_paths_async, load_attachment_paths_sync, LoadedAttachment,
};
use mxr_outbound::email::{
    build_message, build_message_with_attachments, format_message_for_gmail,
};
use mxr_outbound::render::render_markdown;
use std::path::PathBuf;
use std::time::Instant;

/// Build an RFC 5322 message from a Draft and return the raw bytes.
pub fn build_rfc2822(draft: &Draft, from: &Address) -> Result<Vec<u8>, GmailSendError> {
    let message =
        build_message(draft, from, true).map_err(|err| GmailSendError::Build(err.to_string()))?;
    Ok(format_message_for_gmail(&message))
}

pub async fn build_rfc2822_async(draft: &Draft, from: &Address) -> Result<Vec<u8>, GmailSendError> {
    let attachments = load_attachments_async(&draft.attachments).await?;
    let started_at = Instant::now();
    let message = build_message_with_attachments(draft, from, true, &attachments)
        .map_err(|err| GmailSendError::Build(err.to_string()))?;
    tracing::trace!(
        attachment_count = attachments.len(),
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "gmail message build completed"
    );
    Ok(format_message_for_gmail(&message))
}

/// Build an RFC 5322 draft for Gmail save_draft.
///
/// Gmail server-side drafts can exist without recipients, so this avoids the
/// transport-envelope requirement imposed by lettre's builder path.
pub fn build_draft_rfc2822(draft: &Draft, from: &Address) -> Result<Vec<u8>, GmailSendError> {
    let attachments = load_attachments_sync(&draft.attachments)?;
    build_draft_rfc2822_with_attachments(draft, from, &attachments)
}

pub async fn build_draft_rfc2822_async(
    draft: &Draft,
    from: &Address,
) -> Result<Vec<u8>, GmailSendError> {
    let attachments = load_attachments_async(&draft.attachments).await?;
    build_draft_rfc2822_with_attachments(draft, from, &attachments)
}

fn build_draft_rfc2822_with_attachments(
    draft: &Draft,
    from: &Address,
    attachments: &[LoadedAttachment],
) -> Result<Vec<u8>, GmailSendError> {
    let started_at = Instant::now();
    let rendered = render_markdown(&draft.body_markdown);
    let mut builder = address_with_name(MessageBuilder::new(), HeaderAddressKind::From, from)
        .subject(draft.subject.clone())
        .text_body(rendered.plain)
        .html_body(rendered.html);

    for address in &draft.to {
        builder = address_with_name(builder, HeaderAddressKind::To, address);
    }
    for address in &draft.cc {
        builder = address_with_name(builder, HeaderAddressKind::Cc, address);
    }
    for address in &draft.bcc {
        builder = address_with_name(builder, HeaderAddressKind::Bcc, address);
    }

    if let Some(reply_headers) = &draft.reply_headers {
        builder = builder.in_reply_to(normalize_message_id(&reply_headers.in_reply_to));

        let mut references: Vec<String> = reply_headers
            .references
            .iter()
            .map(|reference| normalize_message_id(reference))
            .collect();
        let in_reply_to = normalize_message_id(&reply_headers.in_reply_to);
        if !references.iter().any(|reference| reference == &in_reply_to) {
            references.push(in_reply_to);
        }
        if !references.is_empty() {
            builder = builder.references(references);
        }
    }

    for attachment in attachments {
        builder = builder.attachment(
            attachment.mime_type.clone(),
            attachment.filename.clone(),
            attachment.bytes.clone(),
        );
    }

    builder
        .write_to_vec()
        .map_err(|err| GmailSendError::Build(err.to_string()))
        .inspect(|_| {
            tracing::trace!(
                attachment_count = attachments.len(),
                elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
                "gmail draft build completed"
            );
        })
}

#[derive(Debug, Clone, Copy)]
enum HeaderAddressKind {
    From,
    To,
    Cc,
    Bcc,
}

fn address_with_name<'x>(
    builder: MessageBuilder<'x>,
    kind: HeaderAddressKind,
    address: &'x Address,
) -> MessageBuilder<'x> {
    match address.name.as_deref().filter(|name| !name.is_empty()) {
        Some(name) => match kind {
            HeaderAddressKind::From => builder.from((name, address.email.as_str())),
            HeaderAddressKind::To => builder.to((name, address.email.as_str())),
            HeaderAddressKind::Cc => builder.cc((name, address.email.as_str())),
            HeaderAddressKind::Bcc => builder.bcc((name, address.email.as_str())),
        },
        None => match kind {
            HeaderAddressKind::From => builder.from(address.email.as_str()),
            HeaderAddressKind::To => builder.to(address.email.as_str()),
            HeaderAddressKind::Cc => builder.cc(address.email.as_str()),
            HeaderAddressKind::Bcc => builder.bcc(address.email.as_str()),
        },
    }
}

fn normalize_message_id(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .to_string()
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

fn load_attachments_sync(paths: &[PathBuf]) -> Result<Vec<LoadedAttachment>, GmailSendError> {
    load_attachment_paths_sync(paths).map_err(|err| GmailSendError::Build(err.to_string()))
}

async fn load_attachments_async(
    paths: &[PathBuf],
) -> Result<Vec<LoadedAttachment>, GmailSendError> {
    let started_at = Instant::now();
    let attachments = load_attachment_paths_async(paths)
        .await
        .map_err(|err| GmailSendError::Build(err.to_string()))?;

    tracing::trace!(
        attachment_count = attachments.len(),
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "gmail attachment load completed"
    );

    Ok(attachments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MessageParser;
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
    fn draft_rfc2822_allows_missing_recipients_and_subject() {
        let mut draft = test_draft();
        draft.to.clear();
        draft.subject.clear();

        let raw = build_draft_rfc2822(&draft, &from()).unwrap();
        let msg = String::from_utf8(raw.clone()).unwrap();
        let parsed = MessageParser::default().parse(&raw).unwrap();
        let from = parsed.from().unwrap().first().unwrap();

        assert_eq!(from.name.as_deref(), Some("Me"));
        assert_eq!(from.address.as_deref(), Some("me@example.com"));
        assert!(!msg.contains("\nTo:"));
        assert!(msg.contains("Subject: \r\n"));
        assert!(msg.contains("Content-Type: multipart/alternative"));
        assert!(parsed.subject().is_none());
        assert!(parsed.to().is_none());
        let body_text = parsed.body_text(0).unwrap();
        assert!(body_text.contains("Hello"));
        assert!(body_text.contains("world"));
        assert!(parsed.body_html(0).is_some());
    }

    #[test]
    fn draft_rfc2822_keeps_reply_headers_bcc_and_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, "hello attachment").unwrap();

        let mut draft = test_draft();
        draft.to.clear();
        draft.reply_headers = Some(ReplyHeaders {
            in_reply_to: "<parent@example.com>".into(),
            references: vec!["<root@example.com>".into()],
        });
        draft.bcc = vec![Address {
            name: None,
            email: "hidden@example.com".into(),
        }];
        draft.attachments = vec![path];

        let msg = String::from_utf8(build_draft_rfc2822(&draft, &from()).unwrap()).unwrap();
        assert!(msg.contains("In-Reply-To: <parent@example.com>\r\n"));
        assert!(msg.contains("References: <root@example.com> <parent@example.com>\r\n"));
        assert!(msg.contains("Bcc: <hidden@example.com>\r\n"));
        assert!(msg.contains("note.txt"));
    }

    #[tokio::test]
    async fn draft_rfc2822_async_keeps_reply_headers_bcc_and_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, "hello attachment").unwrap();

        let mut draft = test_draft();
        draft.to.clear();
        draft.reply_headers = Some(ReplyHeaders {
            in_reply_to: "<parent@example.com>".into(),
            references: vec!["<root@example.com>".into()],
        });
        draft.bcc = vec![Address {
            name: None,
            email: "hidden@example.com".into(),
        }];
        draft.attachments = vec![path];

        let msg =
            String::from_utf8(build_draft_rfc2822_async(&draft, &from()).await.unwrap()).unwrap();
        assert!(msg.contains("In-Reply-To: <parent@example.com>\r\n"));
        assert!(msg.contains("References: <root@example.com> <parent@example.com>\r\n"));
        assert!(msg.contains("Bcc: <hidden@example.com>\r\n"));
        assert!(msg.contains("note.txt"));
    }

    #[tokio::test]
    async fn rfc2822_async_keeps_bcc_and_attachments_for_send() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, "hello attachment").unwrap();

        let mut draft = test_draft();
        draft.bcc = vec![Address {
            name: None,
            email: "hidden@example.com".into(),
        }];
        draft.attachments = vec![path];

        let msg = String::from_utf8(build_rfc2822_async(&draft, &from()).await.unwrap()).unwrap();
        assert!(msg.contains("To: Alice <alice@example.com>\r\n"));
        assert!(msg.contains("Bcc: hidden@example.com\r\n"));
        assert!(msg.contains("note.txt"));
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
