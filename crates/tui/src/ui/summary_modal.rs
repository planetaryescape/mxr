use super::centered_rect;
use crate::app::ThreadSummaryModalState;
use crate::theme::Theme;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;

const MODAL_WIDTH_PERCENT: u16 = 70;
const MODAL_HEIGHT_PERCENT: u16 = 50;

pub fn draw(frame: &mut Frame, area: Rect, state: &ThreadSummaryModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let modal_area = centered_rect(MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT, area);
    Clear.render(modal_area, frame.buffer_mut());

    let title = match &state.model {
        Some(model) => format!(" Thread summary · {model} · Esc close "),
        None => " Thread summary · Esc close ".to_string(),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area).inner(Margin::new(1, 1));
    frame.render_widget(block, modal_area);

    if let Some(error) = &state.error {
        let paragraph = Paragraph::new(format!("Failed to summarize: {error}"))
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner);
        return;
    }

    if state.loading {
        let paragraph = Paragraph::new("Summarizing thread...")
            .style(Style::default().fg(theme.text_muted))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    if let Some(summary) = &state.summary {
        let paragraph = Paragraph::new(summary.clone())
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner);
    }
}
