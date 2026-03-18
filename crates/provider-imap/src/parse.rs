use chrono::{DateTime, TimeZone, Utc};
use mail_parser::MimeHeaders;
use mxr_core::id::{AccountId, AttachmentId, MessageId, ThreadId};
use mxr_core::types::{
    Address, AttachmentMeta, Envelope, MessageBody, MessageFlags, UnsubscribeMethod,
};

use crate::error::ImapProviderError;
use crate::folders::format_provider_id;
use crate::types::{FetchedMessage, ImapAddress};

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

/// Parse an IMAP date string into DateTime<Utc>.
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

/// Convert ImapAddress to mxr Address.
fn imap_addr_to_address(addr: &ImapAddress) -> Address {
    Address {
        name: addr.name.clone(),
        email: addr.email.clone(),
    }
}

/// Convert a FetchedMessage (from IMAP FETCH with ENVELOPE) into an mxr Envelope.
pub fn imap_fetch_to_envelope(
    msg: &FetchedMessage,
    mailbox: &str,
    account_id: &AccountId,
) -> Result<Envelope, ImapProviderError> {
    let envelope = msg
        .envelope
        .as_ref()
        .ok_or_else(|| ImapProviderError::Parse("Missing ENVELOPE in fetch response".into()))?;

    let date = envelope
        .date
        .as_deref()
        .and_then(|d| parse_imap_date(d).ok())
        .unwrap_or_else(Utc::now);

    let subject = envelope
        .subject
        .clone()
        .unwrap_or_else(|| "(no subject)".to_string());

    let from = envelope
        .from
        .first()
        .map(imap_addr_to_address)
        .unwrap_or_else(|| Address {
            name: None,
            email: "unknown@unknown".to_string(),
        });

    let to: Vec<Address> = envelope.to.iter().map(imap_addr_to_address).collect();
    let cc: Vec<Address> = envelope.cc.iter().map(imap_addr_to_address).collect();
    let bcc: Vec<Address> = envelope.bcc.iter().map(imap_addr_to_address).collect();

    let provider_id = format_provider_id(mailbox, msg.uid);

    // Generate snippet from header or subject
    let snippet = subject.chars().take(100).collect::<String>();

    // Detect attachments from header if available
    let has_attachments = msg
        .header
        .as_ref()
        .map(|h| {
            let header_str = String::from_utf8_lossy(h).to_lowercase();
            header_str.contains("content-disposition: attachment")
                || header_str.contains("content-type: multipart/mixed")
        })
        .unwrap_or(false);

    let flags = flags_from_imap(&msg.flags);

    // Add folder-based flags
    let mut flags = flags;
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

    // Extract references from header if available
    let references = msg
        .header
        .as_ref()
        .map(|h| extract_references_from_header(h))
        .unwrap_or_default();

    Ok(Envelope {
        id: MessageId::from_provider_id("imap", &provider_id),
        account_id: account_id.clone(),
        provider_id,
        thread_id: ThreadId::new(), // Will be recomputed by threading algorithm
        message_id_header: envelope.message_id.clone(),
        in_reply_to: envelope.in_reply_to.clone(),
        references,
        from,
        to,
        cc,
        bcc,
        subject,
        date,
        flags,
        snippet,
        has_attachments,
        size_bytes: msg.size.unwrap_or(0) as u64,
        unsubscribe: UnsubscribeMethod::None,
        label_provider_ids: vec![mailbox.to_string()],
    })
}

/// Extract References header values from raw header bytes.
fn extract_references_from_header(header: &[u8]) -> Vec<String> {
    let header_str = String::from_utf8_lossy(header);
    // Find References: header line (may be folded across multiple lines)
    let mut references = Vec::new();
    let mut in_references = false;

    for line in header_str.lines() {
        if line.to_lowercase().starts_with("references:") {
            in_references = true;
            let value = line.splitn(2, ':').nth(1).unwrap_or("").trim();
            extract_message_ids_from(value, &mut references);
        } else if in_references {
            if line.starts_with(' ') || line.starts_with('\t') {
                // Continuation line
                extract_message_ids_from(line.trim(), &mut references);
            } else {
                in_references = false;
            }
        }
    }

    references
}

/// Extract <message-id> tokens from a header value string.
fn extract_message_ids_from(value: &str, out: &mut Vec<String>) {
    let mut start = None;
    for (i, ch) in value.char_indices() {
        match ch {
            '<' => start = Some(i),
            '>' => {
                if let Some(s) = start {
                    out.push(value[s..=i].to_string());
                    start = None;
                }
            }
            _ => {}
        }
    }
}

/// Parse raw RFC822 message body into mxr MessageBody.
pub fn parse_message_body(raw: &[u8], message_id: &MessageId) -> MessageBody {
    let parsed = mail_parser::MessageParser::default().parse(raw);

    match parsed {
        Some(msg) => {
            let text_plain = msg.body_text(0).map(|t| t.to_string());
            let text_html = msg.body_html(0).map(|t| t.to_string());

            let mut attachments = Vec::new();
            for attachment in msg.attachments() {
                let idx = msg
                    .parts
                    .iter()
                    .position(|p| std::ptr::eq(p, attachment))
                    .unwrap_or(attachments.len());
                let filename = attachment
                    .attachment_name()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("attachment-{idx}"));
                let mime_type = attachment
                    .content_type()
                    .map(|ct| {
                        let subtype = ct.subtype().unwrap_or("octet-stream");
                        format!("{}/{subtype}", ct.ctype())
                    })
                    .unwrap_or_else(|| "application/octet-stream".to_string());
                let size = attachment.len();

                attachments.push(AttachmentMeta {
                    id: AttachmentId::from_provider_id("imap", &format!("{message_id}:{idx}")),
                    message_id: message_id.clone(),
                    filename,
                    mime_type,
                    size_bytes: size as u64,
                    local_path: None,
                    provider_id: format!("{idx}"),
                });
            }

            MessageBody {
                message_id: message_id.clone(),
                text_plain,
                text_html,
                attachments,
                fetched_at: Utc::now(),
            }
        }
        None => MessageBody {
            message_id: message_id.clone(),
            text_plain: None,
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ImapEnvelope;

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
    fn imap_fetch_to_envelope_basic() {
        let account_id = AccountId::new();
        let msg = FetchedMessage {
            uid: 42,
            flags: vec!["\\Seen".to_string()],
            envelope: Some(ImapEnvelope {
                date: Some("Mon, 1 Jan 2024 12:00:00 +0000".to_string()),
                subject: Some("Test email".to_string()),
                from: vec![ImapAddress {
                    name: Some("Alice".to_string()),
                    email: "alice@example.com".to_string(),
                }],
                to: vec![ImapAddress {
                    name: None,
                    email: "bob@example.com".to_string(),
                }],
                cc: vec![],
                bcc: vec![],
                message_id: Some("<msg1@example.com>".to_string()),
                in_reply_to: None,
            }),
            body: None,
            header: None,
            size: Some(2048),
        };

        let env = imap_fetch_to_envelope(&msg, "INBOX", &account_id).unwrap();
        assert_eq!(env.provider_id, "INBOX:42");
        assert_eq!(env.subject, "Test email");
        assert_eq!(env.from.email, "alice@example.com");
        assert_eq!(env.from.name, Some("Alice".to_string()));
        assert_eq!(env.to.len(), 1);
        assert_eq!(env.to[0].email, "bob@example.com");
        assert!(env.flags.contains(MessageFlags::READ));
        assert_eq!(env.size_bytes, 2048);
        assert_eq!(
            env.message_id_header,
            Some("<msg1@example.com>".to_string())
        );
        assert_eq!(env.label_provider_ids, vec!["INBOX".to_string()]);
    }

    #[test]
    fn imap_fetch_to_envelope_sent_folder_adds_sent_flag() {
        let account_id = AccountId::new();
        let msg = FetchedMessage {
            uid: 1,
            flags: vec![],
            envelope: Some(ImapEnvelope {
                date: None,
                subject: Some("Sent test".to_string()),
                from: vec![ImapAddress {
                    name: None,
                    email: "me@example.com".to_string(),
                }],
                to: vec![],
                cc: vec![],
                bcc: vec![],
                message_id: None,
                in_reply_to: None,
            }),
            body: None,
            header: None,
            size: None,
        };

        let env = imap_fetch_to_envelope(&msg, "Sent", &account_id).unwrap();
        assert!(env.flags.contains(MessageFlags::SENT));
    }

    #[test]
    fn imap_fetch_to_envelope_missing_envelope_errors() {
        let account_id = AccountId::new();
        let msg = FetchedMessage {
            uid: 1,
            flags: vec![],
            envelope: None,
            body: None,
            header: None,
            size: None,
        };

        assert!(imap_fetch_to_envelope(&msg, "INBOX", &account_id).is_err());
    }

    #[test]
    fn imap_fetch_to_envelope_with_references_header() {
        let account_id = AccountId::new();
        let header = b"From: alice@example.com\r\nReferences: <ref1@example.com> <ref2@example.com>\r\nSubject: Re: Test\r\n";
        let msg = FetchedMessage {
            uid: 5,
            flags: vec![],
            envelope: Some(ImapEnvelope {
                date: Some("Mon, 1 Jan 2024 12:00:00 +0000".to_string()),
                subject: Some("Re: Test".to_string()),
                from: vec![ImapAddress {
                    name: None,
                    email: "alice@example.com".to_string(),
                }],
                to: vec![],
                cc: vec![],
                bcc: vec![],
                message_id: Some("<msg5@example.com>".to_string()),
                in_reply_to: Some("<ref2@example.com>".to_string()),
            }),
            body: None,
            header: Some(header.to_vec()),
            size: None,
        };

        let env = imap_fetch_to_envelope(&msg, "INBOX", &account_id).unwrap();
        assert_eq!(
            env.references,
            vec![
                "<ref1@example.com>".to_string(),
                "<ref2@example.com>".to_string()
            ]
        );
        assert_eq!(
            env.in_reply_to,
            Some("<ref2@example.com>".to_string())
        );
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
    fn extract_message_ids_from_value() {
        let mut ids = Vec::new();
        extract_message_ids_from(
            "<id1@example.com> <id2@example.com>",
            &mut ids,
        );
        assert_eq!(ids, vec!["<id1@example.com>", "<id2@example.com>"]);
    }

    use chrono::Datelike;
}
