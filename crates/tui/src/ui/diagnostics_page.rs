use crate::app::DiagnosticsPageState;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &DiagnosticsPageState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Min(8),
        ])
        .split(area);

    let status_lines = vec![
        Line::from(format!(
            "Uptime: {}s",
            state.uptime_secs.unwrap_or_default()
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
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        chunks[0],
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
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .wrap(Wrap { trim: false }),
        chunks[1],
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
                    .border_style(Style::default().fg(Color::Green)),
            )
            .wrap(Wrap { trim: false }),
        chunks[2],
    );
}
