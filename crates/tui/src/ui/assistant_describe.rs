//! Words for what the assistant does: a title for each change it asks to
//! make, the messages a preview names, and its markdown as plain text. The
//! same reading as the web app's `features/assistant/describe.ts`, so an
//! approval reads "Archive 3 messages" in both clients.
//!
//! Every string here may come from someone else's email (subjects, senders,
//! a model's reply about them): it is displayed, never acted on.

use serde_json::Value;

/// A tool name as words: `mxr_list_messages` reads "List messages".
pub fn tool_words(tool: &str) -> String {
    let words = tool.strip_prefix("mxr_").unwrap_or(tool).replace('_', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn count(value: &Value, one: &str) -> String {
    let n = value.as_array().map_or(0, Vec::len);
    match n {
        0 => format!("{one}s"),
        1 => format!("1 {one}"),
        n => format!("{n} {one}s"),
    }
}

fn text(value: &Value, fallback: &str) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => fallback.to_string(),
        other => other.to_string(),
    }
}

/// The title of an approval.
pub fn describe_change(tool: &str, args: &Value) -> String {
    match tool {
        "mxr_mutate" => {
            let things = count(&args["message_ids"], "message");
            match args["action"].as_str().unwrap_or_default() {
                "archive" => format!("Archive {things}"),
                "read_and_archive" => format!("Mark {things} read and archive"),
                "trash" => format!("Move {things} to Trash"),
                "spam" => format!("Report {things} as spam"),
                "mark_read" => format!("Mark {things} read"),
                "mark_unread" => format!("Mark {things} unread"),
                "star" => format!("Star {things}"),
                "unstar" => format!("Unstar {things}"),
                "" => format!("{} {things}", tool_words(tool)),
                action => format!("{} {things}", tool_words(action)),
            }
        }
        "mxr_send_draft" => "Send a draft".into(),
        "mxr_archive_submit" => "Archive the reviewed messages".into(),
        "mxr_move" => {
            let mode = text(&args["mode"], "another mode");
            if args.get("sender").is_some_and(|sender| !sender.is_null()) {
                format!("Send this sender's mail to {mode}")
            } else {
                format!("Move an email to {mode}")
            }
        }
        "mxr_todo_set_state" => format!(
            "{} {}",
            tool_words(&text(&args["action"], "change")),
            count(&args["todo_ids"], "to-do")
        ),
        "mxr_delete_draft" => "Delete a draft".into(),
        "mxr_update_draft" => "Replace a draft's content".into(),
        "mxr_got_it" => "Send a short acknowledgement".into(),
        "mxr_sorting_rule" => "Save a sorting rule".into(),
        "mxr_category_rule" => {
            if args["delete"].as_bool().unwrap_or(false) {
                "Delete a category rule".into()
            } else {
                "Save a category rule".into()
            }
        }
        "mxr_category_create" => format!("Create the category \"{}\"", text(&args["name"], "")),
        "mxr_category_rename" => {
            format!("Rename a category to \"{}\"", text(&args["name"], ""))
        }
        "mxr_category_delete" => "Delete a category".into(),
        "mxr_category_assignment_apply" => "Change message categories".into(),
        "mxr_reading_later" => {
            if args["later"].as_bool() == Some(false) {
                "Take items off Later".into()
            } else {
                "Put items on Later".into()
            }
        }
        _ => tool_words(tool),
    }
}

/// One message a preview names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewMessage {
    pub subject: String,
    pub from: String,
}

/// The messages a preview names, wherever the tool put them: the first
/// list of objects with a subject, a few levels down.
pub fn preview_messages(preview: &Value) -> Vec<PreviewMessage> {
    fn find(value: &Value, depth: u8) -> Option<&Vec<Value>> {
        if depth > 4 {
            return None;
        }
        match value {
            Value::Array(items)
                if !items.is_empty() && items.iter().all(|item| item.get("subject").is_some()) =>
            {
                Some(items)
            }
            Value::Array(items) => items.iter().find_map(|item| find(item, depth + 1)),
            Value::Object(map) => map.values().find_map(|item| find(item, depth + 1)),
            _ => None,
        }
    }
    let non_empty = |value: &Value| value.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    find(preview, 0)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let from = &item["from"];
                    PreviewMessage {
                        subject: non_empty(&item["subject"])
                            .unwrap_or_else(|| "(no subject)".into()),
                        from: non_empty(&from["name"])
                            .or_else(|| non_empty(&from["email"]))
                            .or_else(|| non_empty(from))
                            .or_else(|| non_empty(&item["sender"]))
                            .unwrap_or_default(),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A draft a preview shows (sending one previews it whole).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewDraft {
    pub to: String,
    pub subject: String,
    pub body: String,
}

pub fn preview_draft(preview: &Value) -> Option<PreviewDraft> {
    let draft = preview.get("draft").unwrap_or(preview);
    if !draft.is_object() || (draft.get("subject").is_none() && draft.get("body").is_none()) {
        return None;
    }
    let to = match &draft["to"] {
        Value::Array(addresses) => addresses
            .iter()
            .map(|address| match address.get("email") {
                Some(email) => text(email, ""),
                None => text(address, ""),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => text(other, ""),
    };
    let body = match draft.get("body") {
        Some(body) if !body.is_null() => text(body, ""),
        _ => text(&draft["body_markdown"], ""),
    };
    Some(PreviewDraft {
        to,
        subject: text(&draft["subject"], ""),
        body,
    })
}

/// "Work, plus a note" from an `ask_user` result the user wrote.
pub fn answered_with(result: Option<&str>) -> Option<String> {
    let parsed: Value = serde_json::from_str(result?).ok()?;
    let mut parts = Vec::new();
    for answer in parsed["answers"].as_array()? {
        if let Some(selected) = answer["selected"].as_array() {
            parts.extend(selected.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        if let Some(other) = answer["other"].as_str().filter(|other| !other.is_empty()) {
            parts.push(other.to_owned());
        }
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// The assistant's light markdown as terminal text: markers dropped,
/// bullets kept, links as "text (url)", images as their description only
/// (nothing remote is fetched or shown).
pub fn plain_text(markdown: &str) -> String {
    let mut out = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            continue;
        }
        let indent = &line[..line.len() - trimmed.len()];
        let body = if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            format!("{indent}• {}", inline(rest))
        } else if trimmed.starts_with('#') {
            inline(trimmed.trim_start_matches('#').trim_start())
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            format!("{indent}│ {}", inline(rest))
        } else {
            format!("{indent}{}", inline(trimmed))
        };
        out.push(body);
    }
    out.join("\n")
}

/// Emphasis and code markers dropped; links and images rewritten.
fn inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let image = c == '!' && chars.get(i + 1) == Some(&'[');
        if c == '[' || image {
            let start = if image { i + 2 } else { i + 1 };
            if let Some((label, url, end)) = link_at(&chars, start) {
                // An image is only its description: nothing remote loads.
                if image || label == url || url.is_empty() {
                    out.push_str(&label);
                } else {
                    out.push_str(&format!("{label} ({url})"));
                }
                i = end;
                continue;
            }
        }
        if c == '`' || ((c == '*' || c == '_') && is_marker_run(&chars, i)) {
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// `**`/`__` runs and lone `*`/`_` next to a word, not ones in the middle
/// of `snake_case` or `2 * 3`.
fn is_marker_run(chars: &[char], i: usize) -> bool {
    let c = chars[i];
    if chars.get(i + 1) == Some(&c) || (i > 0 && chars[i - 1] == c) {
        return true;
    }
    let before = i.checked_sub(1).map(|j| chars[j]);
    let after = chars.get(i + 1).copied();
    let word = |ch: Option<char>| ch.is_some_and(char::is_alphanumeric);
    word(before) != word(after)
}

/// `label](url)` starting at `start`: the label, the url and the index
/// after the closing parenthesis.
fn link_at(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let close = (start..chars.len()).find(|&j| chars[j] == ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = (close + 2..chars.len()).find(|&j| chars[j] == ')')?;
    let label: String = chars[start..close].iter().collect();
    let url: String = chars[close + 2..end].iter().collect();
    Some((label, url, end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn describes_changes_in_words() {
        assert_eq!(
            describe_change(
                "mxr_mutate",
                &json!({"action": "archive", "message_ids": ["a", "b", "c"]})
            ),
            "Archive 3 messages"
        );
        assert_eq!(
            describe_change(
                "mxr_mutate",
                &json!({"action": "trash", "message_ids": ["a"]})
            ),
            "Move 1 message to Trash"
        );
        assert_eq!(
            describe_change("mxr_move", &json!({"mode": "Reading", "sender": "a@b"})),
            "Send this sender's mail to Reading"
        );
        assert_eq!(
            describe_change("mxr_list_messages", &json!({})),
            "List messages"
        );
    }

    #[test]
    fn finds_the_messages_a_preview_names() {
        let preview = json!({"result": {"messages": [
            {"subject": "Invoice", "from": {"name": "Acme", "email": "a@acme.test"}},
            {"subject": "", "from": {"email": "b@x.test"}}
        ]}});
        assert_eq!(
            preview_messages(&preview),
            vec![
                PreviewMessage {
                    subject: "Invoice".into(),
                    from: "Acme".into()
                },
                PreviewMessage {
                    subject: "(no subject)".into(),
                    from: "b@x.test".into()
                },
            ]
        );
        assert!(preview_messages(&json!({"count": 2})).is_empty());
    }

    #[test]
    fn reads_a_draft_preview() {
        let draft = preview_draft(&json!({"draft": {
            "to": [{"email": "a@x.test"}, {"email": "b@x.test"}],
            "subject": "Hi",
            "body": "Hello"
        }}))
        .expect("draft");
        assert_eq!(draft.to, "a@x.test, b@x.test");
        assert_eq!(draft.subject, "Hi");
        assert_eq!(draft.body, "Hello");
    }

    #[test]
    fn markdown_becomes_plain_text() {
        assert_eq!(
            plain_text("## Today\n- **Acme** sent [the invoice](https://x.test)\n`code` and snake_case\n![pixel](https://t.test/p.gif)"),
            "Today\n• Acme sent the invoice (https://x.test)\ncode and snake_case\npixel"
        );
    }

    #[test]
    fn reads_what_the_user_answered() {
        assert_eq!(
            answered_with(Some(
                r#"{"answers":[{"selected":["Work"],"other":"and family"}]}"#
            )),
            Some("Work, and family".into())
        );
        assert_eq!(answered_with(Some("not json")), None);
    }
}
