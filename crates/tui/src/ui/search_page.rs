use crate::app::{ActivePane, MailListMode, MailListRow, SearchPageState, SearchPane};
use crate::ui::{mail_list, message_view, search_query::highlight_search_query};
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
    selected_set: &HashSet<mxr_core::MessageId>,
    mail_list_mode: MailListMode,
    preview_messages: &[message_view::ThreadMessageBlock],
    preview_scroll: u16,
    theme: &crate::theme::Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let result_count = rows.len();
    let query_title = if state.editing {
        " Search Query (editing) ".to_string()
    } else if result_count > 0 {
        let suffix = if state.loading_more {
            " · loading"
        } else if state.has_more {
            " · more"
        } else {
            ""
        };
        format!(" Search Results ({result_count}{suffix}) ")
    } else {
        " Search Query ".to_string()
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

    mail_list::draw_view(
        frame,
        inner[0],
        &mail_list::MailListView {
            rows,
            selected_index: state.selected_index,
            scroll_offset: state.scroll_offset,
            active_pane: &list_active_pane,
            title: "Search Results",
            selected_set,
            mode: mail_list_mode,
        },
        theme,
    );

    message_view::draw(
        frame,
        inner[1],
        preview_messages,
        preview_scroll,
        &preview_active_pane,
        theme,
    );
}
