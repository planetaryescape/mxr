use ratatui::prelude::*;
use ratatui::widgets::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarState {
    pub mailbox_name: String,
    pub total_count: usize,
    pub unread_count: usize,
    pub starred_count: usize,
    pub body_status: Option<String>,
    pub sync_status: Option<String>,
    pub status_message: Option<String>,
    pub pending_mutation_count: usize,
    pub pending_mutation_status: Option<String>,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &StatusBarState,
    theme: &crate::mxr_tui::theme::Theme,
) {
    let sync_part = state.sync_status.as_deref().unwrap_or("not synced");

    let status = if state
        .status_message
        .as_deref()
        .is_some_and(|message| message.starts_with("Error:"))
    {
        state.status_message.clone().unwrap_or_default()
    } else if state.pending_mutation_count > 0 {
        let message = state
            .pending_mutation_status
            .as_deref()
            .or(state.status_message.as_deref())
            .unwrap_or("Working...");
        format!("[pending:{}] {}", state.pending_mutation_count, message)
    } else if let Some(msg) = state.status_message.as_deref() {
        msg.to_string()
    } else {
        let mut status = format!(
            "={} [Msgs:{} New:{} Starred:{}]= {}",
            state.mailbox_name,
            state.total_count,
            state.unread_count,
            state.starred_count,
            sync_part
        );
        if let Some(body_status) = state.body_status.as_deref() {
            status.push_str(" | ");
            status.push_str(body_status);
        }
        status
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
