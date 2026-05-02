#![cfg_attr(test, allow(clippy::unwrap_used))]

use crate::types::{GmailHeader, GmailMessage, GmailPayload};
use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use chrono::{TimeZone, Utc};
use mxr_core::{
    AccountId, Address, AttachmentDisposition, AttachmentId, AttachmentMeta, BodyPartSource,
    Envelope, MessageBody, MessageFlags, MessageId, TextPlainFormat, ThreadId, UnsubscribeMethod,
};
use mxr_mail_parse::{
    body_unsubscribe_from_html, calendar_metadata_from_text,
    parse_address_list as parse_rfc_address_list, parse_headers_from_pairs,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Missing required header: {0}")]
    MissingHeader(String),

    #[error("Invalid date: {0}")]
    InvalidDate(String),

    #[error("Decode error: {0}")]
    Decode(String),

    #[error("Invalid headers: {0}")]
    Headers(String),
}

pub fn gmail_message_to_envelope(
    msg: &GmailMessage,
    account_id: &AccountId,
) -> Result<Envelope, ParseError> {
    let headers = msg
        .payload
        .as_ref()
        .and_then(|p| p.headers.as_ref())
        .map(|h| h.as_slice())
        .unwrap_or(&[]);

    let internal_date = parse_internal_date(msg.internal_date.as_deref())?;
    let header_pairs: Vec<(String, String)> = headers
        .iter()
        .map(|header| (header.name.clone(), header.value.clone()))
        .collect();
    let parsed_headers = parse_headers_from_pairs(&header_pairs, internal_date)
        .map_err(|err| ParseError::Headers(err.to_string()))?;
    let body_data = extract_body_data(msg, account_id);

    let label_ids = msg.label_ids.as_deref().unwrap_or(&[]);
    let flags = labels_to_flags(label_ids);
    let has_attachments = check_has_attachments(msg.payload.as_ref());
    let unsubscribe = match parsed_headers.unsubscribe {
        UnsubscribeMethod::None => body_data
            .text_html
            .as_deref()
            .and_then(body_unsubscribe_from_html)
            .unwrap_or(UnsubscribeMethod::None),
        unsubscribe => unsubscribe,
    };

    Ok(Envelope {
        id: MessageId::from_scoped_provider_id(account_id, "gmail", &msg.id),
        account_id: account_id.clone(),
        provider_id: msg.id.clone(),
        thread_id: ThreadId::from_scoped_provider_id(account_id, "gmail", &msg.thread_id),
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
        // Gmail's internalDate is the canonical received timestamp and matches
        // Gmail mailbox ordering better than arbitrary sender-controlled Date headers.
        date: internal_date.unwrap_or(parsed_headers.date),
        flags,
        snippet: msg.snippet.clone().unwrap_or_default(),
        has_attachments,
        size_bytes: msg.size_estimate.unwrap_or(0),
        unsubscribe,
        label_provider_ids: msg.label_ids.clone().unwrap_or_default(),
    })
}

fn parse_internal_date(
    internal_date: Option<&str>,
) -> Result<Option<chrono::DateTime<Utc>>, ParseError> {
    let Some(internal_date) = internal_date else {
        return Ok(None);
    };

    let millis: i64 = internal_date
        .parse()
        .map_err(|_| ParseError::InvalidDate(internal_date.to_string()))?;
    Ok(Some(
        Utc.timestamp_millis_opt(millis)
            .single()
            .unwrap_or_else(Utc::now),
    ))
}

pub fn labels_to_flags(label_ids: &[String]) -> MessageFlags {
    let mut flags = MessageFlags::empty();

    // Gmail: absence of UNREAD means the message is read
    let has_unread = label_ids.iter().any(|l| l == "UNREAD");
    if !has_unread {
        flags |= MessageFlags::READ;
    }

    for label in label_ids {
        match label.as_str() {
            "STARRED" => flags |= MessageFlags::STARRED,
            "DRAFT" => flags |= MessageFlags::DRAFT,
            "SENT" => flags |= MessageFlags::SENT,
            "TRASH" => flags |= MessageFlags::TRASH,
            "SPAM" => flags |= MessageFlags::SPAM,
            _ => {}
        }
    }

    flags
}

pub fn parse_list_unsubscribe(headers: &[GmailHeader]) -> UnsubscribeMethod {
    let header_pairs: Vec<(String, String)> = headers
        .iter()
        .map(|header| (header.name.clone(), header.value.clone()))
        .collect();
    parse_headers_from_pairs(&header_pairs, Some(Utc::now()))
        .map(|parsed| parsed.unsubscribe)
        .unwrap_or(UnsubscribeMethod::None)
}

pub fn parse_address(raw: &str) -> Address {
    parse_rfc_address_list(raw)
        .into_iter()
        .next()
        .unwrap_or(Address {
            name: None,
            email: raw.trim().to_string(),
        })
}

pub fn parse_address_list(raw: &str) -> Vec<Address> {
    parse_rfc_address_list(raw)
}

pub fn base64_decode_url(data: &str) -> Result<String, anyhow::Error> {
    let bytes = URL_SAFE_NO_PAD
        .decode(data)
        .or_else(|_| URL_SAFE.decode(data))?;
    Ok(String::from_utf8(bytes)?)
}

fn check_has_attachments(payload: Option<&GmailPayload>) -> bool {
    let payload = match payload {
        Some(p) => p,
        None => return false,
    };
    let mime = payload
        .mime_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    if is_attachment_part(payload, mime, payload_disposition(payload)) {
        return true;
    }

    // Recurse into child parts
    if let Some(ref parts) = payload.parts {
        for part in parts {
            if check_has_attachments(Some(part)) {
                return true;
            }
        }
    }

    false
}

#[derive(Debug, Default)]
struct ExtractedBodyData {
    text_plain: Option<String>,
    text_html: Option<String>,
    text_plain_format: Option<TextPlainFormat>,
    attachments: Vec<AttachmentMeta>,
    calendar: Option<mxr_core::types::CalendarMetadata>,
}

/// Extract text_plain and text_html from a GmailMessage payload.
pub fn extract_body(msg: &GmailMessage) -> (Option<String>, Option<String>, Vec<AttachmentMeta>) {
    let account_id = AccountId::from_provider_id("gmail", "legacy");
    let body_data = extract_body_data(msg, &account_id);
    (
        body_data.text_plain,
        body_data.text_html,
        body_data.attachments,
    )
}

fn extract_body_data(msg: &GmailMessage, account_id: &AccountId) -> ExtractedBodyData {
    let mut data = ExtractedBodyData::default();
    if let Some(ref payload) = msg.payload {
        walk_parts(payload, &msg.id, account_id, "0", &mut data);
    }
    data
}

fn walk_parts(
    payload: &GmailPayload,
    provider_msg_id: &str,
    account_id: &AccountId,
    part_path: &str,
    body_data: &mut ExtractedBodyData,
) {
    let mime = payload
        .mime_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    let disposition = payload_disposition(payload);
    let filename = payload.filename.as_ref().filter(|value| !value.is_empty());
    let provider_id = payload
        .body
        .as_ref()
        .and_then(|body| body.attachment_id.clone())
        .unwrap_or_else(|| part_path.to_string());
    let content_id =
        normalize_content_id(find_header_value(payload.headers.as_deref(), "Content-ID"));
    let content_location =
        find_header_value(payload.headers.as_deref(), "Content-Location").map(str::to_string);
    let decoded_text = payload
        .body
        .as_ref()
        .and_then(|body| body.data.as_deref())
        .and_then(|data| base64_decode_url(data).ok());

    if is_attachment_part(payload, mime, disposition) {
        body_data.attachments.push(AttachmentMeta {
            id: AttachmentId::from_scoped_provider_id(
                account_id,
                "gmail",
                &format!("{provider_msg_id}:{provider_id}"),
            ),
            message_id: MessageId::from_scoped_provider_id(account_id, "gmail", provider_msg_id),
            filename: filename
                .map(|value| (*value).clone())
                .unwrap_or_else(|| format!("attachment-{part_path}")),
            mime_type: mime.to_string(),
            disposition,
            content_id,
            content_location,
            size_bytes: payload.body.as_ref().and_then(|b| b.size).unwrap_or(0),
            local_path: None,
            provider_id,
        });
    }

    // Leaf text node
    match mime {
        "text/plain" if body_data.text_plain.is_none() => {
            if !is_attachment_part(payload, mime, disposition) {
                if let Some(decoded) = decoded_text {
                    body_data.text_plain = Some(decoded);
                    body_data.text_plain_format = parse_text_plain_format_from_payload(payload)
                        .or(Some(TextPlainFormat::Fixed));
                }
            }
        }
        "text/html" if body_data.text_html.is_none() => {
            if !is_attachment_part(payload, mime, disposition) {
                if let Some(decoded) = decoded_text {
                    body_data.text_html = Some(decoded);
                }
            }
        }
        "text/calendar" if body_data.calendar.is_none() => {
            if let Some(decoded) = decoded_text {
                body_data.calendar = calendar_metadata_from_text(&decoded);
            }
        }
        _ => {}
    }

    // Recurse into child parts
    if let Some(ref parts) = payload.parts {
        for (index, part) in parts.iter().enumerate() {
            walk_parts(
                part,
                provider_msg_id,
                account_id,
                &format!("{part_path}.{index}"),
                body_data,
            );
        }
    }
}

pub fn extract_message_body_for_account(msg: &GmailMessage, account_id: &AccountId) -> MessageBody {
    let header_pairs: Vec<(String, String)> = msg
        .payload
        .as_ref()
        .and_then(|payload| payload.headers.as_ref())
        .map(|headers| {
            headers
                .iter()
                .map(|header| (header.name.clone(), header.value.clone()))
                .collect()
        })
        .unwrap_or_default();
    let parsed_headers = parse_headers_from_pairs(&header_pairs, Some(Utc::now())).ok();
    let body_data = extract_body_data(msg, account_id);
    let mut metadata = parsed_headers
        .map(|parsed| parsed.metadata)
        .unwrap_or_default();
    metadata.calendar = body_data.calendar.clone();
    metadata.text_plain_format = body_data.text_plain_format.or(metadata.text_plain_format);
    metadata.text_plain_source = body_data.text_plain.as_ref().map(|_| BodyPartSource::Exact);
    metadata.text_html_source = body_data.text_html.as_ref().map(|_| BodyPartSource::Exact);
    MessageBody {
        message_id: MessageId::from_scoped_provider_id(account_id, "gmail", &msg.id),
        text_plain: body_data.text_plain,
        text_html: body_data.text_html,
        attachments: body_data.attachments,
        fetched_at: Utc::now(),
        metadata,
    }
}

pub fn extract_message_body(msg: &GmailMessage) -> MessageBody {
    let account_id = AccountId::from_provider_id("gmail", "legacy");
    extract_message_body_for_account(msg, &account_id)
}

fn is_attachment_part(
    payload: &GmailPayload,
    mime: &str,
    disposition: AttachmentDisposition,
) -> bool {
    if mime.starts_with("multipart/") {
        return false;
    }

    if payload
        .body
        .as_ref()
        .and_then(|body| body.attachment_id.as_ref())
        .is_some()
    {
        return true;
    }

    if payload
        .filename
        .as_ref()
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }

    matches!(disposition, AttachmentDisposition::Attachment)
        || (matches!(disposition, AttachmentDisposition::Inline)
            && mime != "text/plain"
            && mime != "text/html")
}

fn payload_disposition(payload: &GmailPayload) -> AttachmentDisposition {
    match find_header_value(payload.headers.as_deref(), "Content-Disposition") {
        Some(value)
            if value
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("attachment") =>
        {
            AttachmentDisposition::Attachment
        }
        Some(value)
            if value
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("inline") =>
        {
            AttachmentDisposition::Inline
        }
        _ => AttachmentDisposition::Unspecified,
    }
}

fn find_header_value<'a>(headers: Option<&'a [GmailHeader]>, name: &str) -> Option<&'a str> {
    headers?
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

fn parse_text_plain_format_from_payload(payload: &GmailPayload) -> Option<TextPlainFormat> {
    let content_type = find_header_value(payload.headers.as_deref(), "Content-Type")?;
    let lower = content_type.to_ascii_lowercase();
    if !lower.starts_with("text/plain") {
        return None;
    }

    let delsp = lower.contains("delsp=yes");
    if lower.contains("format=flowed") {
        Some(TextPlainFormat::Flowed { delsp })
    } else {
        Some(TextPlainFormat::Fixed)
    }
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
    use crate::types::GmailBody;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use chrono::Datelike;
    use mail_parser::MessageParser;
    use mxr_mail_parse::extract_raw_header_block;
    use mxr_test_support::{fixture_stem, standards_fixture_bytes, standards_fixture_names};
    use serde_json::json;

    fn make_headers(pairs: &[(&str, &str)]) -> Vec<GmailHeader> {
        pairs
            .iter()
            .map(|(n, v)| GmailHeader {
                name: n.to_string(),
                value: v.to_string(),
            })
            .collect()
    }

    fn make_test_message() -> GmailMessage {
        GmailMessage {
            id: "msg-001".to_string(),
            thread_id: "thread-001".to_string(),
            label_ids: Some(vec!["INBOX".to_string(), "UNREAD".to_string()]),
            snippet: Some("Hello world preview".to_string()),
            history_id: Some("12345".to_string()),
            internal_date: Some("1700000000000".to_string()),
            size_estimate: Some(2048),
            payload: Some(GmailPayload {
                mime_type: Some("text/plain".to_string()),
                headers: Some(make_headers(&[
                    ("From", "Alice <alice@example.com>"),
                    ("To", "Bob <bob@example.com>"),
                    ("Subject", "Test email"),
                    ("Message-ID", "<test123@example.com>"),
                    ("In-Reply-To", "<prev@example.com>"),
                    ("References", "<first@example.com> <prev@example.com>"),
                ])),
                body: Some(GmailBody {
                    attachment_id: None,
                    size: Some(100),
                    data: None,
                }),
                parts: None,
                filename: None,
            }),
        }
    }

    fn gmail_message_from_fixture(name: &str) -> GmailMessage {
        let raw = standards_fixture_bytes(name);
        let parsed = MessageParser::default().parse(&raw).unwrap();
        let mut headers = Vec::new();
        let mut current_name = String::new();
        let mut current_value = String::new();
        for line in extract_raw_header_block(&raw).unwrap().lines() {
            if line.starts_with(' ') || line.starts_with('\t') {
                current_value.push(' ');
                current_value.push_str(line.trim());
                continue;
            }

            if !current_name.is_empty() {
                headers.push(GmailHeader {
                    name: current_name.clone(),
                    value: current_value.trim().to_string(),
                });
            }

            if let Some((name, value)) = line.split_once(':') {
                current_name = name.to_string();
                current_value = value.trim().to_string();
            } else {
                current_name.clear();
                current_value.clear();
            }
        }
        if !current_name.is_empty() {
            headers.push(GmailHeader {
                name: current_name,
                value: current_value.trim().to_string(),
            });
        }
        let body = parsed
            .body_text(0)
            .or_else(|| parsed.body_html(0))
            .unwrap_or_default();

        GmailMessage {
            id: format!("fixture-{}", fixture_stem(name)),
            thread_id: format!("fixture-thread-{}", fixture_stem(name)),
            label_ids: Some(vec!["INBOX".to_string(), "UNREAD".to_string()]),
            snippet: Some(body.lines().next().unwrap_or_default().to_string()),
            history_id: Some("500".to_string()),
            internal_date: Some("1710495000000".to_string()),
            size_estimate: Some(raw.len() as u64),
            payload: Some(GmailPayload {
                mime_type: Some("text/plain".to_string()),
                headers: Some(headers),
                body: Some(GmailBody {
                    attachment_id: None,
                    size: Some(body.len() as u64),
                    data: Some(URL_SAFE_NO_PAD.encode(body.as_bytes())),
                }),
                parts: None,
                filename: None,
            }),
        }
    }

    #[test]
    fn parse_gmail_message_to_envelope() {
        let msg = make_test_message();
        let account_id = AccountId::from_provider_id("gmail", "test-account");
        let env = gmail_message_to_envelope(&msg, &account_id).unwrap();

        assert_eq!(env.provider_id, "msg-001");
        assert_eq!(env.from.email, "alice@example.com");
        assert_eq!(env.from.name, Some("Alice".to_string()));
        assert_eq!(env.to.len(), 1);
        assert_eq!(env.to[0].email, "bob@example.com");
        assert_eq!(env.subject, "Test email");
        assert_eq!(
            env.message_id_header,
            Some("<test123@example.com>".to_string())
        );
        assert_eq!(env.in_reply_to, Some("<prev@example.com>".to_string()));
        assert_eq!(env.references.len(), 2);
        assert_eq!(env.snippet, "Hello world preview");
        assert_eq!(env.size_bytes, 2048);
        assert_eq!(
            env.date,
            Utc.timestamp_millis_opt(1_700_000_000_000)
                .single()
                .unwrap()
        );
        // UNREAD present → not read
        assert!(!env.flags.contains(MessageFlags::READ));
        // Deterministic IDs
        assert_eq!(
            env.id,
            MessageId::from_scoped_provider_id(&account_id, "gmail", "msg-001")
        );
        assert_eq!(
            env.thread_id,
            ThreadId::from_scoped_provider_id(&account_id, "gmail", "thread-001")
        );
    }

    #[test]
    fn same_gmail_provider_id_is_distinct_across_accounts() {
        let msg = make_test_message();
        let first_account = AccountId::from_provider_id("gmail", "first@example.com");
        let second_account = AccountId::from_provider_id("gmail", "second@example.com");

        let first = gmail_message_to_envelope(&msg, &first_account).unwrap();
        let second = gmail_message_to_envelope(&msg, &second_account).unwrap();

        assert_eq!(first.provider_id, second.provider_id);
        assert_ne!(first.account_id, second.account_id);
        assert_ne!(first.id, second.id);
        assert_ne!(first.thread_id, second.thread_id);
    }

    #[test]
    fn parse_list_unsubscribe_one_click() {
        let headers = make_headers(&[
            (
                "List-Unsubscribe",
                "<https://unsub.example.com/oneclick>, <mailto:unsub@example.com>",
            ),
            ("List-Unsubscribe-Post", "List-Unsubscribe=One-Click"),
        ]);
        let result = parse_list_unsubscribe(&headers);
        assert!(matches!(
            result,
            UnsubscribeMethod::OneClick { ref url } if url == "https://unsub.example.com/oneclick"
        ));
    }

    #[test]
    fn parse_list_unsubscribe_mailto() {
        let headers = make_headers(&[("List-Unsubscribe", "<mailto:unsub@example.com>")]);
        let result = parse_list_unsubscribe(&headers);
        assert!(matches!(
            result,
            UnsubscribeMethod::Mailto { ref address, .. } if address == "unsub@example.com"
        ));
    }

    #[test]
    fn parse_list_unsubscribe_http() {
        let headers = make_headers(&[("List-Unsubscribe", "<https://unsub.example.com/link>")]);
        let result = parse_list_unsubscribe(&headers);
        assert!(matches!(
            result,
            UnsubscribeMethod::HttpLink { ref url } if url == "https://unsub.example.com/link"
        ));
    }

    #[test]
    fn parse_address_name_angle() {
        let addr = parse_address("Alice <alice@example.com>");
        assert_eq!(addr.name, Some("Alice".to_string()));
        assert_eq!(addr.email, "alice@example.com");
    }

    #[test]
    fn parse_address_bare() {
        let addr = parse_address("alice@example.com");
        assert_eq!(addr.name, None);
        assert_eq!(addr.email, "alice@example.com");
    }

    #[test]
    fn labels_to_flags_all_combinations() {
        // No UNREAD → READ
        let flags = labels_to_flags(&["INBOX".to_string()]);
        assert!(flags.contains(MessageFlags::READ));

        // UNREAD present → not READ
        let flags = labels_to_flags(&["UNREAD".to_string()]);
        assert!(!flags.contains(MessageFlags::READ));

        // All special labels
        let flags = labels_to_flags(&[
            "STARRED".to_string(),
            "DRAFT".to_string(),
            "SENT".to_string(),
            "TRASH".to_string(),
            "SPAM".to_string(),
        ]);
        assert!(flags.contains(MessageFlags::READ)); // no UNREAD
        assert!(flags.contains(MessageFlags::STARRED));
        assert!(flags.contains(MessageFlags::DRAFT));
        assert!(flags.contains(MessageFlags::SENT));
        assert!(flags.contains(MessageFlags::TRASH));
        assert!(flags.contains(MessageFlags::SPAM));
    }

    #[test]
    fn base64url_decode() {
        // "Hello, World!" in URL-safe base64 no padding
        let encoded = "SGVsbG8sIFdvcmxkIQ";
        let decoded = base64_decode_url(encoded).unwrap();
        assert_eq!(decoded, "Hello, World!");
    }

    #[test]
    fn base64url_decode_accepts_padding() {
        let encoded = "SGVsbG8sIFdvcmxkIQ==";
        let decoded = base64_decode_url(encoded).unwrap();
        assert_eq!(decoded, "Hello, World!");
    }

    #[test]
    fn parse_list_unsubscribe_multi_uri_prefers_one_click() {
        // Multiple URIs: mailto + https with one-click header
        let headers = make_headers(&[
            (
                "List-Unsubscribe",
                "<mailto:unsub@example.com>, <https://unsub.example.com/oneclick>",
            ),
            ("List-Unsubscribe-Post", "List-Unsubscribe=One-Click"),
        ]);
        let result = parse_list_unsubscribe(&headers);
        // With one-click header, prefers the HTTPS URL for OneClick
        assert!(matches!(
            result,
            UnsubscribeMethod::OneClick { ref url } if url == "https://unsub.example.com/oneclick"
        ));
    }

    #[test]
    fn parse_list_unsubscribe_missing() {
        let headers = make_headers(&[("Subject", "No unsubscribe here")]);
        let result = parse_list_unsubscribe(&headers);
        assert!(matches!(result, UnsubscribeMethod::None));
    }

    #[test]
    fn parse_address_quoted_name() {
        let addr = parse_address("\"Last, First\" <first.last@example.com>");
        assert_eq!(addr.name, Some("Last, First".to_string()));
        assert_eq!(addr.email, "first.last@example.com");
    }

    #[test]
    fn parse_address_empty_string() {
        let addr = parse_address("");
        assert!(addr.name.is_none());
        assert!(addr.email.is_empty());
    }

    #[test]
    fn parse_address_list_with_quoted_commas() {
        let addrs = parse_address_list("\"Last, First\" <a@example.com>, Bob <b@example.com>");
        assert_eq!(addrs.len(), 2);
        assert_eq!(addrs[0].name, Some("Last, First".to_string()));
        assert_eq!(addrs[0].email, "a@example.com");
        assert_eq!(addrs[1].email, "b@example.com");
    }

    #[test]
    fn parse_deeply_nested_mime() {
        // multipart/mixed containing multipart/alternative
        let msg = GmailMessage {
            id: "msg-nested".to_string(),
            thread_id: "thread-nested".to_string(),
            label_ids: None,
            snippet: None,
            history_id: None,
            internal_date: None,
            size_estimate: None,
            payload: Some(GmailPayload {
                mime_type: Some("multipart/mixed".to_string()),
                headers: None,
                body: None,
                parts: Some(vec![
                    GmailPayload {
                        mime_type: Some("multipart/alternative".to_string()),
                        headers: None,
                        body: None,
                        parts: Some(vec![
                            GmailPayload {
                                mime_type: Some("text/plain".to_string()),
                                headers: None,
                                body: Some(GmailBody {
                                    attachment_id: None,
                                    size: Some(5),
                                    data: Some("SGVsbG8".to_string()), // "Hello"
                                }),
                                parts: None,
                                filename: None,
                            },
                            GmailPayload {
                                mime_type: Some("text/html".to_string()),
                                headers: None,
                                body: Some(GmailBody {
                                    attachment_id: None,
                                    size: Some(12),
                                    data: Some("PGI-SGVsbG88L2I-".to_string()),
                                }),
                                parts: None,
                                filename: None,
                            },
                        ]),
                        filename: None,
                    },
                    GmailPayload {
                        mime_type: Some("application/pdf".to_string()),
                        headers: None,
                        body: Some(GmailBody {
                            attachment_id: Some("att-001".to_string()),
                            size: Some(50000),
                            data: None,
                        }),
                        parts: None,
                        filename: Some("report.pdf".to_string()),
                    },
                ]),
                filename: None,
            }),
        };

        let (text_plain, text_html, attachments) = extract_body(&msg);
        assert_eq!(text_plain, Some("Hello".to_string()));
        assert!(text_html.is_some());
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].filename, "report.pdf");
        assert_eq!(attachments[0].mime_type, "application/pdf");
        assert_eq!(attachments[0].size_bytes, 50000);
    }

    #[test]
    fn parse_message_with_attachments_metadata() {
        let msg = GmailMessage {
            id: "msg-att".to_string(),
            thread_id: "thread-att".to_string(),
            label_ids: Some(vec!["INBOX".to_string()]),
            snippet: Some("See attached".to_string()),
            history_id: None,
            internal_date: Some("1700000000000".to_string()),
            size_estimate: Some(100000),
            payload: Some(GmailPayload {
                mime_type: Some("multipart/mixed".to_string()),
                headers: Some(make_headers(&[
                    ("From", "alice@example.com"),
                    ("To", "bob@example.com"),
                    ("Subject", "Files attached"),
                ])),
                body: None,
                parts: Some(vec![
                    GmailPayload {
                        mime_type: Some("text/plain".to_string()),
                        headers: None,
                        body: Some(GmailBody {
                            attachment_id: None,
                            size: Some(5),
                            data: Some("SGVsbG8".to_string()),
                        }),
                        parts: None,
                        filename: None,
                    },
                    GmailPayload {
                        mime_type: Some("image/png".to_string()),
                        headers: None,
                        body: Some(GmailBody {
                            attachment_id: Some("att-img".to_string()),
                            size: Some(25000),
                            data: None,
                        }),
                        parts: None,
                        filename: Some("screenshot.png".to_string()),
                    },
                ]),
                filename: None,
            }),
        };

        let account_id = AccountId::from_provider_id("gmail", "test-account");
        let env = gmail_message_to_envelope(&msg, &account_id).unwrap();
        assert!(env.has_attachments);
        assert_eq!(env.subject, "Files attached");

        let (_, _, attachments) = extract_body(&msg);
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].filename, "screenshot.png");
        assert_eq!(attachments[0].mime_type, "image/png");
    }

    #[test]
    fn gmail_envelope_prefers_internal_date_over_header_date() {
        let mut msg = make_test_message();
        msg.internal_date = Some("1710495000000".to_string());
        msg.payload.as_mut().unwrap().headers = Some(make_headers(&[
            ("From", "Alice <alice@example.com>"),
            ("To", "Bob <bob@example.com>"),
            ("Subject", "Timestamp sanity"),
            ("Date", "Sun, 15 Jun 2025 09:08:00 +0000"),
        ]));

        let account_id = AccountId::from_provider_id("gmail", "test-account");
        let env = gmail_message_to_envelope(&msg, &account_id).unwrap();

        assert_eq!(
            env.date,
            Utc.timestamp_millis_opt(1_710_495_000_000)
                .single()
                .unwrap()
        );
    }

    #[test]
    fn gmail_envelope_falls_back_to_header_date_when_internal_date_missing() {
        let mut msg = make_test_message();
        msg.internal_date = None;
        msg.payload.as_mut().unwrap().headers = Some(make_headers(&[
            ("From", "Alice <alice@example.com>"),
            ("To", "Bob <bob@example.com>"),
            ("Subject", "Header date fallback"),
            ("Date", "Sun, 15 Jun 2025 09:08:00 +0000"),
        ]));

        let account_id = AccountId::from_provider_id("gmail", "test-account");
        let env = gmail_message_to_envelope(&msg, &account_id).unwrap();

        assert_eq!(env.date.year(), 2025);
        assert_eq!(env.date.month(), 6);
        assert_eq!(env.date.day(), 15);
    }

    #[test]
    fn body_extraction_multipart() {
        let msg = GmailMessage {
            id: "msg-mp".to_string(),
            thread_id: "thread-mp".to_string(),
            label_ids: None,
            snippet: None,
            history_id: None,
            internal_date: None,
            size_estimate: None,
            payload: Some(GmailPayload {
                mime_type: Some("multipart/alternative".to_string()),
                headers: None,
                body: None,
                parts: Some(vec![
                    GmailPayload {
                        mime_type: Some("text/plain".to_string()),
                        headers: None,
                        body: Some(GmailBody {
                            attachment_id: None,
                            size: Some(5),
                            // "Hello" in URL-safe base64 no padding
                            data: Some("SGVsbG8".to_string()),
                        }),
                        parts: None,
                        filename: None,
                    },
                    GmailPayload {
                        mime_type: Some("text/html".to_string()),
                        headers: None,
                        body: Some(GmailBody {
                            attachment_id: None,
                            size: Some(12),
                            // "<b>Hello</b>" in URL-safe base64 no padding
                            data: Some("PGI-SGVsbG88L2I-".to_string()),
                        }),
                        parts: None,
                        filename: None,
                    },
                ]),
                filename: None,
            }),
        };

        let (text_plain, text_html, _) = extract_body(&msg);
        assert_eq!(text_plain, Some("Hello".to_string()));
        assert!(text_html.is_some());
    }

    #[test]
    fn standards_fixture_like_gmail_message_snapshot() {
        let msg: GmailMessage = serde_json::from_value(json!({
            "id": "fixture-1",
            "threadId": "fixture-thread",
            "labelIds": ["INBOX", "UNREAD"],
            "snippet": "Fixture snippet",
            "historyId": "500",
            "internalDate": "1710495000000",
            "sizeEstimate": 4096,
            "payload": {
                "mimeType": "multipart/mixed",
                "headers": [
                    {"name": "From", "value": "Alice Smith <alice@example.com>"},
                    {"name": "To", "value": "Bob Example <bob@example.com>"},
                    {"name": "Subject", "value": "Planning meeting"},
                    {"name": "Date", "value": "Tue, 19 Mar 2024 14:15:00 +0000"},
                    {"name": "Message-ID", "value": "<calendar@example.com>"},
                    {"name": "Authentication-Results", "value": "mx.example.net; dkim=pass"},
                    {"name": "Content-Language", "value": "en"},
                    {"name": "List-Unsubscribe", "value": "<https://example.com/unsubscribe>"}
                ],
                "parts": [
                    {
                        "mimeType": "text/plain",
                        "body": {"size": 33, "data": "UGxlYXNlIGpvaW4gdGhlIHBsYW5uaW5nIG1lZXRpbmcu"}
                    },
                    {
                        "mimeType": "text/html",
                        "body": {"size": 76, "data": "PHA-PlBsZWFzZSBqb2luIHRoZSA8YSBocmVmPSJodHRwczovL2V4YW1wbGUuY29tL3Vuc3Vic2NyaWJlIj5tYWlsIHByZWZlcmVuY2VzPC9hPi48L3A-"}
                    },
                    {
                        "mimeType": "application/pdf",
                        "filename": "report.pdf",
                        "body": {"attachmentId": "att-1", "size": 5}
                    },
                    {
                        "mimeType": "text/calendar",
                        "body": {"size": 82, "data": "QkVHSU46VkNBTEVOREFSDQpNRVRIT0Q6UkVRVUVTVA0KQkVHSU46VkVWRU5UDQpTVU1NQVJZOlBsYW5uaW5nIG1lZXRpbmcNCkVORDpWRVZFTlQNCkVORDpWQ0FMRU5EQVI"}
                    }
                ]
            }
        }))
        .unwrap();

        let account_id = AccountId::from_provider_id("gmail", "test-account");
        let envelope = gmail_message_to_envelope(&msg, &account_id).unwrap();
        let body = extract_message_body(&msg);
        insta::assert_yaml_snapshot!(
            "gmail_fixture_message",
            json!({
                "subject": envelope.subject,
                "unsubscribe": format!("{:?}", envelope.unsubscribe),
                "flags": envelope.flags.bits(),
                "attachment_filenames": body.attachments.iter().map(|attachment| attachment.filename.clone()).collect::<Vec<_>>(),
                "calendar": body.metadata.calendar,
                "auth_results": body.metadata.auth_results,
                "content_language": body.metadata.content_language,
                "plain_text": body.text_plain,
            })
        );
    }

    #[test]
    fn standards_fixture_gmail_header_matrix_snapshots() {
        let account_id = AccountId::from_provider_id("gmail", "matrix-account");

        for fixture in standards_fixture_names() {
            let msg = gmail_message_from_fixture(fixture);
            let envelope = gmail_message_to_envelope(&msg, &account_id).unwrap();
            let body = extract_message_body(&msg);

            insta::assert_yaml_snapshot!(
                format!("gmail_fixture__{}", fixture_stem(fixture)),
                json!({
                    "subject": envelope.subject,
                    "from": envelope.from,
                    "to": envelope.to,
                    "cc": envelope.cc,
                    "message_id": envelope.message_id_header,
                    "in_reply_to": envelope.in_reply_to,
                    "references": envelope.references,
                    "unsubscribe": format!("{:?}", envelope.unsubscribe),
                    "list_id": body.metadata.list_id,
                    "auth_results": body.metadata.auth_results,
                    "content_language": body.metadata.content_language,
                    "text_plain_format": format!("{:?}", body.metadata.text_plain_format),
                    "plain_excerpt": body.text_plain.as_deref().map(|text| text.lines().take(2).collect::<Vec<_>>().join("\n")),
                })
            );
        }
    }

    #[test]
    fn plain_only_body_preserves_exact_text_and_does_not_invent_html() {
        let plain = "Hello team, \nthis paragraph is flowed and \nshould stay exact.\n";
        let msg = GmailMessage {
            id: "msg-flowed".to_string(),
            thread_id: "thread-flowed".to_string(),
            label_ids: Some(vec!["INBOX".to_string()]),
            snippet: Some("Hello team,".to_string()),
            history_id: None,
            internal_date: Some("1700000000000".to_string()),
            size_estimate: Some(plain.len() as u64),
            payload: Some(GmailPayload {
                mime_type: Some("text/plain".to_string()),
                headers: Some(make_headers(&[
                    ("From", "Alice <alice@example.com>"),
                    ("To", "Bob <bob@example.com>"),
                    ("Subject", "Flowed body"),
                    (
                        "Content-Type",
                        "text/plain; charset=UTF-8; format=flowed; delsp=yes",
                    ),
                ])),
                body: Some(GmailBody {
                    attachment_id: None,
                    size: Some(plain.len() as u64),
                    data: Some(URL_SAFE_NO_PAD.encode(plain.as_bytes())),
                }),
                parts: None,
                filename: None,
            }),
        };

        let body = extract_message_body(&msg);
        assert_eq!(body.text_plain.as_deref(), Some(plain));
        assert_eq!(body.text_html, None);
        assert_eq!(
            body.metadata.text_plain_format,
            Some(TextPlainFormat::Flowed { delsp: true })
        );
        assert_eq!(body.metadata.text_plain_source, Some(BodyPartSource::Exact));
        assert_eq!(body.metadata.text_html_source, None);
    }

    #[test]
    fn extract_message_body_preserves_inline_asset_metadata() {
        let html = r#"<p>Hello <img src="cid:logo@example.com" alt="Logo"></p>"#;
        let msg = GmailMessage {
            id: "msg-inline".to_string(),
            thread_id: "thread-inline".to_string(),
            label_ids: Some(vec!["INBOX".to_string()]),
            snippet: Some("Hello".to_string()),
            history_id: None,
            internal_date: Some("1700000000000".to_string()),
            size_estimate: Some(1024),
            payload: Some(GmailPayload {
                mime_type: Some("multipart/related".to_string()),
                headers: Some(make_headers(&[
                    ("From", "Alice <alice@example.com>"),
                    ("To", "Bob <bob@example.com>"),
                    ("Subject", "Inline image"),
                ])),
                body: None,
                parts: Some(vec![
                    GmailPayload {
                        mime_type: Some("text/html".to_string()),
                        headers: Some(make_headers(&[(
                            "Content-Type",
                            "text/html; charset=UTF-8",
                        )])),
                        body: Some(GmailBody {
                            attachment_id: None,
                            size: Some(html.len() as u64),
                            data: Some(URL_SAFE_NO_PAD.encode(html.as_bytes())),
                        }),
                        parts: None,
                        filename: None,
                    },
                    GmailPayload {
                        mime_type: Some("image/png".to_string()),
                        headers: Some(make_headers(&[
                            ("Content-Disposition", "inline; filename=\"logo.png\""),
                            ("Content-ID", "<logo@example.com>"),
                            ("Content-Location", "https://example.com/logo.png"),
                        ])),
                        body: Some(GmailBody {
                            attachment_id: Some("att-inline".to_string()),
                            size: Some(256),
                            data: None,
                        }),
                        parts: None,
                        filename: Some("logo.png".to_string()),
                    },
                ]),
                filename: None,
            }),
        };

        let body = extract_message_body(&msg);
        assert!(body
            .text_html
            .as_deref()
            .is_some_and(|value| value.contains("cid:logo@example.com")));
        assert_eq!(body.metadata.text_html_source, Some(BodyPartSource::Exact));
        assert_eq!(body.attachments.len(), 1);

        let attachment = &body.attachments[0];
        assert_eq!(attachment.filename, "logo.png");
        assert_eq!(attachment.disposition, AttachmentDisposition::Inline);
        assert_eq!(attachment.content_id.as_deref(), Some("logo@example.com"));
        assert_eq!(
            attachment.content_location.as_deref(),
            Some("https://example.com/logo.png")
        );
    }

    #[test]
    fn extract_body_handles_padded_gmail_base64_parts() {
        let plain = "Hello from padded plain text";
        let html = "<p>Hello from padded html</p>";
        let msg = GmailMessage {
            id: "msg-padded".to_string(),
            thread_id: "thread-padded".to_string(),
            label_ids: Some(vec!["INBOX".to_string()]),
            snippet: Some("Hello from padded plain text".to_string()),
            history_id: None,
            internal_date: Some("1700000000000".to_string()),
            size_estimate: Some((plain.len() + html.len()) as u64),
            payload: Some(GmailPayload {
                mime_type: Some("multipart/alternative".to_string()),
                headers: Some(make_headers(&[
                    ("From", "Alice <alice@example.com>"),
                    ("To", "Bob <bob@example.com>"),
                    ("Subject", "Padded body"),
                ])),
                body: Some(GmailBody {
                    attachment_id: None,
                    size: Some((plain.len() + html.len()) as u64),
                    data: None,
                }),
                parts: Some(vec![
                    GmailPayload {
                        mime_type: Some("text/plain".to_string()),
                        headers: Some(make_headers(&[(
                            "Content-Type",
                            "text/plain; charset=UTF-8",
                        )])),
                        body: Some(GmailBody {
                            attachment_id: None,
                            size: Some(plain.len() as u64),
                            data: Some(URL_SAFE.encode(plain.as_bytes())),
                        }),
                        parts: None,
                        filename: None,
                    },
                    GmailPayload {
                        mime_type: Some("text/html".to_string()),
                        headers: Some(make_headers(&[(
                            "Content-Type",
                            "text/html; charset=UTF-8",
                        )])),
                        body: Some(GmailBody {
                            attachment_id: None,
                            size: Some(html.len() as u64),
                            data: Some(URL_SAFE.encode(html.as_bytes())),
                        }),
                        parts: None,
                        filename: None,
                    },
                ]),
                filename: None,
            }),
        };

        let body = extract_message_body(&msg);
        assert_eq!(body.text_plain.as_deref(), Some(plain));
        assert_eq!(body.text_html.as_deref(), Some(html));
        assert_eq!(body.metadata.text_plain_source, Some(BodyPartSource::Exact));
        assert_eq!(body.metadata.text_html_source, Some(BodyPartSource::Exact));
    }
}
