use ratatui::prelude::*;
use ratatui::widgets::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarState {
    pub mailbox_name: String,
    pub total_count: usize,
    pub unread_count: usize,
    pub starred_count: usize,
    pub sync_status: Option<String>,
    pub status_message: Option<String>,
}

pub fn draw(frame: &mut Frame, area: Rect, state: &StatusBarState, theme: &crate::theme::Theme) {
    let sync_part = state.sync_status.as_deref().unwrap_or("not synced");

    let status = if let Some(msg) = state.status_message.as_deref() {
        msg.to_string()
    } else {
        format!(
            "={} [Msgs:{} New:{} Starred:{}]= {}",
            state.mailbox_name,
            state.total_count,
            state.unread_count,
            state.starred_count,
            sync_part
        )
    };

    let bar = Paragraph::new(status).style(
        Style::default()
            .bg(theme.hint_bar_bg)
            .fg(theme.text_primary),
    );

    frame.render_widget(bar, area);
}

/// Format a sync status string for display.
pub fn format_sync_status(unread: usize, sync_status: Option<&str>) -> String {
    let sync_part = sync_status.unwrap_or("not synced");
    format!("[INBOX] {} unread | {}", unread, sync_part)
}
