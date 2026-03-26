use crate::mxr_tui::app::{ActivePane, SubscriptionEntry};
use crate::mxr_tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct SubscriptionsPageView<'a> {
    pub entries: &'a [SubscriptionEntry],
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub active_pane: &'a ActivePane,
    pub preview_blocks: &'a [crate::mxr_tui::ui::message_view::ThreadMessageBlock],
    pub message_scroll_offset: u16,
    pub html_images:
        &'a mut std::collections::HashMap<
            crate::mxr_core::MessageId,
            std::collections::HashMap<
                String,
                crate::mxr_tui::terminal_images::HtmlImageEntry,
            >,
        >,
}

pub fn draw(frame: &mut Frame, area: Rect, view: &mut SubscriptionsPageView<'_>, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    let is_focused = *view.active_pane == ActivePane::MailList;
    let border_style = theme.border_style(is_focused);

    let items = view
        .entries
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
                .title(format!(" Subscriptions ({}) ", view.entries.len()))
                .border_type(BorderType::Rounded)
                .border_style(border_style),
        )
        .highlight_style(theme.highlight_style());

    let mut state = ListState::default()
        .with_selected((!view.entries.is_empty()).then_some(view.selected_index))
        .with_offset(view.scroll_offset);
    frame.render_stateful_widget(list, chunks[0], &mut state);

    crate::mxr_tui::ui::message_view::draw(
        frame,
        chunks[1],
        view.preview_blocks,
        view.message_scroll_offset,
        view.active_pane,
        theme,
        view.html_images,
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
