//! What Updates' Space opens under a line: the parts the one-line row
//! leaves out, as the web's peek shows them (`UpdateLineRow.tsx`, `Peek`).
//! The whole fact when the row cuts it, when the latest email arrived and
//! its first lines, why the line is here, the numbers as the mail quoted
//! them, a tracker's state, how many emails a line folds, a parcel's items
//! and references, the time, and where `L` and `o` go. All of it is from the
//! digest the daemon served, except the first lines, which come from the
//! body the reader fetches. Plain text: mail-controlled strings pass through
//! `one_line` before they reach the terminal.

use chrono::{DateTime, Utc};
use mxr_protocol::UpdateLineData;
use regex::Regex;
use std::sync::LazyLock;

use crate::ui::received_label::{received_label, DisplayZone};
use crate::ui::sanitize::one_line;

/// A fact this long may not fit on the row's one line (the web's
/// `LONG_FACT`).
const LONG_FACT: usize = 70;
/// Lines of the email shown, the longest one in characters, and how far
/// into the message to look (the web's `emailPreview.ts`).
const PREVIEW_LINES: usize = 4;
const LINE_MAX_CHARS: usize = 160;
const SCAN_LINES: usize = 80;

// The web's patterns, so both clients pick the same lines. Static literals,
// so a failed compile is a bug caught by the tests below.
static BARE_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:\[[^\]]*\]\s*)?<?(?:https?://|www\.)\S+>?$").expect("valid pattern")
});
static IMAGE_PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^\[(?:image|img|cid)[^\]]*\]$").expect("valid pattern"));
static BOILERPLATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:view (?:this )?(?:email )?in (?:your )?browser|unsubscribe|manage (?:your )?(?:email )?preferences|privacy policy|terms of (?:service|use)|all rights reserved|you(?:'re| are) receiving this|do not reply|this is an automated)\b|^©",
    )
    .expect("valid pattern")
});
static GREETING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:hi|hello|hey|dear)\b[^.!?]{0,40}[,!]?$").expect("valid pattern")
});
static LEADING_MARKS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\s>*\-#|]+").expect("valid pattern"));

fn clip(line: &str) -> String {
    if line.chars().count() <= LINE_MAX_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(LINE_MAX_CHARS - 1).collect();
    let kept = match cut.rfind(' ') {
        Some(word) if word > cut.len() / 2 => &cut[..word],
        _ => cut.as_str(),
    };
    format!("{}\u{2026}", kept.trim_end())
}

/// Whether a line says something: not a greeting, footer, bare link or
/// fragment. A receipt's "Total $24.00" is two words; a lone word is a label.
fn informative(line: &str) -> bool {
    if !line.chars().any(char::is_alphanumeric) {
        return false;
    }
    if BARE_LINK.is_match(line) || IMAGE_PLACEHOLDER.is_match(line) {
        return false;
    }
    if BOILERPLATE.is_match(line) || GREETING.is_match(line) {
        return false;
    }
    line.split(' ').count() >= 2 || line.chars().any(|c| c.is_ascii_digit())
}

/// The first informative lines of a message's text, whitespace collapsed.
pub(crate) fn preview_lines(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.lines().take(SCAN_LINES) {
        let stripped = LEADING_MARKS.replace(raw, "");
        let line = one_line(
            stripped
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .trim(),
        );
        if line.is_empty() || !informative(&line) || out.last() == Some(&line) {
            continue;
        }
        out.push(clip(&line));
        if out.len() == PREVIEW_LINES {
            break;
        }
    }
    out
}

fn tracker_status(line: &UpdateLineData) -> Option<String> {
    let tracker = line.tracker.as_ref()?;
    let label = one_line(&tracker.state_label);
    let status = match tracker.kind.as_str() {
        "parcel" => None,
        "build" => Some(format!("Build {label}")),
        "incident" => Some(format!("Incident {label}")),
        _ => Some(label),
    };
    let detail = tracker.detail.as_deref().map(one_line);
    let parts: Vec<String> = status.into_iter().chain(detail).collect();
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
}

/// The peek's lines, top to bottom. `body` is the latest email's plain text
/// once the reader has it; until then the other lines stand on their own.
pub(crate) fn peek_lines(
    line: &UpdateLineData,
    body: Option<&str>,
    now: DateTime<Utc>,
    zone: DisplayZone,
) -> Vec<String> {
    let mut lines = Vec::new();
    let fact = one_line(&line.fact);
    if fact.chars().count() > LONG_FACT {
        lines.push(fact);
    }
    lines.push(format!(
        "Received {}",
        received_label(line.latest_at, now, zone)
    ));
    lines.extend(body.map(preview_lines).unwrap_or_default());
    lines.push(one_line(&line.why));
    if !line.numbers.is_empty() {
        let numbers: Vec<String> = line.numbers.iter().map(|n| one_line(&n.raw)).collect();
        lines.push(format!("Numbers: {}", numbers.join(", ")));
    }
    lines.extend(tracker_status(line));
    // Repeats of one alert fold into the line; the peek says how many.
    if line.tracker.is_none() && line.count > 1 {
        lines.push(format!("{} emails in this digest", line.count));
    }
    if let Some(tracker) = &line.tracker {
        for item in tracker
            .items
            .iter()
            .filter(|item| !item.name.trim().is_empty())
        {
            let quantity = item
                .quantity
                .map(|quantity| format!(" \u{b7} quantity {quantity}"))
                .unwrap_or_default();
            lines.push(format!("\u{2022} {}{quantity}", one_line(&item.name)));
        }
        let reference = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(one_line)
        };
        let references: Vec<String> = [
            reference(&tracker.tracking_number).map(|r| format!("Tracking reference: {r}")),
            reference(&tracker.order_number).map(|r| format!("Order reference: {r}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !references.is_empty() {
            lines.push(references.join(" \u{b7} "));
        }
    }
    if let Some(time) = &line.time_label {
        lines.push(format!("At {}", one_line(time)));
    }
    let mut keys = Vec::new();
    if let Some(link) = &line.link {
        keys.push(format!("L opens {}", one_line(&link.domain)));
    }
    if line.latest_thread_id.is_some() {
        keys.push("o opens the email".to_string());
    }
    if !keys.is_empty() {
        lines.push(keys.join("  "));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_keeps_lines_that_say_something() {
        let text = "Hi Sam,\n\n> Your run: 21.3 km in 1:52\nView this email in your browser\nhttps://example.com/x\nTotal\nYour run: 21.3 km in 1:52\nNext race on Sunday\n";
        assert_eq!(
            preview_lines(text),
            vec!["Your run: 21.3 km in 1:52", "Next race on Sunday"]
        );
    }

    #[test]
    fn a_long_line_is_cut_at_a_word() {
        let long = "word ".repeat(60);
        let lines = preview_lines(&long);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with('\u{2026}'));
        assert!(lines[0].chars().count() <= LINE_MAX_CHARS);
    }

    #[test]
    fn terminal_controls_never_reach_the_preview() {
        let lines = preview_lines("Build \u{1b}]0;pwned\u{7} failed on main\n");
        assert!(lines.iter().all(|line| !line.contains('\u{1b}')));
    }
}
