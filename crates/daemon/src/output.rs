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
    let control = |c: char| matches!(c as u32, 0x00..=0x1F | 0x7F..=0x9F);
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
