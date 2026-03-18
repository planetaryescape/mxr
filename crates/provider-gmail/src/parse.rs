use crate::types::{GmailHeader, GmailMessage, GmailPayload};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{TimeZone, Utc};
use mxr_core::{
    AccountId, Address, AttachmentId, AttachmentMeta, Envelope, MessageBody, MessageFlags,
    MessageId, ThreadId, UnsubscribeMethod,
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

    let from_raw = find_header(headers, "From").unwrap_or_default();
    let to_raw = find_header(headers, "To").unwrap_or_default();
    let cc_raw = find_header(headers, "Cc").unwrap_or_default();
    let bcc_raw = find_header(headers, "Bcc").unwrap_or_default();
    let subject = find_header(headers, "Subject").unwrap_or_default();
    let message_id_header =
        find_header(headers, "Message-ID").or_else(|| find_header(headers, "Message-Id"));
    let in_reply_to = find_header(headers, "In-Reply-To");
    let references_raw = find_header(headers, "References").unwrap_or_default();

    let references: Vec<String> = if references_raw.is_empty() {
        vec![]
    } else {
        references_raw
            .split_whitespace()
            .map(|s| s.to_string())
            .collect()
    };

    let date = if let Some(ref internal_date) = msg.internal_date {
        let millis: i64 = internal_date
            .parse()
            .map_err(|_| ParseError::InvalidDate(internal_date.clone()))?;
        Utc.timestamp_millis_opt(millis)
            .single()
            .unwrap_or_else(Utc::now)
    } else {
        Utc::now()
    };

    let label_ids = msg.label_ids.as_deref().unwrap_or(&[]);
    let flags = labels_to_flags(label_ids);
    let unsubscribe = parse_list_unsubscribe(headers);
    let has_attachments = check_has_attachments(msg.payload.as_ref());

    Ok(Envelope {
        id: MessageId::from_provider_id("gmail", &msg.id),
        account_id: account_id.clone(),
        provider_id: msg.id.clone(),
        thread_id: ThreadId::from_provider_id("gmail", &msg.thread_id),
        message_id_header,
        in_reply_to,
        references,
        from: parse_address(&from_raw),
        to: parse_address_list(&to_raw),
        cc: parse_address_list(&cc_raw),
        bcc: parse_address_list(&bcc_raw),
        subject,
        date,
        flags,
        snippet: msg.snippet.clone().unwrap_or_default(),
        has_attachments,
        size_bytes: msg.size_estimate.unwrap_or(0),
        unsubscribe,
        label_provider_ids: msg.label_ids.clone().unwrap_or_default(),
    })
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
    let unsub_header = match find_header(headers, "List-Unsubscribe") {
        Some(v) => v,
        None => return UnsubscribeMethod::None,
    };

    let has_one_click = find_header(headers, "List-Unsubscribe-Post").is_some();

    // Extract URLs/mailto from angle brackets
    let entries: Vec<&str> = unsub_header
        .split(',')
        .map(|s| {
            s.trim()
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim()
        })
        .collect();

    // If one-click is available, find the HTTPS URL
    if has_one_click {
        for entry in &entries {
            if entry.starts_with("https://") || entry.starts_with("http://") {
                return UnsubscribeMethod::OneClick {
                    url: entry.to_string(),
                };
            }
        }
    }

    // Check for mailto
    for entry in &entries {
        if let Some(mailto) = entry.strip_prefix("mailto:") {
            let (address, subject) = if let Some(idx) = mailto.find('?') {
                let addr = &mailto[..idx];
                let query = &mailto[idx + 1..];
                let subj = query
                    .split('&')
                    .find_map(|param| param.strip_prefix("subject="))
                    .map(|s| s.to_string());
                (addr.to_string(), subj)
            } else {
                (mailto.to_string(), None)
            };
            return UnsubscribeMethod::Mailto { address, subject };
        }
    }

    // Fall back to HTTP link
    for entry in &entries {
        if entry.starts_with("https://") || entry.starts_with("http://") {
            return UnsubscribeMethod::HttpLink {
                url: entry.to_string(),
            };
        }
    }

    UnsubscribeMethod::None
}

pub fn parse_address(raw: &str) -> Address {
    let raw = raw.trim();
    if raw.is_empty() {
        return Address {
            name: None,
            email: String::new(),
        };
    }

    // "Name <email>" format
    if let Some(angle_start) = raw.rfind('<') {
        if let Some(angle_end) = raw.rfind('>') {
            let name = raw[..angle_start].trim().trim_matches('"').to_string();
            let email = raw[angle_start + 1..angle_end].trim().to_string();
            return Address {
                name: if name.is_empty() { None } else { Some(name) },
                email,
            };
        }
    }

    // Bare email
    Address {
        name: None,
        email: raw.to_string(),
    }
}

pub fn parse_address_list(raw: &str) -> Vec<Address> {
    if raw.trim().is_empty() {
        return vec![];
    }

    // Split on commas, but be careful about commas inside quoted names
    let mut addresses = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut in_angle = false;

    for ch in raw.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            '<' => {
                in_angle = true;
                current.push(ch);
            }
            '>' => {
                in_angle = false;
                current.push(ch);
            }
            ',' if !in_quotes && !in_angle => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    addresses.push(parse_address(&trimmed));
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        addresses.push(parse_address(&trimmed));
    }

    addresses
}

pub fn base64_decode_url(data: &str) -> Result<String, anyhow::Error> {
    let bytes = URL_SAFE_NO_PAD.decode(data)?;
    Ok(String::from_utf8(bytes)?)
}

fn find_header(headers: &[GmailHeader], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case(name))
        .map(|h| h.value.clone())
}

fn check_has_attachments(payload: Option<&GmailPayload>) -> bool {
    let payload = match payload {
        Some(p) => p,
        None => return false,
    };

    // If this part has a non-empty filename, it's an attachment
    if let Some(ref filename) = payload.filename {
        if !filename.is_empty() {
            return true;
        }
    }

    // If this part has an attachment_id in its body, it's an attachment
    if let Some(ref body) = payload.body {
        if body.attachment_id.is_some() {
            return true;
        }
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

/// Extract text_plain and text_html from a GmailMessage payload.
pub fn extract_body(msg: &GmailMessage) -> (Option<String>, Option<String>, Vec<AttachmentMeta>) {
    let mut text_plain = None;
    let mut text_html = None;
    let mut attachments = Vec::new();

    if let Some(ref payload) = msg.payload {
        walk_parts(
            payload,
            &msg.id,
            &mut text_plain,
            &mut text_html,
            &mut attachments,
        );
    }

    (text_plain, text_html, attachments)
}

fn walk_parts(
    payload: &GmailPayload,
    provider_msg_id: &str,
    text_plain: &mut Option<String>,
    text_html: &mut Option<String>,
    attachments: &mut Vec<AttachmentMeta>,
) {
    let mime = payload
        .mime_type
        .as_deref()
        .unwrap_or("application/octet-stream");

    // Check for attachment (has filename or attachment_id)
    let is_attachment = payload
        .filename
        .as_ref()
        .map(|f| !f.is_empty())
        .unwrap_or(false)
        || payload
            .body
            .as_ref()
            .and_then(|b| b.attachment_id.as_ref())
            .is_some();

    if is_attachment && !mime.starts_with("multipart/") {
        let filename = payload
            .filename
            .clone()
            .unwrap_or_else(|| "unnamed".to_string());
        let size = payload.body.as_ref().and_then(|b| b.size).unwrap_or(0);
        let provider_id = payload
            .body
            .as_ref()
            .and_then(|b| b.attachment_id.clone())
            .unwrap_or_default();

        attachments.push(AttachmentMeta {
            id: AttachmentId::from_provider_id(
                "gmail",
                &format!("{provider_msg_id}:{provider_id}"),
            ),
            message_id: MessageId::from_provider_id("gmail", provider_msg_id),
            filename,
            mime_type: mime.to_string(),
            size_bytes: size,
            local_path: None,
            provider_id,
        });
        return;
    }

    // Leaf text node
    match mime {
        "text/plain" if text_plain.is_none() => {
            if let Some(data) = payload.body.as_ref().and_then(|b| b.data.as_ref()) {
                if let Ok(decoded) = base64_decode_url(data) {
                    *text_plain = Some(decoded);
                }
            }
        }
        "text/html" if text_html.is_none() => {
            if let Some(data) = payload.body.as_ref().and_then(|b| b.data.as_ref()) {
                if let Ok(decoded) = base64_decode_url(data) {
                    *text_html = Some(decoded);
                }
            }
        }
        _ => {}
    }

    // Recurse into child parts
    if let Some(ref parts) = payload.parts {
        for part in parts {
            walk_parts(part, provider_msg_id, text_plain, text_html, attachments);
        }
    }
}

pub fn extract_message_body(msg: &GmailMessage) -> MessageBody {
    let (text_plain, text_html, attachments) = extract_body(msg);
    MessageBody {
        message_id: MessageId::from_provider_id("gmail", &msg.id),
        text_plain,
        text_html,
        attachments,
        fetched_at: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GmailBody;

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
        // UNREAD present → not read
        assert!(!env.flags.contains(MessageFlags::READ));
        // Deterministic IDs
        assert_eq!(env.id, MessageId::from_provider_id("gmail", "msg-001"));
        assert_eq!(
            env.thread_id,
            ThreadId::from_provider_id("gmail", "thread-001")
        );
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
}
