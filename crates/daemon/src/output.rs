use crate::cli::OutputFormat;
use serde::Serialize;
use std::io::IsTerminal;

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
