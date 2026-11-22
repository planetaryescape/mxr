use crate::ExportThread;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Serialize)]
struct JsonThread {
    thread_id: String,
    subject: String,
    participants: Vec<String>,
    message_count: usize,
    messages: Vec<JsonMessage>,
}

#[derive(Serialize)]
struct JsonMessage {
    id: String,
    from: JsonAddress,
    to: Vec<String>,
    date: String,
    subject: String,
    body_text: Option<String>,
    attachments: Vec<JsonAttachment>,
}

#[derive(Serialize)]
struct JsonAddress {
    name: Option<String>,
    email: String,
}

#[derive(Serialize)]
struct JsonAttachment {
    filename: String,
    size_bytes: u64,
}

pub fn export_json(thread: &ExportThread) -> String {
    let participants: Vec<String> = thread
        .messages
        .iter()
        .map(|m| m.from_email.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let json_thread = JsonThread {
        thread_id: thread.thread_id.clone(),
        subject: thread.subject.clone(),
        message_count: thread.messages.len(),
        participants,
        messages: thread
            .messages
            .iter()
            .map(|m| JsonMessage {
                id: m.id.clone(),
                from: JsonAddress {
                    name: m.from_name.clone(),
                    email: m.from_email.clone(),
                },
                to: m.to.clone(),
                date: m.date.to_rfc3339(),
                subject: m.subject.clone(),
                body_text: m.body_text.clone(),
                attachments: m
                    .attachments
                    .iter()
                    .map(|a| JsonAttachment {
                        filename: a.filename.clone(),
                        size_bytes: a.size_bytes,
                    })
                    .collect(),
            })
            .collect(),
    };

    serde_json::to_string_pretty(&json_thread).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use crate::tests::{empty_body_thread, sample_thread, single_message_thread};

    #[test]
    fn json_export_carries_thread_message_and_attachment_fields() {
        let result = export_json(&sample_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["thread_id"], "thread_abc");
        assert_eq!(parsed["subject"], "Deployment rollback plan");
        assert_eq!(parsed["message_count"], 2);
        let msgs = parsed["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["id"], "msg_1");
        assert_eq!(msgs[1]["id"], "msg_2");
        assert!(msgs[0]["date"].as_str().unwrap().contains('T'));
        assert_eq!(msgs[0]["from"]["name"], "Alice");
        assert_eq!(msgs[0]["from"]["email"], "alice@example.com");
        assert_eq!(msgs[0]["to"][0], "team@example.com");
        assert_eq!(msgs[1]["attachments"][0]["filename"], "runbook.pdf");
        assert_eq!(msgs[1]["attachments"][0]["size_bytes"], 245_760);
    }

    #[test]
    fn json_export_uses_null_for_missing_name_and_body() {
        let solo: serde_json::Value =
            serde_json::from_str(&export_json(&single_message_thread())).unwrap();
        assert!(solo["messages"][0]["from"]["name"].is_null());
        let empty: serde_json::Value =
            serde_json::from_str(&export_json(&empty_body_thread())).unwrap();
        assert!(empty["messages"][0]["body_text"].is_null());
    }
}
