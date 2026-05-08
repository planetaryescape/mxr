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

    let modal_area = centered_rect(area, MODAL_WIDTH_PERCENT, MODAL_HEIGHT_PERCENT);
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
    use mxr_protocol::SnippetData;
    use mxr_test_support::render_to_string;

    fn snippet(name: &str, body: &str) -> SnippetData {
        SnippetData {
            name: name.to_string(),
            body: body.to_string(),
            vars: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn draws_loading_placeholder_while_request_in_flight() {
        let mut state = SnippetsModalState::default();
        state.open_loading();
        let snapshot = render_to_string(80, 20, |frame| {
            draw(frame, Rect::new(0, 0, 80, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("Loading snippets..."),
            "loading placeholder must surface while request is in-flight; got:\n{snapshot}"
        );
    }

    #[test]
    fn renders_snippet_name_in_list_and_body_in_preview() {
        let mut state = SnippetsModalState::default();
        state.open_loading();
        state.set_snippets(vec![
            snippet("thanks", "Thanks for reaching out!"),
            snippet("sig", "— Bhekani"),
        ]);

        let snapshot = render_to_string(100, 20, |frame| {
            draw(frame, Rect::new(0, 0, 100, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("thanks"),
            "list must render snippet name; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("Thanks for reaching out!"),
            "body of selected snippet must appear in preview pane; got:\n{snapshot}",
        );
    }

    #[test]
    fn empty_state_shows_cli_hint() {
        let mut state = SnippetsModalState::default();
        state.open_loading();
        state.set_snippets(vec![]);
        let snapshot = render_to_string(80, 20, |frame| {
            draw(frame, Rect::new(0, 0, 80, 20), &state, &Theme::default());
        });
        assert!(
            snapshot.contains("No snippets yet"),
            "empty state copy must surface; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("mxr snippets set"),
            "empty state must point users at the CLI command; got:\n{snapshot}",
        );
    }

    #[test]
    fn select_next_wraps_at_end() {
        let mut state = SnippetsModalState::default();
        state.open_loading();
        state.set_snippets(vec![snippet("a", "a"), snippet("b", "b")]);
        state.select_next();
        assert_eq!(state.selected_index, 1);
        state.select_next();
        assert_eq!(
            state.selected_index, 0,
            "select_next must wrap at end of list"
        );
    }

    #[test]
    fn select_prev_wraps_at_zero() {
        let mut state = SnippetsModalState::default();
        state.open_loading();
        state.set_snippets(vec![snippet("a", "a"), snippet("b", "b")]);
        assert_eq!(state.selected_index, 0);
        state.select_prev();
        assert_eq!(
            state.selected_index, 1,
            "select_prev must wrap from index 0 to last"
        );
    }
}
