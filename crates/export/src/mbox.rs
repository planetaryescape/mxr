use std::time::SystemTime;

use mailbox_formats::{MboxVariant, MboxWriter, RawMessage};

use crate::ExportThread;

/// Export thread as RFC 4155 mbox format (mboxrd variant).
///
/// Thin adapter over [`mailbox-formats`](https://crates.io/crates/mailbox-formats):
/// convert each `ExportMessage` into a `RawMessage` and let
/// `MboxWriter` handle From-line escaping, CRLF normalisation, and the
/// `From ` envelope line.
pub fn export_mbox(thread: &ExportThread) -> String {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut writer = MboxWriter::new(&mut buf, MboxVariant::Mboxrd);
        for msg in &thread.messages {
            let raw = export_message_to_raw(msg);
            writer
                .write_message(&raw)
                .expect("writing to Vec<u8> cannot fail");
        }
        writer.finish().expect("finish writes to Vec<u8>");
    }
    String::from_utf8(buf).expect("mbox output is ASCII-clean")
}

fn export_message_to_raw(msg: &crate::ExportMessage) -> RawMessage {
    let headers = if let Some(raw) = &msg.headers_raw {
        // Parse the caller-provided raw header block (already in
        // RFC 5322 shape) into our (name, value) pairs.
        parse_raw_headers(raw)
    } else {
        // Reconstruct minimal headers.
        reconstruct_headers(msg)
    };

    let body = msg
        .body_text
        .as_deref()
        .map(|s| {
            // Normalise to LF; the writer canonicalises to CRLF.
            s.replace("\r\n", "\n").into_bytes()
        })
        .unwrap_or_default();

    let timestamp: SystemTime = msg.date.into();

    RawMessage::new(headers, body)
        .with_envelope_from(msg.from_email.clone())
        .with_timestamp(timestamp)
}

fn parse_raw_headers(raw: &str) -> Vec<(String, Vec<u8>)> {
    let mut headers: Vec<(String, Vec<u8>)> = Vec::new();
    // Normalise line endings to LF for splitting.
    let normalised = raw.replace("\r\n", "\n");
    for line in normalised.split('\n') {
        if line.is_empty() {
            continue;
        }
        // Folded continuation.
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, value)) = headers.last_mut() {
                value.push(b' ');
                value.extend_from_slice(line.trim_start().as_bytes());
            }
            continue;
        }
        if let Some(colon) = line.find(':') {
            let name = line[..colon].to_string();
            let value = line[colon + 1..].trim_start().as_bytes().to_vec();
            headers.push((name, value));
        }
    }
    headers
}

fn reconstruct_headers(msg: &crate::ExportMessage) -> Vec<(String, Vec<u8>)> {
    let mut headers: Vec<(String, Vec<u8>)> = Vec::new();
    let from_value = match &msg.from_name {
        Some(name) => format!("{} <{}>", name, msg.from_email),
        None => msg.from_email.clone(),
    };
    headers.push(("From".to_string(), from_value.into_bytes()));
    headers.push(("Subject".to_string(), msg.subject.clone().into_bytes()));
    headers.push(("Date".to_string(), msg.date.to_rfc2822().into_bytes()));
    if !msg.to.is_empty() {
        headers.push(("To".to_string(), msg.to.join(", ").into_bytes()));
    }
    headers
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use crate::tests::{empty_body_thread, sample_thread};
    use crate::{ExportMessage, ExportThread};
    use chrono::TimeZone;
    use mxr_test_support::standards_fixture_string;

    #[test]
    fn mbox_starts_with_from_line() {
        let result = export_mbox(&sample_thread());
        assert!(result.starts_with("From alice@example.com"));
    }

    #[test]
    fn mbox_from_line_has_asctime_date() {
        let result = export_mbox(&sample_thread());
        let first_line = result.lines().next().unwrap();
        // asctime format: "Tue Mar 17 09:30:00 2026"
        assert!(first_line.contains("Tue Mar 17"));
        assert!(first_line.contains("09:30:00 2026"));
    }

    #[test]
    fn mbox_reconstructs_minimal_headers_when_no_raw() {
        let result = export_mbox(&sample_thread());
        assert!(result.contains("From: Alice <alice@example.com>"));
        assert!(result.contains("Subject: Deployment rollback plan"));
        assert!(result.contains("Date: "));
        assert!(result.contains("To: team@example.com"));
    }

    #[test]
    fn mbox_uses_raw_headers_when_available() {
        let mut thread = sample_thread();
        thread.messages[0].headers_raw =
            Some("From: custom@header.com\r\nX-Custom: yes\r\n".into());
        let result = export_mbox(&thread);
        assert!(result.contains("X-Custom: yes"));
        assert!(!result.contains("From: Alice <alice@example.com>"));
    }

    #[test]
    fn mbox_escapes_from_in_body() {
        let mut thread = sample_thread();
        thread.messages[0].body_text =
            Some("From the beginning of time\nNormal line\nFrom space comes light".into());
        let result = export_mbox(&thread);
        assert!(result.contains(">From the beginning of time"));
        assert!(result.contains("Normal line"));
        assert!(result.contains(">From space comes light"));
    }

    #[test]
    fn mbox_does_not_escape_from_mid_line() {
        let mut thread = sample_thread();
        thread.messages[0].body_text = Some("This is From the meeting".into());
        let result = export_mbox(&thread);
        assert!(result.contains("This is From the meeting"));
    }

    #[test]
    fn mbox_multiple_messages_separated_by_from_lines() {
        let result = export_mbox(&sample_thread());
        let from_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.starts_with("From ") && l.contains('@'))
            .collect();
        assert_eq!(from_lines.len(), 2);
    }

    #[test]
    fn mbox_handles_empty_body() {
        let result = export_mbox(&empty_body_thread());
        assert!(result.starts_with("From ghost@void.com"));
        assert!(result.contains("Subject: No body"));
    }

    #[test]
    fn mbox_omits_to_header_when_empty() {
        let result = export_mbox(&empty_body_thread());
        assert!(!result.contains("To: "));
    }

    #[test]
    fn mbox_from_header_omits_name_when_missing() {
        let thread = ExportThread {
            thread_id: "t".into(),
            subject: "test".into(),
            messages: vec![ExportMessage {
                id: "m".into(),
                from_name: None,
                from_email: "plain@example.com".into(),
                to: vec![],
                date: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                subject: "test".into(),
                body_text: None,
                body_html: None,
                headers_raw: None,
                attachments: vec![],
            }],
        };
        let result = export_mbox(&thread);
        assert!(result.contains("From: plain@example.com\r\n"));
        assert!(!result.contains("From:  <"));
    }

    #[test]
    fn mbox_uses_crlf_line_endings() {
        let result = export_mbox(&sample_thread());
        assert!(result.contains("\r\nSubject: Deployment rollback plan\r\n"));
        assert!(result.contains("\r\n\r\nWhat's the rollback strategy"));
        assert!(!result.contains("Subject: Deployment rollback plan\nDate:"));
    }

    #[test]
    fn standards_fixture_exports_as_mbox_snapshot() {
        let raw = standards_fixture_string("folded-flowed.eml");
        let (headers, body) = raw.split_once("\n\n").unwrap();
        let thread = ExportThread {
            thread_id: "fixture-thread".into(),
            subject: "Fixture Subject".into(),
            messages: vec![ExportMessage {
                id: "fixture-1".into(),
                from_name: Some("José Example".into()),
                from_email: "jose@example.com".into(),
                to: vec!["team@example.com".into()],
                date: chrono::DateTime::parse_from_rfc2822("Fri, 15 Mar 2024 09:30:00 +0000")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                subject: "Quarterly update".into(),
                body_text: Some(body.to_string()),
                body_html: None,
                headers_raw: Some(headers.replace('\n', "\r\n")),
                attachments: vec![],
            }],
        };

        insta::assert_snapshot!("fixture_mbox_export", export_mbox(&thread));
    }
}
