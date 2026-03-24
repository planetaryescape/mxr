use crate::mxr_protocol::{AccountSyncStatus, DoctorDataStats, EventLogEntry};
use crate::mxr_tui::app::{DiagnosticsPageState, DiagnosticsPaneKind};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    theme: &crate::mxr_tui::theme::Theme,
) {
    if let Some(pane) = state.fullscreen_pane {
        render_detail_layout(frame, area, state, pane, theme, true);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(26), Constraint::Percentage(74)])
        .split(area);

    render_selector(frame, chunks[0], state, theme);
    render_detail_layout(frame, chunks[1], state, state.selected_pane, theme, false);
}

pub fn pane_details_text(state: &DiagnosticsPageState, pane: DiagnosticsPaneKind) -> String {
    pane_lines(state, pane).join("\n")
}

fn render_selector(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    theme: &crate::mxr_tui::theme::Theme,
) {
    let items = all_panes()
        .iter()
        .map(|&pane| {
            let meta = match pane {
                DiagnosticsPaneKind::Status => state
                    .doctor
                    .as_ref()
                    .map(|report| report.health_class.as_str().to_string())
                    .unwrap_or_else(|| "status".into()),
                DiagnosticsPaneKind::Data => state
                    .total_messages
                    .map(|count| format!("{count} msgs"))
                    .unwrap_or_else(|| "storage".into()),
                DiagnosticsPaneKind::Sync => format!("{} accounts", state.sync_statuses.len()),
                DiagnosticsPaneKind::Events => format!("{} recent", state.events.len()),
                DiagnosticsPaneKind::Logs => format!("{} lines", state.logs.len()),
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    pane_label(pane),
                    Style::default().fg(theme.text_primary).bold(),
                ),
                Span::styled(
                    format!("  [{meta}]"),
                    Style::default().fg(theme.text_secondary),
                ),
            ]))
        })
        .collect::<Vec<_>>();
    let selected = all_panes()
        .iter()
        .position(|pane| *pane == state.selected_pane)
        .unwrap_or(0);
    let list = List::new(items)
        .block(
            Block::default()
                .title(" Diagnostics ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        )
        .highlight_style(theme.highlight_style())
        .highlight_symbol("> ");
    let mut list_state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_detail_layout(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    pane: DiagnosticsPaneKind,
    theme: &crate::mxr_tui::theme::Theme,
    fullscreen: bool,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);
    render_summary(frame, chunks[0], state, pane, theme, fullscreen);
    render_detail(frame, chunks[1], state, pane, theme, fullscreen);
    render_footer(
        frame,
        chunks[2],
        if fullscreen {
            "j/k:section  Ctrl-d/u:scroll  Enter/o:exit full  d:details  r:refresh  L:logs"
        } else {
            "j/k:section  Ctrl-d/u:scroll  Enter/o:full  d:details  r:refresh  b:bug"
        },
        theme,
    );
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

fn render_summary(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    pane: DiagnosticsPaneKind,
    theme: &crate::mxr_tui::theme::Theme,
    fullscreen: bool,
) {
    let block = Block::default()
        .title(format!(
            " Summary · {}{} ",
            pane_label(pane),
            if fullscreen { " [full]" } else { "" }
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warning));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(summary_lines(state, pane)).wrap(Wrap { trim: false }),
        chunks[0],
    );

    let health = state
        .doctor
        .as_ref()
        .map(|report| report.health_class.as_str())
        .unwrap_or("unknown");
    let lexical = state
        .doctor
        .as_ref()
        .map(|report| report.lexical_index_freshness.as_str())
        .unwrap_or("unknown");

    frame.render_widget(
        LineGauge::default()
            .label(format!("health {health}"))
            .filled_style(Style::default().fg(theme.success))
            .unfilled_style(Style::default().fg(theme.border_unfocused))
            .ratio(health_ratio(health)),
        chunks[1],
    );
    frame.render_widget(
        LineGauge::default()
            .label(format!("index {lexical}"))
            .filled_style(Style::default().fg(theme.accent))
            .unfilled_style(Style::default().fg(theme.border_unfocused))
            .ratio(freshness_ratio(lexical)),
        chunks[2],
    );
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    pane: DiagnosticsPaneKind,
    theme: &crate::mxr_tui::theme::Theme,
    fullscreen: bool,
) {
    let title = if fullscreen {
        format!(" {} [full] ", pane_label(pane))
    } else {
        format!(" {} ", pane_label(pane))
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
            .border_style(Style::default().fg(theme.accent)),
    )
    .scroll((state.scroll_offset(pane), 0))
    .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);

    let body_height = area.height.saturating_sub(2) as usize;
    let line_count = pane_lines(state, pane).len();
    if line_count > body_height {
        let mut scrollbar_state = ScrollbarState::new(line_count.saturating_sub(body_height))
            .position(state.scroll_offset(pane) as usize);
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(theme.warning)),
            area,
            &mut scrollbar_state,
        );
    }
}

fn render_footer(frame: &mut Frame, area: Rect, text: &str, theme: &crate::mxr_tui::theme::Theme) {
    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        area,
    );
}

fn all_panes() -> [DiagnosticsPaneKind; 5] {
    [
        DiagnosticsPaneKind::Status,
        DiagnosticsPaneKind::Data,
        DiagnosticsPaneKind::Sync,
        DiagnosticsPaneKind::Events,
        DiagnosticsPaneKind::Logs,
    ]
}

fn summary_lines(state: &DiagnosticsPageState, pane: DiagnosticsPaneKind) -> Vec<Line<'static>> {
    match pane {
        DiagnosticsPaneKind::Status => vec![
            Line::from(format!(
                "daemon={}  pid={}",
                state.status.as_deref().unwrap_or("unknown"),
                state
                    .daemon_pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "unknown".into())
            )),
            Line::from(format!(
                "accounts={}  messages={}",
                state.accounts.len(),
                state
                    .total_messages
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "unknown".into())
            )),
        ],
        DiagnosticsPaneKind::Data => vec![
            Line::from("Local database, index, and semantic storage."),
            Line::from("Use this when counts or disk footprint look wrong."),
        ],
        DiagnosticsPaneKind::Sync => vec![
            Line::from(format!(
                "{} account sync runtimes visible.",
                state.sync_statuses.len()
            )),
            Line::from("Check failures, backoff, and current cursor state here."),
        ],
        DiagnosticsPaneKind::Events => vec![
            Line::from(format!(
                "{} recent daemon events loaded.",
                state.events.len()
            )),
            Line::from("Inspect recent sync summaries and event details."),
        ],
        DiagnosticsPaneKind::Logs => vec![
            Line::from(format!("{} recent log lines loaded.", state.logs.len())),
            Line::from("Open logs for the full file in your editor."),
        ],
    }
}

fn health_ratio(status: &str) -> f64 {
    match status {
        "healthy" => 1.0,
        "degraded" => 0.66,
        "unhealthy" => 0.33,
        _ => 0.15,
    }
}

fn freshness_ratio(status: &str) -> f64 {
    match status {
        "fresh" => 1.0,
        "stale" => 0.5,
        "missing" => 0.2,
        _ => 0.15,
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
