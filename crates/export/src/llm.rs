use crate::ExportThread;
use mxr_reader::{clean, ReaderConfig};
use std::collections::HashSet;

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

        let reader_output = clean(
            msg.body_text.as_deref(),
            msg.body_html.as_deref(),
            reader_config,
        );
        out.push_str(&reader_output.content);
        out.push('\n');

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
    use crate::tests::sample_thread;

    fn default_config() -> ReaderConfig {
        ReaderConfig::default()
    }

    #[test]
    fn llm_context_is_compact_metadata_bodies_and_attachments() {
        let result = export_llm_context(&sample_thread(), &default_config());
        assert!(result.starts_with("Thread: Deployment rollback plan\n"));
        assert!(result.contains("Messages: 2\n"));
        assert!(result.contains("[alice@example.com,"));
        assert!(result.contains("Mar 17 09:30"));
        assert!(result.contains("blue-green deployment"));
        assert!(result.contains("Attachments: runbook.pdf (240KB)"));
        assert!(!result.contains("Subject:"));
        assert!(!result.contains("## "));
        assert_eq!(result.matches("\n---\n").count(), 2);
    }
}
