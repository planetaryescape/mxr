use mxr_protocol::{AssistantLinkData, AssistantReadoutData};
use serde_json::Value;

pub fn tool_words(tool: &str) -> String {
    let words = tool.strip_prefix("mxr_").unwrap_or(tool).replace('_', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn text(value: &Value, fallback: &str) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => fallback.to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewMessage {
    pub subject: String,
    pub from: String,
}

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

pub fn readout_line(readout: &AssistantReadoutData) -> Option<String> {
    if let Some(total) = readout.total {
        let names: Vec<&str> = readout.rows.iter().map(|row| row.title.as_str()).collect();
        let rest = total.saturating_sub(names.len() as u64);
        let mut line = format!("{total} found");
        if !names.is_empty() {
            line.push_str(": ");
            line.push_str(&names.join(", "));
            if rest > 0 {
                line.push_str(&format!(" and {rest} more"));
            }
        }
        return Some(line);
    }
    let facts: Vec<String> = readout
        .facts
        .iter()
        .map(|fact| format!("{} {}", fact.label, fact.value))
        .collect();
    (!facts.is_empty()).then(|| facts.join(" · "))
}

pub fn plain_text(markdown: &str) -> String {
    render_plain(markdown, None)
}

pub const MAX_NUMBERED_LINKS: usize = 9;

pub fn plain_text_with_links(markdown: &str) -> (String, Vec<AssistantLinkData>) {
    let mut links = Vec::new();
    let text = render_plain(markdown, Some(&mut links));
    (text, links)
}

fn parse_mxr_link(url: &str) -> Option<AssistantLinkData> {
    let (kind, id) = url.strip_prefix("mxr:")?.split_once('/')?;
    if id.is_empty() {
        return None;
    }
    match kind {
        "thread" => Some(AssistantLinkData::Thread {
            thread_id: id.to_owned(),
        }),
        "person" => Some(AssistantLinkData::Person {
            person_id: id.to_owned(),
        }),
        _ => None,
    }
}

fn render_plain(markdown: &str, mut links: Option<&mut Vec<AssistantLinkData>>) -> String {
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
            format!("{indent}• {}", inline(rest, links.as_deref_mut()))
        } else if trimmed.starts_with('#') {
            inline(
                trimmed.trim_start_matches('#').trim_start(),
                links.as_deref_mut(),
            )
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            format!("{indent}│ {}", inline(rest, links.as_deref_mut()))
        } else {
            format!("{indent}{}", inline(trimmed, links.as_deref_mut()))
        };
        out.push(body);
    }
    out.join("\n")
}

fn inline(text: &str, mut links: Option<&mut Vec<AssistantLinkData>>) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let image = c == '!' && chars.get(i + 1) == Some(&'[');
        if c == '[' || image {
            let start = if image { i + 2 } else { i + 1 };
            if let Some((label, url, end)) = link_at(&chars, start) {
                if image || label == url || url.is_empty() || url.starts_with("mxr:") {
                    out.push_str(&label);
                    let numbered = links
                        .as_deref_mut()
                        .zip(parse_mxr_link(&url).filter(|_| !image));
                    if let Some((links, link)) = numbered {
                        let number = match links.iter().position(|known| *known == link) {
                            Some(at) => Some(at + 1),
                            None if links.len() < MAX_NUMBERED_LINKS => {
                                links.push(link);
                                Some(links.len())
                            }
                            None => None,
                        };
                        if let Some(number) = number {
                            out.push_str(&format!(" [{number}]"));
                        }
                    }
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
}
