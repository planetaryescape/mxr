use crate::ExportThread;

/// Export thread as RFC 4155 mbox format.
/// Each message starts with "From " line followed by RFC 2822 headers + body.
pub fn export_mbox(thread: &ExportThread) -> String {
    let mut out = String::new();

    for msg in &thread.messages {
        // Mbox "From " line: From sender@email.com Tue Mar 17 09:45:00 2026
        let mbox_date = msg.date.format("%a %b %e %H:%M:%S %Y");
        out.push_str(&format!("From {} {}\n", msg.from_email, mbox_date));

        // Headers
        if let Some(raw) = &msg.headers_raw {
            out.push_str(raw);
            if !raw.ends_with('\n') {
                out.push('\n');
            }
        } else {
            // Reconstruct minimal headers
            if let Some(name) = &msg.from_name {
                out.push_str(&format!("From: {} <{}>\n", name, msg.from_email));
            } else {
                out.push_str(&format!("From: {}\n", msg.from_email));
            }
            out.push_str(&format!("Subject: {}\n", msg.subject));
            out.push_str(&format!("Date: {}\n", msg.date.to_rfc2822()));
            if !msg.to.is_empty() {
                out.push_str(&format!("To: {}\n", msg.to.join(", ")));
            }
        }
        out.push('\n');

        // Body (escape lines starting with "From " per mbox convention)
        if let Some(text) = &msg.body_text {
            for line in text.lines() {
                if line.starts_with("From ") {
                    out.push('>');
                }
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{empty_body_thread, sample_thread};
    use crate::{ExportMessage, ExportThread};
    use chrono::TimeZone;

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
            Some("From: custom@header.com\nX-Custom: yes\n".into());
        let result = export_mbox(&thread);
        assert!(result.contains("X-Custom: yes"));
        // Should NOT contain reconstructed headers
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
        // "From" not at start of line — no escaping needed
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
        // Should still produce valid mbox with From line and headers
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
        // Should be "From: plain@example.com" not "From:  <plain@example.com>"
        assert!(result.contains("From: plain@example.com\n"));
        assert!(!result.contains("From:  <"));
    }
}
