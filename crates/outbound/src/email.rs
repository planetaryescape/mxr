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
            let content_type = ContentType::parse(&attachment.mime_type).unwrap_or_else(|_| {
                ContentType::parse("application/octet-stream")
                    .expect("static octet-stream content type should parse")
            });
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
            from: None,
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

    fn plain_draft(subject: &str, body: &str) -> Draft {
        Draft {
            id: mxr_core::id::DraftId::new(),
            account_id: mxr_core::id::AccountId::new(),
            from: None,
            reply_headers: None,
            intent: mxr_core::types::DraftIntent::New,
            to: vec![Address {
                name: Some("Alice Example".into()),
                email: "alice@example.com".into(),
            }],
            cc: vec![Address {
                name: None,
                email: "carol@example.com".into(),
            }],
            bcc: vec![Address {
                name: None,
                email: "secret@example.com".into(),
            }],
            subject: subject.into(),
            body_markdown: body.into(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn sender() -> Address {
        Address {
            name: Some("Bob Sender".into()),
            email: "bob@example.com".into(),
        }
    }

    #[test]
    fn build_message_addresses_subject_and_alternative_body() {
        let draft = plain_draft("Weekly sync", "Hello **world**");
        let message =
            build_message_with_id(&draft, &sender(), false, &[], "<m1@example.com>").unwrap();
        let raw = String::from_utf8(message.formatted()).unwrap();

        assert!(raw.contains("bob@example.com"));
        assert!(raw.contains("alice@example.com"));
        assert!(raw.contains("Cc:") && raw.contains("carol@example.com"));
        assert!(raw.contains("Subject: Weekly sync"));
        assert!(raw.contains("Message-ID: <m1@example.com>"));
        // markdown renders to a text/plain + text/html alternative.
        assert!(raw.contains("Content-Type: multipart/alternative"));
        assert!(raw.contains("Content-Type: text/plain"));
        assert!(raw.contains("Content-Type: text/html"));
        // The markdown emphasis survives into the HTML part.
        assert!(raw.contains("<strong>world</strong>"));
    }

    #[test]
    fn keep_bcc_controls_whether_bcc_header_is_emitted() {
        let draft = plain_draft("Bcc test", "body");
        let without =
            build_message_with_id(&draft, &sender(), false, &[], "<m@example.com>").unwrap();
        assert!(
            !String::from_utf8(without.formatted())
                .unwrap()
                .contains("secret@example.com"),
            "Bcc recipient must not leak into the formatted message by default"
        );

        let with = build_message_with_id(&draft, &sender(), true, &[], "<m@example.com>").unwrap();
        assert!(
            String::from_utf8(with.formatted())
                .unwrap()
                .contains("secret@example.com"),
            "keep_bcc=true must emit the Bcc recipient (for IMAP append / Sent copies)"
        );
    }

    #[test]
    fn attachment_emits_mixed_multipart_with_disposition_and_payload() {
        let draft = plain_draft("With attachment", "see attached");
        let attachment = crate::attachments::LoadedAttachment {
            filename: "report.pdf".into(),
            mime_type: "application/pdf".into(),
            bytes: b"%PDF-1.4 fake pdf bytes".to_vec(),
        };
        let message =
            build_message_with_id(&draft, &sender(), false, &[attachment], "<a@example.com>")
                .unwrap();
        let raw = String::from_utf8(message.formatted()).unwrap();

        assert!(raw.contains("Content-Type: multipart/mixed"));
        assert!(raw.contains("application/pdf"));
        assert!(
            raw.contains("Content-Disposition: attachment") && raw.contains("report.pdf"),
            "attachment must carry an attachment disposition with its filename"
        );
    }

    #[test]
    fn binary_attachment_is_base64_transfer_encoded() {
        // Truly binary content (non-printable bytes) must be base64
        // transfer-encoded so it survives an 8-bit-unclean transport.
        let draft = plain_draft("Binary attachment", "see attached");
        let attachment = crate::attachments::LoadedAttachment {
            filename: "logo.png".into(),
            mime_type: "image/png".into(),
            bytes: vec![
                0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0xD8, 0x00, 0x01,
            ],
        };
        let message =
            build_message_with_id(&draft, &sender(), false, &[attachment], "<b@example.com>")
                .unwrap();
        let raw = String::from_utf8(message.formatted()).unwrap();
        assert!(
            raw.contains("Content-Transfer-Encoding: base64"),
            "binary attachment must be base64 transfer-encoded"
        );
    }

    #[test]
    fn non_ascii_subject_is_rfc2047_encoded() {
        let draft = plain_draft("Déjà vu — café", "body");
        let message =
            build_message_with_id(&draft, &sender(), false, &[], "<u@example.com>").unwrap();
        let raw = String::from_utf8(message.formatted()).unwrap();
        // The raw 8-bit subject must not appear unencoded; it's an
        // encoded-word per RFC 2047.
        assert!(!raw.contains("Subject: Déjà vu — café"));
        assert!(
            raw.to_lowercase().contains("=?utf-8?"),
            "non-ASCII subject must be RFC 2047 encoded; got:\n{}",
            raw.lines().take(12).collect::<Vec<_>>().join("\n")
        );
    }

    #[test]
    fn generate_message_id_uses_sender_domain_and_is_unique() {
        let from = Address {
            name: None,
            email: "bob@mail.example.org".into(),
        };
        let a = generate_message_id(&from);
        let b = generate_message_id(&from);
        assert!(a.starts_with('<') && a.ends_with('>'));
        assert!(a.contains("@mail.example.org"));
        assert_ne!(a, b, "each generated Message-ID must be unique");

        // A from-address with no domain falls back rather than producing a
        // malformed `@`-less id.
        let no_domain = generate_message_id(&Address {
            name: None,
            email: "weird".into(),
        });
        assert!(no_domain.contains('@'));
    }
}
