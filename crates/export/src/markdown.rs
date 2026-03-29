use crate::ExportThread;
use std::collections::HashSet;

pub fn export_markdown(thread: &ExportThread) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Thread: {}\n\n", thread.subject));

    for msg in &thread.messages {
        let sender = msg.from_name.as_deref().unwrap_or(&msg.from_email);
        let date = msg.date.format("%b %d, %Y %H:%M");
        out.push_str(&format!("## {} — {}\n\n", sender, date));

        if let Some(text) = &msg.body_text {
            out.push_str(text.trim());
        }
        out.push_str("\n\n");

        if !msg.attachments.is_empty() {
            out.push_str("**Attachments:**\n");
            for att in &msg.attachments {
                let size_kb = att.size_bytes / 1024;
                out.push_str(&format!("- {} ({}KB)\n", att.filename, size_kb));
            }
            out.push('\n');
        }
    }

    let participants: HashSet<&str> = thread
        .messages
        .iter()
        .map(|m| m.from_email.as_str())
        .collect();

    out.push_str(&format!(
        "---\nExported from mxr | {} messages | {} participants\n",
        thread.messages.len(),
        participants.len(),
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{empty_body_thread, sample_thread, single_message_thread};
    use pretty_assertions::assert_eq;

    #[test]
    fn markdown_starts_with_thread_subject() {
        let result = export_markdown(&sample_thread());
        assert!(result.starts_with("# Thread: Deployment rollback plan\n"));
    }

    #[test]
    fn markdown_uses_sender_name_when_available() {
        let result = export_markdown(&sample_thread());
        assert!(result.contains("## Alice —"));
        assert!(result.contains("## Bob —"));
    }

    #[test]
    fn markdown_falls_back_to_email_when_no_name() {
        let result = export_markdown(&single_message_thread());
        assert!(result.contains("## noreply@service.com —"));
    }

    #[test]
    fn markdown_includes_deterministic_dates() {
        let result = export_markdown(&sample_thread());
        assert!(result.contains("Mar 17, 2026 09:30"));
        assert!(result.contains("Mar 17, 2026 10:15"));
    }

    #[test]
    fn markdown_includes_message_body() {
        let result = export_markdown(&sample_thread());
        assert!(result.contains("What's the rollback strategy"));
        assert!(result.contains("blue-green deployment"));
    }

    #[test]
    fn markdown_lists_attachments() {
        let result = export_markdown(&sample_thread());
        assert!(result.contains("**Attachments:**"));
        assert!(result.contains("- runbook.pdf (240KB)"));
    }

    #[test]
    fn markdown_footer_has_correct_counts() {
        let result = export_markdown(&sample_thread());
        assert!(result.contains("2 messages"));
        assert!(result.contains("2 participants"));
    }

    #[test]
    fn markdown_deduplicates_participants() {
        // Both messages from same sender would count as 1
        let mut thread = sample_thread();
        thread.messages[1].from_email = "alice@example.com".into();
        let result = export_markdown(&thread);
        assert!(result.contains("1 participants"));
    }

    #[test]
    fn markdown_handles_empty_body() {
        let result = export_markdown(&empty_body_thread());
        // Should not crash; header still present
        assert!(result.contains("## Ghost —"));
    }

    #[test]
    fn markdown_is_valid_structure() {
        let result = export_markdown(&sample_thread());
        // H1 for thread, H2 for each message, footer separator
        let h1_count = result.matches("# Thread:").count();
        let h2_count = result.matches("\n## ").count();
        assert_eq!(h1_count, 1);
        assert_eq!(h2_count, 2);
        assert!(result.contains("\n---\n"));
    }
}
