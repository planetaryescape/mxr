use super::centered_rect;
use crate::app::ScreenerModalState;
use crate::theme::Theme;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;

const MODAL_WIDTH_PERCENT: u16 = 80;
const MODAL_HEIGHT_PERCENT: u16 = 70;

pub fn draw(frame: &mut Frame, area: Rect, state: &ScreenerModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let modal_area = centered_rect(MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT, area);
    Clear.render(modal_area, frame.buffer_mut());

    let title = " Screener — a:allow d:deny f:feed p:paper-trail · Esc close ";
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    if let Some(message) = &state.error {
        let paragraph = Paragraph::new(format!("Failed to load screener queue: {message}"))
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.loading {
        let paragraph = Paragraph::new("Loading screener queue...")
            .style(Style::default().fg(theme.text_muted))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.entries.is_empty() {
        let paragraph = Paragraph::new(
            "Screener queue is empty.\n\nNew senders awaiting consent will appear here as they arrive.",
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
        .entries
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let style = if idx == state.selected_index {
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.selection_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let display = entry
                .display_name
                .clone()
                .unwrap_or_else(|| entry.sender_email.clone());
            let label = format!(" {} ({})", display, entry.message_count);
            ListItem::new(label).style(style)
        })
        .collect();
    let list = List::new(items).block(
        Block::default()
            .title(" Senders ")
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(theme.text_muted)),
    );
    frame.render_widget(list, chunks[0]);

    let detail_area = chunks[1].inner(Margin::new(1, 0));
    if let Some(entry) = state.selected() {
        let label_style = Style::default().fg(theme.text_muted);
        let lines = vec![
            Line::from(vec![
                Span::styled("Email:    ", label_style),
                Span::raw(entry.sender_email.clone()),
            ]),
            Line::from(vec![
                Span::styled("Name:     ", label_style),
                Span::raw(
                    entry
                        .display_name
                        .clone()
                        .unwrap_or_else(|| "(no display name)".into()),
                ),
            ]),
            Line::from(vec![
                Span::styled("Latest:   ", label_style),
                Span::raw(entry.latest_subject.clone()),
            ]),
            Line::from(vec![
                Span::styled("Last at:  ", label_style),
                Span::raw(entry.latest_at.format("%Y-%m-%d %H:%M").to_string()),
            ]),
            Line::from(vec![
                Span::styled("Messages: ", label_style),
                Span::raw(entry.message_count.to_string()),
            ]),
        ];
        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, detail_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::AccountId;
    use mxr_protocol::ScreenerQueueEntryData;
    use mxr_test_support::render_to_string;

    fn entry(email: &str, display: Option<&str>, count: u32) -> ScreenerQueueEntryData {
        ScreenerQueueEntryData {
            sender_email: email.to_string(),
            display_name: display.map(std::string::ToString::to_string),
            message_count: count,
            latest_subject: format!("From {email}"),
            latest_at: Utc::now(),
        }
    }

    #[test]
    fn renders_sender_list_and_detail_pane() {
        let mut state = ScreenerModalState::default();
        state.open_loading(AccountId::new());
        state.set_entries(vec![
            entry("alice@example.com", Some("Alice"), 4),
            entry("spam@junk.tld", None, 21),
        ]);
        let snapshot = render_to_string(100, 18, |frame| {
            draw(frame, Rect::new(0, 0, 100, 18), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Alice"),
            "sender list must render display name; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("alice@example.com"),
            "detail pane must render selected sender's email; got:\n{snapshot}",
        );
    }
}
