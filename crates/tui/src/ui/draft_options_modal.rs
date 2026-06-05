//! Draft Options modal — choose the tone (register) and length for an AI
//! reply draft, or leave them on "Auto" to let the daemon infer them from how
//! you write to this person (the TUI equivalent of the web "Adjust"
//! disclosure). Tab switches field, ←/→ cycle the option, Enter generates,
//! Esc cancels.

use crate::app::{DraftOptionsField, DraftOptionsModalState};
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &DraftOptionsModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let popup_area = centered_rect(56, 40, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Draft reply — tone & length ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Tab: field   ←/→: option   Enter: generate   Esc: cancel",
            Style::default().fg(theme.text_muted),
        ))),
        chunks[0],
    );

    render_field(
        frame,
        chunks[2],
        "Register",
        &DraftOptionsModalState::REGISTER_OPTIONS,
        state.register_idx,
        state.active == DraftOptionsField::Register,
        theme,
    );
    render_field(
        frame,
        chunks[3],
        "Length",
        &DraftOptionsModalState::LENGTH_OPTIONS,
        state.length_idx,
        state.active == DraftOptionsField::Length,
        theme,
    );
}

fn render_field(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    options: &[&str],
    selected: usize,
    active: bool,
    theme: &Theme,
) {
    let label_style = if active {
        Style::default().fg(theme.accent).bold()
    } else {
        Style::default().fg(theme.text_muted)
    };
    let mut spans = Vec::new();
    for (idx, option) in options.iter().enumerate() {
        let is_selected = idx == selected;
        let style = if is_selected && active {
            Style::default()
                .bg(theme.selection_bg)
                .fg(theme.selection_fg)
                .add_modifier(Modifier::BOLD)
        } else if is_selected {
            theme.accent_style().add_modifier(Modifier::BOLD)
        } else {
            theme.muted_style()
        };
        spans.push(Span::styled(format!(" {option} "), style));
        spans.push(Span::raw(" "));
    }
    let lines = vec![
        Line::from(Span::styled(label.to_string(), label_style)),
        Line::from(spans),
    ];
    frame.render_widget(Paragraph::new(lines), area);
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
