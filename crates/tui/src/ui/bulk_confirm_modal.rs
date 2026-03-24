use crate::mxr_tui::app::PendingBulkConfirm;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    pending: Option<&PendingBulkConfirm>,
    theme: &crate::mxr_tui::theme::Theme,
) {
    let Some(pending) = pending else {
        return;
    };

    let popup = centered_rect(60, 24, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(format!(" {} ", pending.title))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = vec![
        Line::from(pending.detail.clone()),
        Line::from(""),
        Line::from("[Enter] confirm   [y] confirm   [Esc] cancel"),
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
    use crate::mxr_protocol::{MutationCommand, Request};
    use crate::mxr_tui::app::{MutationEffect, PendingBulkConfirm};

    #[test]
    fn pending_bulk_confirm_is_constructible() {
        let pending = PendingBulkConfirm {
            title: "Archive messages".into(),
            detail: "You are about to archive these 15 messages.".into(),
            request: Request::Mutation(MutationCommand::Archive {
                message_ids: vec![],
            }),
            effect: MutationEffect::RefreshList,
            optimistic_effect: None,
            status_message: "Archiving...".into(),
        };
        let _ = draw;
        assert!(pending.detail.contains("15"));
    }
}
