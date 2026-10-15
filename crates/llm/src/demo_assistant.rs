//! The demo profile's assistant: a short script instead of a model, so
//! `mxr demo` shows the assistant's steps, an approval and a reply without
//! an API key. It lists the latest mail, then either describes it or, when
//! asked to tidy up, proposes archiving two messages and waits for the user.

use crate::tool_chat::{ToolCall, ToolChatMessage, ToolChatRequest, ToolChatResponse};
use serde_json::{json, Value};

const LIST_CALL: &str = "demo-list";
const ARCHIVE_CALL: &str = "demo-archive";
const MODEL: &str = "mxr-demo-canned";

pub(crate) fn reply(req: &ToolChatRequest) -> ToolChatResponse {
    let Some(user_at) = req
        .messages
        .iter()
        .rposition(|message| matches!(message, ToolChatMessage::User { .. }))
    else {
        return text("Ask me something about your mail.");
    };
    let ToolChatMessage::User { content } = &req.messages[user_at] else {
        return text("Ask me something about your mail.");
    };
    let asked = content
        .split_once("[/context]\n\n")
        .map_or(content.as_str(), |(_, text)| text)
        .to_lowercase();
    let tidy = ["archive", "tidy", "clean"]
        .iter()
        .any(|word| asked.contains(word));
    let since = &req.messages[user_at + 1..];

    let result_of = |call_id: &str| {
        since.iter().find_map(|message| match message {
            ToolChatMessage::Tool {
                call_id: id,
                content,
                ..
            } if id == call_id => Some(content.as_str()),
            _ => None,
        })
    };

    if let Some(outcome) = result_of(ARCHIVE_CALL) {
        return text(
            if outcome.contains("\"rejected\"") || outcome.contains("not_answered") {
                "Okay, I left them where they are."
            } else {
                "Done: both are archived. This is the demo profile, so nothing left your machine."
            },
        );
    }
    let Some(listed) = result_of(LIST_CALL) else {
        return call(
            LIST_CALL,
            "mxr_list_messages",
            json!({"limit": 5, "reason": "Look at your latest mail."}),
        );
    };
    let messages = envelopes(listed);
    if tidy && messages.len() >= 2 {
        let oldest: Vec<&str> = messages
            .iter()
            .rev()
            .take(2)
            .map(|(id, _, _)| id.as_str())
            .collect();
        return call(
            ARCHIVE_CALL,
            "mxr_mutate",
            json!({
                "action": "archive",
                "message_ids": oldest,
                "confirm": true,
                "reason": "Archive the two oldest of these."
            }),
        );
    }
    if messages.is_empty() {
        return text("There's no mail in the demo inbox yet.");
    }
    let lines: Vec<String> = messages
        .iter()
        .map(|(_, subject, from)| format!("- **{subject}** from {from}"))
        .collect();
    text(&format!(
        "This is the demo profile, so I'm following a script rather than a model. Your latest mail:\n\n{}\n\nAsk me to tidy up to see how a change waits for your approval.",
        lines.join("\n")
    ))
}

/// (id, subject, sender) for each message a result lists.
fn envelopes(content: &str) -> Vec<(String, String, String)> {
    let (Some(start), Some(end)) = (content.find('{'), content.rfind('}')) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&content[start..=end]) else {
        return Vec::new();
    };
    let Some(list) = value
        .get("envelopes")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
    else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|envelope| {
            let id = envelope.get("id")?.as_str()?.to_string();
            let subject = envelope
                .get("subject")
                .and_then(Value::as_str)
                .unwrap_or("(no subject)")
                .to_string();
            let from = &envelope["from"];
            let sender = from
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
                .or_else(|| from.get("email").and_then(Value::as_str))
                .unwrap_or("someone")
                .to_string();
            Some((id, subject, sender))
        })
        .collect()
}

fn call(id: &str, name: &str, arguments: Value) -> ToolChatResponse {
    ToolChatResponse {
        text: String::new(),
        tool_calls: vec![ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments,
        }],
        model: MODEL.to_string(),
        finish_reason: Some("tool_use".to_string()),
        usage: None,
        provider_turn: None,
    }
}

fn text(text: &str) -> ToolChatResponse {
    ToolChatResponse {
        text: text.to_string(),
        tool_calls: Vec::new(),
        model: MODEL.to_string(),
        finish_reason: Some("end_turn".to_string()),
        usage: None,
        provider_turn: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(messages: Vec<ToolChatMessage>) -> ToolChatRequest {
        ToolChatRequest {
            messages,
            tools: Vec::new(),
            max_tokens: None,
        }
    }

    fn listed() -> ToolChatMessage {
        ToolChatMessage::Tool {
            call_id: LIST_CALL.into(),
            content: crate::wrap_untrusted_mail(
                &json!({"kind": "Envelopes", "envelopes": [
                    {"id": "m1", "subject": "New", "from": {"name": "Ana", "email": "a@x"}},
                    {"id": "m2", "subject": "Older", "from": {"email": "b@x"}},
                    {"id": "m3", "subject": "Oldest", "from": {"email": "c@x"}}
                ]})
                .to_string(),
            ),
            is_error: false,
        }
    }

    fn user(text: &str) -> ToolChatMessage {
        ToolChatMessage::User {
            content: format!("[context]\nnow: x\n[/context]\n\n{text}"),
        }
    }

    #[test]
    fn it_lists_first_then_describes_what_it_found() {
        let first = reply(&request(vec![user("what's new?")]));
        assert_eq!(first.tool_calls[0].name, "mxr_list_messages");
        let second = reply(&request(vec![
            user("what's new?"),
            first.to_message(),
            listed(),
        ]));
        assert!(second.tool_calls.is_empty());
        assert!(second.text.contains("**New** from Ana"));
    }

    #[test]
    fn asked_to_tidy_it_proposes_archiving_the_two_oldest() {
        let ask = user("tidy my inbox");
        let first = reply(&request(vec![ask.clone()]));
        let second = reply(&request(vec![ask.clone(), first.to_message(), listed()]));
        assert_eq!(second.tool_calls[0].name, "mxr_mutate");
        assert_eq!(
            second.tool_calls[0].arguments["message_ids"],
            json!(["m3", "m2"])
        );
        let declined = ToolChatMessage::Tool {
            call_id: ARCHIVE_CALL.into(),
            content: "{\"source\":\"user\",\"status\":\"rejected\"}".into(),
            is_error: true,
        };
        let done = reply(&request(vec![
            ask,
            first.to_message(),
            listed(),
            second.to_message(),
            declined,
        ]));
        assert!(done.text.contains("left them"));
    }
}
