use crate::app::{ActivePane, SubscriptionEntry};
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    entries: &[SubscriptionEntry],
    selected_index: usize,
    scroll_offset: usize,
    active_pane: &ActivePane,
    preview_blocks: &[crate::ui::message_view::ThreadMessageBlock],
    message_scroll_offset: u16,
    theme: &Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    let is_focused = *active_pane == ActivePane::MailList;
    let border_style = theme.border_style(is_focused);

    let items = entries
        .iter()
        .map(|entry| {
            let label = entry
                .summary
                .sender_name
                .as_deref()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(entry.summary.sender_email.as_str());
            let count = entry.summary.message_count.to_string();
            let width = chunks[0].width.saturating_sub(4) as usize;
            let name_part = format!(
                "  {}",
                truncate(label, width.saturating_sub(count.len() + 1))
            );
            let padding = width.saturating_sub(name_part.len() + count.len());
            ListItem::new(format!("{}{}{}", name_part, " ".repeat(padding), count))
        })
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(
            Block::bordered()
                .title(format!(" Subscriptions ({}) ", entries.len()))
                .border_type(BorderType::Rounded)
                .border_style(border_style),
        )
        .highlight_style(theme.highlight_style());

    let mut state = ListState::default()
        .with_selected((!entries.is_empty()).then_some(selected_index))
        .with_offset(scroll_offset);
    frame.render_stateful_widget(list, chunks[0], &mut state);

    crate::ui::message_view::draw(
        frame,
        chunks[1],
        preview_blocks,
        message_scroll_offset,
        active_pane,
        theme,
    );
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    value
        .chars()
        .take(width.saturating_sub(3))
        .collect::<String>()
        + "..."
}
