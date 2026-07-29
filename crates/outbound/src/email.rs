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
    build_message_with_id_and_parts(draft, from, keep_bcc, attachments, inline_assets, &message_id)
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
        let mut html_part = SinglePart::builder().header(
            ContentType::parse("text/html; charset=utf-8")
                .expect("static text/html content type should parse"),
        );

        if draft.content.is_html() {
            // Base64, not the quoted-printable lettre would pick on its own.
            // QP round-trips the content but canonicalises line endings to
            // CRLF, so an LF-only source file would not decode back to the
            // bytes the caller supplied. A designed template is the caller's
            // artifact; it comes out the far end exactly as it went in.
            // The markdown path keeps QP — mxr generated that HTML, so there
            // are no caller bytes to preserve.
            html_part = html_part.header(header::ContentTransferEncoding::Base64);
        }

        MultiPart::alternative()
            .singlepart(
                SinglePart::builder()
                    .header(
                        ContentType::parse("text/plain; charset=utf-8")
                            .expect("static text/plain content type should parse"),
                    )
                    .body(rendered.plain),
            )
            .singlepart(html_part.body(rendered.html))
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
                Attachment::new_inline(asset.cid.clone())
                    .body(asset.bytes.clone(), content_type),
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

    /// Pull the decoded body of the first part whose content type matches.
    fn decoded_part(raw: &[u8], want: &str) -> String {
        let parsed = mail_parser::MessageParser::default()
            .parse(raw)
            .expect("built message should re-parse");
        for part in parsed.parts.iter() {
            let ctype = part
                .headers
                .iter()
                .find(|h| h.name().eq_ignore_ascii_case("content-type"))
                .map(|h| format!("{:?}", h.value()).to_ascii_lowercase())
                .unwrap_or_default();
            if ctype.contains(want) {
                if let mail_parser::PartType::Text(text) | mail_parser::PartType::Html(text) =
                    &part.body
                {
                    return text.to_string();
                }
            }
        }
        String::new()
    }

    #[test]
    fn supplied_html_survives_byte_for_byte() {
        // The core promise. Base64 is used precisely so this holds:
        // quoted-printable would have rewritten the line endings.
        let draft = html_draft(BRANDED_HTML, Some("Hi Dumi"));
        let message =
            build_message_with_id(&draft, &sender(), false, &[], "<m@example.com>").unwrap();
        let raw = message.formatted();

        let decoded = decoded_part(&raw, "text/html");
        assert_eq!(
            decoded, BRANDED_HTML,
            "supplied HTML was altered on the way to the wire"
        );
    }

    #[test]
    fn tables_inline_css_media_queries_and_outlook_comments_all_survive() {
        let draft = html_draft(BRANDED_HTML, Some("t"));
        let raw =
            build_message_with_id(&draft, &sender(), false, &[], "<m@example.com>")
                .unwrap()
                .formatted();
        let html = decoded_part(&raw, "text/html");

        assert!(html.contains("<table"), "table layout lost");
        assert!(html.contains("style=\"padding:24px"), "inline CSS lost");
        assert!(html.contains("@media only screen"), "media query lost");
        assert!(html.contains("<!--[if mso]>"), "Outlook conditional comment lost");
        assert!(html.contains("<![endif]-->"), "conditional comment terminator lost");
        assert!(html.contains("role=\"presentation\""), "a11y attribute lost");
    }

    #[test]
    fn unicode_and_registered_marks_survive() {
        let html = "<p>Notto® — café, naïve, 日本語, 🎉</p>";
        let draft = html_draft(html, Some("t"));
        let raw =
            build_message_with_id(&draft, &sender(), false, &[], "<m@example.com>")
                .unwrap()
                .formatted();
        assert_eq!(decoded_part(&raw, "text/html"), html);
    }

    #[test]
    fn lf_only_html_is_not_silently_converted_to_crlf() {
        // The specific failure quoted-printable would have introduced.
        let html = "<p>one</p>\n<p>two</p>\n<p>three</p>";
        let draft = html_draft(html, Some("t"));
        let raw =
            build_message_with_id(&draft, &sender(), false, &[], "<m@example.com>")
                .unwrap()
                .formatted();
        let decoded = decoded_part(&raw, "text/html");
        assert_eq!(decoded, html);
        assert!(!decoded.contains("\r\n"), "line endings were rewritten");
    }

    #[test]
    fn html_part_is_base64_and_markdown_part_is_not() {
        let html_raw = String::from_utf8(
            build_message_with_id(
                &html_draft("<p>hi</p>", Some("hi")),
                &sender(),
                false,
                &[],
                "<m@example.com>",
            )
            .unwrap()
            .formatted(),
        )
        .unwrap();
        assert!(html_raw.contains("Content-Transfer-Encoding: base64"));

        // The markdown path is untouched by this change.
        let md_raw = String::from_utf8(
            build_message_with_id(
                &plain_draft("s", "Hello **world**"),
                &sender(),
                false,
                &[],
                "<m@example.com>",
            )
            .unwrap()
            .formatted(),
        )
        .unwrap();
        assert!(md_raw.contains("quoted-printable"));
    }

    #[test]
    fn a_supplied_text_alternative_is_used_verbatim() {
        let draft = html_draft("<p>ignored</p>", Some("The hand-written version."));
        let raw =
            build_message_with_id(&draft, &sender(), false, &[], "<m@example.com>")
                .unwrap()
                .formatted();
        assert_eq!(decoded_part(&raw, "text/plain"), "The hand-written version.");
    }

    #[test]
    fn a_missing_text_alternative_is_generated_without_touching_the_html() {
        let html = "<h1>Digest</h1><p>Hi Dumi, the report is ready.</p>";
        let draft = html_draft(html, None);
        let raw =
            build_message_with_id(&draft, &sender(), false, &[], "<m@example.com>")
                .unwrap()
                .formatted();

        let text = decoded_part(&raw, "text/plain");
        assert!(text.contains("Digest"), "generated text was empty: {text:?}");
        assert!(text.contains("Dumi"));
        assert!(!text.contains("<h1>"), "generated text still contains markup");
        // Generation is read-only with respect to the HTML.
        assert_eq!(decoded_part(&raw, "text/html"), html);
    }

    #[test]
    fn inline_images_nest_as_multipart_related_around_the_alternative() {
        // multipart/related is the discriminator: mixed leaves the logo
        // rendering as an attachment instead of resolving the cid.
        let draft = html_draft(BRANDED_HTML, Some("t"));
        let raw = String::from_utf8(
            build_message_with_id_and_parts(
                &draft,
                &sender(),
                false,
                &[],
                &[inline_logo()],
                "<m@example.com>",
            )
            .unwrap()
            .formatted(),
        )
        .unwrap();

        assert!(raw.contains("multipart/related"), "no multipart/related wrapper");
        assert!(raw.contains("multipart/alternative"), "alternative was dropped");
        assert!(raw.contains("Content-ID: <notto-logo>"), "cid header missing");
        assert!(raw.contains("Content-Disposition: inline"), "not marked inline");

        let related = raw.find("multipart/related").unwrap();
        let alternative = raw.find("multipart/alternative").unwrap();
        assert!(related < alternative, "related must wrap the alternative");
    }

    #[test]
    fn attachments_and_inline_images_nest_mixed_over_related() {
        let draft = html_draft(BRANDED_HTML, Some("t"));
        let attachment = LoadedAttachment {
            filename: "report.pdf".into(),
            mime_type: "application/pdf".into(),
            bytes: b"%PDF-1.7".to_vec(),
        };
        let raw = String::from_utf8(
            build_message_with_id_and_parts(
                &draft,
                &sender(),
                false,
                &[attachment],
                &[inline_logo()],
                "<m@example.com>",
            )
            .unwrap()
            .formatted(),
        )
        .unwrap();

        let mixed = raw.find("multipart/mixed").expect("no mixed");
        let related = raw.find("multipart/related").expect("no related");
        let alternative = raw.find("multipart/alternative").expect("no alternative");
        assert!(mixed < related && related < alternative, "wrong nesting order");
        assert!(raw.contains("filename=\"report.pdf\""));
        assert!(raw.contains("Content-ID: <notto-logo>"));
    }

    #[test]
    fn without_inline_assets_no_related_level_is_added() {
        // Unused levels collapse — no gratuitous nesting.
        let raw = String::from_utf8(
            build_message_with_id(
                &html_draft("<p>hi</p>", Some("hi")),
                &sender(),
                false,
                &[],
                "<m@example.com>",
            )
            .unwrap()
            .formatted(),
        )
        .unwrap();
        assert!(!raw.contains("multipart/related"));
        assert!(raw.contains("multipart/alternative"));
    }

    #[test]
    fn an_html_draft_still_carries_headers_and_recipients() {
        let draft = html_draft("<p>hi</p>", Some("hi"));
        let raw = String::from_utf8(
            build_message_with_id(&draft, &sender(), false, &[], "<m1@example.com>")
                .unwrap()
                .formatted(),
        )
        .unwrap();
        assert!(raw.contains("Subject: Product Digest"));
        assert!(raw.contains("alice@example.com"));
        assert!(raw.contains("Message-ID: <m1@example.com>"));
    }
}
