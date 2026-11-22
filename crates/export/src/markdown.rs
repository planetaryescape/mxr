use crate::ExportThread;
use std::collections::HashSet;

pub fn export_markdown(thread: &ExportThread) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Thread: {}\n\n", thread.subject));

    for msg in &thread.messages {
        let sender = msg.from_name.as_deref().unwrap_or(&msg.from_email);
        let date = msg.date.format("%b %d, %Y %H:%M");
        out.push_str(&format!("## {sender} — {date}\n\n"));

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
    use crate::tests::sample_thread;

    #[test]
    fn markdown_export_renders_thread_messages_attachments_and_footer() {
        let result = export_markdown(&sample_thread());
        assert!(result.starts_with("# Thread: Deployment rollback plan\n"));
        assert!(result.contains("## Alice —"));
        assert!(result.contains("## Bob —"));
        assert!(result.contains("Mar 17, 2026 09:30"));
        assert!(result.contains("blue-green deployment"));
        assert!(result.contains("- runbook.pdf (240KB)"));
        assert!(result.contains("2 messages"));
        assert!(result.contains("2 participants"));

        let mut one_sender = sample_thread();
        one_sender.messages[1].from_email = "alice@example.com".into();
        assert!(export_markdown(&one_sender).contains("1 participants"));
    }
}
