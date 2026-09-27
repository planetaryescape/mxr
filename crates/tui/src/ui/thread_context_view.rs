//! The thread's context above the messages, from the daemon's
//! `GetThreadContext` and `GetThreadGist`. The words come from
//! `crate::thread_context_rows`, shared with `mxr briefing context`; this
//! only styles them.

use crate::theme::Theme;
use crate::thread_context_rows::{context_rows, ContextRowKind};
use chrono::{DateTime, Utc};
use mxr_protocol::{ThreadContextData, ThreadGistData};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// Label column width, so the values line up like `mxr briefing context`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::{AccountId, ThreadId};
    use mxr_protocol::ThreadCounterpartyData;

    #[test]
    fn rows_become_aligned_lines() {
        let context = ThreadContextData {
            thread_id: ThreadId::new(),
            account_id: AccountId::new(),
            counterparty: Some(ThreadCounterpartyData {
                email: "maya@example.com".into(),
                display_name: Some("Maya Ortiz".into()),
                messages_from_them: 3,
                messages_from_you: 2,
                your_reply_p50_seconds: None,
                your_reply_samples: 0,
                their_reply_p50_seconds: None,
                their_reply_samples: 0,
                last_contact_elsewhere_at: None,
                bulk_sender: false,
            }),
            owed_reply: None,
            commitments: vec![],
        };
        let lines = context_lines(Some(&context), None, &Theme::default(), Utc::now());
        let text: Vec<String> = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect();
        assert_eq!(
            text,
            vec!["With      Maya (maya@example.com): 5 emails · your first conversation"]
        );
    }
}
