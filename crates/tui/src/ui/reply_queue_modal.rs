use super::centered_rect;
use crate::app::ReplyQueueModalState;
use crate::theme::Theme;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;

const MODAL_WIDTH_PERCENT: u16 = 80;
const MODAL_HEIGHT_PERCENT: u16 = 70;

pub fn draw(frame: &mut Frame, area: Rect, state: &ReplyQueueModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let modal_area = centered_rect(MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT, area);
    Clear.render(modal_area, frame.buffer_mut());

    let title = " Reply Later — j/k move · Enter/r reply · F reply to each · w out of the queue · Esc close ";
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    if let Some(message) = &state.error {
        let paragraph = Paragraph::new(format!("Failed to load reply queue: {message}"))
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.loading {
        let paragraph = Paragraph::new("Loading reply queue...")
            .style(Style::default().fg(theme.text_muted))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.messages.is_empty() {
        let paragraph = Paragraph::new(
            "Reply queue is empty.\n\nFlag messages with `b` while reading to add them here.",
        )
        .style(Style::default().fg(theme.text_muted))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, inner.inner(Margin::new(2, 2)));
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(inner);

    let items: Vec<ListItem> = state
        .messages
        .iter()
        .enumerate()
        .map(|(idx, env)| {
            let style = if idx == state.selected_index {
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.selection_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let sender = env
                .from
                .name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| env.from.email.clone());
            let label = format!(" {sender} · {}", env.subject);
            ListItem::new(label).style(style)
        })
        .collect();
    let list = List::new(items).block(
        Block::default()
            .title(" Flagged ")
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(theme.text_muted)),
    );
    frame.render_widget(list, chunks[0]);

    let detail_area = chunks[1].inner(Margin::new(1, 0));
    if let Some(env) = state.selected() {
        let label_style = Style::default().fg(theme.text_muted);
        let lines = vec![
            Line::from(vec![
                Span::styled("Subject: ", label_style),
                Span::raw(env.subject.clone()),
            ]),
            Line::from(vec![
                Span::styled("From:    ", label_style),
                Span::raw(env.from.email.clone()),
            ]),
            Line::from(vec![
                Span::styled("Date:    ", label_style),
                Span::raw(env.date.format("%Y-%m-%d %H:%M").to_string()),
            ]),
            Line::from(""),
            Line::from(env.snippet.clone()),
        ];
        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, detail_area);
    }
}
