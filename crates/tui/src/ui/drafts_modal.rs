use super::centered_rect;
use crate::app::DraftsModalState;
use crate::theme::Theme;
use mxr_compose::draft_codec::format_addresses;
use ratatui::layout::Margin;
use ratatui::prelude::*;
use ratatui::widgets::*;

const MODAL_WIDTH_PERCENT: u16 = 80;
const MODAL_HEIGHT_PERCENT: u16 = 70;

pub fn draw(frame: &mut Frame, area: Rect, state: &DraftsModalState, theme: &Theme) {
    if !state.visible {
        return;
    }

    let modal_area = centered_rect(MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT, area);
    Clear.render(modal_area, frame.buffer_mut());

    let title = " Drafts — ↑/↓ navigate · Enter/e edit · Esc close ";
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    if let Some(message) = &state.error {
        let paragraph = Paragraph::new(format!("Failed to load drafts: {message}"))
            .style(Style::default().fg(theme.error))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.loading {
        let paragraph = Paragraph::new("Loading drafts...")
            .style(Style::default().fg(theme.text_muted))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

    if state.drafts.is_empty() {
        let paragraph =
            Paragraph::new("No saved drafts.\n\nDrafts you save from Compose (`c`) show up here.")
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
        .drafts
        .iter()
        .enumerate()
        .map(|(idx, draft)| {
            let style = if idx == state.selected_index {
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.selection_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let subject = if draft.subject.trim().is_empty() {
                "(no subject)"
            } else {
                draft.subject.as_str()
            };
            let to = format_addresses(&draft.to);
            let label = if to.is_empty() {
                format!(" {subject}")
            } else {
                format!(" {subject} · {to}")
            };
            ListItem::new(label).style(style)
        })
        .collect();
    let list = List::new(items).block(
        Block::default()
            .title(" Saved ")
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(theme.text_muted)),
    );
    frame.render_widget(list, chunks[0]);

    let detail_area = chunks[1].inner(Margin::new(1, 0));
    if let Some(draft) = state.selected() {
        let label_style = Style::default().fg(theme.text_muted);
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Subject: ", label_style),
                Span::raw(if draft.subject.trim().is_empty() {
                    "(no subject)".to_string()
                } else {
                    draft.subject.clone()
                }),
            ]),
            Line::from(vec![
                Span::styled("To:      ", label_style),
                Span::raw(format_addresses(&draft.to)),
            ]),
        ];
        if !draft.cc.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Cc:      ", label_style),
                Span::raw(format_addresses(&draft.cc)),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("Updated: ", label_style),
            Span::raw(draft.updated_at.format("%Y-%m-%d %H:%M").to_string()),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(draft.body_markdown.clone()));

        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, detail_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use mxr_core::id::{AccountId, DraftId};
    use mxr_core::types::Address;
    use mxr_core::Draft;
    use mxr_test_support::render_to_string;

    fn draft(subject: &str, to_email: &str, body: &str) -> Draft {
        let now: DateTime<Utc> = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![Address {
                name: None,
                email: to_email.to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: subject.to_string(),
            body_markdown: body.to_string(),
            attachments: vec![],
            inline_calendar_reply: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn draws_loading_placeholder_while_request_in_flight() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        let snapshot = render_to_string(80, 20, |frame| {
            draw(frame, Rect::new(0, 0, 80, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Loading drafts..."),
            "loading placeholder must surface while request is in-flight; got:\n{snapshot}"
        );
    }

    #[test]
    fn empty_state_points_at_compose_key() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        state.set_drafts(vec![]);
        let snapshot = render_to_string(80, 20, |frame| {
            draw(frame, Rect::new(0, 0, 80, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("No saved drafts"),
            "empty state copy must surface; got:\n{snapshot}",
        );
    }

    #[test]
    fn renders_subject_and_recipient_in_list_and_detail() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        state.set_drafts(vec![draft(
            "Q4 plan",
            "alice@example.com",
            "Draft body text.",
        )]);

        let snapshot = render_to_string(100, 20, |frame| {
            draw(frame, Rect::new(0, 0, 100, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Q4 plan"),
            "list must render draft subject; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("alice@example.com"),
            "detail pane must render recipient; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("Draft body text."),
            "detail pane must render body preview; got:\n{snapshot}",
        );
    }

    #[test]
    fn select_next_wraps() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        state.set_drafts(vec![draft("a", "a@x.com", "a"), draft("b", "b@x.com", "b")]);
        state.select_next();
        assert_eq!(state.selected_index, 1);
        state.select_next();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn select_prev_wraps_at_zero() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        state.set_drafts(vec![draft("a", "a@x.com", "a"), draft("b", "b@x.com", "b")]);
        state.select_prev();
        assert_eq!(state.selected_index, 1);
    }
}
