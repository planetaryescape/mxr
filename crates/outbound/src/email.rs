use crate::attachments::{load_attachment_paths_sync, AttachmentLoadError, LoadedAttachment};
use crate::render::render_markdown;
use lettre::message::{header::ContentType, Attachment, Mailbox, Message, MultiPart, SinglePart};
use mxr_core::types::{Address, CalendarReplyMessage, Draft};

/// Generate a stable RFC 5322 Message-ID header for an outgoing message.
/// Daemon callers persist this on the draft before send so retries / failure
/// recovery can reuse it for IMAP dedupe.
pub fn generate_message_id(from: &Address) -> String {
    let domain = from
        .email
        .split_once('@')
        .map(|(_, d)| d)
        .filter(|d| !d.is_empty())
        .unwrap_or("localhost");
    format!("<{}@{}>", uuid::Uuid::now_v7(), domain)
}

pub fn build_message(
    draft: &Draft,
    from: &Address,
    keep_bcc: bool,
) -> Result<Message, EmailBuildError> {
    let attachments = load_attachment_paths_sync(&draft.attachments)?;
    build_message_with_attachments(draft, from, keep_bcc, &attachments)
}

pub fn build_message_with_attachments(
    draft: &Draft,
    from: &Address,
    keep_bcc: bool,
    attachments: &[LoadedAttachment],
) -> Result<Message, EmailBuildError> {
    let message_id = generate_message_id(from);
    build_message_with_id(draft, from, keep_bcc, attachments, &message_id)
}

pub fn build_message_with_id(
    draft: &Draft,
    from: &Address,
    keep_bcc: bool,
    attachments: &[LoadedAttachment],
    message_id: &str,
) -> Result<Message, EmailBuildError> {
    let from_mailbox = to_mailbox(from)?;

    let mut builder = Message::builder()
        .from(from_mailbox)
        .subject(&draft.subject)
        .message_id(Some(message_id.to_string()));

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
    let alternative = if let Some(inline_reply) = draft.inline_calendar_reply.as_ref() {
        // Invite-reply-with-comment path: the alternative carries text/plain
        // (the user's comment) + text/calendar; method=REPLY (the pre-built
        // ICS). RFC 6047 §2.4 — the `method` parameter MUST match the iCal
        // METHOD property. We deliberately omit the text/html half because
        // calendar-server-side processors (Exchange, Google, iCloud) match on
        // the calendar part and never the HTML alternative.
        MultiPart::alternative()
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
                        ContentType::parse(
                            "text/calendar; method=REPLY; charset=utf-8; component=vevent",
                        )
                        .expect("static text/calendar content type should parse"),
                    )
                    .body(inline_reply.ics_body.clone()),
            )
    } else {
        MultiPart::alternative()
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
            )
    };

    let body = if attachments.is_empty() {
        alternative
    } else {
        let mut mixed = MultiPart::mixed().multipart(alternative);
        for attachment in attachments {
            let content_type = ContentType::parse(&attachment.mime_type).unwrap_or(
                ContentType::parse("application/octet-stream")
                    .expect("static octet-stream content type should parse"),
            );
            mixed = mixed.singlepart(
                Attachment::new(attachment.filename.clone())
                    .body(attachment.bytes.clone(), content_type),
            );
        }
        mixed
    };

    builder
        .multipart(body)
        .map_err(|err| EmailBuildError::Message(err.to_string()))
}

pub fn build_calendar_reply_message_with_id(
    reply: &CalendarReplyMessage,
    from: &Address,
    message_id: &str,
) -> Result<Message, EmailBuildError> {
    let builder = Message::builder()
        .from(to_mailbox(from)?)
        .to(to_mailbox(&reply.to)?)
        .subject(&reply.subject)
        .message_id(Some(message_id.to_string()));

    let body = MultiPart::alternative()
        .singlepart(
            SinglePart::builder()
                .header(
                    ContentType::parse("text/plain; charset=utf-8")
                        .expect("static text/plain content type should parse"),
                )
                .body(reply.body_text.clone()),
        )
        .singlepart(
            SinglePart::builder()
                .header(
                    ContentType::parse(
                        "text/calendar; method=REPLY; charset=utf-8; component=vevent",
                    )
                    .expect("static text/calendar content type should parse"),
                )
                .body(reply.ics.clone()),
        );

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
    Attachment(#[from] AttachmentLoadError),
    #[error("failed to build message: {0}")]
    Message(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_reply_message_contains_imip_reply_part() {
        let reply = CalendarReplyMessage {
            to: Address {
                name: Some("Organizer".into()),
                email: "organizer@example.com".into(),
            },
            subject: "Accepted: Demo".into(),
            body_text: "user@example.com has accepted this invitation.".into(),
            ics: concat!(
                "BEGIN:VCALENDAR\r\n",
                "VERSION:2.0\r\n",
                "METHOD:REPLY\r\n",
                "BEGIN:VEVENT\r\n",
                "UID:demo-uid\r\n",
                "ATTENDEE;PARTSTAT=ACCEPTED:mailto:user@example.com\r\n",
                "END:VEVENT\r\n",
                "END:VCALENDAR\r\n"
            )
            .into(),
        };
        let from = Address {
            name: Some("User".into()),
            email: "user@example.com".into(),
        };

        let message =
            build_calendar_reply_message_with_id(&reply, &from, "<reply@example.com>").unwrap();
        let raw = String::from_utf8(message.formatted()).unwrap();

        assert!(raw.contains("Content-Type: multipart/alternative;"));
        assert!(raw.contains("Content-Type: text/calendar; method=REPLY;"));
        assert!(raw.contains("METHOD:REPLY"));
        assert!(raw.contains("PARTSTAT=ACCEPTED"));
    }

    /// A draft with `inline_calendar_reply` must emit the
    /// `multipart/alternative(text/plain + text/calendar;method=REPLY)`
    /// layout instead of the regular `text/plain + text/html` alternative.
    /// This is what makes the comment-compose path interop with
    /// CalDAV-aware organizers.
    #[test]
    fn build_message_inline_calendar_reply_emits_imip_alternative() {
        use mxr_core::id::MessageId;
        use mxr_core::types::{CalendarPartstat, DraftIntent, InlineCalendarReply};

        let from = Address {
            name: Some("User".into()),
            email: "user@example.com".into(),
        };
        let now = chrono::Utc::now();
        let draft = Draft {
            id: mxr_core::id::DraftId::new(),
            account_id: mxr_core::id::AccountId::new(),
            reply_headers: None,
            intent: DraftIntent::Reply,
            to: vec![Address {
                name: None,
                email: "organizer@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Accepted: Demo".into(),
            body_markdown: "Looking forward to it.".into(),
            attachments: vec![],
            inline_calendar_reply: Some(InlineCalendarReply {
                source_message_id: MessageId::new(),
                attendee_email: "user@example.com".into(),
                partstat: CalendarPartstat::Accepted,
                ics_body: concat!(
                    "BEGIN:VCALENDAR\r\n",
                    "VERSION:2.0\r\n",
                    "METHOD:REPLY\r\n",
                    "BEGIN:VEVENT\r\n",
                    "UID:demo-uid\r\n",
                    "ATTENDEE;PARTSTAT=ACCEPTED:mailto:user@example.com\r\n",
                    "END:VEVENT\r\n",
                    "END:VCALENDAR\r\n"
                )
                .into(),
            }),
            created_at: now,
            updated_at: now,
        };

        let message = build_message_with_id(&draft, &from, false, &[], "<msg@example.com>")
            .expect("build_message_with_id must succeed for inline calendar reply");
        let raw = String::from_utf8(message.formatted()).unwrap();

        assert!(raw.contains("Content-Type: multipart/alternative;"));
        assert!(raw.contains("Content-Type: text/calendar; method=REPLY;"));
        assert!(raw.contains("METHOD:REPLY"));
        assert!(raw.contains("PARTSTAT=ACCEPTED"));
        // The user's typed comment must round-trip into the text/plain half.
        assert!(raw.contains("Looking forward to it."));
        // We deliberately *don't* emit a text/html alternative for invite replies.
        assert!(
            !raw.contains("Content-Type: text/html;"),
            "invite-reply MIME should omit text/html alternative"
        );
    }
}
