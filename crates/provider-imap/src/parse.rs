#![cfg_attr(test, allow(clippy::unwrap_used))]

use chrono::{DateTime, TimeZone, Utc};
use mail_parser::{MimeHeaders, PartType};
use mxr_core::id::{AccountId, AttachmentId, MessageId, ThreadId};
use mxr_core::types::{
    Address, AttachmentDisposition, AttachmentMeta, BodyPartSource, Envelope, MessageBody,
    MessageFlags, SyncedMessage, TextPlainFormat, UnsubscribeMethod,
};
use mxr_mail_parse::{
    body_unsubscribe_from_html, calendar_metadata_from_text, extract_raw_header_block,
    parse_headers_from_raw,
};

use crate::error::ImapProviderError;
use crate::folders::format_provider_id;
use crate::types::FetchedMessage;

/// Convert IMAP flags to mxr MessageFlags.
pub fn flags_from_imap(flags: &[String]) -> MessageFlags {
    let mut result = MessageFlags::empty();
    for flag in flags {
        let lower = flag.to_lowercase();
        if lower.contains("seen") {
            result |= MessageFlags::READ;
        }
        if lower.contains("flagged") {
            result |= MessageFlags::STARRED;
        }
        if lower.contains("draft") {
            result |= MessageFlags::DRAFT;
        }
        if lower.contains("deleted") {
            result |= MessageFlags::TRASH;
        }
        if lower.contains("answered") {
            result |= MessageFlags::ANSWERED;
        }
    }
    result
}

/// Parse an IMAP date string into `DateTime<Utc>`.
/// Handles common RFC 2822 formats and IMAP INTERNALDATE formats.
pub fn parse_imap_date(date_str: &str) -> Result<DateTime<Utc>, ImapProviderError> {
    // Try RFC 2822 first (most common in ENVELOPE)
    if let Ok(dt) = DateTime::parse_from_rfc2822(date_str) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try common IMAP INTERNALDATE format: "01-Jan-2024 12:00:00 +0000"
    if let Ok(dt) = DateTime::parse_from_str(date_str, "%d-%b-%Y %H:%M:%S %z") {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try without timezone
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(date_str, "%d-%b-%Y %H:%M:%S") {
        return Ok(Utc.from_utc_datetime(&naive));
    }

    Err(ImapProviderError::Parse(format!(
        "Cannot parse date: {date_str}"
    )))
}

/// Convert a FetchedMessage (from IMAP FETCH with BODY.PEEK[]) into a SyncedMessage
/// containing both envelope and body parsed from the raw RFC822 message.
pub fn imap_fetch_to_synced_message(
    msg: &FetchedMessage,
    mailbox: &str,
    account_id: &AccountId,
) -> Result<SyncedMessage, ImapProviderError> {
    let raw = msg
        .body
        .as_ref()
        .ok_or_else(|| ImapProviderError::Parse("Missing body data in FETCH response".into()))?;

    let provider_id = format_provider_id(mailbox, msg.uid);
    let message_id = MessageId::from_scoped_provider_id(account_id, "imap", &provider_id);

    let parsed = mail_parser::MessageParser::default().parse(raw);

    let parsed_msg = parsed.ok_or_else(|| {
        ImapProviderError::Parse(format!(
            "Failed to parse RFC822 message for UID {}",
            msg.uid
        ))
    })?;
    let raw_headers = extract_raw_header_block(raw)
        .ok_or_else(|| ImapProviderError::Parse("Missing RFC822 header block".into()))?;
    let parsed_headers = parse_headers_from_raw(&raw_headers, None)
        .map_err(|err| ImapProviderError::Parse(err.to_string()))?;

    let has_attachments = parsed_msg.attachments().next().is_some();
    let body = parse_message_body(raw, &message_id);
    let unsubscribe = match parsed_headers.unsubscribe {
        UnsubscribeMethod::None => body
            .text_html
            .as_deref()
            .and_then(body_unsubscribe_from_html)
            .unwrap_or(UnsubscribeMethod::None),
        unsubscribe => unsubscribe,
    };
    let snippet = body
        .text_plain
        .as_deref()
        .or(body.text_html.as_deref())
        .map(|text| text.chars().take(200).collect::<String>())
        .unwrap_or_else(|| parsed_headers.subject.chars().take(100).collect());

    let mut flags = flags_from_imap(&msg.flags);
    let mailbox_lower = mailbox.to_lowercase();
    if mailbox_lower.contains("sent") {
        flags |= MessageFlags::SENT;
    }
    if mailbox_lower.contains("trash") {
        flags |= MessageFlags::TRASH;
    }
    if mailbox_lower.contains("spam") || mailbox_lower.contains("junk") {
        flags |= MessageFlags::SPAM;
    }

    let envelope = Envelope {
        id: message_id.clone(),
        account_id: account_id.clone(),
        provider_id,
        thread_id: ThreadId::new(),
        message_id_header: parsed_headers.message_id_header,
        in_reply_to: parsed_headers.in_reply_to,
        references: parsed_headers.references,
        from: parsed_headers.from.unwrap_or_else(|| Address {
            name: None,
            email: "unknown@unknown".to_string(),
        }),
        to: parsed_headers.to,
        cc: parsed_headers.cc,
        bcc: parsed_headers.bcc,
        subject: parsed_headers.subject,
        date: parsed_headers.date,
        flags,
        snippet,
        has_attachments,
        size_bytes: msg.size.unwrap_or(0) as u64,
        unsubscribe,
        label_provider_ids: vec![mailbox.to_string()],
    };

    Ok(SyncedMessage { envelope, body })
}

/// Parse raw RFC822 message body into mxr MessageBody.
pub fn parse_message_body(raw: &[u8], message_id: &MessageId) -> MessageBody {
    let parsed = mail_parser::MessageParser::default().parse(raw);

    match parsed {
        Some(msg) => {
            let raw_headers = extract_raw_header_block(raw);
            let mut metadata = raw_headers
                .as_deref()
                .and_then(|headers| parse_headers_from_raw(headers, None).ok())
                .map(|parsed| parsed.metadata)
                .unwrap_or_default();
            let mut text_plain = None;
            let mut text_html = None;
            let mut attachments = Vec::new();
            let mut calendar = None;
            for (idx, part) in msg.parts.iter().enumerate() {
                if let Some(content_type) = part.content_type() {
                    let subtype = content_type.subtype().unwrap_or_default();
                    if content_type.ctype().eq_ignore_ascii_case("text")
                        && subtype.eq_ignore_ascii_case("calendar")
                    {
                        if let Some(text) = part.text_contents() {
                            calendar = calendar.or_else(|| calendar_metadata_from_text(text));
                        }
                    }
                }

                let disposition = attachment_disposition(part);
                if is_attachment_part(part, disposition) {
                    let filename = part
                        .attachment_name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("attachment-{idx}"));
                    attachments.push(AttachmentMeta {
                        id: AttachmentId::from_provider_id("imap", &format!("{message_id}:{idx}")),
                        message_id: message_id.clone(),
                        filename,
                        mime_type: part_mime_type(part),
                        disposition,
                        content_id: normalize_content_id(part.content_id()),
                        content_location: part.content_location().map(str::to_string),
                        size_bytes: part.len() as u64,
                        local_path: None,
                        provider_id: format!("{idx}"),
                    });
                    continue;
                }

                match &part.body {
                    PartType::Text(text) if text_plain.is_none() => {
                        text_plain = Some(text.to_string());
                        metadata.text_plain_source = Some(BodyPartSource::Exact);
                        metadata.text_plain_format = part
                            .content_type()
                            .and_then(parse_text_plain_format_from_content_type);
                    }
                    PartType::Html(html) if text_html.is_none() => {
                        text_html = Some(html.to_string());
                        metadata.text_html_source = Some(BodyPartSource::Exact);
                    }
                    _ => {}
                }
            }
            metadata.calendar = calendar;

            MessageBody {
                message_id: message_id.clone(),
                text_plain,
                text_html,
                attachments,
                fetched_at: Utc::now(),
                metadata,
            }
        }
        None => MessageBody {
            message_id: message_id.clone(),
            text_plain: None,
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: Default::default(),
        },
    }
}

fn parse_text_plain_format_from_content_type(
    content_type: &mail_parser::ContentType<'_>,
) -> Option<TextPlainFormat> {
    if !content_type.ctype().eq_ignore_ascii_case("text")
        || !content_type
            .subtype()
            .unwrap_or_default()
            .eq_ignore_ascii_case("plain")
    {
        return None;
    }

    let delsp = content_type
        .attribute("delsp")
        .map(|value| value.eq_ignore_ascii_case("yes"))
        .unwrap_or(false);

    match content_type.attribute("format") {
        Some(value) if value.eq_ignore_ascii_case("flowed") => {
            Some(TextPlainFormat::Flowed { delsp })
        }
        _ => Some(TextPlainFormat::Fixed),
    }
}

fn attachment_disposition<'a>(headers: &impl MimeHeaders<'a>) -> AttachmentDisposition {
    match headers.content_disposition() {
        Some(disposition) if disposition.is_attachment() => AttachmentDisposition::Attachment,
        Some(disposition) if disposition.is_inline() => AttachmentDisposition::Inline,
        _ => AttachmentDisposition::Unspecified,
    }
}

fn is_attachment_part(
    part: &mail_parser::MessagePart<'_>,
    disposition: AttachmentDisposition,
) -> bool {
    if part.is_multipart() || part.is_message() {
        return false;
    }

    if matches!(disposition, AttachmentDisposition::Attachment) {
        return true;
    }

    if matches!(part.body, PartType::Binary(_) | PartType::InlineBinary(_)) {
        return true;
    }

    if part.attachment_name().is_some() {
        return true;
    }

    matches!(disposition, AttachmentDisposition::Inline)
        && !part.is_content_type("text", "plain")
        && !part.is_content_type("text", "html")
}

fn part_mime_type(part: &mail_parser::MessagePart<'_>) -> String {
    part.content_type()
        .map(|content_type| {
            let subtype = content_type.subtype().unwrap_or("octet-stream");
            format!("{}/{}", content_type.ctype(), subtype)
        })
        .unwrap_or_else(|| "application/octet-stream".to_string())
}

fn normalize_content_id(content_id: Option<&str>) -> Option<String> {
    content_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .trim_start_matches('<')
                .trim_end_matches('>')
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_test_support::{fixture_stem, standards_fixture_bytes, standards_fixture_names};
    use serde_json::json;

    #[test]
    fn flags_from_imap_converts_standard_flags() {
        let flags = flags_from_imap(&[
            "\\Seen".to_string(),
            "\\Flagged".to_string(),
            "\\Draft".to_string(),
            "\\Answered".to_string(),
        ]);
        assert!(flags.contains(MessageFlags::READ));
        assert!(flags.contains(MessageFlags::STARRED));
        assert!(flags.contains(MessageFlags::DRAFT));
        assert!(flags.contains(MessageFlags::ANSWERED));
        assert!(!flags.contains(MessageFlags::TRASH));
    }

    #[test]
    fn flags_from_imap_handles_deleted() {
        let flags = flags_from_imap(&["\\Deleted".to_string()]);
        assert!(flags.contains(MessageFlags::TRASH));
    }

    #[test]
    fn flags_from_imap_empty() {
        let flags = flags_from_imap(&[]);
        assert!(flags.is_empty());
    }

    #[test]
    fn standards_fixture_multipart_calendar_snapshot() {
        let raw = standards_fixture_bytes("multipart-calendar.eml");
        let account_id = AccountId::new();
        let msg = FetchedMessage {
            uid: 42,
            flags: vec!["\\Seen".into()],
            envelope: None,
            body: Some(raw.clone()),
            header: None,
            size: Some(raw.len() as u32),
        };

        let synced = imap_fetch_to_synced_message(&msg, "INBOX", &account_id).unwrap();
        insta::assert_yaml_snapshot!(
            "imap_multipart_calendar",
            json!({
                "subject": synced.envelope.subject,
                "from": synced.envelope.from,
                "unsubscribe": format!("{:?}", synced.envelope.unsubscribe),
                "attachment_filenames": synced.body.attachments.iter().map(|attachment| attachment.filename.clone()).collect::<Vec<_>>(),
                "calendar": synced.body.metadata.calendar,
                "content_language": synced.body.metadata.content_language,
                "auth_results": synced.body.metadata.auth_results,
                "html_present": synced.body.text_html.is_some(),
                "plain_present": synced.body.text_plain.is_some(),
            })
        );
    }

    #[test]
    fn standards_fixture_imap_matrix_snapshots() {
        let account_id = AccountId::new();

        for (index, fixture) in standards_fixture_names().iter().enumerate() {
            let raw = standards_fixture_bytes(fixture);
            let msg = FetchedMessage {
                uid: index as u32 + 1,
                flags: vec!["\\Seen".into()],
                envelope: None,
                body: Some(raw.clone()),
                header: None,
                size: Some(raw.len() as u32),
            };

            let synced = imap_fetch_to_synced_message(&msg, "INBOX", &account_id).unwrap();
            insta::assert_yaml_snapshot!(
                format!("imap_fixture__{}", fixture_stem(fixture)),
                json!({
                    "subject": synced.envelope.subject,
                    "from": synced.envelope.from,
                    "to": synced.envelope.to,
                    "cc": synced.envelope.cc,
                    "message_id": synced.envelope.message_id_header,
                    "unsubscribe": format!("{:?}", synced.envelope.unsubscribe),
                    "attachment_filenames": synced.body.attachments.iter().map(|attachment| attachment.filename.clone()).collect::<Vec<_>>(),
                    "calendar": synced.body.metadata.calendar,
                    "content_language": synced.body.metadata.content_language,
                    "auth_results": synced.body.metadata.auth_results,
                    "text_plain_format": format!("{:?}", synced.body.metadata.text_plain_format),
                    "plain_excerpt": synced.body.text_plain.as_deref().map(|body| body.lines().take(3).collect::<Vec<_>>().join("\n")),
                    "html_present": synced.body.text_html.is_some(),
                })
            );
        }
    }

    #[test]
    fn alternative_html_first_preserves_exact_plain_and_html_parts() {
        let raw = standards_fixture_bytes("alternative-html-first.eml");
        let account_id = AccountId::new();
        let msg = FetchedMessage {
            uid: 1,
            flags: vec!["\\Seen".into()],
            envelope: None,
            body: Some(raw.clone()),
            header: None,
            size: Some(raw.len() as u32),
        };

        let synced = imap_fetch_to_synced_message(&msg, "INBOX", &account_id).unwrap();
        assert_eq!(
            synced.body.text_plain.as_deref().map(str::trim),
            Some("Plain alternative second.")
        );
        assert!(synced
            .body
            .text_html
            .as_deref()
            .is_some_and(|html| html.contains("<p>HTML first.</p>")));
        assert_eq!(
            synced.body.metadata.text_plain_source,
            Some(BodyPartSource::Exact)
        );
        assert_eq!(
            synced.body.metadata.text_html_source,
            Some(BodyPartSource::Exact)
        );
        assert_eq!(
            synced.body.metadata.text_plain_format,
            Some(TextPlainFormat::Fixed)
        );
    }

    #[test]
    fn folded_flowed_preserves_exact_plain_text_without_deriving_html() {
        let raw = standards_fixture_bytes("folded-flowed.eml");
        let account_id = AccountId::new();
        let msg = FetchedMessage {
            uid: 2,
            flags: vec!["\\Seen".into()],
            envelope: None,
            body: Some(raw.clone()),
            header: None,
            size: Some(raw.len() as u32),
        };

        let synced = imap_fetch_to_synced_message(&msg, "INBOX", &account_id).unwrap();
        let plain = synced.body.text_plain.as_deref().expect("plain body");
        assert!(plain.contains("Hello team, \nthis paragraph is flowed and \nshould join cleanly."));
        assert!(plain.contains("-- \nJosé"));
        assert_eq!(synced.body.text_html, None);
        assert_eq!(
            synced.body.metadata.text_plain_source,
            Some(BodyPartSource::Exact)
        );
        assert_eq!(synced.body.metadata.text_html_source, None);
        assert_eq!(
            synced.body.metadata.text_plain_format,
            Some(TextPlainFormat::Flowed { delsp: true })
        );
    }

    #[test]
    fn nested_multipart_preserves_attachment_metadata() {
        let raw = standards_fixture_bytes("nested-multipart.eml");
        let account_id = AccountId::new();
        let msg = FetchedMessage {
            uid: 3,
            flags: vec!["\\Seen".into()],
            envelope: None,
            body: Some(raw.clone()),
            header: None,
            size: Some(raw.len() as u32),
        };

        let synced = imap_fetch_to_synced_message(&msg, "INBOX", &account_id).unwrap();
        assert_eq!(
            synced.body.text_plain.as_deref().map(str::trim),
            Some("Nested plain body.")
        );
        assert!(synced
            .body
            .text_html
            .as_deref()
            .is_some_and(|html| html.contains("<strong>HTML</strong>")));
        assert_eq!(synced.body.attachments.len(), 1);
        assert_eq!(synced.body.attachments[0].filename, "nested.pdf");
        assert_eq!(
            synced.body.attachments[0].disposition,
            AttachmentDisposition::Attachment
        );
    }

    #[test]
    fn flags_from_imap_case_insensitive() {
        // async_imap formats flags as Debug which may vary
        let flags = flags_from_imap(&["Seen".to_string(), "FLAGGED".to_string()]);
        assert!(flags.contains(MessageFlags::READ));
        assert!(flags.contains(MessageFlags::STARRED));
    }

    #[test]
    fn parse_imap_date_rfc2822() {
        let dt = parse_imap_date("Mon, 1 Jan 2024 12:00:00 +0000").unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
    }

    #[test]
    fn parse_imap_date_internaldate_format() {
        let dt = parse_imap_date("15-Mar-2024 09:30:00 +0000").unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 3);
        assert_eq!(dt.day(), 15);
    }

    #[test]
    fn parse_imap_date_invalid() {
        assert!(parse_imap_date("not a date").is_err());
    }

    #[test]
    fn synced_message_basic() {
        let account_id = AccountId::new();
        let raw = b"From: Alice <alice@example.com>\r\nTo: bob@example.com\r\nSubject: Test email\r\nDate: Mon, 1 Jan 2024 12:00:00 +0000\r\nMessage-ID: <msg1@example.com>\r\nContent-Type: text/plain\r\n\r\nHello world";
        let msg = FetchedMessage {
            uid: 42,
            flags: vec!["\\Seen".to_string()],
            envelope: None,
            body: Some(raw.to_vec()),
            header: None,
            size: Some(2048),
        };

        let sm = imap_fetch_to_synced_message(&msg, "INBOX", &account_id).unwrap();
        assert_eq!(sm.envelope.provider_id, "INBOX:42");
        assert_eq!(sm.envelope.subject, "Test email");
        assert_eq!(sm.envelope.from.email, "alice@example.com");
        assert_eq!(sm.envelope.to.len(), 1);
        assert_eq!(sm.envelope.to[0].email, "bob@example.com");
        assert!(sm.envelope.flags.contains(MessageFlags::READ));
        assert_eq!(sm.envelope.size_bytes, 2048);
        assert!(sm.body.text_plain.unwrap().contains("Hello world"));
    }

    #[test]
    fn same_imap_mailbox_uid_in_different_accounts_gets_distinct_local_ids() {
        let first_account = AccountId::from_provider_id("imap", "first@example.com");
        let second_account = AccountId::from_provider_id("imap", "second@example.com");
        let raw = b"From: Alice <alice@example.com>\r\nTo: bob@example.com\r\nSubject: Shared UID\r\nContent-Type: text/plain\r\n\r\nHello world";
        let msg = FetchedMessage {
            uid: 42,
            flags: vec!["\\Seen".to_string()],
            envelope: None,
            body: Some(raw.to_vec()),
            header: None,
            size: None,
        };

        let first = imap_fetch_to_synced_message(&msg, "INBOX", &first_account).unwrap();
        let second = imap_fetch_to_synced_message(&msg, "INBOX", &second_account).unwrap();

        assert_eq!(first.envelope.provider_id, "INBOX:42");
        assert_eq!(second.envelope.provider_id, "INBOX:42");
        assert_ne!(first.envelope.id, second.envelope.id);
    }

    #[test]
    fn synced_message_sent_folder_adds_sent_flag() {
        let account_id = AccountId::new();
        let raw = b"From: me@example.com\r\nSubject: Sent test\r\nContent-Type: text/plain\r\n\r\nSent body";
        let msg = FetchedMessage {
            uid: 1,
            flags: vec![],
            envelope: None,
            body: Some(raw.to_vec()),
            header: None,
            size: None,
        };

        let sm = imap_fetch_to_synced_message(&msg, "Sent", &account_id).unwrap();
        assert!(sm.envelope.flags.contains(MessageFlags::SENT));
    }

    #[test]
    fn synced_message_missing_body_errors() {
        let account_id = AccountId::new();
        let msg = FetchedMessage {
            uid: 1,
            flags: vec![],
            envelope: None,
            body: None,
            header: None,
            size: None,
        };

        assert!(imap_fetch_to_synced_message(&msg, "INBOX", &account_id).is_err());
    }

    #[test]
    fn parse_message_body_plain_text() {
        let raw = b"From: alice@example.com\r\nTo: bob@example.com\r\nSubject: Test\r\nContent-Type: text/plain\r\n\r\nHello, this is a test email.";
        let msg_id = MessageId::new();
        let body = parse_message_body(raw, &msg_id);

        assert_eq!(body.message_id, msg_id);
        assert!(body.text_plain.is_some());
        assert!(body.text_plain.unwrap().contains("Hello, this is a test"));
        assert!(body.attachments.is_empty());
    }

    #[test]
    fn parse_message_body_multipart() {
        let raw = concat!(
            "From: alice@example.com\r\n",
            "To: bob@example.com\r\n",
            "Subject: Test\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/alternative; boundary=\"boundary1\"\r\n",
            "\r\n",
            "--boundary1\r\n",
            "Content-Type: text/plain\r\n",
            "\r\n",
            "Plain text body\r\n",
            "--boundary1\r\n",
            "Content-Type: text/html\r\n",
            "\r\n",
            "<p>HTML body</p>\r\n",
            "--boundary1--\r\n",
        );
        let msg_id = MessageId::new();
        let body = parse_message_body(raw.as_bytes(), &msg_id);

        assert!(body.text_plain.is_some());
        assert!(body.text_plain.unwrap().contains("Plain text body"));
        assert!(body.text_html.is_some());
        assert!(body.text_html.unwrap().contains("<p>HTML body</p>"));
    }

    #[test]
    fn parse_message_body_with_attachment() {
        let raw = concat!(
            "From: alice@example.com\r\n",
            "To: bob@example.com\r\n",
            "Subject: Test\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: multipart/mixed; boundary=\"boundary2\"\r\n",
            "\r\n",
            "--boundary2\r\n",
            "Content-Type: text/plain\r\n",
            "\r\n",
            "See attached.\r\n",
            "--boundary2\r\n",
            "Content-Type: application/pdf; name=\"report.pdf\"\r\n",
            "Content-Disposition: attachment; filename=\"report.pdf\"\r\n",
            "Content-Transfer-Encoding: base64\r\n",
            "\r\n",
            "SGVsbG8gV29ybGQ=\r\n",
            "--boundary2--\r\n",
        );
        let msg_id = MessageId::new();
        let body = parse_message_body(raw.as_bytes(), &msg_id);

        assert!(body.text_plain.is_some());
        assert_eq!(body.attachments.len(), 1);
        assert_eq!(body.attachments[0].filename, "report.pdf");
        assert!(body.attachments[0].mime_type.contains("pdf"));
    }

    #[test]
    fn parse_message_body_unparseable() {
        let raw = b"not a valid email";
        let msg_id = MessageId::new();
        let body = parse_message_body(raw, &msg_id);
        // Should not panic, returns empty body
        assert_eq!(body.message_id, msg_id);
    }

    use chrono::Datelike;
}
