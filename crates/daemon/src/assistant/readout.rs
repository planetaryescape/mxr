//! A tool result in words, for the steps a client shows. mxr's tools
//! answer in JSON shaped for the model; this reads any of them the same
//! way, so a new tool reads well without a case of its own: the things it
//! lists (emails, people, to-dos) with where to open each, or its plain
//! fields when it lists nothing.

use mxr_protocol::{
    AssistantLinkData, AssistantReadoutData, AssistantReadoutFactData, AssistantReadoutRowData,
};
use serde_json::{Map, Value};

/// Rows a step shows; the total says how many more there were.
const ROWS: usize = 5;
const FACTS: usize = 5;
const DETAIL_CHARS: usize = 90;
/// How deep to look for lists. mxr's results nest one or two wrappers.
const DEPTH: usize = 4;

/// The readout of a whole result, or `None` when it is not JSON or holds
/// nothing worth reading out.
pub(crate) fn readout(content: &str) -> Option<AssistantReadoutData> {
    let value: Value = serde_json::from_str(content).ok()?;
    let root = unwrap(&value)?;

    let mut lists = Vec::new();
    collect_lists(root, 0, &mut lists);
    lists.sort_by_key(|list| list_rank(list.key));
    if !lists.is_empty() {
        let listed: u64 = lists.iter().map(|list| list.total).sum();
        let total = root
            .get("total")
            .and_then(Value::as_u64)
            .map_or(listed, |total| total.max(listed));
        let rows = lists
            .iter()
            .flat_map(|list| list.items.iter())
            .filter_map(|item| item.as_object())
            .take(ROWS)
            .map(row)
            .collect();
        return Some(AssistantReadoutData {
            total: Some(total),
            rows,
            facts: Vec::new(),
        });
    }

    let facts: Vec<_> = root
        .iter()
        .filter(|(key, _)| !skipped_fact(key))
        .filter_map(|(key, value)| {
            Some(AssistantReadoutFactData {
                label: words(key),
                value: fact_value(value)?,
            })
        })
        .take(FACTS)
        .collect();
    (!facts.is_empty()).then_some(AssistantReadoutData {
        total: None,
        rows: Vec::new(),
        facts,
    })
}

/// The object that holds the result, past wrappers such as
/// `{"kind": "Messages", "messages": {...}}`.
fn unwrap(value: &Value) -> Option<&Map<String, Value>> {
    let mut object = value.as_object()?;
    for _ in 0..DEPTH {
        let mut inner = object.iter().filter(|(key, _)| key.as_str() != "kind");
        match (inner.next(), inner.next()) {
            (Some((_, Value::Object(child))), None) => object = child,
            _ => break,
        }
    }
    Some(object)
}

struct List<'a> {
    key: &'a str,
    items: &'a [Value],
    total: u64,
}

/// Every list of nameable things, in the order the result gives them. A
/// list's own `<name>_total` counts what a capped list left out.
fn collect_lists<'a>(object: &'a Map<String, Value>, depth: usize, out: &mut Vec<List<'a>>) {
    if depth > DEPTH {
        return;
    }
    for (key, value) in object {
        match value {
            Value::Array(items) if items.iter().any(nameable) => {
                let shown = items.len() as u64;
                let total = object
                    .get(&format!("{key}_total"))
                    .and_then(Value::as_u64)
                    .map_or(shown, |total| total.max(shown));
                out.push(List { key, items, total });
            }
            Value::Array(items) => {
                for item in items {
                    if let Value::Object(child) = item {
                        collect_lists(child, depth + 1, out);
                    }
                }
            }
            Value::Object(child) => collect_lists(child, depth + 1, out),
            _ => {}
        }
    }
}

/// JSON objects come back with sorted keys, so the result's own order is
/// lost. Show what waits on the user first and what is quiet last; other
/// lists keep their sorted order.
fn list_rank(key: &str) -> u8 {
    match key {
        "your_turn" | "overdue" | "pinned" => 0,
        "quiet" | "later" => 2,
        _ => 1,
    }
}

fn nameable(item: &Value) -> bool {
    item.as_object().is_some_and(|object| {
        ["subject", "title", "name", "person"]
            .iter()
            .any(|key| object.contains_key(*key))
    })
}

fn row(object: &Map<String, Value>) -> AssistantReadoutRowData {
    let person_name = object
        .get("person")
        .and_then(|person| text(person.get("name")));
    let title = text(object.get("subject"))
        .or_else(|| text(object.get("title")))
        .or_else(|| text(object.get("name")))
        .or_else(|| person_name.clone())
        .unwrap_or_else(|| "(no subject)".to_string());

    let from = object.get("from").and_then(|from| {
        text(from.get("name"))
            .or_else(|| text(from.get("email")))
            .or_else(|| text(Some(from)))
    });
    let first_topic = object
        .get("topics")
        .and_then(Value::as_array)
        .and_then(|topics| topics.first());
    let detail = [
        from,
        first_topic.and_then(|topic| text(topic.get("subject"))),
        object
            .get("preview")
            .and_then(|preview| text(preview.get("text"))),
        text(object.get("sender")),
        text(object.get("snippet")),
    ]
    .into_iter()
    .flatten()
    .find(|detail| *detail != title)
    .map(|detail| clipped(&detail, DETAIL_CHARS));

    AssistantReadoutRowData {
        title: clipped(&title, DETAIL_CHARS),
        detail,
        link: link(object),
    }
}

/// Where the row opens: its conversation, or the person's row in Messages.
fn link(object: &Map<String, Value>) -> Option<AssistantLinkData> {
    if let Some(thread_id) = text(object.get("thread_id")) {
        return Some(AssistantLinkData::Thread { thread_id });
    }
    let id = text(object.get("id"))?;
    if id.starts_with("person:") || id.starts_with("group:") {
        return Some(AssistantLinkData::Person { person_id: id });
    }
    None
}

fn text(value: Option<&Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    (!text.is_empty()).then(|| one_line(text))
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clipped(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max - 1).collect();
    format!("{}…", kept.trim_end())
}

/// Fields that mean something to code, not to the person reading.
fn skipped_fact(key: &str) -> bool {
    key == "kind"
        || key == "id"
        || key == "header"
        || key == "source"
        || key.ends_with("_id")
        || key.ends_with("_at")
        || key.ends_with("_bits")
}

fn fact_value(value: &Value) -> Option<String> {
    match value {
        Value::Bool(true) => Some("yes".into()),
        Value::Bool(false) => Some("no".into()),
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) if !text.trim().is_empty() && text.chars().count() <= 60 => {
            Some(one_line(text))
        }
        Value::Array(items) => Some(items.len().to_string()),
        _ => None,
    }
}

/// `undo_available` reads "Undo available".
fn words(key: &str) -> String {
    let spaced = key.replace('_', " ");
    let mut chars = spaced.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(value: Value) -> AssistantReadoutData {
        readout(&value.to_string()).expect("a readout")
    }

    #[test]
    fn lists_emails_with_sender_and_conversation() {
        let out = read(json!({
            "kind": "Envelopes",
            "envelopes": [
                {"id": "m1", "thread_id": "t1", "subject": "Weekly sync",
                 "from": {"name": "Quinn Dev", "email": "quinn@work.com"}},
                {"id": "m2", "thread_id": "t2", "subject": "Invoice",
                 "from": {"name": "", "email": "grace@work.com"}},
            ],
        }));
        assert_eq!(out.total, Some(2));
        assert_eq!(out.rows[0].title, "Weekly sync");
        assert_eq!(out.rows[0].detail.as_deref(), Some("Quinn Dev"));
        assert_eq!(
            out.rows[0].link,
            Some(AssistantLinkData::Thread {
                thread_id: "t1".into()
            })
        );
        assert_eq!(out.rows[1].detail.as_deref(), Some("grace@work.com"));
    }

    #[test]
    fn people_open_in_messages_and_count_what_bands_left_out() {
        let person = |id: &str, name: &str| {
            json!({"id": id, "kind": "person", "title": name,
                   "person": {"name": name},
                   "topics": [{"subject": "Q1 report", "thread_id": "t9"}]})
        };
        let out = read(json!({
            "kind": "Messages",
            "messages": {
                "header": "People you talk with",
                "your_turn": [person("person:a@x.com", "Ann"), person("group:g1", "Bo, Cy")],
                "quiet": [person("person:d@x.com", "Di")],
                "quiet_total": 7,
                "pinned": [],
            },
        }));
        assert_eq!(out.total, Some(9));
        assert_eq!(out.rows.len(), 3);
        assert_eq!(out.rows[0].title, "Ann");
        assert_eq!(out.rows[0].detail.as_deref(), Some("Q1 report"));
        assert_eq!(
            out.rows[1].link,
            Some(AssistantLinkData::Person {
                person_id: "group:g1".into()
            })
        );
    }

    #[test]
    fn a_result_without_lists_reads_as_its_plain_fields() {
        let out = read(json!({
            "kind": "Archive",
            "operation": {
                "accepted": true,
                "mutation_id": "mut1",
                "undo_available": true,
                "items": [{"message_id": "m1"}, {"message_id": "m2"}],
                "started_at": 5,
            },
        }));
        assert_eq!(out.total, None);
        let facts: Vec<_> = out
            .facts
            .iter()
            .map(|fact| format!("{}: {}", fact.label, fact.value))
            .collect();
        assert_eq!(facts, ["Accepted: yes", "Items: 2", "Undo available: yes"]);
    }

    #[test]
    fn caps_rows_and_trusts_a_larger_total() {
        let results: Vec<_> = (0..8)
            .map(|n| json!({"subject": format!("s{n}"), "thread_id": format!("t{n}")}))
            .collect();
        let out = read(json!({"kind": "Search", "results": results, "total": 40}));
        assert_eq!(out.rows.len(), ROWS);
        assert_eq!(out.total, Some(40));
    }

    #[test]
    fn text_and_empty_results_have_no_readout() {
        assert_eq!(readout("not json"), None);
        assert_eq!(readout("{}"), None);
    }
}
