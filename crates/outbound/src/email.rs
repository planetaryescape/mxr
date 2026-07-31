use crate::attachments::{
    load_attachment_paths_sync, load_inline_assets_sync, AttachmentLoadError, LoadedAttachment,
    LoadedInlineAsset,
};
use crate::render::{render_markdown, RenderedMessage};
use lettre::message::{
    header::{self, ContentType},
    Attachment, Mailbox, Message, MultiPart, SinglePart,
};
use mxr_core::types::{Address, CalendarReplyMessage, Draft, DraftContent};

/// Resolve a draft's body into the `text/plain` and `text/html` halves.
///
/// Markdown drafts render through comrak as before. HTML drafts pass their
/// document through untouched and take the caller's text alternative, falling
/// back to a generated one — generation reads the HTML and never rewrites it.
fn render_body(content: &DraftContent) -> RenderedMessage {
    match content {
        DraftContent::Markdown { source } => render_markdown(source),
        DraftContent::Html { html, text } => RenderedMessage {
            plain: text
                .clone()
                .unwrap_or_else(|| crate::html::generate_text_alternative(html)),
            html: html.clone(),
        },
    }
}

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
    let inline_assets = load_inline_assets_sync(&draft.inline_assets)?;
    build_message_with_parts(draft, from, keep_bcc, &attachments, &inline_assets)
}

pub fn build_message_with_attachments(
    draft: &Draft,
    from: &Address,
    keep_bcc: bool,
    attachments: &[LoadedAttachment],
) -> Result<Message, EmailBuildError> {
    build_message_with_parts(draft, from, keep_bcc, attachments, &[])
}

pub fn build_message_with_parts(
    draft: &Draft,
    from: &Address,
    keep_bcc: bool,
    attachments: &[LoadedAttachment],
    inline_assets: &[LoadedInlineAsset],
) -> Result<Message, EmailBuildError> {
    let message_id = generate_message_id(from);
    build_message_with_id_and_parts(
        draft,
        from,
        keep_bcc,
        attachments,
        inline_assets,
        &message_id,
    )
}

pub fn build_message_with_id(
    draft: &Draft,
    from: &Address,
    keep_bcc: bool,
    attachments: &[LoadedAttachment],
    message_id: &str,
) -> Result<Message, EmailBuildError> {
    build_message_with_id_and_parts(draft, from, keep_bcc, attachments, &[], message_id)
}

pub fn build_message_with_id_and_parts(
    draft: &Draft,
    from: &Address,
    keep_bcc: bool,
    attachments: &[LoadedAttachment],
    inline_assets: &[LoadedInlineAsset],
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

    let rendered = render_body(&draft.content);
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
        let html_part = SinglePart::builder().header(
            ContentType::parse("text/html; charset=utf-8")
                .expect("static text/html content type should parse"),
        );

        let html_part = if draft.content.is_html() {
            // A designed template is the caller's artifact; it comes out the
            // far end exactly as it went in. Two things are needed for that.
            //
            // Base64, not the quoted-printable lettre would pick on its own,
            // because QP round-trips the content but canonicalises line
            // endings to CRLF.
            //
            // And the body handed over as bytes rather than a `String`, because
            // lettre canonicalises line endings for a string body too
            // (`Body::encode_crlf`), whatever the transfer encoding. Its
            // binary path leaves them alone.
            //
            // The markdown path keeps the string and quoted-printable it
            // always had — mxr generated that HTML, so there are no caller
            // bytes to preserve.
            html_part
                .header(header::ContentTransferEncoding::Base64)
                .body(rendered.html.into_bytes())
        } else {
            html_part.body(rendered.html)
        };

        MultiPart::alternative()
            .singlepart(
                SinglePart::builder()
                    .header(
                        ContentType::parse("text/plain; charset=utf-8")
                            .expect("static text/plain content type should parse"),
                    )
                    .body(rendered.plain),
            )
            .singlepart(html_part)
    };

    // multipart/related wraps the alternative so `cid:` references resolve.
    // Gmail's own composer uses exactly this shape; multipart/mixed is the
    // common mistake and leaves inline images rendering as attachments.
    let related = if inline_assets.is_empty() {
        alternative
    } else {
        let mut related = MultiPart::related().multipart(alternative);
        for asset in inline_assets {
            let content_type = ContentType::parse(&asset.mime_type).unwrap_or_else(|_| {
                ContentType::parse("application/octet-stream")
                    .expect("static octet-stream content type should parse")
            });
            related = related.singlepart(
                Attachment::new_inline(asset.cid.clone()).body(asset.bytes.clone(), content_type),
            );
        }
        related
    };

    let body = if attachments.is_empty() {
        related
    } else {
        let mut mixed = MultiPart::mixed().multipart(related);
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
    use base64::Engine as _;
    use mail_parser::MimeHeaders as _;

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
            content: DraftContent::markdown("Looking forward to it."),
            attachments: vec![],
            inline_assets: vec![],
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
            content: DraftContent::markdown(body),
            attachments: vec![],
            inline_assets: vec![],
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
    // ---- HTML bodies -----------------------------------------------------

    /// A realistic designed email: table layout, inline CSS, a `<style>` block
    /// with a media query, an Outlook conditional comment, a CID image, and a
    /// registered mark. Every one of these is something a sanitiser or
    /// reformatter would damage.
    const BRANDED_HTML: &str = concat!(
        "<!DOCTYPE html>\n",
        "<html>\n<head>\n<meta charset=\"utf-8\">\n",
        "<style>\n  @media only screen and (max-width:600px){.c{width:100%!important}}\n</style>\n",
        "<!--[if mso]><style>.f{font-family:Arial,sans-serif}</style><![endif]-->\n",
        "</head>\n<body>\n",
        "<table class=\"c\" role=\"presentation\" cellpadding=\"0\" width=\"600\">\n",
        "<tr><td style=\"padding:24px;font-family:Georgia,serif;color:#1a1a1a\">\n",
        "<img src=\"cid:notto-logo\" alt=\"Notto\" width=\"120\">\n",
        "<p>Hi Dumi — the Notto® digest is ready.</p>\n",
        "</td></tr>\n</table>\n</body>\n</html>",
    );

    fn html_draft(html: &str, text: Option<&str>) -> Draft {
        let mut draft = plain_draft("Product Digest", "");
        draft.content = DraftContent::html(html, text.map(str::to_string));
        draft
    }

    fn inline_logo() -> LoadedInlineAsset {
        LoadedInlineAsset {
            cid: "notto-logo".to_string(),
            filename: "notto-logo.png".to_string(),
            mime_type: "image/png".to_string(),
            bytes: vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        }
    }

    fn pdf_attachment() -> LoadedAttachment {
        LoadedAttachment {
            filename: "report.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            bytes: b"%PDF-1.7".to_vec(),
        }
    }

    /// Build `draft` and hand back the bytes that would go on the wire.
    fn wire(
        draft: &Draft,
        attachments: &[LoadedAttachment],
        inline_assets: &[LoadedInlineAsset],
    ) -> Vec<u8> {
        build_message_with_id_and_parts(
            draft,
            &sender(),
            false,
            attachments,
            inline_assets,
            "<m@example.com>",
        )
        .unwrap()
        .formatted()
    }

    fn parse(raw: &[u8]) -> mail_parser::Message<'_> {
        mail_parser::MessageParser::default()
            .parse(raw)
            .expect("built message should re-parse")
    }

    /// Index of the one part with this content type, or fail loudly. Returning
    /// an index rather than a body keeps a missing part from looking like an
    /// empty one.
    fn part_index(parsed: &mail_parser::Message<'_>, ctype: &str, subtype: &str) -> usize {
        let found: Vec<usize> = parsed
            .parts
            .iter()
            .enumerate()
            .filter(|(_, part)| part.is_content_type(ctype, subtype))
            .map(|(index, _)| index)
            .collect();

        match found.as_slice() {
            [index] => *index,
            [] => panic!("no {ctype}/{subtype} part in the built message"),
            many => panic!(
                "{} {ctype}/{subtype} parts, expected exactly one",
                many.len()
            ),
        }
    }

    /// The MIME skeleton, one indented line per part.
    ///
    /// Asserting on this proves the nesting. Substring order in the raw text
    /// does not: siblings and children read the same way.
    fn mime_tree(raw: &[u8]) -> String {
        fn walk(parsed: &mail_parser::Message<'_>, index: usize, depth: usize, out: &mut String) {
            let part = &parsed.parts[index];
            let ctype = part.content_type().map_or_else(
                || "(none)".to_string(),
                |content_type| match content_type.subtype() {
                    Some(subtype) => format!("{}/{subtype}", content_type.ctype()),
                    None => content_type.ctype().to_string(),
                },
            );
            out.push_str(&"  ".repeat(depth));
            out.push_str(&ctype);
            out.push('\n');
            if let mail_parser::PartType::Multipart(children) = &part.body {
                for child in children {
                    walk(parsed, *child as usize, depth + 1, out);
                }
            }
        }

        let parsed = parse(raw);
        let mut out = String::new();
        walk(&parsed, 0, 0, &mut out);
        out
    }

    /// The `text/html` part decoded by the test itself rather than by
    /// mail-parser, so byte-exactness cannot be inherited from the parser's
    /// idea of what the content should look like. Also pins the transfer
    /// encoding, which is the mechanism the guarantee rests on.
    fn html_part_bytes(raw: &[u8]) -> Vec<u8> {
        let parsed = parse(raw);
        let index = part_index(&parsed, "text", "html");
        let part = &parsed.parts[index];
        assert_eq!(
            part.content_transfer_encoding(),
            Some("base64"),
            "text/html must be base64: quoted-printable rewrites line endings"
        );

        let body = &raw[part.offset_body as usize..part.offset_end as usize];
        let encoded: Vec<u8> = body
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("text/html part should be valid base64")
    }

    fn text_part(raw: &[u8]) -> String {
        let parsed = parse(raw);
        let index = part_index(&parsed, "text", "plain");
        match &parsed.parts[index].body {
            mail_parser::PartType::Text(text) => text.to_string(),
            other => panic!("text/plain part was not text: {other:?}"),
        }
    }

    #[test]
    fn a_designed_template_reaches_the_wire_byte_for_byte() {
        // The core promise. Everything in BRANDED_HTML is something a
        // sanitiser, prettifier or re-encoder would damage.
        let raw = wire(&html_draft(BRANDED_HTML, Some("Hi Dumi")), &[], &[]);

        assert_eq!(
            String::from_utf8(html_part_bytes(&raw)).unwrap(),
            BRANDED_HTML,
            "supplied HTML was altered on the way to the wire"
        );
    }

    #[test]
    fn a_designed_template_survives_alongside_its_inline_image() {
        // Same promise, but through the multipart/related path, which is where
        // the body gets re-wrapped.
        let raw = wire(
            &html_draft(BRANDED_HTML, Some("Hi Dumi")),
            &[pdf_attachment()],
            &[inline_logo()],
        );

        assert_eq!(
            String::from_utf8(html_part_bytes(&raw)).unwrap(),
            BRANDED_HTML
        );
    }

    #[test]
    fn lf_only_html_keeps_its_lf_endings() {
        // The specific regression base64 exists to prevent: quoted-printable
        // canonicalises every line ending to CRLF, so an LF-only source file
        // would not decode back to the bytes the caller supplied.
        let html = "<p>one</p>\n<p>two</p>\n<p>three</p>\n";

        let bytes = html_part_bytes(&wire(&html_draft(html, Some("t")), &[], &[]));

        assert_eq!(String::from_utf8(bytes.clone()).unwrap(), html);
        assert!(
            !bytes.windows(2).any(|pair| pair == b"\r\n"),
            "line endings were rewritten to CRLF"
        );
    }

    #[test]
    fn crlf_html_keeps_its_crlf_endings() {
        let html = "<p>one</p>\r\n<p>two</p>\r\n";

        let bytes = html_part_bytes(&wire(&html_draft(html, Some("t")), &[], &[]));

        assert_eq!(String::from_utf8(bytes).unwrap(), html);
    }

    #[test]
    fn a_line_far_longer_than_the_smtp_limit_comes_back_unbroken() {
        // base64 wraps the *encoded* stream, which decodes back to one line.
        // A transfer encoding that soft-wrapped the source would not.
        let html = format!("<p>{}</p>", "x".repeat(4000));

        let bytes = html_part_bytes(&wire(&html_draft(&html, Some("t")), &[], &[]));

        assert_eq!(String::from_utf8(bytes).unwrap(), html);
    }

    #[test]
    fn trailing_whitespace_and_tabs_are_not_tidied_away() {
        let html = "<p>a</p>   \n\t<p>b</p>  ";

        let bytes = html_part_bytes(&wire(&html_draft(html, Some("t")), &[], &[]));

        assert_eq!(String::from_utf8(bytes).unwrap(), html);
    }

    #[test]
    fn unicode_emoji_and_registered_marks_survive_byte_for_byte() {
        let html = "<p>Notto® — café, naïve, 日本語, 🎉, \u{200b}zero-width</p>";

        let bytes = html_part_bytes(&wire(&html_draft(html, Some("t")), &[], &[]));

        assert_eq!(bytes, html.as_bytes());
    }

    #[test]
    fn an_empty_html_body_still_builds_a_well_formed_message() {
        let raw = wire(&html_draft("", None), &[], &[]);

        assert_eq!(html_part_bytes(&raw), Vec::<u8>::new());
        assert_eq!(
            mime_tree(&raw),
            "multipart/alternative\n  text/plain\n  text/html\n"
        );
    }

    #[test]
    fn the_markdown_path_still_uses_quoted_printable() {
        // This change must not reach ordinary markdown compose: mxr generated
        // that HTML itself, so there are no caller bytes to preserve.
        let raw = wire(&plain_draft("s", "Hello **world**"), &[], &[]);

        let parsed = parse(&raw);
        let index = part_index(&parsed, "text", "html");
        assert_eq!(
            parsed.parts[index].content_transfer_encoding(),
            Some("quoted-printable")
        );
        match &parsed.parts[index].body {
            mail_parser::PartType::Html(html) => {
                assert!(
                    html.contains("<strong>world</strong>"),
                    "markdown was not rendered: {html}"
                );
            }
            other => panic!("text/html part was not html: {other:?}"),
        }
    }

    #[test]
    fn inline_assets_put_related_outside_the_alternative() {
        // multipart/related is the discriminator: mixed leaves the logo
        // rendering as an attachment instead of resolving the cid.
        let raw = wire(&html_draft(BRANDED_HTML, Some("t")), &[], &[inline_logo()]);

        assert_eq!(
            mime_tree(&raw),
            concat!(
                "multipart/related\n",
                "  multipart/alternative\n",
                "    text/plain\n",
                "    text/html\n",
                "  image/png\n",
            )
        );
    }

    #[test]
    fn attachments_and_inline_assets_nest_mixed_over_related_over_alternative() {
        let raw = wire(
            &html_draft(BRANDED_HTML, Some("t")),
            &[pdf_attachment()],
            &[inline_logo()],
        );

        assert_eq!(
            mime_tree(&raw),
            concat!(
                "multipart/mixed\n",
                "  multipart/related\n",
                "    multipart/alternative\n",
                "      text/plain\n",
                "      text/html\n",
                "    image/png\n",
                "  application/pdf\n",
            )
        );
    }

    #[test]
    fn without_inline_assets_the_related_level_collapses() {
        let raw = wire(&html_draft("<p>hi</p>", Some("hi")), &[], &[]);

        assert_eq!(
            mime_tree(&raw),
            "multipart/alternative\n  text/plain\n  text/html\n"
        );
    }

    #[test]
    fn attachments_without_inline_assets_wrap_the_alternative_in_mixed_alone() {
        let raw = wire(
            &html_draft("<p>hi</p>", Some("hi")),
            &[pdf_attachment()],
            &[],
        );

        assert_eq!(
            mime_tree(&raw),
            concat!(
                "multipart/mixed\n",
                "  multipart/alternative\n",
                "    text/plain\n",
                "    text/html\n",
                "  application/pdf\n",
            )
        );
    }

    #[test]
    fn the_inline_part_carries_its_content_id_and_inline_disposition() {
        let logo = inline_logo();
        let raw = wire(
            &html_draft(BRANDED_HTML, Some("t")),
            &[],
            std::slice::from_ref(&logo),
        );

        let parsed = parse(&raw);
        let part = &parsed.parts[part_index(&parsed, "image", "png")];
        assert_eq!(
            part.content_disposition()
                .map(mail_parser::ContentType::ctype),
            Some("inline"),
            "the image would render as an attachment, not in the layout"
        );
        match &part.body {
            mail_parser::PartType::Binary(bytes) | mail_parser::PartType::InlineBinary(bytes) => {
                assert_eq!(bytes.as_ref(), logo.bytes.as_slice());
            }
            other => panic!("inline image part was not binary: {other:?}"),
        }

        // The angle brackets are load-bearing: RFC 2392 matches `cid:x`
        // against a `Content-ID: <x>`, so assert the wire form directly.
        let text = String::from_utf8_lossy(&raw);
        assert!(
            text.contains("Content-ID: <notto-logo>"),
            "cid header missing or unbracketed"
        );
    }

    #[test]
    fn a_caller_supplied_text_alternative_is_used_verbatim() {
        // The HTML says "ignored"; if the builder generated the text part
        // instead of taking the caller's, that word would show up.
        let raw = wire(
            &html_draft("<p>ignored</p>", Some("The hand-written version.")),
            &[],
            &[],
        );

        let text = text_part(&raw);
        assert_eq!(text, "The hand-written version.");
        assert!(!text.contains("ignored"));
    }

    #[test]
    fn a_missing_text_alternative_is_generated_and_leaves_the_html_alone() {
        let html = "<h1>Digest</h1><p>Hi Dumi, the report is ready.</p>";

        let raw = wire(&html_draft(html, None), &[], &[]);

        let text = text_part(&raw);
        assert!(!text.trim().is_empty(), "generated text was empty");
        assert!(text.contains("Digest"), "{text:?}");
        assert!(text.contains("Hi Dumi, the report is ready."), "{text:?}");
        assert!(
            !text.contains('<'),
            "generated text still carries markup: {text:?}"
        );
        // Generation reads the HTML; it never rewrites it.
        assert_eq!(String::from_utf8(html_part_bytes(&raw)).unwrap(), html);
    }

    #[test]
    fn an_html_draft_still_carries_headers_and_recipients() {
        let raw = build_message_with_id(
            &html_draft("<p>hi</p>", Some("hi")),
            &sender(),
            false,
            &[],
            "<m1@example.com>",
        )
        .unwrap()
        .formatted();

        // Headers are a flat namespace, so the raw text is the honest place to
        // look for them.
        let text = String::from_utf8(raw).unwrap();
        assert!(text.contains("Subject: Product Digest"), "{text}");
        assert!(text.contains("alice@example.com"), "{text}");
        assert!(text.contains("Message-ID: <m1@example.com>"), "{text}");
    }
}
