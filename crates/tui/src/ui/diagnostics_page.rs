use crate::app::{DiagnosticsPageState, DiagnosticsPaneKind};
use mxr_protocol::{
    AccountSyncStatus, DoctorDataStats, DoctorFinding, DoctorFindingCategory,
    DoctorFindingSeverity, EventLogEntry, FeatureHealth, FeatureHealthReport,
};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    theme: &crate::theme::Theme,
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
    theme: &crate::theme::Theme,
) {
    let items = all_panes()
        .iter()
        .map(|&pane| {
            let meta = match pane {
                DiagnosticsPaneKind::Status => state.doctor.as_ref().map_or_else(
                    || "status".into(),
                    |report| report.health_class.as_str().to_string(),
                ),
                DiagnosticsPaneKind::Data => state
                    .total_messages
                    .map_or_else(|| "storage".into(), |count| format!("{count} msgs")),
                DiagnosticsPaneKind::Sync => format!("{} accounts", state.sync_statuses.len()),
                DiagnosticsPaneKind::Events => format!("{} recent", state.events.len()),
                DiagnosticsPaneKind::Logs => format!("{} lines", state.logs.len()),
                DiagnosticsPaneKind::Jobs => format!("{} jobs", state.jobs.len()),
                DiagnosticsPaneKind::Activity => format!("{} rows", state.activity.len()),
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
    theme: &crate::theme::Theme,
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
    let footer_text: String = if let Some(target) = state.search_input {
        // Active /-search input: show what the user is typing.
        let buf = match target {
            DiagnosticsPaneKind::Events => state.events_search.as_str(),
            DiagnosticsPaneKind::Logs => state.logs_search.as_str(),
            DiagnosticsPaneKind::Activity => state.activity_search.as_str(),
            _ => "",
        };
        format!(
            "/  {} {} _   Enter:apply  Esc:clear",
            pane_label(target),
            buf
        )
    } else {
        let active_search = match pane {
            DiagnosticsPaneKind::Events => state.events_search.as_str(),
            DiagnosticsPaneKind::Logs => state.logs_search.as_str(),
            DiagnosticsPaneKind::Activity => state.activity_search.as_str(),
            _ => "",
        };
        let filter_hint = if !active_search.is_empty() {
            format!("filter='{active_search}'  ")
        } else {
            String::new()
        };
        if fullscreen {
            format!("{filter_hint}j/k:section  Ctrl-d/u:scroll  Enter/o:exit full  /:search  d:details  c:config  L:logs")
        } else {
            format!("{filter_hint}j/k:section  Ctrl-d/u:scroll  Enter/o:full  /:search  r:refresh  c:config  L:logs")
        }
    };
    render_footer(frame, chunks[2], &footer_text, theme);
}

fn pane_label(pane: DiagnosticsPaneKind) -> &'static str {
    match pane {
        DiagnosticsPaneKind::Status => "Status / Doctor",
        DiagnosticsPaneKind::Data => "Data / Storage",
        DiagnosticsPaneKind::Sync => "Sync Health",
        DiagnosticsPaneKind::Events => "Recent Events",
        DiagnosticsPaneKind::Logs => "Recent Logs",
        DiagnosticsPaneKind::Jobs => "Background Jobs",
        DiagnosticsPaneKind::Activity => "Activity Log",
    }
}

fn render_summary(
    frame: &mut Frame,
    area: Rect,
    state: &DiagnosticsPageState,
    pane: DiagnosticsPaneKind,
    theme: &crate::theme::Theme,
    fullscreen: bool,
) {
    let block = Block::default()
        .title(format!(
            " Summary / {}{} ",
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
        .map_or("unknown", |report| report.health_class.as_str());
    let lexical = state
        .doctor
        .as_ref()
        .map_or("unknown", |report| report.lexical_index_freshness.as_str());

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
    theme: &crate::theme::Theme,
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

fn render_footer(frame: &mut Frame, area: Rect, text: &str, theme: &crate::theme::Theme) {
    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        area,
    );
}

fn all_panes() -> [DiagnosticsPaneKind; 7] {
    [
        DiagnosticsPaneKind::Status,
        DiagnosticsPaneKind::Data,
        DiagnosticsPaneKind::Sync,
        DiagnosticsPaneKind::Events,
        DiagnosticsPaneKind::Logs,
        DiagnosticsPaneKind::Jobs,
        DiagnosticsPaneKind::Activity,
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
                    .map_or_else(|| "unknown".into(), |pid| pid.to_string())
            )),
            Line::from(format!(
                "accounts={}  messages={}",
                state.accounts.len(),
                state
                    .total_messages
                    .map_or_else(|| "unknown".into(), |count| count.to_string())
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
            Line::from("`/` to filter; `L` opens the full file in your editor."),
        ],
        DiagnosticsPaneKind::Jobs => vec![
            Line::from(format!("{} background jobs loaded.", state.jobs.len())),
            Line::from("Large mutation progress, failures, and undo ids."),
        ],
        DiagnosticsPaneKind::Activity => vec![
            Line::from(format!("{} activity rows loaded.", state.activity.len())),
            Line::from("Local-only; `/` to filter; `r` to refresh."),
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
    let data_stats = doctor.map_or(&empty_stats, |report| &report.data_stats);
    let total_messages = state
        .total_messages
        .filter(|count| *count > 0 || data_stats.messages == 0)
        .unwrap_or(data_stats.messages);
    let health_text = doctor.map_or_else(
        || {
            if loading {
                "loading".to_string()
            } else {
                "unknown".to_string()
            }
        },
        |report| report.health_class.as_str().to_string(),
    );
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
                yes_no(doctor.is_some_and(|report| report.restart_required)),
                yes_no(doctor.is_some_and(|report| report.repair_required)),
            ),
            format!(
                "Uptime: {}",
                state.uptime_secs.map_or_else(
                    || if loading {
                        "loading".into()
                    } else {
                        "unknown".into()
                    },
                    |secs| format!("{secs}s")
                )
            ),
            format!(
                "Daemon: pid={} version={} protocol={}",
                state.daemon_pid.map_or_else(
                    || if loading {
                        "loading".into()
                    } else {
                        "unknown".into()
                    },
                    |pid| pid.to_string()
                ),
                doctor
                    .and_then(|report| report.daemon_version.as_deref())
                    .unwrap_or(if loading { "loading" } else { "unknown" }),
                doctor.map_or_else(
                    || if loading {
                        "loading".into()
                    } else {
                        "unknown".into()
                    },
                    |report| report.daemon_protocol_version.to_string()
                ),
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
                doctor.map_or("unknown", |report| report.lexical_index_freshness.as_str()),
                doctor.map_or_else(
                    || if loading {
                        "loading".into()
                    } else {
                        "never".into()
                    },
                    |report| format_timestamp_compact(
                        report.last_successful_sync_at.as_deref(),
                        "never"
                    )
                ),
            ),
            format!(
                "Lexical rebuilt: {}",
                doctor.map_or_else(
                    || if loading {
                        "loading".into()
                    } else {
                        "-".into()
                    },
                    |report| format_timestamp_compact(
                        report.lexical_last_rebuilt_at.as_deref(),
                        "-"
                    )
                ),
            ),
            {
                let findings = doctor.map_or(&[][..], |report| report.findings.as_slice());
                if findings.is_empty() {
                    "Findings: none".to_string()
                } else {
                    format!("Findings: {} issue(s)", findings.len())
                }
            },
        ]
        .into_iter()
        .chain(
            doctor
                .and_then(|report| report.feature_health.as_ref())
                .into_iter()
                .flat_map(feature_health_lines),
        )
        .chain(
            doctor
                .map_or(&[][..], |report| report.findings.as_slice())
                .iter()
                .flat_map(format_finding_lines),
        )
        .chain(std::iter::once(
            "Hints: Tab/Shift-Tab pane  Enter fullscreen  d details  L logs".to_string(),
        ))
        .collect::<Vec<_>>(),
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
                "Semantic: {} p={} chunks={} e={} missing_chunks={} missing_embeddings={} drift={}",
                doctor.map_or("unknown", |report| report.semantic_index_freshness.as_str()),
                data_stats.semantic_profiles,
                data_stats.semantic_chunks,
                data_stats.semantic_embeddings,
                data_stats.messages_missing_semantic_chunks,
                data_stats.semantic_chunks_missing_embeddings,
                data_stats.relationship_drifts,
            ),
            format!(
                "Semantic indexed at: {}",
                doctor.map_or_else(
                    || if loading {
                        "loading".into()
                    } else {
                        "-".into()
                    },
                    |report| format_timestamp_compact(
                        report.semantic_last_indexed_at.as_deref(),
                        "-"
                    )
                ),
            ),
            format!(
                "Storage: db={} index={} logs={}",
                doctor.map_or_else(
                    || "-".into(),
                    |report| format_bytes(report.database_size_bytes)
                ),
                doctor.map_or_else(
                    || "-".into(),
                    |report| format_bytes(report.index_size_bytes)
                ),
                doctor.map_or_else(|| "-".into(), |report| format_bytes(report.log_size_bytes)),
            ),
            doctor.map_or_else(
                || "Paths: loading".into(),
                |report| {
                    format!(
                        "Paths: db={} index={} logs={}",
                        report.database_path, report.index_path, report.log_path
                    )
                },
            ),
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
            let needle = state.events_search.trim().to_lowercase();
            let filtered: Vec<&EventLogEntry> = event_entries
                .iter()
                .filter(|event| {
                    if needle.is_empty() {
                        return true;
                    }
                    event.summary.to_lowercase().contains(&needle)
                        || event.level.to_lowercase().contains(&needle)
                        || event.category.to_lowercase().contains(&needle)
                        || event
                            .details
                            .as_deref()
                            .is_some_and(|d| d.to_lowercase().contains(&needle))
                })
                .collect();
            if filtered.is_empty() {
                vec![if loading {
                    "Loading events...".into()
                } else if !needle.is_empty() {
                    format!("No events match '{}'", state.events_search)
                } else {
                    "No events".into()
                }]
            } else {
                filtered
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
            // Apply free-text search if present.
            let needle = state.logs_search.trim().to_lowercase();
            let lines: Vec<String> = if needle.is_empty() {
                log_entries
            } else {
                log_entries
                    .into_iter()
                    .filter(|line| line.to_lowercase().contains(&needle))
                    .collect()
            };
            if lines.is_empty() {
                vec![if loading {
                    "Loading logs...".into()
                } else if !state.logs_search.is_empty() {
                    format!("No logs match '{}'", state.logs_search)
                } else {
                    "No logs".into()
                }]
            } else {
                lines
            }
        }
        DiagnosticsPaneKind::Jobs => {
            if state.jobs.is_empty() {
                vec![if loading {
                    "Loading jobs...".into()
                } else {
                    "No jobs".into()
                }]
            } else {
                state
                    .jobs
                    .iter()
                    .map(|job| {
                        format!(
                            "{} {} {} {}/{} ok={} skipped={} failed={} undo={}{}",
                            job.job_id,
                            format!("{:?}", job.status).to_ascii_lowercase(),
                            job.kind,
                            job.progress.completed,
                            job.progress.total,
                            job.progress.succeeded,
                            job.progress.skipped,
                            job.progress.failed,
                            job.undo_ids.join(","),
                            job.error
                                .as_ref()
                                .map(|error| format!(" error={error}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect()
            }
        }
        DiagnosticsPaneKind::Activity => {
            if state.activity.is_empty() {
                vec![if loading {
                    "Loading activity...".into()
                } else {
                    "No recent activity. Open the activity modal with `g y` to fetch.".into()
                }]
            } else {
                let needle = state.activity_search.trim().to_lowercase();
                let filtered: Vec<&mxr_protocol::ActivityEntry> = state
                    .activity
                    .iter()
                    .filter(|e| {
                        if needle.is_empty() {
                            return true;
                        }
                        if e.action.to_lowercase().contains(&needle) {
                            return true;
                        }
                        if let Some(ctx) = &e.context {
                            let blob = serde_json::to_string(ctx).unwrap_or_default();
                            return blob.to_lowercase().contains(&needle);
                        }
                        false
                    })
                    .collect();
                if filtered.is_empty() {
                    return vec![format!("No activity matches '{}'", state.activity_search)];
                }
                filtered
                    .iter()
                    .map(|e| {
                        let ts = chrono::DateTime::from_timestamp_millis(e.ts).map_or_else(
                            || e.ts.to_string(),
                            |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                        );
                        let target = match (&e.target_kind, &e.target_id) {
                            (Some(k), Some(id)) => {
                                let trimmed: String = id.chars().take(12).collect();
                                format!("{k}:{trimmed}")
                            }
                            (Some(k), None) => k.clone(),
                            _ => "—".into(),
                        };
                        let context = if e.redacted {
                            "(redacted)".to_string()
                        } else if let Some(c) = &e.context {
                            serde_json::to_string(c)
                                .unwrap_or_default()
                                .chars()
                                .take(60)
                                .collect()
                        } else {
                            String::new()
                        };
                        let source_str = format!("{:?}", e.source).to_lowercase();
                        format!(
                            "{ts} {source_str:<6} {:<24} {target:<20} {context}",
                            e.action
                        )
                    })
                    .collect()
            }
        }
    }
}

fn feature_health_lines(report: &FeatureHealthReport) -> Vec<String> {
    vec![
        "Feature health:".to_string(),
        format!(
            "  semantic={} summarize={} relationship={}",
            feature_health_label(&report.semantic),
            feature_health_label(&report.summarize),
            feature_health_label(&report.relationship_profile),
        ),
        format!(
            "  commitments={} draft_assist={} voice_match={} humanizer={}",
            feature_health_label(&report.commitments),
            feature_health_label(&report.draft_assist),
            feature_health_label(&report.voice_match),
            feature_health_label(&report.humanizer),
        ),
    ]
}

fn feature_health_label(health: &FeatureHealth) -> String {
    match health {
        FeatureHealth::Healthy => "healthy".into(),
        FeatureHealth::Disabled => "disabled".into(),
        FeatureHealth::Degraded { reason } => format!("degraded({})", reason.replace('\n', " ")),
    }
}

/// Render a single doctor finding as one or more lines: a leading
/// "<glyph> <category>: <message>" line, then any remediation steps as
/// indented "→ <command>" lines so the user can copy-paste.
fn format_finding_lines(finding: &DoctorFinding) -> Vec<String> {
    let glyph = match finding.severity {
        DoctorFindingSeverity::Error => "✗",
        DoctorFindingSeverity::Warning => "!",
        DoctorFindingSeverity::Info => "·",
    };
    let category = match finding.category {
        DoctorFindingCategory::Generic => "general",
        DoctorFindingCategory::Sync => "sync",
        DoctorFindingCategory::OAuth => "oauth",
        DoctorFindingCategory::Network => "network",
        DoctorFindingCategory::SearchIndex => "search-index",
        DoctorFindingCategory::Semantic => "semantic",
        DoctorFindingCategory::SqliteLock => "sqlite-lock",
        DoctorFindingCategory::Storage => "storage",
        DoctorFindingCategory::Daemon => "daemon",
    };
    let mut lines = vec![format!("  {glyph} {category}: {}", finding.message)];
    for step in &finding.remediation {
        lines.push(format!("    → {step}"));
    }
    lines
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
        .map_or_else(
            || default.to_string(),
            |value| {
                value
                    .with_timezone(&chrono::Utc)
                    .format("%m-%d %H:%MZ")
                    .to_string()
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::{DoctorFinding, DoctorFindingCategory, DoctorFindingSeverity, DoctorReport};

    fn empty_doctor_with_findings(findings: Vec<DoctorFinding>) -> DoctorReport {
        DoctorReport {
            healthy: false,
            health_class: mxr_protocol::DaemonHealthClass::Degraded,
            lexical_index_freshness: mxr_protocol::IndexFreshness::Current,
            last_successful_sync_at: None,
            lexical_last_rebuilt_at: None,
            semantic_enabled: false,
            semantic_active_profile: None,
            semantic_index_freshness: mxr_protocol::IndexFreshness::Current,
            semantic_last_indexed_at: None,
            feature_health: None,
            data_stats: DoctorDataStats::default(),
            data_dir_exists: true,
            database_exists: true,
            index_exists: true,
            socket_exists: true,
            socket_reachable: true,
            stale_socket: false,
            daemon_running: true,
            daemon_pid: None,
            daemon_protocol_version: 1,
            daemon_version: None,
            daemon_build_id: None,
            index_lock_held: false,
            index_lock_error: None,
            restart_required: false,
            repair_required: false,
            database_path: String::new(),
            database_size_bytes: 0,
            index_path: String::new(),
            index_size_bytes: 0,
            log_path: String::new(),
            log_size_bytes: 0,
            sync_statuses: vec![],
            recent_sync_events: vec![],
            recent_error_logs: vec![],
            recommended_next_steps: vec![],
            findings,
        }
    }

    #[test]
    fn format_finding_lines_emits_message_and_remediation() {
        let finding = DoctorFinding {
            category: DoctorFindingCategory::OAuth,
            severity: DoctorFindingSeverity::Error,
            message: "OAuth token expired for me@example.com".into(),
            remediation: vec!["mxr accounts reauth me@example.com".into()],
        };
        let lines = format_finding_lines(&finding);
        assert_eq!(
            lines,
            vec![
                "  ✗ oauth: OAuth token expired for me@example.com".to_string(),
                "    → mxr accounts reauth me@example.com".to_string(),
            ],
            "an OAuth error finding must lead with the cross glyph and indent remediation",
        );
    }

    #[test]
    fn status_pane_lines_include_findings_count_and_remediation_step() {
        let doctor = empty_doctor_with_findings(vec![DoctorFinding {
            category: DoctorFindingCategory::Network,
            severity: DoctorFindingSeverity::Warning,
            message: "DNS resolution failed for imap.example.com".into(),
            remediation: vec!["check internet connectivity".into()],
        }]);
        let state = DiagnosticsPageState {
            doctor: Some(doctor),
            ..Default::default()
        };

        let lines = pane_lines(&state, DiagnosticsPaneKind::Status);
        assert!(
            lines.iter().any(|line| line == "Findings: 1 issue(s)"),
            "Status pane must summarise the findings count; got {lines:?}",
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("DNS resolution failed")),
            "Status pane must surface the finding's message; got {lines:?}",
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("→ check internet connectivity")),
            "Status pane must surface the remediation step; got {lines:?}",
        );
    }

    #[test]
    fn status_pane_says_no_findings_when_doctor_clean() {
        let doctor = empty_doctor_with_findings(vec![]);
        let state = DiagnosticsPageState {
            doctor: Some(doctor),
            ..Default::default()
        };
        let lines = pane_lines(&state, DiagnosticsPaneKind::Status);
        assert!(
            lines.iter().any(|line| line == "Findings: none"),
            "Status pane must show 'Findings: none' on a clean doctor; got {lines:?}",
        );
    }

    #[test]
    fn status_pane_renders_feature_health() {
        let mut doctor = empty_doctor_with_findings(vec![]);
        doctor.feature_health = Some(FeatureHealthReport {
            semantic: FeatureHealth::Healthy,
            summarize: FeatureHealth::Disabled,
            relationship_profile: FeatureHealth::Healthy,
            commitments: FeatureHealth::Degraded {
                reason: "LLM disabled".into(),
            },
            draft_assist: FeatureHealth::Healthy,
            voice_match: FeatureHealth::Healthy,
            humanizer: FeatureHealth::Healthy,
        });
        let state = DiagnosticsPageState {
            doctor: Some(doctor),
            ..Default::default()
        };

        let lines = pane_lines(&state, DiagnosticsPaneKind::Status);

        assert!(lines.iter().any(|line| line == "Feature health:"));
        assert!(
            lines.iter().any(|line| line.contains("semantic=healthy")),
            "feature health must surface semantic status; got {lines:?}",
        );
        assert!(
            lines
                .iter()
                .any(|line| line.contains("commitments=degraded(LLM disabled)")),
            "feature health must surface degraded reasons; got {lines:?}",
        );
    }
}
