use crate::app::ActivePane;
use mxr_core::types::Label;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, labels: &[Label], active_pane: &ActivePane) {
    let is_focused = *active_pane == ActivePane::Sidebar;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = labels
        .iter()
        .map(|label| {
            let count_str = if label.unread_count > 0 {
                format!(" ({})", label.unread_count)
            } else {
                String::new()
            };
            ListItem::new(format!("{}{}", label.name, count_str))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Labels ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    frame.render_widget(list, area);
}
