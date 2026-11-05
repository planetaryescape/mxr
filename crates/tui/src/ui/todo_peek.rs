//! What To do's Space opens under a row: the details the web shows under
//! it (`TodoRow.tsx`, `Details`). The dates in full, when the source email
//! arrived or left the inbox, why the row is here and what happens next,
//! where its link goes, the email's first lines, where each field came
//! from, and where `o` goes. All of it is from the runway the daemon
//! served, except the first lines, which come from the body the reader
//! fetches. Plain text: mail-controlled strings pass through `one_line`
//! before they reach the terminal.

use chrono::{DateTime, Utc};
use mxr_protocol::TodoData;

use crate::ui::received_label::{received_label, DisplayZone};
use crate::ui::sanitize::one_line;
use crate::ui::updates_peek::preview_lines;

/// The web's field names and the order its provenance list uses
/// (`todoRows.ts`, `FIELD_LABELS`).
const FIELD_LABELS: [(&str, &str); 10] = [
    ("title", "What"),
    ("kind", "Kind"),
    ("counterparty", "Who"),
    ("amount", "Amount"),
    ("due_at", "Due"),
    ("act_by_at", "Act by"),
    ("surface_at", "Shows up"),
    ("relevant_until", "Lets go"),
    ("action_url", "Link"),
    ("event_start", "Event"),
];

fn field_rank(field: &str) -> usize {
    FIELD_LABELS
        .iter()
        .position(|(name, _)| *name == field)
        .unwrap_or(FIELD_LABELS.len())
}

fn field_label(field: &str) -> String {
    FIELD_LABELS
        .iter()
        .find(|(name, _)| *name == field)
        .map_or_else(
            || field.replace('_', " "),
            |(_, label)| (*label).to_string(),
        )
}

/// The peek's lines, top to bottom. `body` is the source email's plain text
/// once the reader has it; until then the other lines stand on their own.
pub(crate) fn peek_lines(
    todo: &TodoData,
    body: Option<&str>,
    now: DateTime<Utc>,
    zone: DisplayZone,
) -> Vec<String> {
    let mut lines = Vec::new();
    // The row cuts its dates to fit; the peek has room for all of them.
    let when = one_line(&todo.when_label);
    if !when.is_empty() {
        lines.push(when);
    }
    if let Some(arrived) = todo.source_date {
        lines.push(format!("Arrived {}", received_label(arrived, now, zone)));
    }
    if let Some(filed) = todo.source_filed_at {
        lines.push(format!(
            "Left your inbox {}; this to-do is still open",
            zone.at(filed).format("%-d %b")
        ));
    }
    lines.push(one_line(&todo.why));
    if let Some(next) = &todo.next {
        lines.push(format!("Next: {}", one_line(next)));
    }
    if let Some(domain) = todo
        .action
        .as_ref()
        .and_then(|action| action.domain.as_deref())
    {
        lines.push(format!(
            "The link in the email goes to {}",
            one_line(domain)
        ));
    }
    lines.extend(body.map(preview_lines).unwrap_or_default());
    // Where each field came from. Evidence the why line already quotes is
    // not repeated, and the model's asks are diagnostics the web folds away.
    let mut seen = vec![todo.why.clone()];
    let mut fields: Vec<_> = todo
        .fields
        .iter()
        .filter(|field| !field.field.starts_with("asks_"))
        .collect();
    fields.sort_by_key(|field| field_rank(&field.field));
    for field in fields {
        let mut line = format!(
            "{}: {}",
            field_label(&field.field),
            one_line(&field.source_label)
        );
        if let Some(evidence) = field.evidence.as_ref().filter(|e| !seen.contains(e)) {
            line.push_str(&format!(": \u{201c}{}\u{201d}", one_line(evidence)));
            seen.push(evidence.clone());
        }
        if !field.checked {
            line.push_str(" (unchecked)");
        }
        lines.push(line);
    }
    if todo.source_message_id.is_some() {
        lines.push("o opens the email".to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::TodoFieldData;

    fn field(name: &str, evidence: Option<&str>, checked: bool) -> TodoFieldData {
        TodoFieldData {
            field: name.into(),
            source: "rule".into(),
            checked,
            evidence: evidence.map(Into::into),
            source_label: "a pattern in the email".into(),
        }
    }

    #[test]
    fn fields_follow_the_web_order_without_repeating_the_why() {
        let mut todo = crate::ui::todo_lens::tests::todo("t1", "Pay council tax");
        todo.fields = vec![
            field("due_at", Some("payment due 9 October"), false),
            field("asks_reply", Some("Can you confirm?"), true),
            field("amount", Some("£1,234.00"), true),
            field("title", Some("payment due 9 October"), true),
        ];
        let lines = peek_lines(&todo, None, Utc::now(), DisplayZone::Local);
        let fields: Vec<&String> = lines
            .iter()
            .filter(|line| line.contains("a pattern in the email"))
            .collect();
        assert_eq!(
            fields,
            vec![
                "What: a pattern in the email: \u{201c}payment due 9 October\u{201d}",
                "Amount: a pattern in the email: \u{201c}£1,234.00\u{201d}",
                "Due: a pattern in the email (unchecked)",
            ],
            "{lines:?}"
        );
        assert!(lines.iter().all(|line| !line.contains("Can you confirm")));
    }

    #[test]
    fn the_email_lines_join_once_the_body_is_fetched() {
        let todo = crate::ui::todo_lens::tests::todo("t1", "Pay council tax");
        let before = peek_lines(&todo, None, Utc::now(), DisplayZone::Local);
        let after = peek_lines(
            &todo,
            Some("Dear resident,\nYour bill of £1,234 is due on 9 October\nUnsubscribe"),
            Utc::now(),
            DisplayZone::Local,
        );
        assert!(after.contains(&"Your bill of £1,234 is due on 9 October".to_string()));
        assert_eq!(after.len(), before.len() + 1, "{after:?}");
        assert_eq!(after.last().map(String::as_str), Some("o opens the email"));
    }

    #[test]
    fn terminal_controls_never_reach_the_peek() {
        let mut todo = crate::ui::todo_lens::tests::todo("t1", "Pay");
        todo.next = Some("Pay \u{1b}]0;pwned\u{7} soon".into());
        let lines = peek_lines(&todo, None, Utc::now(), DisplayZone::Local);
        assert!(lines.iter().all(|line| !line.contains('\u{1b}')));
    }
}
