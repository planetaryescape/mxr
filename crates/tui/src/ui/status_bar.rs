use ratatui::prelude::*;
use ratatui::widgets::*;
use throbber_widgets_tui::{Throbber, ThrobberState, BRAILLE_SIX};

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
    /// Peak size of the current mutation batch; > 1 switches the pending
    /// prefix to "n/m" bulk progress.
    pub mutation_batch_total: usize,
    /// True while any replaceable request or queued mutation is in
    /// flight — renders the spinner at the left edge of the bar.
    pub busy: bool,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &StatusBarState,
    spinner: Option<&ThrobberState>,
    theme: &crate::theme::Theme,
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
        format!("{} {}", pending_progress_prefix(state), message)
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

    // Prepend an animated spinner while background work is in flight so
    // the user can tell the daemon is busy even without a pane-local
    // loading indicator.
    let line = match spinner.filter(|_| state.busy) {
        Some(spinner) => Line::from(vec![
            Throbber::default()
                .throbber_set(BRAILLE_SIX)
                .throbber_style(Style::default().fg(theme.accent))
                .to_symbol_span(spinner),
            Span::raw(" "),
            Span::raw(status),
        ]),
        None => Line::from(status),
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
        let bar = Paragraph::new(line).style(
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

    let bar = Paragraph::new(line).style(
        Style::default()
            .bg(theme.hint_bar_bg)
            .fg(theme.text_primary),
    );

    frame.render_widget(bar, area);
}

/// Prefix for the in-flight mutation status. Bulk batches (more than one
/// queued mutation) render "n/m" completed-of-total progress; a single
/// pending mutation keeps the existing "[pending:1]" form.
fn pending_progress_prefix(state: &StatusBarState) -> String {
    if state.mutation_batch_total > 1 {
        let done = state
            .mutation_batch_total
            .saturating_sub(state.pending_mutation_count);
        format!("[{}/{}]", done, state.mutation_batch_total)
    } else {
        format!("[pending:{}]", state.pending_mutation_count)
    }
}

/// Format a sync status string for display.
pub fn format_sync_status(unread: usize, sync_status: Option<&str>) -> String {
    let sync_part = sync_status.unwrap_or("not synced");
    format!("[INBOX] {unread} unread | {sync_part}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(pending: usize, total: usize) -> StatusBarState {
        StatusBarState {
            mailbox_name: "INBOX".into(),
            total_count: 0,
            unread_count: 0,
            starred_count: 0,
            body_status: None,
            sync_status: None,
            feature_health_status: None,
            status_message: None,
            pending_mutation_count: pending,
            pending_mutation_status: None,
            mutation_batch_total: total,
            busy: true,
        }
    }

    #[test]
    fn bulk_batches_render_done_of_total_progress() {
        assert_eq!(pending_progress_prefix(&state(5, 5)), "[0/5]");
        assert_eq!(pending_progress_prefix(&state(2, 5)), "[3/5]");
    }

    #[test]
    fn single_pending_mutation_keeps_pending_prefix() {
        assert_eq!(pending_progress_prefix(&state(1, 1)), "[pending:1]");
    }
}
