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
    pub feature_health_status: Option<String>,
    pub status_message: Option<String>,
    pub pending_mutation_count: usize,
    pub pending_mutation_status: Option<String>,
}

pub fn draw(frame: &mut Frame, area: Rect, state: &StatusBarState, theme: &crate::theme::Theme) {
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
        if let Some(feature_health_status) = state.feature_health_status.as_deref() {
            status.push_str(" | ");
            status.push_str(feature_health_status);
        }
        status
    };

    // Reserve room on the right for a DEMO chip when the process is bound to
    // the demo instance — this way a recording always shows whether the user
    // is on demo data or their real inbox.
    if mxr_config::is_demo_instance() {
        let chip = " DEMO ";
        let chip_width = chip.len() as u16;
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(chip_width)])
            .split(area);
        let bar = Paragraph::new(status).style(
            Style::default()
                .bg(theme.hint_bar_bg)
                .fg(theme.text_primary),
        );
        let chip_widget = Paragraph::new(chip).alignment(Alignment::Center).style(
            Style::default()
                .bg(theme.warning)
                .fg(theme.modal_bg)
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(bar, split[0]);
        frame.render_widget(chip_widget, split[1]);
        return;
    }

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
