//! Draft Options modal: say what an AI reply should say, and choose the
//! tone (register) and length or leave them on "Auto" to follow how you
//! write to this person (the TUI equivalent of the web "Adjust"
//! disclosure). Tab switches field, ←/→ cycle an option, Enter generates,
//! Esc cancels.

use super::centered_rect;
use crate::app::{DraftOptionsField, DraftOptionsModalState};
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, state: &DraftOptionsModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let popup_area = centered_rect(60, 46, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Draft reply ")
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
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Tab: field   ←/→: option   Enter: draft   Esc: cancel",
            Style::default().fg(theme.text_muted),
        ))),
        chunks[0],
    );

    let typing = state.active == DraftOptionsField::Instruction;
    let label_style = if typing {
        Style::default().fg(theme.accent).bold()
    } else {
        Style::default().fg(theme.text_muted)
    };
    let text = if state.instruction.is_empty() && !typing {
        Span::styled("(optional) answer what they asked", theme.muted_style())
    } else {
        Span::raw(format!(
            "{}{}",
            state.instruction,
            if typing { "▏" } else { "" }
        ))
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("What should it say?", label_style)),
            Line::from(text),
        ]),
        chunks[2],
    );

    render_field(
        frame,
        chunks[4],
        "Register",
        &DraftOptionsModalState::REGISTER_OPTIONS,
        state.register_idx,
        state.active == DraftOptionsField::Register,
        theme,
    );
    render_field(
        frame,
        chunks[5],
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
