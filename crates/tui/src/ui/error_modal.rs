use crate::mxr_tui::app::ErrorModalState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    error: Option<&ErrorModalState>,
    theme: &crate::mxr_tui::theme::Theme,
) {
    let Some(error) = error else {
        return;
    };

    let popup = centered_rect(72, 58, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(format!(" {} ", error.title))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.error))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let detail_lines = error
        .detail
        .lines()
        .map(|line| Line::from(line.to_string()))
        .collect::<Vec<_>>();
    let paragraph = Paragraph::new(detail_lines.clone())
        .scroll((error.scroll_offset as u16, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, chunks[0]);

    let footer = Line::from("[j/k] scroll   [Ctrl-d/u] page   [Enter] dismiss   [Esc] dismiss");
    frame.render_widget(footer, chunks[1]);

    let body_height = chunks[0].height as usize;
    if detail_lines.len() > body_height {
        let mut scrollbar_state =
            ScrollbarState::new(detail_lines.len().saturating_sub(body_height))
                .position(error.scroll_offset);
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            chunks[0],
            &mut scrollbar_state,
        );
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::draw;
    use crate::mxr_tui::app::ErrorModalState;

    #[test]
    fn error_modal_state_is_constructible() {
        let error = ErrorModalState::new(
            "Mutation Failed",
            "Optimistic changes could not be applied.",
        );
        let _ = draw;
        assert!(error.detail.contains("Optimistic"));
        assert_eq!(error.scroll_offset, 0);
    }
}
