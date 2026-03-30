use crate::attachments::{resolve_attachment_paths, AttachmentError};
use crate::render::render_markdown;
use lettre::message::{header::ContentType, Attachment, Mailbox, Message, MultiPart, SinglePart};
use mxr_core::types::{Address, Draft};
use std::fs;

pub fn build_message(
    draft: &Draft,
    from: &Address,
    keep_bcc: bool,
) -> Result<Message, EmailBuildError> {
    let from_mailbox = to_mailbox(from)?;
    let message_id_domain = from
        .email
        .split_once('@')
        .map(|(_, domain)| domain)
        .filter(|domain| !domain.is_empty())
        .unwrap_or("localhost");

    let mut builder = Message::builder()
        .from(from_mailbox)
        .subject(&draft.subject)
        .message_id(Some(format!(
            "<{}@{}>",
            uuid::Uuid::now_v7(),
            message_id_domain
        )));

    if keep_bcc {
        builder = builder.keep_bcc();
    }

    for addr in &draft.to {
        builder = builder.to(to_mailbox(addr)?);
    }

    for addr in &draft.cc {
        builder = builder.cc(to_mailbox(addr)?);
    }

    for addr in &draft.bcc {
        builder = builder.bcc(to_mailbox(addr)?);
    }

    if let Some(reply_headers) = &draft.reply_headers {
        builder = builder.in_reply_to(reply_headers.in_reply_to.clone());

        let mut references = reply_headers.references.clone();
        if !references
            .iter()
            .any(|reference| reference == &reply_headers.in_reply_to)
        {
            references.push(reply_headers.in_reply_to.clone());
        }

        if !references.is_empty() {
            builder = builder.references(references.join(" "));
        }
    }

    let rendered = render_markdown(&draft.body_markdown);
    let alternative = MultiPart::alternative()
        .singlepart(
            SinglePart::builder()
                .header(
                    ContentType::parse("text/plain; charset=utf-8")
                        .expect("static text/plain content type should parse"),
                )
                .body(rendered.plain),
        )
        .singlepart(
            SinglePart::builder()
                .header(
                    ContentType::parse("text/html; charset=utf-8")
                        .expect("static text/html content type should parse"),
                )
                .body(rendered.html),
        );

    let body = if draft.attachments.is_empty() {
        alternative
    } else {
        let mut mixed = MultiPart::mixed().multipart(alternative);
        for attachment in resolve_attachment_paths(&draft.attachments)? {
            let content_type = ContentType::parse(&attachment.mime_type).unwrap_or(
                ContentType::parse("application/octet-stream")
                    .expect("static octet-stream content type should parse"),
            );
            let bytes = fs::read(&attachment.path)?;
            mixed =
                mixed.singlepart(Attachment::new(attachment.filename).body(bytes, content_type));
        }
        mixed
    };

    builder
        .multipart(body)
        .map_err(|err| EmailBuildError::Message(err.to_string()))
}

pub fn format_message_for_gmail(message: &Message) -> Vec<u8> {
    message.formatted()
}

fn to_mailbox(addr: &Address) -> Result<Mailbox, EmailBuildError> {
    let email = addr
        .email
        .parse()
        .map_err(|err: lettre::address::AddressError| {
            EmailBuildError::InvalidAddress(err.to_string())
        })?;
    Ok(Mailbox::new(addr.name.clone(), email))
}

#[derive(Debug, thiserror::Error)]
pub enum EmailBuildError {
    #[error("invalid address: {0}")]
    InvalidAddress(String),
    #[error("attachment error: {0}")]
    Attachment(#[from] AttachmentError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to build message: {0}")]
    Message(String),
}
