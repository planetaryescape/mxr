use crate::app::AttachmentPanelState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, panel: &AttachmentPanelState, theme: &crate::theme::Theme) {
    if !panel.visible {
        return;
    }

    let popup = centered_rect(60, 55, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered()
        .title(" Attachments ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.success))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(inner);

    let items: Vec<ListItem> = panel
        .attachments
        .iter()
        .enumerate()
        .map(|(index, attachment)| {
            let marker = if attachment.local_path.is_some() {
                "downloaded"
            } else {
                "remote"
            };
            let style = if index == panel.selected_index {
                theme.highlight_style()
            } else {
                Style::default()
            };
            ListItem::new(format!(
                " {}  {}  {}  ({})",
                index + 1,
                attachment.filename,
                attachment.mime_type,
                marker
            ))
            .style(style)
        })
        .collect();
    let list_height = chunks[0].height as usize;
    frame.render_widget(List::new(items), chunks[0]);

    if panel.attachments.len() > list_height {
        let mut scrollbar_state = ScrollbarState::new(panel.attachments.len().saturating_sub(list_height))
            .position(panel.selected_index);
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(theme.success)),
            chunks[0],
            &mut scrollbar_state,
        );
    }

    let footer = panel.status.clone().unwrap_or_else(|| {
        "Enter/o open  d download  j/k move  Esc close".to_string()
    });
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme.text_secondary)),
        chunks[1],
    );
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
