use crate::cli::OutputFormat;
use serde::Serialize;
use std::io::IsTerminal;

/// Mail-controlled text made safe for a terminal line: C0 and C1 control
/// characters (escape sequences, carriage returns, newlines, tabs) become
/// spaces, so a subject cannot move the cursor or retitle the terminal.
/// Same character classes as the TUI's `strip_control_chars`, but a
/// one-line table cell keeps no line breaks either.
pub fn terminal_text(input: &str) -> std::borrow::Cow<'_, str> {
    let unsafe_char = |c: char| matches!(c as u32, 0x00..=0x1F | 0x7F..=0x9F);
    if input.chars().any(unsafe_char) {
        std::borrow::Cow::Owned(
            input
                .chars()
                .map(|c| if unsafe_char(c) { ' ' } else { c })
                .collect(),
        )
    } else {
        std::borrow::Cow::Borrowed(input)
    }
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
