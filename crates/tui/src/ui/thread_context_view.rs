use crate::theme::Theme;
use crate::thread_context_rows::{context_rows, ContextRowKind};
use chrono::{DateTime, Utc};
use mxr_protocol::{ThreadContextData, ThreadGistData};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

const LABEL_WIDTH: usize = 10;

pub fn context_lines(
    context: Option<&ThreadContextData>,
    gist: Option<&ThreadGistData>,
    theme: &Theme,
    now: DateTime<Utc>,
) -> Vec<Line<'static>> {
    context_rows(context, gist, now)
        .into_iter()
        .map(|row| {
            let (label, text) = match row.kind {
                ContextRowKind::Gist => (Style::default().fg(theme.accent), theme.text_primary),
                ContextRowKind::Ask => (Style::default().fg(theme.warning), theme.text_primary),
                ContextRowKind::Quote => (Style::default(), theme.warning),
                ContextRowKind::Promise => (
                    Style::default().fg(theme.text_secondary),
                    theme.text_primary,
                ),
                ContextRowKind::Fact => {
                    (Style::default().fg(theme.text_secondary), theme.text_muted)
                }
                ContextRowKind::Quiet | ContextRowKind::Source => {
                    (Style::default().fg(theme.text_muted), theme.text_muted)
                }
            };
            let mut value = Style::default().fg(text);
            if row.kind == ContextRowKind::Quote {
                value = value.add_modifier(Modifier::ITALIC);
            }
            Line::from(vec![
                Span::styled(
                    format!("{:<LABEL_WIDTH$}", row.label),
                    label.add_modifier(Modifier::BOLD),
                ),
                Span::styled(row.text, value),
            ])
        })
        .collect()
}
