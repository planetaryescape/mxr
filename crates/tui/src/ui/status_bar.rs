use mxr_core::types::{Envelope, MessageFlags};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    envelopes: &[Envelope],
    sync_status: Option<&str>,
    status_message: Option<&str>,
    theme: &crate::theme::Theme,
) {
    let total = envelopes.len();
    let unread_count = envelopes
        .iter()
        .filter(|e| !e.flags.contains(MessageFlags::READ))
        .count();
    let starred_count = envelopes
        .iter()
        .filter(|e| e.flags.contains(MessageFlags::STARRED))
        .count();

    let sync_part = match sync_status {
        Some(s) => format!("synced {s}"),
        None => "not synced".to_string(),
    };

    let status = if let Some(msg) = status_message {
        msg.to_string()
    } else {
        format!("=INBOX [Msgs:{total} New:{unread_count} Starred:{starred_count}]= {sync_part}")
    };

    let bar = Paragraph::new(status).style(Style::default().bg(theme.hint_bar_bg).fg(theme.text_primary));

    frame.render_widget(bar, area);
}

/// Format a sync status string for display.
pub fn format_sync_status(unread: usize, last_sync: Option<&str>) -> String {
    let sync_part = match last_sync {
        Some(s) => format!("synced {}", s),
        None => "not synced".to_string(),
    };
    format!("[INBOX] {} unread | {}", unread, sync_part)
}
