//! The two options of a destroying confirm, the lit one marked, and the
//! keys that answer it (`app::ConfirmChoice`).

use crate::app::ConfirmChoice;
use ratatui::prelude::*;

/// `› Keep it   Delete draft`, with the lit option marked and coloured.
pub fn options(
    keep: &str,
    action: &str,
    choice: ConfirmChoice,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let option = |label: &str, lit: bool, colour: Color| {
        let style = if lit {
            Style::default().fg(colour).bold()
        } else {
            Style::default().fg(theme.text_secondary)
        };
        [
            Span::raw(if lit { "\u{203a} " } else { "  " }),
            Span::styled(label.to_string(), style),
        ]
    };
    let mut spans = Vec::new();
    spans.extend(option(keep, !choice.on_action, theme.accent));
    spans.push(Span::raw("   "));
    spans.extend(option(action, choice.on_action, theme.error));
    Line::from(spans)
}

/// The keys under the options.
pub fn keys_hint(theme: &crate::theme::Theme) -> Line<'static> {
    Line::from(Span::styled(
        "y yes \u{b7} n/Esc/q keep \u{b7} j/k move \u{b7} Enter the lit one",
        Style::default().fg(theme.text_muted),
    ))
}
