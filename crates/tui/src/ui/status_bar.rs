use ratatui::prelude::*;
use ratatui::widgets::*;
use throbber_widgets_tui::{Throbber, ThrobberState, BRAILLE_SIX};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarState {
    pub mailbox_name: String,
    /// `None` when the daemon has not managed a real reading yet — a degraded
    /// status snapshot carries no count, and the mailbox length behind it is
    /// capped, so printing either would be a number the user could act on
    /// wrongly.
    pub total_count: Option<usize>,
    pub unread_count: usize,
    pub starred_count: usize,
    pub body_status: Option<String>,
    pub sync_status: Option<String>,
    /// The sync words are a warning (failing, paused or stale sync): drawn
    /// in the warning colour so they stand out from the counts.
    pub sync_warning: bool,
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

    let status: Vec<Span<'static>> = if state
        .status_message
        .as_deref()
        .is_some_and(|message| message.starts_with("Error:"))
    {
        vec![Span::raw(state.status_message.clone().unwrap_or_default())]
    } else if state.pending_mutation_count > 0 {
        let message = state
            .pending_mutation_status
            .as_deref()
            .or(state.status_message.as_deref())
            .unwrap_or("Working...");
        vec![Span::raw(format!(
            "{} {}",
            pending_progress_prefix(state),
            message
        ))]
    } else if let Some(msg) = state.status_message.as_deref() {
        vec![Span::raw(msg.to_string())]
    } else {
        let counts = format!(
            "={} [Msgs:{} New:{} Starred:{}]= ",
            state.mailbox_name,
            state
                .total_count
                .map_or_else(|| "?".to_string(), |count| count.to_string()),
            state.unread_count,
            state.starred_count,
        );
        let sync = if state.sync_warning {
            Span::styled(
                format!("! {sync_part}"),
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::raw(sync_part.to_string())
        };
        let mut tail = String::new();
        if let Some(body_status) = state.body_status.as_deref() {
            tail.push_str(" | ");
            tail.push_str(body_status);
        }
        if let Some(feature_health_status) = state.feature_health_status.as_deref() {
            tail.push_str(" | ");
            tail.push_str(feature_health_status);
        }
        vec![Span::raw(counts), sync, Span::raw(tail)]
    };

    // Prepend an animated spinner while background work is in flight so
    // the user can tell the daemon is busy even without a pane-local
    // loading indicator.
    let line = match spinner.filter(|_| state.busy) {
        Some(spinner) => {
            let mut spans = vec![
                Throbber::default()
                    .throbber_set(BRAILLE_SIX)
                    .throbber_style(Style::default().fg(theme.accent))
                    .to_symbol_span(spinner),
                Span::raw(" "),
            ];
            spans.extend(status);
            Line::from(spans)
        }
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
            total_count: Some(0),
            unread_count: 0,
            starred_count: 0,
            body_status: None,
            sync_status: None,
            sync_warning: false,
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

    fn rendered(state: &StatusBarState) -> ratatui::buffer::Buffer {
        let theme = crate::theme::Theme::default();
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(110, 1)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), state, None, &theme))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn text(buffer: &ratatui::buffer::Buffer) -> String {
        buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn the_bar_shows_the_newest_mail_and_sync_calmly_when_all_is_well() {
        let mut calm = state(0, 0);
        calm.busy = false;
        calm.sync_status = Some("Latest mail 5m ago · synced 1m ago".into());
        let buffer = rendered(&calm);
        assert!(
            text(&buffer).contains("Latest mail 5m ago · synced 1m ago"),
            "{}",
            text(&buffer)
        );
        assert!(!text(&buffer).contains('!'));
    }

    #[test]
    fn a_sync_warning_is_marked_and_drawn_in_the_warning_colour() {
        let theme = crate::theme::Theme::default();
        let mut warning = state(0, 0);
        warning.busy = false;
        warning.sync_status =
            Some("Latest mail 2h ago · Gmail paused: rate limited, retrying 09:50".into());
        warning.sync_warning = true;
        let buffer = rendered(&warning);
        let line = text(&buffer);
        let at = line
            .find("! Latest mail 2h ago · Gmail paused: rate limited, retrying 09:50")
            .unwrap_or_else(|| panic!("warning missing: {line}"));
        let column = u16::try_from(line[..at].chars().count()).unwrap();
        assert_eq!(buffer[(column, 0)].fg, theme.warning);
    }
}
