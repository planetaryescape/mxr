use super::centered_rect;
use crate::app::{DraftsModalState, StoredDraftOperation};
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

    let title = " Drafts — ↑/↓ navigate · Enter/e edit · d delete · p provider copy · Esc close ";
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    if let Some(operation) = &state.confirmation {
        draw_confirmation(frame, inner, operation, theme);
        return;
    }

    if state.operation_in_flight {
        let paragraph = Paragraph::new("Applying draft operation...")
            .style(Style::default().fg(theme.text_muted))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner.inner(Margin::new(1, 1)));
        return;
    }

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
        if draft.content.is_html() {
            lines.push(Line::from(Span::styled(
                "HTML body — not editable here",
                Style::default().fg(theme.warning),
            )));
            lines.push(Line::from(Span::styled(
                "Edit the source, then `mxr compose --html-file <path>`.",
                label_style,
            )));
        }
        lines.push(Line::from(""));
        lines.extend(
            preview_body(&draft.content)
                .lines()
                .map(|line| Line::from(line.to_string())),
        );

        let paragraph = Paragraph::new(lines)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, detail_area);
    }
}

fn draw_confirmation(
    frame: &mut Frame,
    area: Rect,
    operation: &StoredDraftOperation,
    theme: &Theme,
) {
    let (heading, explanation, draft, confirm_label) = match operation {
        StoredDraftOperation::Delete { draft } => (
            "Delete this local draft?",
            "This permanently removes mxr's canonical copy.",
            draft,
            "[y] delete",
        ),
        StoredDraftOperation::Push { draft, provider } => (
            "Copy this draft to the provider?",
            "The local draft stays canonical. Repeating creates another provider draft.",
            draft,
            if provider.eq_ignore_ascii_case("gmail") {
                "[y] copy to Gmail Drafts"
            } else {
                "[y] copy to provider Drafts"
            },
        ),
    };

    let subject = if draft.subject.trim().is_empty() {
        "(no subject)"
    } else {
        draft.subject.as_str()
    };
    let mut lines = vec![
        Line::from(Span::styled(
            heading,
            Style::default().fg(theme.warning).bold(),
        )),
        Line::from(""),
        Line::from(format!("Subject: {subject}")),
        Line::from(format!("To:      {}", format_addresses(&draft.to))),
        Line::from(format!("Draft:   {}", draft.id)),
    ];
    if let StoredDraftOperation::Push { provider, .. } = operation {
        lines.push(Line::from(format!("Provider: {provider}")));
    }
    lines.extend([
        Line::from(""),
        Line::from(explanation),
        Line::from(""),
        Line::from(vec![
            Span::styled(confirm_label, Style::default().fg(theme.accent).bold()),
            Span::raw("   [Esc/n] cancel"),
        ]),
    ]);

    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false }),
        area.inner(Margin::new(2, 1)),
    );
}

/// Flatten a draft body into the text the detail pane shows.
///
/// `DraftContent::analysis_text` is empty for an HTML draft that carries no
/// `text/plain` alternative, which would leave the pane blank on exactly the
/// drafts the user most wants to eyeball. Routing through the reader gives
/// every kind something readable. The stripping passes are off: this previews
/// what the author wrote, not what the reader would trim from someone else's
/// mail.
fn preview_body(content: &mxr_core::DraftContent) -> String {
    let (text, html) = content.reader_input();
    // A blank alternative is dropped rather than preferred: `--text-file`
    // accepts an empty file, and the reader takes a supplied text over the
    // document whenever there is one — which would blank the pane just as
    // surely as no alternative at all.
    let text = text.filter(|text| !text.trim().is_empty());
    let Some(html) = html else {
        return text.unwrap_or_default().to_string();
    };
    let config = mxr_reader::ReaderConfig {
        html_command: None,
        strip_signatures: false,
        collapse_quotes: false,
        strip_boilerplate: false,
        strip_tracking: false,
    };
    mxr_reader::clean(text, Some(html), &config).content
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use mxr_core::id::{AccountId, DraftId};
    use mxr_core::types::{Address, DraftContent};
    use mxr_core::Draft;
    use mxr_test_support::render_to_string;

    fn draft(subject: &str, to_email: &str, body: &str) -> Draft {
        let now: DateTime<Utc> = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![Address {
                name: None,
                email: to_email.to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: subject.to_string(),
            content: DraftContent::markdown(body),
            attachments: vec![],
            inline_assets: vec![],
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
    fn delete_preview_names_the_exact_draft_and_requires_confirmation() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        let draft = draft("Q4 plan", "alice@example.com", "Draft body text.");
        let draft_id = draft.id.clone();
        state.set_drafts(vec![draft]);
        assert!(state.preview_delete());

        let snapshot = render_to_string(100, 20, |frame| {
            draw(frame, Rect::new(0, 0, 100, 20), &state, &Theme::default());
        });
        assert!(snapshot.contains("Delete this local draft?"));
        assert!(snapshot.contains("Q4 plan"));
        assert!(snapshot.contains(&draft_id.to_string()));
        assert!(snapshot.contains("[y] delete"));
    }

    #[test]
    fn provider_copy_preview_explains_one_way_duplicate_semantics() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        state.set_drafts(vec![draft(
            "Q4 plan",
            "alice@example.com",
            "Draft body text.",
        )]);
        assert!(state.preview_push("gmail".into()));

        let snapshot = render_to_string(100, 20, |frame| {
            draw(frame, Rect::new(0, 0, 100, 20), &state, &Theme::default());
        });
        assert!(snapshot.contains("Copy this draft to the provider?"));
        assert!(snapshot.contains("Provider: gmail"));
        assert!(snapshot.contains("local draft stays canonical"));
        assert!(snapshot.contains("another provider draft"));
        assert!(snapshot.contains("copy to Gmail Drafts"));
    }

    fn html_draft(subject: &str, html: &str, text: Option<&str>) -> Draft {
        let mut draft = draft(subject, "alice@example.com", "");
        draft.content = DraftContent::html(html, text.map(str::to_string));
        draft
    }

    #[test]
    fn html_draft_detail_renders_readable_text_rather_than_a_blank_body() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        state.set_drafts(vec![html_draft(
            "Launch",
            "<html><body><h1>Ship day</h1><p>We go live on Friday.</p></body></html>",
            None,
        )]);

        let snapshot = render_to_string(100, 20, |frame| {
            draw(frame, Rect::new(0, 0, 100, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("We go live on Friday."),
            "an HTML draft with no text alternative must still show its body; got:\n{snapshot}",
        );
    }

    #[test]
    fn html_draft_detail_falls_back_to_the_document_when_the_text_alternative_is_blank() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        // `mxr compose --html-file … --text-file …` accepts an empty or
        // whitespace-only text file, so a draft can reach the modal carrying a
        // text alternative that says nothing. Absent and present-but-blank are
        // the same thing to a reader.
        state.set_drafts(vec![html_draft(
            "Launch",
            "<html><body><h1>Ship day</h1><p>We go live on Friday.</p></body></html>",
            Some("   \n\t\n"),
        )]);

        let snapshot = render_to_string(100, 20, |frame| {
            draw(frame, Rect::new(0, 0, 100, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("We go live on Friday."),
            "a blank text alternative is no alternative at all: the document must \
             still be shown; got:\n{snapshot}",
        );
    }

    #[test]
    fn html_draft_detail_prefers_the_supplied_text_alternative() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        state.set_drafts(vec![html_draft(
            "Launch",
            "<p>markup version</p>",
            Some("author's own plain text"),
        )]);

        let snapshot = render_to_string(100, 20, |frame| {
            draw(frame, Rect::new(0, 0, 100, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("author's own plain text"),
            "the supplied text/plain alternative is what the recipient reads; got:\n{snapshot}",
        );
    }

    #[test]
    fn html_draft_detail_says_it_cannot_be_opened_in_the_editor() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        state.set_drafts(vec![html_draft("Launch", "<p>body</p>", None)]);

        let snapshot = render_to_string(100, 20, |frame| {
            draw(frame, Rect::new(0, 0, 100, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("HTML"),
            "the detail pane must tell the user the draft is HTML; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("not editable"),
            "the user must learn why Enter/e will not open this draft; got:\n{snapshot}",
        );
    }

    #[test]
    fn markdown_draft_detail_is_not_labelled_as_html() {
        let mut state = DraftsModalState::default();
        state.open_loading();
        state.set_drafts(vec![draft("Q4 plan", "alice@example.com", "Plain body.")]);

        let snapshot = render_to_string(100, 20, |frame| {
            draw(frame, Rect::new(0, 0, 100, 20), &state, &Theme::default());
        });
        assert!(
            !snapshot.contains("not editable"),
            "a markdown draft is editable and must not carry the HTML warning; got:\n{snapshot}",
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
