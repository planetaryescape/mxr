use crate::app::DiagnosticsPageState;
use mxr_protocol::{AccountSyncStatus, DoctorDataStats, EventLogEntry};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    theme: &crate::theme::Theme,
) {
    let loading = state.pending_requests > 0;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),
            Constraint::Length(7),
            Constraint::Min(4),
            Constraint::Min(4),
        ])
        .split(area);
    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    let doctor = state.doctor.as_ref();
    let empty_stats = DoctorDataStats::default();
    let data_stats = doctor
        .map(|report| &report.data_stats)
        .unwrap_or(&empty_stats);
    let total_messages = state
        .total_messages
        .filter(|count| *count > 0 || data_stats.messages == 0)
        .unwrap_or(data_stats.messages);
    let health_text = doctor
        .map(|report| report.health_class.as_str().to_string())
        .unwrap_or_else(|| {
            if loading {
                "loading".to_string()
            } else {
                "unknown".to_string()
            }
        });
    let status_text = state.status.clone().unwrap_or_else(|| {
        if loading {
            "loading".to_string()
        } else {
            "ok".to_string()
        }
    });
    let sync_statuses: &[AccountSyncStatus] = if !state.sync_statuses.is_empty() {
        &state.sync_statuses
    } else if let Some(report) = doctor {
        &report.sync_statuses
    } else {
        &[]
    };
    let event_entries: &[EventLogEntry] = if !state.events.is_empty() {
        &state.events
    } else if let Some(report) = doctor {
        &report.recent_sync_events
    } else {
        &[]
    };
    let log_entries: Vec<String> = if !state.logs.is_empty() {
        state.logs.clone()
    } else if let Some(report) = doctor {
        report.recent_error_logs.clone()
    } else {
        Vec::new()
    };

    let status_lines = vec![
        Line::from(format!("Health: {} status={}", health_text, status_text)),
        Line::from(format!(
            "Lifecycle: restart={} repair={}",
            if doctor
                .map(|report| report.restart_required)
                .unwrap_or(false)
            {
                "yes"
            } else {
                "no"
            },
            if doctor.map(|report| report.repair_required).unwrap_or(false) {
                "yes"
            } else {
                "no"
            },
        )),
        Line::from(format!(
            "Uptime: {}",
            state
                .uptime_secs
                .map(|secs| format!("{secs}s"))
                .unwrap_or_else(|| if loading {
                    "loading".into()
                } else {
                    "unknown".into()
                })
        )),
        Line::from(format!(
            "Daemon: pid={} version={} protocol={}",
            state
                .daemon_pid
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| if loading {
                    "loading".into()
                } else {
                    "unknown".into()
                }),
            doctor
                .and_then(|report| report.daemon_version.as_deref())
                .unwrap_or(if loading { "loading" } else { "unknown" }),
            doctor
                .map(|report| report.daemon_protocol_version.to_string())
                .unwrap_or_else(|| if loading {
                    "loading".into()
                } else {
                    "unknown".into()
                }),
        )),
        Line::from(format!(
            "Messages: total={} unread={} starred={}",
            if state.total_messages.is_some() || data_stats.messages > 0 {
                total_messages.to_string()
            } else if loading {
                "loading".into()
            } else {
                "unknown".into()
            },
            data_stats.unread_messages,
            data_stats.starred_messages,
        )),
        Line::from(format!(
            "Freshness: lex={} sync={}",
            doctor
                .map(|report| report.lexical_index_freshness.as_str())
                .unwrap_or("unknown"),
            doctor
                .map(|report| format_timestamp_compact(
                    report.last_successful_sync_at.as_deref(),
                    "never"
                ))
                .unwrap_or_else(|| if loading {
                    "loading".into()
                } else {
                    "never".into()
                }),
        )),
        Line::from(format!(
            "Lexical rebuilt: {}",
            doctor
                .map(|report| format_timestamp_compact(
                    report.lexical_last_rebuilt_at.as_deref(),
                    "-"
                ))
                .unwrap_or_else(|| if loading {
                    "loading".into()
                } else {
                    "-".into()
                }),
        )),
    ];
    frame.render_widget(
        Paragraph::new(status_lines).block(
            Block::default()
                .title(" Status / Doctor ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        top_chunks[0],
    );

    let data_lines = vec![
        Line::from(format!(
            "Records: accounts={} labels={} saved={} rules={}",
            data_stats.accounts, data_stats.labels, data_stats.saved_searches, data_stats.rules,
        )),
        Line::from(format!(
            "Mail: msgs={} unread={} starred={} with_att={}",
            data_stats.messages,
            data_stats.unread_messages,
            data_stats.starred_messages,
            data_stats.messages_with_attachments,
        )),
        Line::from(format!(
            "Bodies: bodies={} attachments={} links={}",
            data_stats.bodies, data_stats.attachments, data_stats.message_labels,
        )),
        Line::from(format!(
            "Workflow: drafts={} snoozed={} runtime={}",
            data_stats.drafts, data_stats.snoozed, data_stats.sync_runtime_statuses,
        )),
        Line::from(format!(
            "Events: event_log={} sync_log={} rule_logs={}",
            data_stats.event_log, data_stats.sync_log, data_stats.rule_logs,
        )),
        Line::from(format!(
            "Semantic: {} p={} e={} at={}",
            doctor
                .map(|report| report.semantic_index_freshness.as_str())
                .unwrap_or("unknown"),
            data_stats.semantic_profiles,
            data_stats.semantic_embeddings,
            doctor
                .map(|report| format_timestamp_compact(
                    report.semantic_last_indexed_at.as_deref(),
                    "-"
                ))
                .unwrap_or_else(|| if loading {
                    "loading".into()
                } else {
                    "-".into()
                }),
        )),
        Line::from(format!(
            "Storage: db={} index={} logs={}",
            doctor
                .map(|report| format_bytes(report.database_size_bytes))
                .unwrap_or_else(|| "-".into()),
            doctor
                .map(|report| format_bytes(report.index_size_bytes))
                .unwrap_or_else(|| "-".into()),
            doctor
                .map(|report| format_bytes(report.log_size_bytes))
                .unwrap_or_else(|| "-".into()),
        )),
    ];
    frame.render_widget(
        Paragraph::new(data_lines).block(
            Block::default()
                .title(" Data / Storage ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning)),
        ),
        top_chunks[1],
    );

    let sync_lines = if sync_statuses.is_empty() {
        vec![Line::from(if loading {
            "Loading sync status..."
        } else if data_stats.accounts == 0 {
            "No accounts configured"
        } else {
            "No sync runtime state yet"
        })]
    } else {
        sync_statuses
            .iter()
            .flat_map(|sync| {
                vec![
                    Line::from(format!(
                        "{} healthy={} in_progress={} failures={} synced={}",
                        sync.account_name,
                        sync.healthy,
                        sync.sync_in_progress,
                        sync.consecutive_failures,
                        sync.last_synced_count,
                    )),
                    Line::from(format!(
                        "  last_success={} error={} backoff={} cursor={}",
                        sync.last_success_at.as_deref().unwrap_or("never"),
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

    let event_lines = if event_entries.is_empty() {
        vec![Line::from(if loading {
            "Loading events..."
        } else {
            "No events"
        })]
    } else {
        event_entries
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

    let log_lines = if log_entries.is_empty() {
        vec![Line::from(if loading {
            "Loading logs..."
        } else {
            "No logs"
        })]
    } else {
        log_entries
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

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn format_timestamp_compact(value: Option<&str>, default: &str) -> String {
    value
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| {
            value
                .with_timezone(&chrono::Utc)
                .format("%m-%d %H:%MZ")
                .to_string()
        })
        .unwrap_or_else(|| default.to_string())
}
