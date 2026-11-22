use crate::ui::search_page::search_mode_label;
use mxr_core::SearchMode;
use ratatui::prelude::*;
use ratatui::widgets::*;
use throbber_widgets_tui::{Throbber, ThrobberState, BRAILLE_SIX};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarState {
    pub mailbox_name: String,
    pub total_count: Option<usize>,
    pub unread_count: usize,
    pub starred_count: usize,
    pub body_status: Option<String>,
    pub sync_status: Option<String>,
    pub compact_sync_status: Option<String>,
    pub sync_warning: bool,
    pub feature_health_status: Option<String>,
    pub status_message: Option<String>,
    pub pending_mutation_count: usize,
    pub pending_mutation_status: Option<String>,
    pub mutation_batch_total: usize,
    pub busy: bool,
    pub search_mode: Option<SearchMode>,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &StatusBarState,
    spinner: Option<&ThrobberState>,
    theme: &crate::theme::Theme,
) {
    let sync_part = state.sync_status.as_deref().unwrap_or("not synced");

    let mut status: Vec<Span<'static>> = if state
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
        if let Some(mode) = state.search_mode {
            tail.push_str(" | [mode: ");
            tail.push_str(search_mode_label(mode));
            tail.push(']');
        }
        vec![Span::raw(counts), sync, Span::raw(tail)]
    };

    let demo_width = if mxr_config::is_demo_instance() { 6 } else { 0 };
    let spinner_width = usize::from(state.busy && spinner.is_some()) * 2;
    let available_width = area.width.saturating_sub(demo_width) as usize;
    if state.sync_warning
        && status.len() == 3
        && Line::from(status.clone()).width() + spinner_width > available_width
    {
        if let Some(compact) = state.compact_sync_status.as_deref() {
            status[1] = Span::styled(
                format!("! {compact}"),
                Style::default()
                    .fg(theme.warning)
                    .add_modifier(Modifier::BOLD),
            );

            if Line::from(status.clone()).width() + spinner_width > available_width {
                let total = state
                    .total_count
                    .map_or_else(|| "?".to_string(), |count| count.to_string());
                status[0] = Span::raw(format!(
                    "={} {}/{}= ",
                    state.mailbox_name, state.unread_count, total
                ));
                status[2] = Span::raw(String::new());
            }

            if Line::from(status.clone()).width() + spinner_width > available_width {
                status[0] = Span::raw(String::new());
            }
        }
    }

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
            compact_sync_status: None,
            sync_warning: false,
            feature_health_status: None,
            status_message: None,
            pending_mutation_count: pending,
            pending_mutation_status: None,
            mutation_batch_total: total,
            busy: true,
            search_mode: None,
        }
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

    #[test]
    fn a_narrow_warning_keeps_the_account_and_retry_time_visible() {
        let mut warning = state(0, 0);
        warning.mailbox_name = "ALL MAIL".into();
        warning.total_count = Some(166);
        warning.unread_count = 77;
        warning.starred_count = 7;
        warning.body_status = Some("Latest mail 42m ago".into());
        warning.sync_status =
            Some("Latest mail 25m ago · Can't reach alex@demo.mxr.local, retrying 14:27".into());
        warning.compact_sync_status = Some("alex@demo.mxr.local: retrying 14:27".into());
        warning.feature_health_status = Some("3 feature warnings".into());
        warning.sync_warning = true;
        let theme = crate::theme::Theme::default();
        let spinner = ThrobberState::default();
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(74, 1)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), &warning, Some(&spinner), &theme))
            .unwrap();
        let line = text(terminal.backend().buffer());
        assert!(
            line.contains("! alex@demo.mxr.local: retrying 14:27"),
            "{line}"
        );
        assert!(line.contains("=ALL MAIL 77/166="), "{line}");
        assert!(!line.contains("Latest mail"), "{line}");
        assert!(!line.contains("feature warnings"), "{line}");
    }

    fn rendered_at_width(state: &StatusBarState, width: u16) -> String {
        let theme = crate::theme::Theme::default();
        let spinner = ThrobberState::default();
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), state, Some(&spinner), &theme))
            .unwrap();
        text(terminal.backend().buffer())
    }

    #[test]
    fn an_active_search_mode_is_named_after_the_counts() {
        let mut searching = state(0, 0);
        searching.busy = false;
        searching.total_count = Some(42);
        searching.search_mode = Some(SearchMode::Hybrid);
        let line = rendered_at_width(&searching, 120);
        let counts = line.find("[Msgs:42").expect("counts");
        let mode = line.find("[mode: hybrid]").expect("mode chip");
        assert!(counts < mode, "{line}");
    }

    #[test]
    fn the_mode_chip_leaves_a_sync_warning_visible_at_120_columns() {
        let mut warning = state(0, 0);
        warning.busy = false;
        warning.mailbox_name = "ALL MAIL".into();
        warning.total_count = Some(4_242);
        warning.unread_count = 77;
        warning.starred_count = 7;
        warning.body_status = Some("Latest mail 42m ago".into());
        warning.sync_status =
            Some("Latest mail 25m ago · Gmail paused: rate limited, retrying 09:50".into());
        warning.compact_sync_status = Some("Gmail: retrying 09:50".into());
        warning.sync_warning = true;
        warning.search_mode = Some(SearchMode::Semantic);
        let line = rendered_at_width(&warning, 120);
        assert!(line.contains("[mode: semantic]"), "{line}");
        assert!(
            line.contains("retrying 09:50"),
            "the sync warning must survive the mode chip: {line}"
        );
    }
}
