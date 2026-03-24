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

    let popup = centered_rect(62, 26, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(format!(" {} ", error.title))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.error))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = vec![
        Line::from(error.detail.clone()),
        Line::from(""),
        Line::from("[Enter] dismiss   [Esc] dismiss"),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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
        let error = ErrorModalState {
            title: "Mutation Failed".into(),
            detail: "Optimistic changes could not be applied.".into(),
        };
        let _ = draw;
        assert!(error.detail.contains("Optimistic"));
    }
}
