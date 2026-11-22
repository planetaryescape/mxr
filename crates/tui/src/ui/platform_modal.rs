use super::centered_rect;
use crate::app::PlatformModalState;
use crate::theme::Theme;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &PlatformModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let modal_area = centered_rect(76, 62, area);
    Clear.render(modal_area, frame.buffer_mut());

    let title = format!(" {} · Esc close ", state.title);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area).inner(Margin::new(1, 1));
    frame.render_widget(block, modal_area);

    if let Some(error) = &state.error {
        let paragraph = Paragraph::new(error.clone())
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner);
        return;
    }

    let text = state.body.as_deref().unwrap_or_default();
    let style = if state.loading {
        Style::default().fg(theme.text_muted)
    } else {
        Style::default().fg(theme.text_primary)
    };
    let paragraph = Paragraph::new(text.to_string())
        .style(style)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}
