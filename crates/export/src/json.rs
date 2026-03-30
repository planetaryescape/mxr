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
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::tests::{empty_body_thread, sample_thread, single_message_thread};

    #[test]
    fn json_is_valid_and_parseable() {
        let result = export_json(&sample_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn json_has_correct_message_count() {
        let result = export_json(&sample_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["message_count"], 2);
    }

    #[test]
    fn json_preserves_thread_metadata() {
        let result = export_json(&sample_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["thread_id"], "thread_abc");
        assert_eq!(parsed["subject"], "Deployment rollback plan");
    }

    #[test]
    fn json_messages_have_rfc3339_dates() {
        let result = export_json(&sample_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let date = parsed["messages"][0]["date"].as_str().unwrap();
        // RFC3339 contains T separator and timezone
        assert!(date.contains('T'));
        assert!(date.ends_with("+00:00") || date.ends_with('Z'));
    }

    #[test]
    fn json_includes_attachments_with_size() {
        let result = export_json(&sample_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let atts = &parsed["messages"][1]["attachments"];
        assert_eq!(atts.as_array().unwrap().len(), 1);
        assert_eq!(atts[0]["filename"], "runbook.pdf");
        assert_eq!(atts[0]["size_bytes"], 245_760);
    }

    #[test]
    fn json_from_address_has_name_and_email() {
        let result = export_json(&sample_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let from = &parsed["messages"][0]["from"];
        assert_eq!(from["name"], "Alice");
        assert_eq!(from["email"], "alice@example.com");
    }

    #[test]
    fn json_from_name_is_null_when_missing() {
        let result = export_json(&single_message_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["messages"][0]["from"]["name"].is_null());
    }

    #[test]
    fn json_body_text_is_null_when_missing() {
        let result = export_json(&empty_body_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["messages"][0]["body_text"].is_null());
    }

    #[test]
    fn json_includes_to_recipients() {
        let result = export_json(&sample_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let to = parsed["messages"][0]["to"].as_array().unwrap();
        assert_eq!(to[0], "team@example.com");
    }

    #[test]
    fn json_roundtrip_preserves_message_ids() {
        let result = export_json(&sample_thread());
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        let ids: Vec<&str> = parsed["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["msg_1", "msg_2"]);
    }
}
