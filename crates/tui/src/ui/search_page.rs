use crate::mxr_core::SearchMode;
use crate::mxr_tui::app::{
    ActivePane, MailListMode, MailListRow, SearchPageState, SearchPane, SearchUiStatus,
};
use crate::mxr_tui::ui::{mail_list, message_view, search_query::highlight_search_query};
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;

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
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let result_count = rows.len();
    let query_title = if state.editing {
        format!(
            " Search All Mail [{}] (editing) ",
            search_mode_label(state.mode)
        )
    } else {
        format!(" Search All Mail [{}] ", search_mode_label(state.mode))
    };
    let query = Paragraph::new(highlight_search_query(&state.query, theme)).block(
        Block::bordered()
            .title(query_title)
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

fn results_title(state: &SearchPageState, result_count: usize) -> String {
    let summary = match state.ui_status {
        SearchUiStatus::Idle if !state.has_session() => "full local index".to_string(),
        SearchUiStatus::FirstLoad => "searching full local index".to_string(),
        SearchUiStatus::LoadingMore => match state.total_count {
            Some(total) => format!("showing {result_count} of {total} · loading more"),
            None => format!("showing {result_count} · loading more"),
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
        " Search Results · {} · {} ",
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
            Line::from("Search all mail in your local index."),
            Line::from(""),
            Line::from("Type a query above, then press Enter."),
            Line::from("This searches the full database, not just the current mailbox viewport."),
        ];
    }

    match state.ui_status {
        SearchUiStatus::FirstLoad | SearchUiStatus::LoadingMore if state.results.is_empty() => {
            vec![
                Line::from("Searching the full local index..."),
                Line::from(""),
                Line::from("Loading first page of matches."),
                Line::from("Counting total results in parallel."),
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
            Line::from("Try a broader query, different mode, or fewer filters."),
        ],
    }
}

fn should_render_preview_blank(
    state: &SearchPageState,
    preview_messages: &[message_view::ThreadMessageBlock],
) -> bool {
    !state.has_session() || preview_messages.is_empty()
}

fn preview_blank_state(state: &SearchPageState) -> Vec<Line<'static>> {
    if !state.has_session() || state.query.is_empty() {
        vec![
            Line::from("No search preview yet."),
            Line::from(""),
            Line::from("Run a full-mail search to preview a message here."),
        ]
    } else if matches!(
        state.ui_status,
        SearchUiStatus::FirstLoad | SearchUiStatus::LoadingMore
    ) {
        vec![
            Line::from("Loading preview..."),
            Line::from(""),
            Line::from("Results will appear here as soon as a message is selected."),
        ]
    } else {
        vec![
            Line::from("No message selected."),
            Line::from(""),
            Line::from("Move through results on the left to preview a message."),
        ]
    }
}
