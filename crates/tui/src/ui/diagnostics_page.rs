use crate::app::DiagnosticsPageState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &DiagnosticsPageState, theme: &crate::theme::Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(9),
            Constraint::Length(8),
            Constraint::Min(8),
        ])
        .split(area);

    let status_lines = vec![
        Line::from(format!(
            "Uptime: {}s",
            state.uptime_secs.unwrap_or_default()
        )),
        Line::from(format!(
            "Daemon PID: {}",
            state
                .daemon_pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )),
        Line::from(format!(
            "Accounts: {}",
            if state.accounts.is_empty() {
                "unknown".to_string()
            } else {
                state.accounts.join(", ")
            }
        )),
        Line::from(format!(
            "Messages: {}",
            state.total_messages.unwrap_or_default()
        )),
        Line::from(format!(
            "Healthy: {}",
            state.doctor.as_ref().map(|r| r.healthy).unwrap_or(false)
        )),
    ];
    frame.render_widget(
        Paragraph::new(status_lines).block(
            Block::default()
                .title(" Status / Doctor ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        chunks[0],
    );

    let sync_lines = if state.sync_statuses.is_empty() {
        vec![Line::from("No sync accounts")]
    } else {
        state
            .sync_statuses
            .iter()
            .flat_map(|sync| {
                vec![
                    Line::from(format!(
                        "{} healthy={} in_progress={} last_success={}",
                        sync.account_name,
                        sync.healthy,
                        sync.sync_in_progress,
                        sync.last_success_at.as_deref().unwrap_or("never"),
                    )),
                    Line::from(format!(
                        "  error={} backoff={} cursor={}",
                        sync.last_error.as_deref().unwrap_or("-"),
                        sync.backoff_until.as_deref().unwrap_or("-"),
                        sync.current_cursor_summary.as_deref().unwrap_or("-"),
                    )),
                ]
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(sync_lines)
            .block(
                Block::default()
                    .title(" Sync Health ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent)),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
    );

    let event_lines = if state.events.is_empty() {
        vec![Line::from("No events")]
    } else {
        state
            .events
            .iter()
            .map(|event| {
                Line::from(format!(
                    "{} [{}:{}] {}",
                    event.timestamp, event.level, event.category, event.summary
                ))
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(event_lines)
            .block(
                Block::default()
                    .title(" Recent Events ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.warning)),
            )
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    let log_lines = if state.logs.is_empty() {
        vec![Line::from("No logs")]
    } else {
        state
            .logs
            .iter()
            .map(|line| Line::from(line.clone()))
            .collect()
    };
    frame.render_widget(
        Paragraph::new(log_lines)
            .block(
                Block::default()
                    .title(" Recent Logs ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.success)),
            )
            .wrap(Wrap { trim: false }),
        chunks[3],
    );
}
