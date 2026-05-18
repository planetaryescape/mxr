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

    let modal_area = centered_rect(area, MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT);
    Clear.render(modal_area, frame.buffer_mut());

    let title = " Reply Later — ↑/↓ navigate · Enter/r reply · Esc close ";
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

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};
    use mxr_core::Envelope;
    use mxr_test_support::render_to_string;

    fn envelope(subject: &str, sender_email: &str, snippet: &str) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: "fake".into(),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: None,
                email: sender_email.to_string(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: subject.to_string(),
            date: Utc::now(),
            flags: MessageFlags::empty(),
            snippet: snippet.to_string(),
            has_attachments: false,
            size_bytes: 1024,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        }
    }

    #[test]
    fn empty_queue_points_user_at_bookmark_key() {
        let mut state = ReplyQueueModalState::default();
        state.open_loading();
        state.set_messages(vec![]);
        let snapshot = render_to_string(80, 18, |frame| {
            draw(frame, Rect::new(0, 0, 80, 18), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Flag messages with `b`"),
            "empty state must teach the bookmark key; got:\n{snapshot}",
        );
    }

    #[test]
    fn renders_subject_and_snippet_for_selected() {
        let mut state = ReplyQueueModalState::default();
        state.open_loading();
        state.set_messages(vec![envelope(
            "Status update",
            "alice@example.com",
            "Plan ready for review",
        )]);
        let snapshot = render_to_string(100, 18, |frame| {
            draw(frame, Rect::new(0, 0, 100, 18), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Status update"),
            "subject must surface; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("Plan ready for review"),
            "snippet must surface in detail pane; got:\n{snapshot}",
        );
    }

    #[test]
    fn select_next_wraps() {
        let mut state = ReplyQueueModalState::default();
        state.open_loading();
        state.set_messages(vec![envelope("a", "a@x", "a"), envelope("b", "b@x", "b")]);
        state.select_next();
        assert_eq!(state.selected_index, 1);
        state.select_next();
        assert_eq!(state.selected_index, 0);
    }
}
