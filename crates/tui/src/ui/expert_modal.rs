//! Slice 5.4 (C2.8 cont): renders the expert-finder modal —
//! ranks people who have answered similar questions before.

use super::centered_rect;
use crate::app::ExpertModalState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &ExpertModalState, theme: &crate::theme::Theme) {
    if !state.visible {
        return;
    }
    let popup = centered_rect(70, 60, area);
    frame.render_widget(Clear, popup);

    let title = match state.query.as_deref() {
        Some(q) if q.len() < 40 => format!(" Find expert: {q} "),
        Some(_) => " Find expert: <message body> ".to_string(),
        None => " Find expert ".to_string(),
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
    } else if state.experts.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No experts found in the local archive for this query.",
            Style::default().fg(theme.text_muted),
        )));
    } else {
        for (i, e) in state.experts.iter().enumerate() {
            let display = e.display_name.as_deref().unwrap_or(e.email.as_str());
            lines.push(Line::from(format!(
                "  {}. {display} — {} answer thread(s)",
                i + 1,
                e.answered_thread_count
            )));
            lines.push(Line::from(format!("       {}", e.reason)));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Esc] close",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::ExpertSuggestionData;
    use mxr_test_support::render_to_string;

    #[test]
    fn loading_state_shows_indicator() {
        let mut state = ExpertModalState::default();
        state.open_loading("kafka rebalance".into());
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("loading"));
        assert!(rendered.contains("Find expert: kafka rebalance"));
    }

    #[test]
    fn populated_state_renders_ranked_experts() {
        let mut state = ExpertModalState::default();
        state.open_loading("kafka".into());
        state.set_experts(vec![
            ExpertSuggestionData {
                email: "bob@example.com".into(),
                display_name: Some("Bob Operator".into()),
                reason: "answered in 3 prior thread(s)".into(),
                answered_thread_count: 3,
                evidence_msg_ids: vec!["msg-1".into(), "msg-2".into(), "msg-3".into()],
            },
            ExpertSuggestionData {
                email: "carol@example.com".into(),
                display_name: None,
                reason: "answered in 1 prior thread(s)".into(),
                answered_thread_count: 1,
                evidence_msg_ids: vec!["msg-4".into()],
            },
        ]);
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("Bob Operator"));
        assert!(rendered.contains("3 answer thread"));
        assert!(rendered.contains("carol@example.com"));
        insta::assert_snapshot!("expert_modal_populated", rendered);
    }

    #[test]
    fn empty_results_render_helpful_message() {
        let mut state = ExpertModalState::default();
        state.open_loading("obscure".into());
        state.set_experts(vec![]);
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(rendered.contains("No experts found"));
    }

    #[test]
    fn invisible_modal_renders_nothing() {
        let state = ExpertModalState::default();
        let rendered = render_to_string(80, 24, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 24),
                &state,
                &crate::theme::Theme::default(),
            );
        });
        assert!(!rendered.contains("Find expert"));
    }
}
