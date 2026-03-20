use crate::app::{RuleFormState, RulesPageState, RulesPanel};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &RulesPageState, theme: &crate::theme::Theme) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    let items = state
        .rules
        .iter()
        .map(|rule| {
            let enabled = if rule["enabled"].as_bool().unwrap_or(false) {
                "on"
            } else {
                "off"
            };
            ListItem::new(format!(
                "{} [{}] {}",
                rule["name"].as_str().unwrap_or("(unnamed)"),
                enabled,
                rule["priority"].as_i64().unwrap_or_default()
            ))
        })
        .collect::<Vec<_>>();
    let list = List::new(items).block(
        Block::default()
            .title(" Rules ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.accent)),
    );
    let mut stateful = ListState::default().with_selected(Some(state.selected_index));
    frame.render_stateful_widget(list, chunks[0], &mut stateful);

    let title = match state.panel {
        RulesPanel::Details => " Rule Details ",
        RulesPanel::History => " Rule History ",
        RulesPanel::DryRun => " Rule Dry Run ",
        RulesPanel::Form => " Rule Form ",
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warning));

    if state.form.visible {
        draw_form(frame, chunks[1], &state.form, theme);
        return;
    }

    let lines = match state.panel {
        RulesPanel::Details => pretty_lines(state.detail.as_ref()),
        RulesPanel::History => value_list_lines(&state.history),
        RulesPanel::DryRun => value_list_lines(&state.dry_run),
        RulesPanel::Form => Vec::new(),
    };
    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, chunks[1]);
}

fn draw_form(frame: &mut Frame, area: Rect, form: &RuleFormState, theme: &crate::theme::Theme) {
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
        None => vec![Line::from("No rule selected")],
    }
}

fn value_list_lines(values: &[serde_json::Value]) -> Vec<Line<'static>> {
    if values.is_empty() {
        return vec![Line::from("No data")];
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
