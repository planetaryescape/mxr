use crate::app::PendingUnsubscribeConfirm;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    pending: Option<&PendingUnsubscribeConfirm>,
    theme: &crate::theme::Theme,
) {
    let Some(pending) = pending else {
        return;
    };

    let popup = centered_rect(66, 55, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Unsubscribe ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let archive_count = pending.archive_message_ids.len();
    let lines = vec![
        Line::from(vec![
            Span::styled("Sender: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                &pending.sender_email,
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(vec![
            Span::styled("Method: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                &pending.method_label,
                Style::default().fg(theme.text_primary),
            ),
        ]),
        Line::from(""),
        Line::from("Choose what to do:"),
        Line::from(vec![
            Span::styled("[Enter] ", Style::default().fg(theme.accent).bold()),
            Span::raw("unsubscribe only"),
        ]),
        Line::from(vec![
            Span::styled("[a] ", Style::default().fg(theme.accent).bold()),
            Span::raw(format!(
                "unsubscribe + archive all from this sender ({archive_count} messages)"
            )),
        ]),
        Line::from(vec![
            Span::styled("[Esc] ", Style::default().fg(theme.text_muted).bold()),
            Span::raw("cancel"),
        ]),
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
