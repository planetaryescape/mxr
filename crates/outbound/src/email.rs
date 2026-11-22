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

#[derive(Clone, Debug)]
struct ContentLocation(String);

impl header::Header for ContentLocation {
    fn name() -> header::HeaderName {
        header::HeaderName::new_from_ascii_str("Content-Location")
    }

    fn parse(value: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self(value.to_string()))
    }

    fn display(&self) -> header::HeaderValue {
        header::HeaderValue::new(Self::name(), self.0.clone())
    }
}

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
    let forwarded = if draft.intent == mxr_core::DraftIntent::Forward {
        draft.managed_attachments.as_deref().unwrap_or_default()
    } else {
        &[]
    };
    let managed: Vec<_> = forwarded
        .iter()
        .filter(|part| part.content_id.is_none())
        .collect();
    let managed_inline: Vec<_> = forwarded
        .iter()
        .filter(|part| part.content_id.is_some())
        .collect();
    let managed_start = attachments
        .len()
        .checked_sub(managed.len())
        .ok_or_else(|| {
            EmailBuildError::Message("Managed forward attachments were not resolved".into())
        })?;
    let inline_start = inline_assets
        .len()
        .checked_sub(managed_inline.len())
        .ok_or_else(|| {
            EmailBuildError::Message("Managed forward inline assets were not resolved".into())
        })?;
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
            // Base64, not the quoted-printable lettre would pick on its own,
            // because QP round-trips the content but canonicalises line
            // endings to CRLF.
            //
            // And the body handed over as bytes rather than a `String`, because
            // lettre canonicalises line endings for a string body too
            // (`Body::encode_crlf`), whatever the transfer encoding. Its
            // binary path leaves them alone.
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
    let related = if inline_assets.is_empty() {
        alternative
    } else {
        let mut related = MultiPart::related().multipart(alternative);
        for (index, asset) in inline_assets.iter().enumerate() {
            let metadata = index
                .checked_sub(inline_start)
                .and_then(|i| managed_inline.get(i));
            let mime_type =
                metadata.map_or(asset.mime_type.as_str(), |part| part.mime_type.as_str());
            let filename = metadata.map_or(asset.filename.as_str(), |part| part.filename.as_str());
            let content_type = ContentType::parse(mime_type).unwrap_or_else(|_| {
                ContentType::parse("application/octet-stream")
                    .expect("static octet-stream content type should parse")
            });
            let inline = if metadata.is_some() {
                Attachment::new_inline_with_name(asset.cid.clone(), filename.to_string())
            } else {
                Attachment::new_inline(asset.cid.clone())
            };
            let part =
                if let Some(location) = metadata.and_then(|part| part.content_location.as_ref()) {
                    SinglePart::builder()
                        .header(header::ContentDisposition::inline_with_name(filename))
                        .header(header::ContentId::from(format!("<{}>", asset.cid)))
                        .header(ContentLocation(location.clone()))
                        .header(content_type)
                        .body(asset.bytes.clone())
                } else {
                    inline.body(asset.bytes.clone(), content_type)
                };
            related = related.singlepart(part);
        }
        related
    };

    let body = if attachments.is_empty() {
        related
    } else {
        let mut mixed = MultiPart::mixed().multipart(related);
        for (index, attachment) in attachments.iter().enumerate() {
            let metadata = index
                .checked_sub(managed_start)
                .and_then(|i| managed.get(i));
            let mime_type = metadata.map_or(attachment.mime_type.as_str(), |part| {
                part.mime_type.as_str()
            });
            let filename =
                metadata.map_or(attachment.filename.as_str(), |part| part.filename.as_str());
            let content_type = ContentType::parse(mime_type).unwrap_or_else(|_| {
                ContentType::parse("application/octet-stream")
                    .expect("static octet-stream content type should parse")
            });
            let part = if let Some(location) =
                metadata.and_then(|part| part.content_location.as_ref())
            {
                SinglePart::builder()
                    .header(header::ContentDisposition::attachment(filename))
                    .header(ContentLocation(location.clone()))
                    .header(content_type)
                    .body(attachment.bytes.clone())
            } else {
                Attachment::new(filename.to_string()).body(attachment.bytes.clone(), content_type)
            };
            mixed = mixed.singlepart(part);
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
            revision: Some(1),
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
            managed_attachments: None,
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
        assert!(raw.contains("Looking forward to it."));
        assert!(
            !raw.contains("Content-Type: text/html;"),
            "invite-reply MIME should omit text/html alternative"
        );
    }

    fn plain_draft(subject: &str, body: &str) -> Draft {
        Draft {
            revision: Some(1),
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
            managed_attachments: None,
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
        assert!(raw.contains("Content-Type: multipart/alternative"));
        assert!(raw.contains("Content-Type: text/plain"));
        assert!(raw.contains("Content-Type: text/html"));
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

    #[test]
    fn a_designed_template_reaches_the_wire_byte_for_byte() {
        let raw = wire(&html_draft(BRANDED_HTML, Some("Hi Dumi")), &[], &[]);

        assert_eq!(
            String::from_utf8(html_part_bytes(&raw)).unwrap(),
            BRANDED_HTML,
            "supplied HTML was altered on the way to the wire"
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
}
