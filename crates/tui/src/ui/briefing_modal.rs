//! Slice 5.1 / 5.2 wiring (C2.6): renders the briefing modal.
//!
//! The modal opens loading, replaces with body+citations on response,
//! shows error inline on failure. Same shape as `summary_modal` /
//! `sender_profile_modal` so the visual language is consistent.

use crate::app::{BriefingModalState, BriefingModalSubject};
use ratatui::prelude::*;
use ratatui::widgets::*;
use super::centered_rect;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &BriefingModalState,
    theme: &crate::theme::Theme,
) {
    if !state.visible {
        return;
    }
    let popup = centered_rect(80, 70, area);
    frame.render_widget(Clear, popup);

    let title = match &state.subject {
        Some(BriefingModalSubject::Thread(id)) => {
            format!(" Briefing: thread {} ", short_id(&id.to_string()))
        }
        Some(BriefingModalSubject::Recipient(email)) => format!(" Briefing: {email} "),
        None => " Briefing ".to_string(),
    };

    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines: Vec<Line<'_>> = Vec::new();

    if state.loading {
        lines.push(Line::from(Span::styled(
            "  loading…",
            Style::default().fg(theme.text_muted),
        )));
    } else if let Some(err) = state.error.as_deref() {
        lines.push(Line::from(Span::styled(
            format!("  error: {err}"),
            Style::default().fg(theme.error),
        )));
    } else if let Some(body) = state.body_markdown.as_deref() {
        for line in body.lines() {
            lines.push(Line::from(format!("  {line}")));
        }
        if !state.citations.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Citations:",
                Style::default().fg(theme.text_muted),
            )));
            for c in &state.citations {
                let mid = c.message_id.as_deref().unwrap_or("?");
                lines.push(Line::from(format!(
                    "    msg={mid} field={} \"{}\"",
                    c.field, c.quote
                )));
            }
        }
        if let Some(when) = state.generated_at {
            let cache_label = if state.from_cache {
                "[cached]"
            } else {
                "[fresh]"
            };
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "  Generated {} {cache_label}",
                    when.format("%Y-%m-%d %H:%M")
                ),
                Style::default().fg(theme.text_muted),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  (no briefing available)",
            Style::default().fg(theme.text_muted),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Esc] close",
        Style::default().fg(theme.text_muted),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn short_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}…{}", &id[..4], &id[id.len() - 4..])
    } else {
        id.to_string()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::CitationRefData;
    use mxr_test_support::render_to_string;

    /// Stable ThreadId for snapshot determinism.
    fn fixed_thread_id() -> mxr_core::ThreadId {
        use std::str::FromStr;
        mxr_core::ThreadId::from_str("019e6f2a-1234-7000-8000-abcdef000001").unwrap()
    }

    #[test]
    fn loading_state_shows_loading_indicator() {
        let mut state = BriefingModalState::default();
        state.open_thread_loading(fixed_thread_id());
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("loading"));
        assert!(rendered.contains("Briefing: thread"));
        insta::assert_snapshot!("briefing_modal_loading", rendered);
    }

    #[test]
    fn populated_thread_briefing_renders_body_and_citations() {
        let mut state = BriefingModalState::default();
        state.open_thread_loading(fixed_thread_id());
        state.set_briefing(
            "## Thread snapshot\n\n- They asked about pricing.\n- We agreed on Postgres.".into(),
            vec![CitationRefData {
                message_id: Some("msg-99".into()),
                thread_id: Some("th-1".into()),
                field: "body".into(),
                quote: "We agreed on Postgres".into(),
            }],
            chrono::DateTime::parse_from_rfc3339("2026-05-13T12:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            false,
        );
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("Thread snapshot"));
        assert!(rendered.contains("agreed on Postgres"));
        assert!(rendered.contains("msg=msg-99"));
        assert!(rendered.contains("[fresh]"));
        insta::assert_snapshot!("briefing_modal_thread_populated", rendered);
    }

    #[test]
    fn cached_briefing_marks_from_cache() {
        let mut state = BriefingModalState::default();
        state.open_recipient_loading("alice@example.com".into());
        state.set_briefing("Recipient summary".into(), vec![], chrono::Utc::now(), true);
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("[cached]"));
        assert!(rendered.contains("Briefing: alice@example.com"));
    }

    #[test]
    fn error_state_renders_inline() {
        let mut state = BriefingModalState::default();
        state.open_thread_loading(mxr_core::ThreadId::new());
        state.set_error("LLM disabled".into());
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("error: LLM disabled"));
    }

    #[test]
    fn invisible_modal_renders_nothing() {
        let state = BriefingModalState::default();
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(
            !rendered.contains("Briefing"),
            "invisible state must not render the modal frame"
        );
    }
}
