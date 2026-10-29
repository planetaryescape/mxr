//! A tool result in words, for the steps a client shows. mxr's tools
//! answer in JSON shaped for the model; this reads any of them the same
//! way, so a new tool reads well without a case of its own: the things it
//! lists (emails, people, to-dos) with where to open each, or its plain
//! fields when it lists nothing.

use mxr_assistant::{call_state, readable_result, CallState};
use mxr_core::AccountId;
use mxr_llm::ToolChatMessage;
use mxr_protocol::{
    AssistantLinkData, AssistantReadoutData, AssistantReadoutFactData, AssistantReadoutRowData,
};
use mxr_store::AssistantMessageRow;
use serde_json::{Map, Value};

/// Rows a step shows; the total says how many more there were.
const ROWS: usize = 5;
const FACTS: usize = 5;
const DETAIL_CHARS: usize = 90;
/// Rows kept per step for a reply to draw from, so a reply that names more
/// emails than a step's readout shows still finds them all.
const REPLY_ROWS: usize = 50;
/// How deep to look for lists. mxr's results nest one or two wrappers.
const DEPTH: usize = 4;

/// The readout of a whole result, or `None` when it is not JSON or holds
/// nothing worth reading out.
pub(crate) fn readout(content: &str) -> Option<AssistantReadoutData> {
    readout_capped(content, ROWS)
}

/// Everything a result lists, up to [`REPLY_ROWS`], for drawing the rows a
/// reply names.
pub(crate) fn listed_rows(content: &str) -> Vec<AssistantReadoutRowData> {
    all_rows(content, REPLY_ROWS).map_or_else(Vec::new, |readout| readout.rows)
}

/// The readout of a result with up to `cap` rows, for a client that shows
/// a step's rest. Rows are the same ones [`readout`] builds, only more of
/// them; the total is the readout's own.
pub(crate) fn all_rows(content: &str, cap: usize) -> Option<AssistantReadoutData> {
    readout_capped(content, cap)
}

fn readout_capped(content: &str, row_cap: usize) -> Option<AssistantReadoutData> {
    let value: Value = serde_json::from_str(content).ok()?;
    let root = unwrap(&value)?;

    let mut lists = Vec::new();
    collect_lists(root, None, 0, &mut lists);
    lists.sort_by_key(|list| list_rank(list.key));
    if !lists.is_empty() {
        let listed: u64 = lists.iter().map(|list| list.total).sum();
        let total = root
            .get("total")
            .and_then(Value::as_u64)
            .map_or(listed, |total| total.max(listed));
        let rows = lists
            .iter()
            .flat_map(|list| list.items.iter().map(move |item| (item, list.account)))
            .filter_map(|(item, account)| Some((item.as_object()?, account)))
            .take(row_cap)
            .map(|(object, account)| row(object, account))
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

/// Every row one step's stored result lists, up to `cap`, and the readout's
/// total. A step is named by the call id and the `seq` of the assistant
/// message that declared it, because models and the stream both reuse ids
/// (`call_0`) across a conversation. Results pair with the latest declaration
/// before them, as `view.rs` pairs them. `None` when no assistant message at
/// `seq` declares the call. A step with no result yet, or one that failed,
/// lists nothing.
pub(crate) fn step_rows(
    messages: &[AssistantMessageRow],
    call_id: &str,
    seq: i64,
    cap: usize,
) -> Option<(Vec<AssistantReadoutRowData>, Option<u64>)> {
    let mut declared = false;
    // Whether the latest declaration of this id is the step being asked for.
    let mut ours = false;
    let mut result = None;
    for row in messages {
        let Ok(message) = serde_json::from_str::<ToolChatMessage>(&row.message_json) else {
            continue;
        };
        match message {
            ToolChatMessage::Assistant { tool_calls, .. } => {
                if tool_calls.iter().any(|call| call.id == call_id) {
                    ours = row.seq == seq;
                    declared |= ours;
                }
            }
            ToolChatMessage::Tool {
                call_id: id,
                content,
                is_error,
            } if ours && id == call_id => result = Some((content, is_error)),
            _ => {}
        }
    }
    if !declared {
        return None;
    }
    let Some((content, is_error)) = result else {
        return Some((Vec::new(), None));
    };
    if is_error || call_state(&content, is_error) == CallState::Failed {
        return Some((Vec::new(), None));
    }
    let whole = readable_result(&content, usize::MAX);
    Some(all_rows(&whole, cap).map_or((Vec::new(), None), |readout| (readout.rows, readout.total)))
}

struct List<'a> {
    key: &'a str,
    items: &'a [Value],
    total: u64,
    /// The account the list's rows came to: the item's own `account_id`, or
    /// the one on the object that holds the list (a place bundle's messages).
    account: Option<&'a str>,
}

/// Every list of nameable things, in the order the result gives them. A
/// list's own `<name>_total` counts what a capped list left out. `account`
/// is the nearest `account_id` above the object, so a place bundle's
/// messages carry the bundle's account.
fn collect_lists<'a>(
    object: &'a Map<String, Value>,
    account: Option<&'a str>,
    depth: usize,
    out: &mut Vec<List<'a>>,
) {
    if depth > DEPTH {
        return;
    }
    let account = object.get("account_id").and_then(Value::as_str).or(account);
    for (key, value) in object {
        match value {
            Value::Array(items) if items.iter().any(nameable) => {
                let shown = items.len() as u64;
                let total = object
                    .get(&format!("{key}_total"))
                    .and_then(Value::as_u64)
                    .map_or(shown, |total| total.max(shown));
                out.push(List {
                    key,
                    items,
                    total,
                    account,
                });
            }
            Value::Array(items) => {
                for item in items {
                    if let Value::Object(child) = item {
                        collect_lists(child, account, depth + 1, out);
                    }
                }
            }
            Value::Object(child) => collect_lists(child, account, depth + 1, out),
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

fn row(object: &Map<String, Value>, account: Option<&str>) -> AssistantReadoutRowData {
    let person_name = object
        .get("person")
        .and_then(|person| text(person.get("name")));
    let title = text(object.get("subject"))
        .or_else(|| text(object.get("title")))
        .or_else(|| text(object.get("name")))
        .or_else(|| person_name.clone())
        .unwrap_or_else(|| "(no subject)".to_string());

    let from = from_name(object);
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

    // Only keys the tools really return: `from` (search, envelopes),
    // `snippet` and `has_attachments` (envelopes, place messages, thread
    // previews), `date` (envelopes, place messages) or `last_at` (Messages
    // rows) or `latest_at` (thread previews, unix seconds), and `unread`
    // (place messages, Messages rows), `unread_count` (thread previews) or
    // `flags` (envelopes). A thread preview names its latest sender in
    // `sender_name` and `sender_email`.
    let sender = from_name(object)
        .or_else(|| text(object.get("sender_name")))
        .or_else(|| text(object.get("sender_email")));
    AssistantReadoutRowData {
        account_id: text(object.get("account_id"))
            .or_else(|| account.map(str::to_string))
            .and_then(|id| id.parse::<AccountId>().ok()),
        title: clipped(&title, DETAIL_CHARS),
        detail,
        link: link(object),
        sender: sender.map(|sender| clipped(&sender, DETAIL_CHARS)),
        snippet: text(object.get("snippet")).map(|snippet| clipped(&snippet, DETAIL_CHARS)),
        at: object
            .get("date")
            .or_else(|| object.get("last_at"))
            .and_then(Value::as_str)
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .map(|at| at.timestamp())
            .or_else(|| object.get("latest_at").and_then(Value::as_i64)),
        attachment: object.get("has_attachments").and_then(Value::as_bool) == Some(true),
        unread: unread(object),
    }
}

fn from_name(object: &Map<String, Value>) -> Option<String> {
    let from = object.get("from")?;
    text(from.get("name"))
        .or_else(|| text(from.get("email")))
        .or_else(|| text(Some(from)))
}

/// An envelope's `flags` serialise as names ("READ | STARRED"); unread is
/// the absence of READ. Place messages and Messages rows say `unread`.
fn unread(object: &Map<String, Value>) -> bool {
    if let Some(unread) = object.get("unread").and_then(Value::as_bool) {
        return unread;
    }
    if let Some(count) = object.get("unread_count").and_then(Value::as_u64) {
        return count > 0;
    }
    object
        .get("flags")
        .and_then(Value::as_str)
        .is_some_and(|flags| !flags.split('|').any(|flag| flag.trim() == "READ"))
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
    Some(one_line(text)).filter(|line| !line.is_empty())
}

/// Mail text on one line, with control characters, escape sequences and
/// bidi overrides taken out: readout text reaches terminals.
fn one_line(text: &str) -> String {
    super::titles::one_line(text)
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
    let spaced = one_line(&key.replace('_', " "));
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
    fn an_email_row_carries_its_account_and_a_person_row_does_not() {
        let account = "7b0a9f2e-0000-4000-8000-000000000001";
        let out = read(json!({
            "kind": "Envelopes",
            "envelopes": [
                {"id": "m1", "thread_id": "t1", "subject": "Weekly sync",
                 "account_id": account,
                 "from": {"name": "Quinn Dev", "email": "quinn@work.com"}},
                {"id": "m2", "thread_id": "t2", "subject": "Invoice",
                 "from": {"name": "", "email": "grace@work.com"}},
            ],
        }));
        assert_eq!(
            out.rows[0]
                .account_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some(account)
        );
        assert_eq!(out.rows[1].account_id, None);
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
    fn a_search_names_its_emails_with_subject_sender_and_conversation() {
        // The shape mxr_search returns: ids and a score, plus the subject
        // and sender the MCP tool adds so the readout can draw rows.
        let out = read(json!({
            "kind": "SearchResults",
            "results": [
                {"message_id": "m1", "thread_id": "t1", "score": 1.0, "mode": "lexical",
                 "subject": "CI failed on main",
                 "from": {"name": "GitHub Actions", "email": "ci@github.example"}},
            ],
            "total": 14,
            "has_more": true,
        }));
        assert_eq!(out.total, Some(14));
        assert_eq!(out.rows[0].title, "CI failed on main");
        assert_eq!(out.rows[0].detail.as_deref(), Some("GitHub Actions"));
        assert_eq!(
            out.rows[0].link,
            Some(AssistantLinkData::Thread {
                thread_id: "t1".into()
            })
        );
    }

    #[test]
    fn flags_serialise_as_names_so_unread_is_the_absence_of_read() {
        // Pins the wire shape `unread()` reads, from the real type.
        use mxr_core::MessageFlags;
        let json = |flags: MessageFlags| serde_json::to_value(flags).unwrap();
        let read = json(MessageFlags::READ | MessageFlags::STARRED);
        assert_eq!(read, json!("READ | STARRED"));
        let out = read_rows(json!({"envelopes": [
            {"subject": "a", "flags": read},
            {"subject": "b", "flags": json(MessageFlags::STARRED)},
            {"subject": "c", "flags": json(MessageFlags::empty())},
        ]}));
        assert_eq!(
            out.iter().map(|row| row.unread).collect::<Vec<_>>(),
            [false, true, true]
        );
    }

    fn read_rows(value: Value) -> Vec<AssistantReadoutRowData> {
        read(value).rows
    }

    #[test]
    fn an_envelope_row_carries_sender_snippet_time_clip_and_unread() {
        // The shape mxr_list_messages returns: full envelopes.
        let out = read_rows(json!({"kind": "Envelopes", "envelopes": [{
            "id": "m1", "thread_id": "t1", "subject": "Invoice 4471",
            "from": {"name": "Dana Ruiz", "email": "dana@work.example"},
            "date": "2026-10-01T09:00:00Z", "flags": "STARRED",
            "snippet": "Please confirm\npayment by Friday", "has_attachments": true,
        }]}));
        let row = &out[0];
        assert_eq!(row.sender.as_deref(), Some("Dana Ruiz"));
        assert_eq!(
            row.snippet.as_deref(),
            Some("Please confirm payment by Friday")
        );
        assert_eq!(row.at, Some(1_790_845_200));
        assert!(row.attachment && row.unread);
        // `detail` is what it always was.
        assert_eq!(row.detail.as_deref(), Some("Dana Ruiz"));
    }

    #[test]
    fn a_search_result_fills_what_the_tool_names_and_no_more() {
        let out = read_rows(json!({"results": [{
            "message_id": "m1", "thread_id": "t1", "score": 1.0,
            "subject": "CI failed", "from": {"name": "", "email": "ci@github.example"},
        }]}));
        let row = &out[0];
        assert_eq!(row.sender.as_deref(), Some("ci@github.example"));
        assert_eq!((row.snippet.as_deref(), row.at), (None, None));
        assert!(!row.attachment && !row.unread);
    }

    #[test]
    fn a_place_message_has_snippet_time_and_unread_but_no_sender() {
        let out = read_rows(
            json!({"bundles": [{"sender_email": "a@b.example", "messages": [{
                "message_id": "m1", "thread_id": "t1", "subject": "Weekly digest",
                "snippet": "Ten links", "date": "2026-10-01T09:00:00+00:00",
                "unread": true, "pinned": false, "starred": false,
            }]}]}),
        );
        let row = &out[0];
        assert_eq!(row.sender, None);
        assert_eq!(row.snippet.as_deref(), Some("Ten links"));
        assert_eq!(row.at, Some(1_790_845_200));
        assert!(row.unread && !row.attachment);
    }

    #[test]
    fn a_place_message_takes_its_bundles_account() {
        let account = "7b0a9f2e-0000-4000-8000-000000000002";
        let out = read_rows(json!({"bundles": [{
            "account_id": account,
            "sender_email": "a@b.example",
            "messages": [{
                "message_id": "m1", "thread_id": "t1", "subject": "Weekly digest",
                "date": "2026-10-01T09:00:00+00:00", "unread": false,
            }],
        }]}));
        assert_eq!(
            out[0]
                .account_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some(account)
        );
    }

    #[test]
    fn a_thread_preview_row_carries_the_account_its_message_came_to() {
        let account = mxr_core::id::AccountId::new();
        let content = serde_json::to_string(&mxr_protocol::ResponseData::ThreadPreviews {
            previews: vec![mxr_protocol::ThreadPreviewData {
                thread_id: mxr_core::id::ThreadId::new(),
                subject: "Invoice 4471".into(),
                sender_name: None,
                sender_email: "dana@work.example".into(),
                latest_at: 1_760_000_000,
                message_count: 1,
                unread_count: 0,
                snippet: "Please confirm".into(),
                has_attachments: false,
                account_id: Some(account.clone()),
            }],
        })
        .expect("serialise previews");
        let out = readout(&content).expect("a readout");
        assert_eq!(out.rows[0].account_id, Some(account));
    }

    #[test]
    fn thread_previews_fill_the_email_row_from_the_real_wire_shape() {
        // Serialised from the daemon's own response type, so this pins what
        // `mxr_thread_previews` hands the readout.
        let first = mxr_core::id::ThreadId::new();
        let second = mxr_core::id::ThreadId::new();
        let content = serde_json::to_string(&mxr_protocol::ResponseData::ThreadPreviews {
            previews: vec![
                mxr_protocol::ThreadPreviewData {
                    thread_id: first.clone(),
                    subject: "Invoice 4471".into(),
                    sender_name: Some("Dana Ruiz".into()),
                    sender_email: "dana@work.example".into(),
                    latest_at: 1_760_000_000,
                    message_count: 4,
                    unread_count: 2,
                    snippet: "Please confirm\npayment by Friday".into(),
                    has_attachments: true,
                    account_id: None,
                },
                mxr_protocol::ThreadPreviewData {
                    thread_id: second.clone(),
                    subject: "Weekly digest".into(),
                    sender_name: None,
                    sender_email: "news@example.com".into(),
                    latest_at: 1_759_900_000,
                    message_count: 1,
                    unread_count: 0,
                    snippet: "Ten links".into(),
                    has_attachments: false,
                    account_id: None,
                },
            ],
        })
        .expect("serialise previews");

        let out = readout(&content).expect("a readout");
        assert_eq!(out.rows.len(), 2);

        let first_row = &out.rows[0];
        assert_eq!(first_row.title, "Invoice 4471");
        assert_eq!(first_row.sender.as_deref(), Some("Dana Ruiz"));
        assert_eq!(first_row.at, Some(1_760_000_000));
        assert_eq!(
            first_row.snippet.as_deref(),
            Some("Please confirm payment by Friday")
        );
        assert!(first_row.unread && first_row.attachment);
        assert!(matches!(
            &first_row.link,
            Some(AssistantLinkData::Thread { thread_id }) if *thread_id == first.to_string()
        ));

        let second_row = &out.rows[1];
        assert_eq!(second_row.sender.as_deref(), Some("news@example.com"));
        assert_eq!(second_row.at, Some(1_759_900_000));
        assert!(!second_row.unread && !second_row.attachment);
    }

    #[test]
    fn a_messages_row_has_time_and_unread_but_no_email_fields() {
        let out = read_rows(json!({"your_turn": [{
            "id": "person:ann@x.example", "kind": "person", "title": "Ann",
            "last_at": "2026-10-01T09:00:00Z", "unread": true,
            "preview": {"kind": "ask", "text": "Can you review?"}, "topics": [],
        }]}));
        let row = &out[0];
        assert_eq!(row.at, Some(1_790_845_200));
        assert!(row.unread);
        assert_eq!(
            (row.sender.as_deref(), row.snippet.as_deref()),
            (None, None)
        );
    }

    #[test]
    fn email_fields_carry_no_control_characters() {
        let out = read_rows(json!({"results": [{
            "subject": "s", "from": {"name": "Eve\u{1b}[31m\u{202e}", "email": "e@x"},
            "snippet": "line\u{7}one\ntwo",
        }]}));
        let row = &out[0];
        for text in [row.sender.as_deref(), row.snippet.as_deref()]
            .into_iter()
            .flatten()
        {
            assert!(
                !text.chars().any(|c| c.is_control() || c == '\u{202e}'),
                "{text:?}"
            );
        }
    }

    #[test]
    fn all_rows_returns_every_listed_row_up_to_the_cap() {
        let results: Vec<_> = (0..12)
            .map(|n| json!({"subject": format!("s{n}"), "thread_id": format!("t{n}")}))
            .collect();
        let whole = json!({"kind": "Search", "results": results, "total": 14}).to_string();

        let out = all_rows(&whole, 500).unwrap();
        assert_eq!(out.rows.len(), 12);
        assert_eq!(out.rows[11].title, "s11");
        assert_eq!(out.total, Some(14));

        let capped = all_rows(&whole, 4).unwrap();
        assert_eq!(capped.rows.len(), 4);
        assert_eq!(capped.total, Some(14));
    }

    #[test]
    fn text_and_empty_results_have_no_readout() {
        assert_eq!(readout("not json"), None);
        assert_eq!(readout("{}"), None);
    }

    #[test]
    fn row_text_from_mail_carries_no_control_characters() {
        let hostile = "\u{1b}[2J\u{1b}[Hforged\u{202e}x\u{9b}";
        let out = read(json!({"results": [{
            "subject": format!("{hostile}!"), "thread_id": "t1",
            "from": {"name": hostile, "email": "a@x.com"},
        }]}));
        for text in [&out.rows[0].title, out.rows[0].detail.as_ref().unwrap()] {
            assert!(
                !text.chars().any(|c| c.is_control() || c == '\u{202e}'),
                "{text:?}"
            );
            assert!(text.contains("forged"));
        }
    }
}
