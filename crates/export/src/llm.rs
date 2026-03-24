use crate::mxr_export::ExportThread;
use crate::mxr_reader::{clean, ReaderConfig};
use std::collections::HashSet;

/// Export thread optimized for AI consumption.
/// Uses the reader pipeline to strip noise, producing a token-efficient representation.
pub fn export_llm_context(thread: &ExportThread, reader_config: &ReaderConfig) -> String {
    let mut out = String::new();

    let participants: Vec<&str> = thread
        .messages
        .iter()
        .map(|m| m.from_email.as_str())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    out.push_str(&format!("Thread: {}\n", thread.subject));
    out.push_str(&format!("Participants: {}\n", participants.join(", ")));
    out.push_str(&format!("Messages: {}\n", thread.messages.len()));

    for msg in &thread.messages {
        out.push_str("\n---\n");

        let date = msg.date.format("%b %d %H:%M");
        out.push_str(&format!("[{}, {}]\n", msg.from_email, date));

        // Run reader pipeline for maximum noise reduction
        let reader_output = clean(
            msg.body_text.as_deref(),
            msg.body_html.as_deref(),
            reader_config,
        );
        out.push_str(&reader_output.content);
        out.push('\n');

        // Attachment metadata (no binary content)
        if !msg.attachments.is_empty() {
            let att_summary: Vec<String> = msg
                .attachments
                .iter()
                .map(|a| format!("{} ({}KB)", a.filename, a.size_bytes / 1024))
                .collect();
            out.push_str(&format!("\nAttachments: {}\n", att_summary.join(", ")));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mxr_export::tests::{empty_body_thread, sample_thread, single_message_thread};
    use crate::mxr_export::{ExportAttachment, ExportMessage, ExportThread};
    use chrono::TimeZone;

    fn default_config() -> ReaderConfig {
        ReaderConfig::default()
    }

    #[test]
    fn llm_starts_with_thread_metadata() {
        let result = export_llm_context(&sample_thread(), &default_config());
        assert!(result.starts_with("Thread: Deployment rollback plan\n"));
        assert!(result.contains("Participants: "));
        assert!(result.contains("Messages: 2\n"));
    }

    #[test]
    fn llm_uses_compact_date_format() {
        let result = export_llm_context(&sample_thread(), &default_config());
        // Compact dates like "Mar 17 09:30", not full RFC2822
        assert!(result.contains("Mar 17 09:30"));
    }

    #[test]
    fn llm_uses_email_not_name() {
        let result = export_llm_context(&sample_thread(), &default_config());
        // LLM context uses email (more precise) not display name
        assert!(result.contains("[alice@example.com,"));
        assert!(result.contains("[bob@example.com,"));
    }

    #[test]
    fn llm_does_not_contain_full_headers() {
        let result = export_llm_context(&sample_thread(), &default_config());
        // No Subject: header or Date: header — just compact metadata
        assert!(!result.contains("Subject:"));
        assert!(!result.contains("Date:"));
    }

    #[test]
    fn llm_includes_cleaned_body_content() {
        let result = export_llm_context(&sample_thread(), &default_config());
        assert!(result.contains("rollback strategy"));
        assert!(result.contains("blue-green deployment"));
    }

    #[test]
    fn llm_strips_signatures_from_body() {
        let mut thread = sample_thread();
        thread.messages[0].body_text = Some(
            "Important content here.\n\n-- \nAlice\nSenior Engineer\nalice@company.com".into(),
        );
        let result = export_llm_context(&thread, &default_config());
        assert!(result.contains("Important content here"));
        assert!(!result.contains("Senior Engineer"));
    }

    #[test]
    fn llm_includes_attachment_summary() {
        let result = export_llm_context(&sample_thread(), &default_config());
        assert!(result.contains("Attachments: runbook.pdf (240KB)"));
    }

    #[test]
    fn llm_handles_empty_body() {
        let result = export_llm_context(&empty_body_thread(), &default_config());
        // Should not crash; message delimiter still present
        assert!(result.contains("---"));
        assert!(result.contains("[ghost@void.com,"));
    }

    #[test]
    fn llm_prefers_text_over_html() {
        let result = export_llm_context(&single_message_thread(), &default_config());
        // Has both text and html; should use text (the reader pipeline picks text first)
        assert!(result.contains("Is this working?"));
    }

    #[test]
    fn llm_falls_back_to_html_when_no_text() {
        let mut thread = single_message_thread();
        thread.messages[0].body_text = None;
        // body_html is still "<p>Is this working?</p>"
        let result = export_llm_context(&thread, &default_config());
        assert!(result.contains("Is this working?"));
    }

    #[test]
    fn llm_omits_markdown_formatting_overhead() {
        let thread = sample_thread();
        let config = default_config();
        let md = crate::mxr_export::export_markdown(&thread);
        let llm = export_llm_context(&thread, &config);
        // LLM format uses compact headers (no "##", no "**Attachments:**", no footer)
        assert!(!llm.contains("## "));
        assert!(!llm.contains("**Attachments:**"));
        assert!(!llm.contains("Exported from mxr"));
        // Both contain the actual content
        assert!(llm.contains("rollback strategy"));
        assert!(md.contains("rollback strategy"));
    }

    #[test]
    fn llm_separates_messages_with_delimiters() {
        let result = export_llm_context(&sample_thread(), &default_config());
        let delimiter_count = result.matches("\n---\n").count();
        assert_eq!(delimiter_count, 2); // One per message
    }

    #[test]
    fn llm_with_many_attachments_lists_all() {
        let thread = ExportThread {
            thread_id: "t".into(),
            subject: "Files".into(),
            messages: vec![ExportMessage {
                id: "m".into(),
                from_name: None,
                from_email: "a@b.com".into(),
                to: vec![],
                date: chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
                subject: "Files".into(),
                body_text: Some("See attached.".into()),
                body_html: None,
                headers_raw: None,
                attachments: vec![
                    ExportAttachment {
                        filename: "report.pdf".into(),
                        size_bytes: 102_400,
                        local_path: None,
                    },
                    ExportAttachment {
                        filename: "data.csv".into(),
                        size_bytes: 51_200,
                        local_path: None,
                    },
                ],
            }],
        };
        let result = export_llm_context(&thread, &default_config());
        assert!(result.contains("report.pdf (100KB)"));
        assert!(result.contains("data.csv (50KB)"));
    }
}
