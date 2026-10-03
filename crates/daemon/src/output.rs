#![cfg_attr(
    test,
    expect(
        clippy::panic,
        reason = "tests panic with diagnostic context for direct failures"
    )
)]

use crate::cli::OutputFormat;
use serde::Serialize;
use std::io::IsTerminal;

/// Mail-controlled text made safe for a terminal line: C0 and C1 control
/// characters (escape sequences, carriage returns, newlines, tabs) become
/// spaces, so a subject cannot move the cursor or retitle the terminal, and
/// bidirectional overrides are dropped, so it cannot reorder what is shown
/// around it. Same character classes as the TUI's `strip_control_chars`,
/// but a one-line table cell keeps no line breaks either.
pub fn terminal_text(input: &str) -> std::borrow::Cow<'_, str> {
    // C0, DEL and C1, plus the Unicode line and paragraph separators, which
    // a terminal or pager may also break a row on.
    let control = |c: char| matches!(c as u32, 0x00..=0x1F | 0x7F..=0x9F | 0x2028 | 0x2029);
    if input.chars().any(|c| control(c) || is_bidi_control(c)) {
        std::borrow::Cow::Owned(
            input
                .chars()
                .filter(|c| !is_bidi_control(*c))
                .map(|c| if control(c) { ' ' } else { c })
                .collect(),
        )
    } else {
        std::borrow::Cow::Borrowed(input)
    }
}

/// [`terminal_text`] for a whole block printed at once: each line made
/// safe, the line breaks the block itself uses kept.
pub fn terminal_block(input: &str) -> String {
    input
        .split('\n')
        .map(terminal_text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Response fields that are one line of mail-controlled text: sender and
/// recipient names and addresses, subjects, snippets, and the titles and
/// reasons built from them.
const SINGLE_LINE_FIELDS: &[&str] = &[
    "subject",
    "snippet",
    "name",
    "email",
    "from_name",
    "from_email",
    "display_name",
    "sender",
    "sender_name",
    "sender_email",
    "counterparty",
    "counterparty_name",
    "counterparty_email",
    "latest_subject",
    "title",
    "summary",
    "person_label",
    "reason",
    "why",
    "next",
    "when_label",
    "headline",
    "due_words",
    "evidence",
    "what",
    "who_owes",
    "label",
    "domain",
    "also_in",
    "copy",
    "line",
    "more_line",
    "overload_line",
    "not_now",
    "empty_state",
];

/// Makes every one-line, mail-controlled field of a daemon response safe
/// to print, once, where the CLI receives it, so no text command can pass
/// an escape sequence, a line break or a bidi override from mail to the
/// terminal. Bodies and other multi-line text keep their line breaks and
/// are rendered by their own commands.
pub fn sanitize_response(response: mxr_protocol::Response) -> mxr_protocol::Response {
    let mxr_protocol::Response::Ok { data } = response else {
        return response;
    };
    let Ok(mut value) = serde_json::to_value(&data) else {
        return mxr_protocol::Response::Ok { data };
    };
    if !sanitize_value(&mut value) {
        return mxr_protocol::Response::Ok { data };
    }
    match serde_json::from_value(value) {
        Ok(data) => mxr_protocol::Response::Ok { data },
        Err(_) => mxr_protocol::Response::Ok { data },
    }
}

/// Returns whether anything changed, so a clean response is passed on as
/// it came.
fn sanitize_value(value: &mut serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            let mut changed = false;
            for (key, field) in map.iter_mut() {
                if let serde_json::Value::String(text) = field {
                    if SINGLE_LINE_FIELDS.contains(&key.as_str()) {
                        if let std::borrow::Cow::Owned(clean) = terminal_text(text) {
                            *text = clean;
                            changed = true;
                        }
                    }
                } else {
                    changed |= sanitize_value(field);
                }
            }
            changed
        }
        serde_json::Value::Array(items) => items
            .iter_mut()
            .fold(false, |changed, item| sanitize_value(item) | changed),
        _ => false,
    }
}

/// Marks and overrides that change the display order of the text around
/// them (the "Trojan Source" characters).
fn is_bidi_control(c: char) -> bool {
    matches!(
        c as u32,
        0x061C | 0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069
    )
}

pub fn resolve_format(explicit: Option<OutputFormat>) -> OutputFormat {
    if let Some(fmt) = explicit {
        return fmt;
    }
    if std::io::stdout().is_terminal() {
        OutputFormat::Table
    } else {
        OutputFormat::Json
    }
}

pub fn jsonl<T: Serialize>(items: &[T]) -> anyhow::Result<String> {
    items
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map(|lines| lines.join("\n"))
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::{Address, Envelope, MessageFlags, UnsubscribeMethod};
    use mxr_protocol::{Response, ResponseData};

    const HOSTILE: &str = "Invoice\u{1b}]0;pwned\u{7}\r\nFake row\u{2028}\u{202e}xat";

    fn hostile_envelope() -> Envelope {
        let id = MessageId::new();
        Envelope {
            id: id.clone(),
            account_id: AccountId::new(),
            provider_id: format!("p-{id}"),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: Some(HOSTILE.to_string()),
                email: "eve@example.com".to_string(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: HOSTILE.to_string(),
            date: chrono::Utc::now(),
            flags: MessageFlags::empty(),
            snippet: HOSTILE.to_string(),
            has_attachments: false,
            size_bytes: 1,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        }
    }

    fn assert_safe(text: &str) {
        assert!(
            !text.chars().any(|c| matches!(
                c as u32,
                0x00..=0x1F | 0x7F..=0x9F | 0x2028 | 0x2029 | 0x202A..=0x202E | 0x2066..=0x2069
            )),
            "{text:?}"
        );
        assert_eq!(text, "Invoice ]0;pwned   Fake row xat");
    }

    /// `mxr search` prints the envelopes `ListEnvelopesByIds` returns.
    #[test]
    fn search_envelopes_arrive_safe_to_print() {
        let response = sanitize_response(Response::Ok {
            data: ResponseData::Envelopes {
                envelopes: vec![hostile_envelope()],
            },
        });
        let Response::Ok {
            data: ResponseData::Envelopes { envelopes },
        } = response
        else {
            panic!("envelopes");
        };
        assert_safe(&envelopes[0].subject);
        assert_safe(&envelopes[0].snippet);
        assert_safe(envelopes[0].from.name.as_deref().unwrap_or_default());
    }

    /// `mxr replies` prints the reply-later queue.
    #[test]
    fn reply_queue_arrives_safe_to_print() {
        let response = sanitize_response(Response::Ok {
            data: ResponseData::ReplyQueue {
                messages: vec![hostile_envelope()],
            },
        });
        let Response::Ok {
            data: ResponseData::ReplyQueue { messages },
        } = response
        else {
            panic!("reply queue");
        };
        assert_safe(&messages[0].subject);
        assert_safe(&messages[0].snippet);
    }

    #[test]
    fn a_block_keeps_its_own_lines_but_no_field_can_add_one() {
        let block = format!("row one {}\nrow two", terminal_text("a\r\nb\u{2029}c"));
        assert_eq!(terminal_block(&block).lines().count(), 2);
    }
}
