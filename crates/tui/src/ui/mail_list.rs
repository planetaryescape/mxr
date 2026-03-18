use crate::app::ActivePane;
use mxr_core::types::{Envelope, MessageFlags};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    envelopes: &[Envelope],
    selected_index: usize,
    scroll_offset: usize,
    active_pane: &ActivePane,
) {
    let is_focused = *active_pane == ActivePane::MailList;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let visible_height = area.height.saturating_sub(2) as usize;
    let items: Vec<ListItem> = envelopes
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(i, env)| {
            let star = if env.flags.contains(MessageFlags::STARRED) {
                "★"
            } else {
                " "
            };
            let is_unread = !env.flags.contains(MessageFlags::READ);
            let from = env.from.name.as_deref().unwrap_or(&env.from.email);
            let from_truncated: String = from.chars().take(15).collect();
            let date = env.date.format("%b %d").to_string();

            let line = format!(
                " {} {:<15} {:<40} {}",
                star,
                from_truncated,
                truncate(&env.subject, 40),
                date,
            );

            let style = if i == selected_index {
                Style::default().bg(Color::DarkGray).bold()
            } else if is_unread {
                Style::default().bold()
            } else {
                Style::default()
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Messages ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    frame.render_widget(list, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        format!("{:<width$}", s, width = max)
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
