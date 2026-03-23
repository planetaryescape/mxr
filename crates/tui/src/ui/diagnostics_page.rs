use crate::app::{DiagnosticsPageState, DiagnosticsPaneKind};
use mxr_protocol::{AccountSyncStatus, DoctorDataStats, EventLogEntry};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    theme: &crate::theme::Theme,
) {
    if let Some(pane) = state.fullscreen_pane {
        render_pane(frame, area, state, pane, theme, true);
        return;
    }

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

    render_pane(
        frame,
        top_chunks[0],
        state,
        DiagnosticsPaneKind::Status,
        theme,
        false,
    );
    render_pane(
        frame,
        top_chunks[1],
        state,
        DiagnosticsPaneKind::Data,
        theme,
        false,
    );
    render_pane(
        frame,
        chunks[1],
        state,
        DiagnosticsPaneKind::Sync,
        theme,
        false,
    );
    render_pane(
        frame,
        chunks[2],
        state,
        DiagnosticsPaneKind::Events,
        theme,
        false,
    );
    render_pane(
        frame,
        chunks[3],
        state,
        DiagnosticsPaneKind::Logs,
        theme,
        false,
    );
}

pub fn pane_details_text(state: &DiagnosticsPageState, pane: DiagnosticsPaneKind) -> String {
    pane_lines(state, pane).join("\n")
}

fn render_pane(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    pane: DiagnosticsPaneKind,
    theme: &crate::theme::Theme,
    fullscreen: bool,
) {
    let selected = state.selected_pane == pane;
    let title = pane_title(pane, selected, fullscreen);
    let border_fg = if selected {
        theme.accent
    } else {
        pane_border_color(pane, theme)
    };

    let paragraph = Paragraph::new(
        pane_lines(state, pane)
            .into_iter()
            .map(Line::from)
            .collect::<Vec<_>>(),
    )
    .block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_fg)),
    )
    .scroll((state.scroll_offset(pane), 0))
    .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn pane_title(pane: DiagnosticsPaneKind, selected: bool, fullscreen: bool) -> String {
    let prefix = if selected { "> " } else { "" };
    let suffix = if fullscreen { " [full]" } else { "" };
    format!(" {prefix}{}{suffix} ", pane_label(pane))
}

fn pane_label(pane: DiagnosticsPaneKind) -> &'static str {
    match pane {
        DiagnosticsPaneKind::Status => "Status / Doctor",
        DiagnosticsPaneKind::Data => "Data / Storage",
        DiagnosticsPaneKind::Sync => "Sync Health",
        DiagnosticsPaneKind::Events => "Recent Events",
        DiagnosticsPaneKind::Logs => "Recent Logs",
    }
}

fn pane_border_color(pane: DiagnosticsPaneKind, theme: &crate::theme::Theme) -> Color {
    match pane {
        DiagnosticsPaneKind::Status | DiagnosticsPaneKind::Sync => theme.accent,
        DiagnosticsPaneKind::Data | DiagnosticsPaneKind::Events => theme.warning,
        DiagnosticsPaneKind::Logs => theme.success,
    }
}

fn pane_lines(state: &DiagnosticsPageState, pane: DiagnosticsPaneKind) -> Vec<String> {
    let loading = state.pending_requests > 0;
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

    match pane {
        DiagnosticsPaneKind::Status => vec![
            format!("Health: {} status={}", health_text, status_text),
            format!(
                "Lifecycle: restart={} repair={}",
                yes_no(
                    doctor
                        .map(|report| report.restart_required)
                        .unwrap_or(false)
                ),
                yes_no(doctor.map(|report| report.repair_required).unwrap_or(false)),
            ),
            format!(
                "Uptime: {}",
                state
                    .uptime_secs
                    .map(|secs| format!("{secs}s"))
                    .unwrap_or_else(|| if loading {
                        "loading".into()
                    } else {
                        "unknown".into()
                    })
            ),
            format!(
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
            ),
            format!(
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
            ),
            format!(
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
            ),
            format!(
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
            ),
            format!("Hints: Tab/Shift-Tab pane  Enter fullscreen  d details  L logs"),
        ],
        DiagnosticsPaneKind::Data => vec![
            format!(
                "Records: accounts={} labels={} saved={} rules={}",
                data_stats.accounts, data_stats.labels, data_stats.saved_searches, data_stats.rules,
            ),
            format!(
                "Mail: msgs={} unread={} starred={} with_att={}",
                data_stats.messages,
                data_stats.unread_messages,
                data_stats.starred_messages,
                data_stats.messages_with_attachments,
            ),
            format!(
                "Bodies: bodies={} attachments={} links={}",
                data_stats.bodies, data_stats.attachments, data_stats.message_labels,
            ),
            format!(
                "Workflow: drafts={} snoozed={} runtime={}",
                data_stats.drafts, data_stats.snoozed, data_stats.sync_runtime_statuses,
            ),
            format!(
                "Events: event_log={} sync_log={} rule_logs={}",
                data_stats.event_log, data_stats.sync_log, data_stats.rule_logs,
            ),
            format!(
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
            ),
            format!(
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
            ),
            doctor
                .map(|report| {
                    format!(
                        "Paths: db={} index={} logs={}",
                        report.database_path, report.index_path, report.log_path
                    )
                })
                .unwrap_or_else(|| "Paths: loading".into()),
        ],
        DiagnosticsPaneKind::Sync => {
            if sync_statuses.is_empty() {
                vec![if loading {
                    "Loading sync status...".into()
                } else if data_stats.accounts == 0 {
                    "No accounts configured".into()
                } else {
                    "No sync runtime state yet".into()
                }]
            } else {
                sync_statuses
                    .iter()
                    .flat_map(|sync| {
                        vec![
                            format!(
                                "{} healthy={} in_progress={} failures={} synced={}",
                                sync.account_name,
                                sync.healthy,
                                sync.sync_in_progress,
                                sync.consecutive_failures,
                                sync.last_synced_count,
                            ),
                            format!(
                                "  last_success={} error={} backoff={} cursor={}",
                                sync.last_success_at.as_deref().unwrap_or("never"),
                                sync.last_error.as_deref().unwrap_or("-"),
                                sync.backoff_until.as_deref().unwrap_or("-"),
                                sync.current_cursor_summary.as_deref().unwrap_or("-"),
                            ),
                        ]
                    })
                    .collect()
            }
        }
        DiagnosticsPaneKind::Events => {
            if event_entries.is_empty() {
                vec![if loading {
                    "Loading events...".into()
                } else {
                    "No events".into()
                }]
            } else {
                event_entries
                    .iter()
                    .flat_map(|event| {
                        let mut lines = vec![format!(
                            "{} [{}:{}] {}",
                            event.timestamp, event.level, event.category, event.summary
                        )];
                        if let Some(details) = event.details.as_deref() {
                            lines.push(format!("  {}", details.replace('\n', " ")));
                        }
                        lines
                    })
                    .collect()
            }
        }
        DiagnosticsPaneKind::Logs => {
            if log_entries.is_empty() {
                vec![if loading {
                    "Loading logs...".into()
                } else {
                    "No logs".into()
                }]
            } else {
                log_entries
            }
        }
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
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
