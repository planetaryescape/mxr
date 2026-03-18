use crate::cli::OutputFormat;
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
