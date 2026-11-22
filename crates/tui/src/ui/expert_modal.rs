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
