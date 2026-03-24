use crate::mxr_tui::app::{RuleFormState, RulesPageState, RulesPanel};
use ratatui::prelude::*;
use ratatui::widgets::*;
use tui_textarea::TextArea;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &RulesPageState,
    condition_editor: &TextArea<'static>,
    action_editor: &TextArea<'static>,
    theme: &crate::mxr_tui::theme::Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);
    let detail_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(chunks[1]);

    draw_rule_list(frame, chunks[0], state, theme);

    let tabs = Tabs::new(["Overview", "History", "Dry Run", "Edit"])
        .select(match state.panel {
            RulesPanel::Details => 0,
            RulesPanel::History => 1,
            RulesPanel::DryRun => 2,
            RulesPanel::Form => 3,
        })
        .block(
            Block::default()
                .title(" Workspace ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning)),
        )
        .highlight_style(Style::default().fg(theme.accent).bold())
        .divider(Span::styled(" | ", Style::default().fg(theme.text_muted)));
    frame.render_widget(tabs, detail_chunks[0]);

    if state.form.visible {
        draw_form(
            frame,
            detail_chunks[1],
            &state.form,
            condition_editor,
            action_editor,
            theme,
        );
        draw_footer(
            frame,
            detail_chunks[2],
            "Tab:next  Ctrl-s:save  Space:toggle enabled  Esc:close",
            theme,
        );
        return;
    }

    let body = match state.panel {
        RulesPanel::Details => overview_lines(state),
        RulesPanel::History => history_lines(&state.history),
        RulesPanel::DryRun => dry_run_lines(&state.dry_run),
        RulesPanel::Form => Vec::new(),
    };
    let paragraph = Paragraph::new(body)
        .block(
            Block::default()
                .title(match state.panel {
                    RulesPanel::Details => " Overview ",
                    RulesPanel::History => " History ",
                    RulesPanel::DryRun => " Dry Run ",
                    RulesPanel::Form => " Edit ",
                })
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, detail_chunks[1]);
    draw_footer(
        frame,
        detail_chunks[2],
        if state.rules.is_empty() {
            "n:new rule  Esc:mailbox"
        } else {
            "j/k:select  Enter:overview  H:history  D:dry run  E:edit  n:new  e:toggle"
        },
        theme,
    );
}

fn draw_rule_list(
    frame: &mut Frame,
    area: Rect,
    state: &RulesPageState,
    theme: &crate::mxr_tui::theme::Theme,
) {
    if state.rules.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("Rules automate repeatable mail cleanup."),
                Line::from(""),
                Line::from("Start with n to create one."),
                Line::from("Dry Run shows what would change before save."),
                Line::from(""),
                Line::from("Starter recipes"),
                Line::from("from:github.com -> label:GitHub"),
                Line::from("label:newsletters -> read + archive"),
                Line::from("from:billing@example.com -> label:Finance"),
            ])
            .block(
                Block::default()
                    .title(" Rules ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent)),
            )
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    let items = state
        .rules
        .iter()
        .map(|rule| {
            let enabled = if rule["enabled"].as_bool().unwrap_or(false) {
                "enabled"
            } else {
                "disabled"
            };
            let priority = rule["priority"].as_i64().unwrap_or_default();
            let condition = rule["condition"]
                .as_str()
                .unwrap_or("condition unavailable")
                .to_string();
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        rule["name"].as_str().unwrap_or("(unnamed)").to_string(),
                        Style::default().fg(theme.text_primary).bold(),
                    ),
                    Span::styled(
                        format!("  [{enabled} | p{priority}]"),
                        Style::default().fg(theme.text_secondary),
                    ),
                ]),
                Line::from(Span::styled(
                    truncate_line(&condition, area.width.saturating_sub(6) as usize),
                    Style::default().fg(theme.text_muted),
                )),
            ])
        })
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(
            Block::default()
                .title(" Rules ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        )
        .highlight_style(theme.highlight_style())
        .highlight_symbol("> ");
    let mut stateful = ListState::default().with_selected(Some(state.selected_index));
    frame.render_stateful_widget(list, area, &mut stateful);
}

fn draw_form(
    frame: &mut Frame,
    area: Rect,
    form: &RuleFormState,
    condition_editor: &TextArea<'static>,
    action_editor: &TextArea<'static>,
    theme: &crate::mxr_tui::theme::Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(8),
            Constraint::Length(6),
        ])
        .split(area);

    let summary_lines = vec![
        summary_line("Name", &form.name, form.active_field == 0, theme),
        summary_line("Priority", &form.priority, form.active_field == 3, theme),
        summary_line(
            "Enabled",
            if form.enabled { "true" } else { "false" },
            form.active_field == 4,
            theme,
        ),
        Line::from(""),
        Line::from("Use dry run before save to verify the selection path."),
    ];
    frame.render_widget(
        Paragraph::new(summary_lines)
            .block(
                Block::default()
                    .title(" Rule Summary ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.warning)),
            )
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let editor_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let mut condition_editor = condition_editor.clone();
    condition_editor.set_block(
        Block::default()
            .title(if form.active_field == 1 {
                " Condition [active] "
            } else {
                " Condition "
            })
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if form.active_field == 1 {
                theme.accent
            } else {
                theme.border_unfocused
            })),
    );
    condition_editor.set_cursor_line_style(Style::default().bg(theme.hint_bar_bg));

    let mut action_editor = action_editor.clone();
    action_editor.set_block(
        Block::default()
            .title(if form.active_field == 2 {
                " Action [active] "
            } else {
                " Action "
            })
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if form.active_field == 2 {
                theme.accent
            } else {
                theme.border_unfocused
            })),
    );
    action_editor.set_cursor_line_style(Style::default().bg(theme.hint_bar_bg));

    frame.render_widget(&condition_editor, editor_chunks[0]);
    frame.render_widget(&action_editor, editor_chunks[1]);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Starter recipes"),
            Line::from("from:github.com"),
            Line::from("action: add_label(\"GitHub\")"),
            Line::from(""),
            Line::from("label:newsletters"),
            Line::from("action: mark_read(); archive()"),
        ])
        .block(
            Block::default()
                .title(" Examples ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        )
        .wrap(Wrap { trim: false }),
        chunks[2],
    );
}

fn overview_lines(state: &RulesPageState) -> Vec<Line<'static>> {
    let Some(rule) = state.detail.as_ref().or_else(|| state.selected_rule()) else {
        return vec![
            Line::from("Pick a rule to inspect what it matches and what it does."),
            Line::from(""),
            Line::from("Use n to create a rule or E to edit the selected rule."),
        ];
    };

    vec![
        Line::from(format!(
            "Name: {}",
            rule["name"].as_str().unwrap_or("(unnamed)")
        )),
        Line::from(format!(
            "Enabled: {}",
            if rule["enabled"].as_bool().unwrap_or(false) {
                "true"
            } else {
                "false"
            }
        )),
        Line::from(format!(
            "Priority: {}",
            rule["priority"]
                .as_i64()
                .map(|value| value.to_string())
                .unwrap_or_else(|| "100".into())
        )),
        Line::from(""),
        Line::from("Condition"),
        Line::from(rule["condition"].as_str().unwrap_or("No condition recorded").to_string()),
        Line::from(""),
        Line::from("Action"),
        Line::from(rule["action"].as_str().unwrap_or("No action recorded").to_string()),
        Line::from(""),
        Line::from(format!(
            "Last run: {}",
            field_or_dash(rule, &["last_run_at", "last_ran_at", "updated_at"])
        )),
        Line::from(format!(
            "Last match: {}",
            field_or_dash(rule, &["last_match_at", "last_matched_at"])
        )),
        Line::from(format!(
            "Last error: {}",
            field_or_dash(rule, &["last_error", "error"])
        )),
        Line::from(""),
        Line::from("Dry run before save to verify affected mail and labels."),
    ]
}

fn history_lines(entries: &[serde_json::Value]) -> Vec<Line<'static>> {
    if entries.is_empty() {
        return vec![
            Line::from("No rule history yet."),
            Line::from(""),
            Line::from("Run the rule, then come back here for recent executions."),
        ];
    }

    entries
        .iter()
        .flat_map(|entry| {
            vec![
                Line::from(format!(
                    "{} / {}",
                    field_or_dash(entry, &["timestamp", "at", "ran_at"]),
                    field_or_dash(entry, &["summary", "message", "status"])
                )),
                Line::from(format!(
                    "matched={} applied={} error={}",
                    field_or_dash(entry, &["matched", "matched_count"]),
                    field_or_dash(entry, &["applied", "applied_count"]),
                    field_or_dash(entry, &["error", "last_error"])
                )),
                Line::from(""),
            ]
        })
        .collect()
}

fn dry_run_lines(entries: &[serde_json::Value]) -> Vec<Line<'static>> {
    if entries.is_empty() {
        return vec![
            Line::from("No dry-run output yet."),
            Line::from(""),
            Line::from("Press D on a rule to preview exactly what would change."),
        ];
    }

    entries
        .iter()
        .flat_map(|entry| {
            vec![
                Line::from(format!(
                    "{} / {}",
                    field_or_dash(entry, &["subject", "summary", "message"]),
                    field_or_dash(entry, &["action", "planned_action"])
                )),
                Line::from(format!(
                    "from={} labels={}",
                    field_or_dash(entry, &["from", "sender"]),
                    field_or_dash(entry, &["labels", "label_names"])
                )),
                Line::from(""),
            ]
        })
        .collect()
}

fn summary_line(
    label: &str,
    value: &str,
    active: bool,
    theme: &crate::mxr_tui::theme::Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<10}"),
            if active {
                Style::default().fg(theme.accent).bold()
            } else {
                Style::default().fg(theme.text_muted)
            },
        ),
        Span::styled(value.to_string(), Style::default().fg(theme.text_primary)),
    ])
}

fn draw_footer(frame: &mut Frame, area: Rect, text: &str, theme: &crate::mxr_tui::theme::Theme) {
    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        area,
    );
}

fn truncate_line(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        text.to_string()
    } else {
        let mut truncated = text.chars().take(max_len.saturating_sub(1)).collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

fn field_or_dash(value: &serde_json::Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(|candidate| candidate.as_str()) {
            if !text.is_empty() {
                return text.to_string();
            }
        }
        if let Some(number) = value.get(*key).and_then(|candidate| candidate.as_i64()) {
            return number.to_string();
        }
    }
    "-".into()
}

trait RulesSelectionExt {
    fn selected_rule(&self) -> Option<&serde_json::Value>;
}

impl RulesSelectionExt for RulesPageState {
    fn selected_rule(&self) -> Option<&serde_json::Value> {
        self.rules.get(self.selected_index)
    }
}
