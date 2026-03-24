use crate::mxr_core::SearchMode;
use crate::mxr_tui::app::{
    ActivePane, MailListMode, MailListRow, SearchPageState, SearchPane, SearchUiStatus,
};
use crate::mxr_tui::ui::{mail_list, message_view, search_query::highlight_search_query};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;
use throbber_widgets_tui::{Throbber, BRAILLE_SIX};

#[expect(
    clippy::too_many_arguments,
    reason = "TUI draw entrypoint keeps call sites explicit"
)]
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &SearchPageState,
    rows: &[MailListRow],
    selected_set: &HashSet<crate::mxr_core::MessageId>,
    mail_list_mode: MailListMode,
    preview_messages: &[message_view::ThreadMessageBlock],
    preview_scroll: u16,
    theme: &crate::mxr_tui::theme::Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    let result_count = rows.len();
    let query_line = if state.query.is_empty() {
        Line::from(Span::styled(
            "type to search the full local index",
            Style::default().fg(theme.text_muted),
        ))
    } else {
        highlight_search_query(&state.query, theme)
    };
    let query = Paragraph::new(vec![
        query_line,
        query_status_line(state, result_count, theme),
    ])
    .block(
        Block::bordered()
            .title(query_title(state))
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.warning)),
    );
    frame.render_widget(query, chunks[0]);

    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[1]);
    let list_active_pane = match state.active_pane {
        SearchPane::Results => ActivePane::MailList,
        SearchPane::Preview => ActivePane::MessageView,
    };
    let preview_active_pane = match state.active_pane {
        SearchPane::Results => ActivePane::MailList,
        SearchPane::Preview => ActivePane::MessageView,
    };

    if should_render_results_blank(state, result_count) {
        frame.render_widget(
            Paragraph::new(results_blank_state(state))
                .block(
                    Block::default()
                        .title(results_title(state, result_count))
                        .borders(Borders::ALL)
                        .border_style(theme.border_style(state.active_pane == SearchPane::Results)),
                )
                .wrap(Wrap { trim: false }),
            inner[0],
        );
    } else {
        let title = results_title(state, result_count);
        mail_list::draw_view(
            frame,
            inner[0],
            &mail_list::MailListView {
                rows,
                selected_index: state.selected_index,
                scroll_offset: state.scroll_offset,
                active_pane: &list_active_pane,
                title: &title,
                selected_set,
                mode: mail_list_mode,
            },
            theme,
        );
    }

    if should_render_preview_blank(state, preview_messages) {
        frame.render_widget(
            Paragraph::new(preview_blank_state(state))
                .block(
                    Block::default()
                        .title(" Preview ")
                        .borders(Borders::ALL)
                        .border_style(theme.border_style(state.active_pane == SearchPane::Preview)),
                )
                .wrap(Wrap { trim: false }),
            inner[1],
        );
    } else {
        message_view::draw(
            frame,
            inner[1],
            preview_messages,
            preview_scroll,
            &preview_active_pane,
            theme,
        );
    }
}

fn search_mode_label(mode: SearchMode) -> &'static str {
    match mode {
        SearchMode::Lexical => "lexical",
        SearchMode::Hybrid => "hybrid",
        SearchMode::Semantic => "semantic",
    }
}

fn query_title(state: &SearchPageState) -> String {
    if state.editing {
        format!(
            " Search All Mail [{}] / query ",
            search_mode_label(state.mode)
        )
    } else {
        format!(" Search All Mail [{}] ", search_mode_label(state.mode))
    }
}

fn query_status_line(
    state: &SearchPageState,
    result_count: usize,
    theme: &crate::mxr_tui::theme::Theme,
) -> Line<'static> {
    let status = match state.ui_status {
        SearchUiStatus::Idle if state.query.is_empty() => {
            "full local index ready / type to search every account".to_string()
        }
        SearchUiStatus::Idle | SearchUiStatus::Loaded => match state.total_count {
            Some(total) => format!("loaded {result_count} of {total} matches"),
            None if state.has_more => format!("loaded {result_count}+ matches"),
            None if state.count_pending => format!("loaded {result_count} matches / counting"),
            None => format!("loaded {result_count} matches"),
        },
        SearchUiStatus::Debouncing => "waiting for a pause before searching".to_string(),
        SearchUiStatus::Searching => "searching full local index".to_string(),
        SearchUiStatus::LoadingMore => match state.total_count {
            Some(total) => format!("loading more / {result_count} of {total} shown"),
            None => format!("loading more / {result_count} shown"),
        },
        SearchUiStatus::Error => "search failed / adjust query and retry".to_string(),
    };

    let mut spans = Vec::new();
    if matches!(
        state.ui_status,
        SearchUiStatus::Debouncing | SearchUiStatus::Searching | SearchUiStatus::LoadingMore
    ) {
        spans.push(
            Throbber::default()
                .throbber_set(BRAILLE_SIX)
                .throbber_style(Style::default().fg(theme.accent))
                .to_symbol_span(&state.throbber),
        );
    }
    spans.push(Span::styled(status, Style::default().fg(theme.text_muted)));
    Line::from(spans)
}

fn results_title(state: &SearchPageState, result_count: usize) -> String {
    let summary = match state.ui_status {
        SearchUiStatus::Idle if !state.has_session() => "full local index".to_string(),
        SearchUiStatus::Debouncing => "debounce".to_string(),
        SearchUiStatus::Searching => "searching".to_string(),
        SearchUiStatus::LoadingMore => match state.total_count {
            Some(total) => format!("showing {result_count} of {total} / loading more"),
            None => format!("showing {result_count} / loading more"),
        },
        SearchUiStatus::Loaded | SearchUiStatus::Idle => match state.total_count {
            Some(total) => format!("showing {result_count} of {total}"),
            None if state.count_pending => format!("showing {result_count} of ..."),
            None if state.has_more => format!("showing {result_count}+"),
            None => format!("showing {result_count}"),
        },
        SearchUiStatus::Error => "search failed".to_string(),
    };
    format!(
        " Search Results / {} / {} ",
        search_mode_label(state.mode),
        summary
    )
}

fn should_render_results_blank(state: &SearchPageState, result_count: usize) -> bool {
    result_count == 0 || !state.has_session()
}

fn results_blank_state(state: &SearchPageState) -> Vec<Line<'static>> {
    if !state.has_session() || state.query.is_empty() {
        return vec![
            Line::from("Search all mail from the full local index."),
            Line::from(""),
            Line::from("Start typing to search every account."),
            Line::from("Enter runs immediately. Tab changes lexical / hybrid / semantic mode."),
            Line::from(
                "This is not the mailbox filter. Use Ctrl-f in Mailbox to filter current mail.",
            ),
        ];
    }

    match state.ui_status {
        SearchUiStatus::Debouncing | SearchUiStatus::Searching | SearchUiStatus::LoadingMore
            if state.results.is_empty() =>
        {
            vec![
                Line::from("Searching the full local index..."),
                Line::from(""),
                Line::from("Looking beyond the currently loaded mailbox slice."),
                Line::from("Counting total matches in parallel."),
            ]
        }
        SearchUiStatus::Error => vec![
            Line::from("Search failed."),
            Line::from(""),
            Line::from("Adjust the query and try again."),
        ],
        _ => vec![
            Line::from("No matches in the local index."),
            Line::from(""),
            Line::from("Try a broader query, another search mode, or fewer terms."),
        ],
    }
}

fn should_render_preview_blank(
    state: &SearchPageState,
    preview_messages: &[message_view::ThreadMessageBlock],
) -> bool {
    !state.has_session() || !state.result_selected || preview_messages.is_empty()
}

fn preview_blank_state(state: &SearchPageState) -> Vec<Line<'static>> {
    if !state.has_session() || state.query.is_empty() {
        vec![
            Line::from("Search tips"),
            Line::from(""),
            Line::from("Results come from sender, subject, snippet, and indexed body text."),
            Line::from("Try names, companies, invoice numbers, or distinct phrases."),
            Line::from("Use hybrid or semantic mode when exact wording is unknown."),
        ]
    } else if matches!(
        state.ui_status,
        SearchUiStatus::Debouncing | SearchUiStatus::Searching | SearchUiStatus::LoadingMore
    ) && state.results.is_empty()
    {
        vec![
            Line::from("Loading preview..."),
            Line::from(""),
            Line::from("Results will appear here as soon as a message is selected."),
        ]
    } else {
        vec![
            Line::from("No message selected."),
            Line::from(""),
            Line::from("Use Enter or l on a result to open the preview here."),
            Line::from("j / k move the result cursor without changing the preview."),
        ]
    }
}
