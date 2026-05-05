use crate::app::{AnalyticsState, AnalyticsView};
use mxr_core::types::{
    ResponseTimeDirection, StaleBallInCourt, StorageGroupBy,
};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, chunks[0], state, theme);
    draw_table(frame, chunks[1], state, theme);
    draw_footer(frame, chunks[2], state, theme);
}

fn draw_header(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    let mut tabs = Vec::new();
    for view in [
        AnalyticsView::Storage,
        AnalyticsView::StaleThreads,
        AnalyticsView::ContactAsymmetry,
        AnalyticsView::ResponseTime,
    ] {
        let label = view.label();
        let style = if state.view == view {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default().fg(theme.text_muted)
        };
        if !tabs.is_empty() {
            tabs.push(Span::styled(" | ", Style::default().fg(theme.text_muted)));
        }
        tabs.push(Span::styled(label, style));
    }
    let title = match state.view {
        AnalyticsView::Storage => format!("Storage  [group_by={}]", group_by_label(state.storage_group_by)),
        AnalyticsView::StaleThreads => format!(
            "Stale Threads  [perspective={}  older_than={}d  within={}d]",
            stale_perspective_label(state.stale_perspective),
            state.stale_older_than_days,
            state.stale_within_days,
        ),
        AnalyticsView::ContactAsymmetry => format!(
            "Contact Asymmetry  [min_inbound={}]",
            state.asymmetry_min_inbound
        ),
        AnalyticsView::ResponseTime => format!(
            "Response Time  [direction={}]",
            response_direction_label(state.response_time_direction)
        ),
    };
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(Line::from(tabs)), inner);
}

fn draw_table(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    if state.loading {
        let block = Block::default()
            .title(" Loading ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.text_muted));
        frame.render_widget(
            Paragraph::new("Computing analytics...").block(block).wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    if let Some(error) = state.error.as_deref() {
        let block = Block::default()
            .title(" Error ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.error));
        frame.render_widget(
            Paragraph::new(error.to_string()).block(block).wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    match state.view {
        AnalyticsView::Storage => draw_storage(frame, area, state, theme),
        AnalyticsView::StaleThreads => draw_stale(frame, area, state, theme),
        AnalyticsView::ContactAsymmetry => draw_asymmetry(frame, area, state, theme),
        AnalyticsView::ResponseTime => draw_response_time(frame, area, state, theme),
    }
}

fn draw_storage(frame: &mut Frame, area: Rect, state: &AnalyticsState, theme: &crate::theme::Theme) {
    if state.storage_rows.is_empty() {
        empty_state(
            frame,
            area,
            "No storage data yet. Sync first, then come back.",
            theme,
        );
        return;
    }

    let header = Row::new(vec!["Key", "Bytes", "Count"])
        .style(Style::default().fg(theme.text_muted).bold());
    let rows: Vec<Row> = state
        .storage_rows
        .iter()
        .map(|row| {
            Row::new(vec![
                row.key.clone(),
                format_bytes(row.bytes),
                row.count.to_string(),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(60),
        Constraint::Percentage(20),
        Constraint::Percentage(20),
    ];
    render_table(
        frame,
        area,
        " Storage ",
        header,
        rows,
        &widths,
        state.selected_index,
        theme,
    );
}

fn draw_stale(frame: &mut Frame, area: Rect, state: &AnalyticsState, theme: &crate::theme::Theme) {
    if state.stale_rows.is_empty() {
        empty_state(
            frame,
            area,
            "No stale threads in this window.",
            theme,
        );
        return;
    }
    let header = Row::new(vec!["Subject", "Counterparty", "Days Stale", "Latest"])
        .style(Style::default().fg(theme.text_muted).bold());
    let rows: Vec<Row> = state
        .stale_rows
        .iter()
        .map(|row| {
            Row::new(vec![
                row.latest_subject.clone(),
                row.counterparty_email.clone(),
                row.days_stale.to_string(),
                row.latest_date.format("%Y-%m-%d").to_string(),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(45),
        Constraint::Percentage(30),
        Constraint::Percentage(10),
        Constraint::Percentage(15),
    ];
    render_table(
        frame,
        area,
        " Stale Threads ",
        header,
        rows,
        &widths,
        state.selected_index,
        theme,
    );
}

fn draw_asymmetry(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    if state.asymmetry_rows.is_empty() {
        empty_state(
            frame,
            area,
            "No contacts crossed the inbound threshold yet.",
            theme,
        );
        return;
    }
    let header = Row::new(vec!["Email", "Inbound", "Outbound", "Asymmetry"])
        .style(Style::default().fg(theme.text_muted).bold());
    let rows: Vec<Row> = state
        .asymmetry_rows
        .iter()
        .map(|row| {
            Row::new(vec![
                row.email.clone(),
                row.total_inbound.to_string(),
                row.total_outbound.to_string(),
                format!("{:.2}", row.asymmetry),
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(50),
        Constraint::Percentage(15),
        Constraint::Percentage(15),
        Constraint::Percentage(20),
    ];
    render_table(
        frame,
        area,
        " Contact Asymmetry ",
        header,
        rows,
        &widths,
        state.selected_index,
        theme,
    );
}

fn draw_response_time(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    let Some(summary) = state.response_time.as_ref() else {
        empty_state(
            frame,
            area,
            "No response-time data yet. Sync first, then refresh.",
            theme,
        );
        return;
    };
    let lines = vec![
        Line::from(format!("Direction: {}", response_direction_label(summary.direction))),
        Line::from(format!("Sample count: {}", summary.sample_count)),
        Line::from(""),
        Line::from(format!(
            "Clock p50: {}",
            format_duration_seconds(summary.clock_p50_seconds)
        )),
        Line::from(format!(
            "Clock p90: {}",
            format_duration_seconds(summary.clock_p90_seconds)
        )),
        Line::from(""),
        Line::from(format!(
            "Business-hours p50: {}",
            summary
                .business_hours_p50_seconds
                .map(format_duration_seconds)
                .unwrap_or_else(|| "(not yet computed)".into())
        )),
        Line::from(format!(
            "Business-hours p90: {}",
            summary
                .business_hours_p90_seconds
                .map(format_duration_seconds)
                .unwrap_or_else(|| "(not yet computed)".into())
        )),
    ];
    let block = Block::default()
        .title(" Response Time ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: false }),
        area,
    );
}

fn render_table<'a>(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    header: Row<'a>,
    rows: Vec<Row<'a>>,
    widths: &[Constraint],
    selected_index: usize,
    theme: &crate::theme::Theme,
) {
    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(title.to_string())
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        )
        .row_highlight_style(theme.highlight_style())
        .highlight_symbol("> ");
    let mut state = TableState::default().with_selected(Some(selected_index));
    frame.render_stateful_widget(table, area, &mut state);
}

fn empty_state(frame: &mut Frame, area: Rect, message: &str, theme: &crate::theme::Theme) {
    let block = Block::default()
        .title(" No Data ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.text_muted));
    frame.render_widget(
        Paragraph::new(message)
            .style(Style::default().fg(theme.text_muted))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_footer(
    frame: &mut Frame,
    area: Rect,
    state: &AnalyticsState,
    theme: &crate::theme::Theme,
) {
    let _ = state;
    let hint = "Tab/Shift-Tab:switch view  j/k:select  r:refresh  Esc:mailbox";
    frame.render_widget(
        Paragraph::new(hint).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        area,
    );
}

fn group_by_label(group_by: StorageGroupBy) -> &'static str {
    match group_by {
        StorageGroupBy::Mimetype => "mimetype",
        StorageGroupBy::Sender => "sender",
        StorageGroupBy::Label => "label",
    }
}

fn stale_perspective_label(perspective: StaleBallInCourt) -> &'static str {
    match perspective {
        StaleBallInCourt::Mine => "mine",
        StaleBallInCourt::Theirs => "theirs",
    }
}

fn response_direction_label(direction: ResponseTimeDirection) -> &'static str {
    match direction {
        ResponseTimeDirection::IReplied => "i_replied",
        ResponseTimeDirection::TheyReplied => "they_replied",
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn format_duration_seconds(seconds: u32) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else if seconds < 86_400 {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    } else {
        format!("{}d{}h", seconds / 86_400, (seconds % 86_400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AnalyticsState;
    use mxr_core::types::{
        ContactAsymmetryRow, ResponseTimeDirection, ResponseTimeSummary, StorageBucket,
    };
    use mxr_test_support::render_to_string;
    use ratatui::layout::Rect;

    fn theme() -> crate::theme::Theme {
        crate::theme::Theme::default()
    }

    /// Phase 2.5 / Behavior 1: a populated storage view renders the
    /// table rows, the sums, and the title. Catches "renderer drops
    /// rows from the daemon response" regressions.
    #[test]
    fn storage_view_renders_rows_from_state() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        state.storage_rows = vec![StorageBucket {
            key: "hello@example.com".into(),
            bytes: 1024 * 1024 * 7,
            count: 42,
        }];
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(rendered.contains("Storage"), "header missing: {rendered}");
        assert!(
            rendered.contains("hello@example.com"),
            "row key missing: {rendered}"
        );
        assert!(rendered.contains("MiB"), "byte format missing: {rendered}");
        assert!(rendered.contains("42"), "row count missing: {rendered}");
    }

    /// Phase 2.5 / Behavior 3: an empty result set renders an
    /// empty-state message instead of a blank pane or a panic.
    #[test]
    fn storage_view_renders_empty_state_when_no_rows() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(
            rendered.contains("No storage data"),
            "empty-state message missing: {rendered}"
        );
    }

    /// Phase 2.5: response-time view shows the summary numbers and a
    /// sentinel when the business-hours percentile hasn't been
    /// computed (regression: would render "0s" and pretend it was
    /// real data).
    #[test]
    fn response_time_view_renders_clock_and_business_hours_status() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::ResponseTime;
        state.response_time = Some(ResponseTimeSummary {
            direction: ResponseTimeDirection::IReplied,
            sample_count: 17,
            clock_p50_seconds: 90,
            clock_p90_seconds: 3600,
            business_hours_p50_seconds: None,
            business_hours_p90_seconds: None,
        });
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(rendered.contains("Sample count: 17"));
        assert!(rendered.contains("Clock p50"));
        assert!(rendered.contains("1h0m"), "p90 should format duration: {rendered}");
        assert!(
            rendered.contains("not yet computed"),
            "business-hours sentinel missing: {rendered}"
        );
    }

    /// Phase 2.5: contact asymmetry renders rows with email + counts.
    #[test]
    fn asymmetry_view_renders_rows() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::ContactAsymmetry;
        state.asymmetry_rows = vec![ContactAsymmetryRow {
            email: "noreply@example.com".into(),
            display_name: None,
            total_inbound: 10,
            total_outbound: 0,
            asymmetry: 1.0,
            last_seen_at: chrono::Utc::now(),
        }];
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(rendered.contains("noreply@example.com"));
        assert!(rendered.contains("1.00"), "asymmetry value missing: {rendered}");
    }

    /// Phase 2.5 / Behavior 5: an error message replaces the table
    /// instead of leaving the view in a usable but empty state, so
    /// the user knows the request failed and isn't an empty result.
    #[test]
    fn loaded_error_replaces_table_with_error_block() {
        let mut state = AnalyticsState::default();
        state.view = AnalyticsView::Storage;
        state.error = Some("daemon unavailable".into());
        let rendered = render_to_string(120, 24, |frame| {
            draw(frame, Rect::new(0, 0, 120, 24), &state, &theme());
        });
        assert!(rendered.contains("Error"));
        assert!(rendered.contains("daemon unavailable"));
        assert!(
            !rendered.contains("No storage data"),
            "empty-state must not render alongside an error"
        );
    }
}
