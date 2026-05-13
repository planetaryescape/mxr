//! Slice 10: modal form for editing the active analytics view's
//! filter parameters in one place. Renders as a centered popup over
//! the analytics screen. Field navigation: Tab/Shift-Tab. Submit:
//! Enter. Cancel: Esc.

use crate::app::AnalyticsFilterModalState;
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: Option<&AnalyticsFilterModalState>,
    theme: &Theme,
) {
    let Some(state) = state else { return };

    let popup_area = centered_rect(60, 60, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(format!(" Filter: {} ", state.view.label()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let mut constraints = vec![Constraint::Length(1)];
    for _ in &state.fields {
        constraints.push(Constraint::Length(2));
    }
    constraints.push(Constraint::Length(1));
    constraints.push(Constraint::Min(0));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.clone())
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Tab: next field   Shift-Tab: prev   ←/→: select option   Enter: apply   Esc: cancel",
            Style::default().fg(theme.text_muted),
        ))),
        chunks[0],
    );

    for (idx, field) in state.fields.iter().enumerate() {
        let chunk = chunks[idx + 1];
        let is_active = idx == state.active_field;
        let label_style = if is_active {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default().fg(theme.text_muted)
        };
        let value_style = if is_active {
            Style::default().fg(theme.accent).bold()
        } else {
            Style::default()
        };
        let value_line = if field.options.is_empty() {
            Line::from(Span::styled(
                if field.value.is_empty() && is_active {
                    "_".into()
                } else {
                    field.value.clone()
                },
                value_style,
            ))
        } else {
            option_line(field, is_active, theme)
        };
        let lines = vec![
            Line::from(Span::styled(field.label.clone(), label_style)),
            value_line,
        ];
        frame.render_widget(Paragraph::new(lines), chunk);
    }

    let err_chunk = chunks[1 + state.fields.len()];
    if let Some(err) = state.validation_error.as_ref() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                err.clone(),
                Style::default().fg(theme.error).bold(),
            ))),
            err_chunk,
        );
    }
}

fn option_line<'a>(
    field: &'a crate::app::AnalyticsFilterField,
    is_active: bool,
    theme: &Theme,
) -> Line<'a> {
    let mut spans = Vec::new();
    for option in &field.options {
        let selected = option == &field.value;
        let style = if selected && is_active {
            Style::default()
                .bg(theme.selection_bg)
                .fg(theme.selection_fg)
                .add_modifier(Modifier::BOLD)
        } else if selected {
            theme.accent_style().add_modifier(Modifier::BOLD)
        } else {
            theme.muted_style()
        };
        spans.push(Span::styled(format!(" {option} "), style));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
