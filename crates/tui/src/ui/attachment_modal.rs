use crate::app::AttachmentPanelState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    panel: &AttachmentPanelState,
    theme: &crate::theme::Theme,
) {
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
            ListItem::new(format!(
                " {}  {}  {}  ({})",
                index + 1,
                attachment.filename,
                attachment.mime_type,
                marker
            ))
        })
        .collect();
    let list_height = chunks[0].height as usize;
    let list = List::new(items).highlight_style(theme.highlight_style());
    let mut list_state = ListState::default().with_selected(Some(panel.selected_index));
    frame.render_stateful_widget(list, chunks[0], &mut list_state);

    if panel.attachments.len() > list_height {
        let mut scrollbar_state =
            ScrollbarState::new(panel.attachments.len().saturating_sub(list_height))
                .position(panel.selected_index);
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(theme.success)),
            chunks[0],
            &mut scrollbar_state,
        );
    }

    let footer = panel
        .status
        .clone()
        .unwrap_or_else(|| "Enter/o open  d download  j/k move  Esc close".to_string());
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

#[cfg(test)]
mod tests {
    use super::draw;
    use crate::app::AttachmentPanelState;
    use mxr_core::id::{AttachmentId, MessageId};
    use mxr_core::types::{AttachmentDisposition, AttachmentMeta};
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn attachment(
        message_id: &MessageId,
        filename: &str,
        mime_type: &str,
        disposition: AttachmentDisposition,
        provider_id: &str,
    ) -> AttachmentMeta {
        AttachmentMeta {
            id: AttachmentId::new(),
            message_id: message_id.clone(),
            filename: filename.into(),
            mime_type: mime_type.into(),
            disposition,
            content_id: None,
            content_location: None,
            size_bytes: 1024,
            local_path: None,
            provider_id: provider_id.into(),
        }
    }

    #[test]
    fn selected_attachment_remains_visible_when_modal_list_scrolls() {
        let message_id = MessageId::new();
        let panel = AttachmentPanelState {
            visible: true,
            message_id: Some(message_id.clone()),
            attachments: vec![
                attachment(
                    &message_id,
                    "inline-1.png",
                    "image/png",
                    AttachmentDisposition::Inline,
                    "att-1",
                ),
                attachment(
                    &message_id,
                    "inline-2.png",
                    "image/png",
                    AttachmentDisposition::Inline,
                    "att-2",
                ),
                attachment(
                    &message_id,
                    "inline-3.png",
                    "image/png",
                    AttachmentDisposition::Inline,
                    "att-3",
                ),
                attachment(
                    &message_id,
                    "inline-4.png",
                    "image/png",
                    AttachmentDisposition::Inline,
                    "att-4",
                ),
                attachment(
                    &message_id,
                    "inline-5.png",
                    "image/png",
                    AttachmentDisposition::Inline,
                    "att-5",
                ),
                attachment(
                    &message_id,
                    "budget.xlsx",
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                    AttachmentDisposition::Attachment,
                    "att-6",
                ),
            ],
            selected_index: 5,
            status: None,
        };

        let snapshot = render_to_string(80, 12, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 80, 12),
                &panel,
                &crate::theme::Theme::default(),
            );
        });

        assert!(snapshot.contains("budget.xlsx"));
    }
}
