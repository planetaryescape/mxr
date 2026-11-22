use super::centered_rect;
use crate::app::SnippetsModalState;
use crate::theme::Theme;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;

const MODAL_WIDTH_PERCENT: u16 = 80;
const MODAL_HEIGHT_PERCENT: u16 = 70;

pub fn draw(frame: &mut Frame, area: Rect, state: &SnippetsModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let modal_area = centered_rect(MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT, area);
    Clear.render(modal_area, frame.buffer_mut());

    let block = Block::default()
        .title(" Snippets — Esc close, ↑/↓ navigate, `mxr snippets` to edit ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    if let Some(message) = &state.error {
        let paragraph = Paragraph::new(format!("Failed to load snippets: {message}"))
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.loading {
        let paragraph = Paragraph::new("Loading snippets...")
            .style(Style::default().fg(theme.text_muted))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.snippets.is_empty() {
        let paragraph = Paragraph::new(
            "No snippets yet.\n\nCreate one via:\n  mxr snippets set <name> --body \"...\"",
        )
        .style(Style::default().fg(theme.text_muted))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner.inner(Margin::new(2, 2)));
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(0)])
        .split(inner);

    let items: Vec<ListItem> = state
        .snippets
        .iter()
        .enumerate()
        .map(|(idx, snippet)| {
            let style = if idx == state.selected_index {
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.selection_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            ListItem::new(snippet.name.clone()).style(style)
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .title(" Names ")
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(theme.text_muted)),
        )
        .highlight_style(
            Style::default()
                .bg(theme.selection_bg)
                .fg(theme.selection_fg),
        );
    frame.render_widget(list, chunks[0]);

    let body_area = chunks[1].inner(Margin::new(1, 0));
    if let Some(snippet) = state.selected() {
        let mut lines = Vec::new();
        if !snippet.vars.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("Vars: {}", snippet.vars.join(", ")),
                Style::default().fg(theme.text_muted),
            )));
            lines.push(Line::from(""));
        }
        for body_line in snippet.body.lines() {
            lines.push(Line::from(body_line.to_string()));
        }
        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, body_area);
    }
}
