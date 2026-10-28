use crate::app::{ActivePane, SubscriptionEntry};
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub struct SubscriptionsPageView<'a> {
    pub entries: &'a [SubscriptionEntry],
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub active_pane: &'a ActivePane,
    pub preview_blocks: &'a [crate::ui::message_view::ThreadMessageBlock],
    pub message_scroll_offset: u16,
    pub html_images: &'a mut std::collections::HashMap<
        mxr_core::MessageId,
        std::collections::HashMap<String, crate::terminal_images::HtmlImageEntry>,
    >,
    pub now: chrono::DateTime<chrono::Utc>,
    /// The zone times show in: `Local` in the app, a fixed one in tests.
    pub zone: crate::ui::received_label::DisplayZone,
    pub account_marks: &'a crate::ui::account_mark::AccountMarks,
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
            let chip = view
                .account_marks
                .row_cell(&entry.summary.account_id, theme);
            let chip_width = view.account_marks.column_width();
            let width = (chunks[0].width.saturating_sub(4) as usize).saturating_sub(chip_width);
            let name_part = format!(
                "  {}",
                truncate(label, width.saturating_sub(count.len() + 1))
            );
            let padding = width.saturating_sub(name_part.len() + count.len());
            let mut spans = vec![Span::raw(format!("{}{}", name_part, " ".repeat(padding)))];
            spans.extend(chip.unwrap_or_default());
            spans.push(Span::raw(count));
            ListItem::new(Line::from(spans))
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

    crate::ui::message_view::draw(
        frame,
        chunks[1],
        view.preview_blocks,
        crate::ui::message_view::DrawOptions {
            account_marks: view.account_marks,
            context: Vec::new(),
            summary: None,
            scroll_offset: view.message_scroll_offset,
            active_pane: view.active_pane,
            theme,
            html_images: view.html_images,
            now: view.now,
            zone: view.zone,
        },
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
