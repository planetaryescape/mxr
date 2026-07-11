#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

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

/// Convert IMAP flag strings into (system bitfield, custom keywords).
///
/// The canonical wire form is `\Seen`, `\Flagged`, etc., but async_imap
/// formats flags via Debug which can strip the backslash — so we accept
/// either form. Custom keywords (anything not matching a known system
/// flag name case-insensitively) are preserved verbatim per RFC 3501
/// §2.3.2: IMAP atoms are case-sensitive on the wire, and round-trip
/// integrity matters more than normalising case.
pub fn flags_and_keywords_from_imap(
    flags: &[String],
) -> (MessageFlags, std::collections::BTreeSet<String>) {
    let mut bits = MessageFlags::empty();
    let mut keywords = std::collections::BTreeSet::new();
    for flag in flags {
        let bare = flag.strip_prefix('\\').unwrap_or(flag);
        let matched = match bare.to_lowercase().as_str() {
            "seen" => {
                bits |= MessageFlags::READ;
                true
            }
            "flagged" => {
                bits |= MessageFlags::STARRED;
                true
            }
            "draft" => {
                bits |= MessageFlags::DRAFT;
                true
            }
            "deleted" => {
                bits |= MessageFlags::TRASH;
                true
            }
            "answered" => {
                bits |= MessageFlags::ANSWERED;
                true
            }
            _ => false,
        };
        if !matched {
            keywords.insert(flag.clone());
        }
    }
    (bits, keywords)
}

/// Convert IMAP flags into the system bitfield only (drops custom
/// keywords). Kept for callers that don't yet thread keywords through.
pub fn flags_from_imap(flags: &[String]) -> MessageFlags {
    flags_and_keywords_from_imap(flags).0
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
    // Fall back to the server's INTERNALDATE (true receive time) when the
    // message has no parseable `Date:` header, so spam/bounces/drafts get a
    // stable time instead of a non-deterministic `Utc::now()` that re-sorts
    // to the top and changes on every re-sync.
    let parsed_headers = parse_headers_from_raw(&raw_headers, msg.internal_date)
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
    let snippet = match body.text_plain.as_deref() {
        Some(text) => text.chars().take(200).collect::<String>(),
        // HTML-only mail has no plain part, so strip tags to readable text
        // rather than surfacing raw `<!DOCTYPE html>...` markup in the snippet.
        None => match body.text_html.as_deref() {
            Some(html) => {
                let stripped = strip_html_to_text(html);
                if stripped.is_empty() {
                    parsed_headers.subject.chars().take(100).collect()
                } else {
                    stripped.chars().take(200).collect::<String>()
                }
            }
            None => parsed_headers.subject.chars().take(100).collect(),
        },
    };

    let (mut flags, keywords) = flags_and_keywords_from_imap(&msg.flags);
    let mut label_provider_ids = if msg.gmail_labels.is_empty() {
        vec![mailbox.to_string()]
    } else {
        let mut labels = Vec::new();
        for label in &msg.gmail_labels {
            if let Some(provider_id) = crate::folders::normalize_gmail_label_provider_id(label) {
                if !labels.iter().any(|existing| existing == &provider_id) {
                    labels.push(provider_id);
                }
            }
        }
        labels
    };
    if label_provider_ids.is_empty() {
        label_provider_ids.push(mailbox.to_string());
    }

    if label_provider_ids
        .iter()
        .any(|label| label.eq_ignore_ascii_case("SENT"))
    {
        flags |= MessageFlags::SENT;
    }
    if label_provider_ids
        .iter()
        .any(|label| label.eq_ignore_ascii_case("TRASH"))
    {
        flags |= MessageFlags::TRASH;
    }
    if label_provider_ids
        .iter()
        .any(|label| label.eq_ignore_ascii_case("SPAM"))
    {
        flags |= MessageFlags::SPAM;
    }

    flags |= mailbox_leaf_flags(mailbox);

    // A message with no parseable From header is rare but real (drafts,
    // malformed senders). We still surface it rather than dropping it, but
    // a silent synthetic address corrupts sender grouping and reply-to-all
    // with no trail — so log it. (A first-class "unknown sender" envelope
    // field is the schema-change follow-up; see the Reply-To/threading
    // migration.)
    let from = parsed_headers.from.unwrap_or_else(|| {
        tracing::warn!(
            provider_id = %provider_id,
            "IMAP message has no parseable From header; using placeholder sender"
        );
        Address {
            name: None,
            email: "unknown@unknown".to_string(),
        }
    });

    let envelope = Envelope {
        id: message_id.clone(),
        account_id: account_id.clone(),
        provider_id,
        thread_id: ThreadId::new(),
        message_id_header: parsed_headers.message_id_header,
        in_reply_to: parsed_headers.in_reply_to,
        references: parsed_headers.references,
        from,
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
        link_count: 0,
        body_word_count: 0,
        label_provider_ids,
        keywords,
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
                    let filename = part.attachment_name().map_or_else(
                        || format!("attachment-{idx}"),
                        std::string::ToString::to_string,
                    );
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
                    // mail_parser reports `text/calendar` as `PartType::Text`. A
                    // bare calendar part (no filename/disposition) would otherwise
                    // land its raw `BEGIN:VCALENDAR...` in `text_plain`. It is
                    // already captured into `metadata.calendar` above, so skip it.
                    PartType::Text(_) if is_calendar_part(part) => {}
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

/// Derive SENT/TRASH/SPAM flags from a mailbox name by matching its LEAF
/// segment (last component after `/` or `.`) against curated folder names.
///
/// The previous `mailbox_lower.contains(...)` substring test over-flagged
/// "Unsent Messages"→SENT, "Presentations"→SENT, and "Junk Drawer"→SPAM,
/// while a curated leaf match still catches the real "Sent Items" /
/// "Deleted Items" that the exact special-use block misses.
fn mailbox_leaf_flags(mailbox: &str) -> MessageFlags {
    let lower = mailbox.to_lowercase();
    let leaf = lower
        .rsplit(['/', '.'])
        .next()
        .unwrap_or(lower.as_str())
        .trim();

    let mut flags = MessageFlags::empty();
    if matches!(leaf, "sent" | "sent items" | "sent mail" | "sent messages") {
        flags |= MessageFlags::SENT;
    }
    if matches!(
        leaf,
        "trash" | "deleted" | "deleted items" | "deleted messages" | "bin"
    ) {
        flags |= MessageFlags::TRASH;
    }
    if matches!(
        leaf,
        "spam" | "junk" | "junk e-mail" | "junk email" | "bulk mail"
    ) {
        flags |= MessageFlags::SPAM;
    }
    flags
}

/// Whether a part's declared content-type is `text/calendar`.
fn is_calendar_part(part: &mail_parser::MessagePart<'_>) -> bool {
    part.content_type().is_some_and(|content_type| {
        content_type.ctype().eq_ignore_ascii_case("text")
            && content_type
                .subtype()
                .is_some_and(|subtype| subtype.eq_ignore_ascii_case("calendar"))
    })
}

/// Minimal HTML-to-text reducer for snippet fallback on HTML-only mail.
/// Drops `<...>` tag runs, decodes a handful of common entities, and
/// collapses runs of whitespace. Deliberately tiny and self-contained —
/// this is a preview string, not a full HTML renderer.
fn strip_html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }

    let decoded = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        // Decode &amp; last so it can't manufacture another entity.
        .replace("&amp;", "&");

    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
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
        .is_some_and(|value| value.eq_ignore_ascii_case("yes"));

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
    part.content_type().map_or_else(
        || "application/octet-stream".to_string(),
        |content_type| {
            let subtype = content_type.subtype().unwrap_or("octet-stream");
            format!("{}/{}", content_type.ctype(), subtype)
        },
    )
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
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
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
                internal_date: None,
                gmail_labels: vec![],
                gmail_msg_id: None,
                gmail_thread_id: None,
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
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
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
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
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
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
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
    fn parse_imap_date_internaldate_single_digit_day() {
        // RFC 3501 INTERNALDATE space-pads single-digit days (" 1-Jul-1996");
        // session capture trims the leading space, so the trimmed form must
        // still parse for the fix-8 fallback to work on real servers.
        let dt = parse_imap_date("1-Jul-1996 02:44:25 -0700").unwrap();
        assert_eq!(dt.year(), 1996);
        assert_eq!(dt.month(), 7);
        assert_eq!(dt.day(), 1);
    }

    #[test]
    fn flags_and_keywords_split_system_from_custom() {
        // Phase E: system flags (`\Foo`) land in the bitfield; everything
        // without a `\` prefix is preserved as a custom keyword.
        let (bits, keywords) = flags_and_keywords_from_imap(&[
            "\\Seen".to_string(),
            "\\Flagged".to_string(),
            "$Forwarded".to_string(),
            "$Work".to_string(),
        ]);
        assert!(bits.contains(MessageFlags::READ));
        assert!(bits.contains(MessageFlags::STARRED));
        assert!(!bits.contains(MessageFlags::DRAFT));
        assert_eq!(keywords.len(), 2);
        assert!(keywords.contains("$Forwarded"));
        assert!(keywords.contains("$Work"));
    }

    #[test]
    fn flags_and_keywords_preserves_case_verbatim() {
        // IMAP atoms are case-sensitive; we do NOT normalise so that
        // round-trips back to the server keep the same on-the-wire form.
        let (_bits, keywords) =
            flags_and_keywords_from_imap(&["$Forwarded".to_string(), "$forwarded".to_string()]);
        assert_eq!(keywords.len(), 2, "case-distinct keywords must coexist");
        assert!(keywords.contains("$Forwarded"));
        assert!(keywords.contains("$forwarded"));
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
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
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
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
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
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
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
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
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

    #[test]
    fn absent_date_header_falls_back_to_internal_date() {
        // Regression (fix 8): a message with no parseable Date header must use
        // the server INTERNALDATE (deterministic) instead of Utc::now().
        let account_id = AccountId::new();
        let internal = Utc.with_ymd_and_hms(2021, 6, 15, 8, 30, 0).unwrap();
        let raw = b"From: spam@example.com\r\nTo: me@test.com\r\nSubject: No date\r\nContent-Type: text/plain\r\n\r\nBody";
        let msg = FetchedMessage {
            uid: 7,
            flags: vec![],
            envelope: None,
            body: Some(raw.to_vec()),
            header: None,
            size: None,
            internal_date: Some(internal),
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
        };

        let synced = imap_fetch_to_synced_message(&msg, "INBOX", &account_id).unwrap();
        assert_eq!(synced.envelope.date, internal);
    }

    #[test]
    fn html_only_message_snippet_strips_tags() {
        // Regression (fix 10): HTML-only mail must not surface raw markup in
        // the snippet.
        let account_id = AccountId::new();
        let raw = concat!(
            "From: alice@example.com\r\n",
            "To: bob@example.com\r\n",
            "Subject: HTML only\r\n",
            "Date: Mon, 1 Jan 2024 12:00:00 +0000\r\n",
            "MIME-Version: 1.0\r\n",
            "Content-Type: text/html\r\n",
            "\r\n",
            "<!DOCTYPE html><html><body><p>Hello &amp; welcome to the show</p></body></html>",
        );
        let msg = FetchedMessage {
            uid: 1,
            flags: vec![],
            envelope: None,
            body: Some(raw.as_bytes().to_vec()),
            header: None,
            size: None,
            internal_date: None,
            gmail_labels: vec![],
            gmail_msg_id: None,
            gmail_thread_id: None,
        };

        let synced = imap_fetch_to_synced_message(&msg, "INBOX", &account_id).unwrap();
        assert!(synced.body.text_plain.is_none());
        assert!(
            synced.envelope.snippet.contains("Hello & welcome"),
            "snippet should hold decoded visible text: {:?}",
            synced.envelope.snippet
        );
        assert!(
            !synced.envelope.snippet.contains('<'),
            "snippet must not contain raw tag markup: {:?}",
            synced.envelope.snippet
        );
    }

    #[test]
    fn calendar_only_message_does_not_leak_into_text_plain() {
        // Regression (fix 11): a bare text/calendar part must not become
        // text_plain; the calendar is captured into metadata separately.
        let msg_id = MessageId::new();
        let raw = concat!(
            "From: organizer@example.com\r\n",
            "To: me@test.com\r\n",
            "Subject: Invite\r\n",
            "Content-Type: text/calendar; method=REQUEST\r\n",
            "\r\n",
            "BEGIN:VCALENDAR\r\n",
            "VERSION:2.0\r\n",
            "BEGIN:VEVENT\r\n",
            "SUMMARY:Sync\r\n",
            "END:VEVENT\r\n",
            "END:VCALENDAR\r\n",
        );

        let body = parse_message_body(raw.as_bytes(), &msg_id);
        assert!(
            body.text_plain.is_none(),
            "VCALENDAR text must not leak into text_plain: {:?}",
            body.text_plain
        );
    }

    #[test]
    fn mailbox_leaf_flags_curated_match_avoids_substring_false_positives() {
        // Regression (fix 12): substring matching over-flagged these.
        assert!(!mailbox_leaf_flags("Unsent Messages").contains(MessageFlags::SENT));
        assert!(!mailbox_leaf_flags("Presentations").contains(MessageFlags::SENT));
        assert!(!mailbox_leaf_flags("Junk Drawer").contains(MessageFlags::SPAM));
        // True positives the exact special-use block misses must still work.
        assert!(mailbox_leaf_flags("Sent Items").contains(MessageFlags::SENT));
        assert!(mailbox_leaf_flags("Deleted Items").contains(MessageFlags::TRASH));
        assert!(mailbox_leaf_flags("[Gmail]/Sent Mail").contains(MessageFlags::SENT));
        assert!(mailbox_leaf_flags("INBOX.Junk").contains(MessageFlags::SPAM));
    }

    use chrono::Datelike;
}
