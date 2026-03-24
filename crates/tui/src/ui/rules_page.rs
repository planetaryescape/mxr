use crate::mxr_tui::app::{RuleFormState, RulesPageState, RulesPanel};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &RulesPageState,
    theme: &crate::mxr_tui::theme::Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(area);
    let detail_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(chunks[1]);

    if state.rules.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("Rules are repeatable mail automations."),
                Line::from(""),
                Line::from("Start with n to create a rule."),
                Line::from("Use H for history and D for dry runs once rules exist."),
            ])
            .block(
                Block::default()
                    .title(" Rules ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent)),
            )
            .wrap(Wrap { trim: false }),
            chunks[0],
        );
    } else {
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
                ListItem::new(Line::from(vec![
                    Span::styled(
                        rule["name"].as_str().unwrap_or("(unnamed)").to_string(),
                        Style::default().fg(theme.text_primary).bold(),
                    ),
                    Span::styled(
                        format!("  [{enabled} · p{priority}]"),
                        Style::default().fg(theme.text_secondary),
                    ),
                ]))
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
        frame.render_stateful_widget(list, chunks[0], &mut stateful);
    }

    let tabs = Tabs::new(["Overview", "History", "Dry Run", "Edit / New"])
        .select(match state.panel {
            RulesPanel::Details => 0,
            RulesPanel::History => 1,
            RulesPanel::DryRun => 2,
            RulesPanel::Form => 3,
        })
        .block(
            Block::default()
                .title(" Rule Panels ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning)),
        )
        .highlight_style(Style::default().fg(theme.accent).bold())
        .divider(Span::styled(" | ", Style::default().fg(theme.text_muted)));
    frame.render_widget(tabs, detail_chunks[0]);

    if state.form.visible {
        draw_form(frame, detail_chunks[1], &state.form, theme);
        draw_footer(
            frame,
            detail_chunks[2],
            "Tab:next  Shift-Tab:prev  Enter:save  Esc:close form",
            theme,
        );
        return;
    }

    let lines = match state.panel {
        RulesPanel::Details => pretty_lines(state.detail.as_ref()),
        RulesPanel::History => value_list_lines(&state.history),
        RulesPanel::DryRun => value_list_lines(&state.dry_run),
        RulesPanel::Form => Vec::new(),
    };
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(match state.panel {
                    RulesPanel::Details => " Overview ",
                    RulesPanel::History => " History ",
                    RulesPanel::DryRun => " Dry Run ",
                    RulesPanel::Form => " Form ",
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

fn draw_form(
    frame: &mut Frame,
    area: Rect,
    form: &RuleFormState,
    theme: &crate::mxr_tui::theme::Theme,
) {
    let fields = [
        ("Name", form.name.as_str()),
        ("Condition", form.condition.as_str()),
        ("Action", form.action.as_str()),
        ("Priority", form.priority.as_str()),
        ("Enabled", if form.enabled { "true" } else { "false" }),
    ];
    let mut lines = Vec::new();
    for (index, (label, value)) in fields.iter().enumerate() {
        let style = if index == form.active_field {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default().fg(theme.text_primary)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{label:<10}"), style),
            Span::raw(*value),
        ]));
    }
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Rule Form ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.warning)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn pretty_lines(value: Option<&serde_json::Value>) -> Vec<Line<'static>> {
    match value {
        Some(value) => serde_json::to_string_pretty(value)
            .unwrap_or_else(|_| "{}".to_string())
            .lines()
            .map(|line| Line::from(line.to_string()))
            .collect(),
        None => vec![
            Line::from("Pick a rule to inspect its details."),
            Line::from(""),
            Line::from("Use H for history or D for a dry run."),
        ],
    }
}

fn value_list_lines(values: &[serde_json::Value]) -> Vec<Line<'static>> {
    if values.is_empty() {
        return vec![
            Line::from("No data yet."),
            Line::from(""),
            Line::from("Run the selected panel again after the rule has activity."),
        ];
    }
    values
        .iter()
        .flat_map(|value| {
            let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string());
            let mut lines = text
                .lines()
                .map(|line| Line::from(line.to_string()))
                .collect::<Vec<_>>();
            lines.push(Line::from(""));
            lines
        })
        .collect()
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
