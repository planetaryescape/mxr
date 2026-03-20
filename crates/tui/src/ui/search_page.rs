use crate::app::{ActivePane, MailListMode, MailListRow, SearchPageState};
use crate::ui::{mail_list, message_view};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &SearchPageState,
    rows: &[MailListRow],
    mail_list_mode: MailListMode,
    preview_messages: &[message_view::ThreadMessageBlock],
    preview_scroll: u16,
    theme: &crate::theme::Theme,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let query_title = if state.editing {
        " Search Query (editing) "
    } else {
        " Search Query "
    };
    let query = Paragraph::new(state.query.as_str()).block(
        Block::default()
            .title(query_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.warning)),
    );
    frame.render_widget(query, chunks[0]);

    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(chunks[1]);

    mail_list::draw_view(
        frame,
        inner[0],
        &mail_list::MailListView {
            rows,
            selected_index: state.selected_index,
            scroll_offset: state.scroll_offset,
            active_pane: &ActivePane::MailList,
            title: "Search Results",
            selected_set: &std::collections::HashSet::new(),
            mode: mail_list_mode,
        },
        theme,
    );

    message_view::draw(
        frame,
        inner[1],
        preview_messages,
        preview_scroll,
        &ActivePane::MessageView,
        theme,
    );
}
