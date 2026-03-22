use crate::app::{ActivePane, MailListMode, MailListRow};
use crate::theme::Theme;
use chrono::{Datelike, Local, Utc};
use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;
#[cfg(test)]
use unicode_width::UnicodeWidthStr;

pub struct MailListView<'a> {
    pub rows: &'a [MailListRow],
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub active_pane: &'a ActivePane,
    pub title: &'a str,
    pub selected_set: &'a HashSet<MessageId>,
    pub mode: MailListMode,
}

#[expect(
    clippy::too_many_arguments,
    reason = "TUI draw entrypoint keeps call sites explicit"
)]
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    rows: &[MailListRow],
    selected_index: usize,
    scroll_offset: usize,
    active_pane: &ActivePane,
    title: &str,
    theme: &Theme,
) {
    draw_view(
        frame,
        area,
        &MailListView {
            rows,
            selected_index,
            scroll_offset,
            active_pane,
            title,
            selected_set: &HashSet::new(),
            mode: MailListMode::Threads,
        },
        theme,
    );
}

pub fn draw_view(frame: &mut Frame, area: Rect, view: &MailListView<'_>, theme: &Theme) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let border_style = theme.border_style(is_focused);

    let visible_height = area.height.saturating_sub(2) as usize;

    let table_rows: Vec<Row> = view
        .rows
        .iter()
        .enumerate()
        .skip(view.scroll_offset)
        .take(visible_height)
        .map(|(i, row)| build_row(view, row, i, theme))
        .collect();

    let widths = [
        Constraint::Length(4),  // line number
        Constraint::Length(1),  // unread indicator
        Constraint::Length(2),  // star
        Constraint::Length(2),  // unsubscribe
        Constraint::Length(22), // sender
        Constraint::Fill(1),    // subject (+ thread count badge)
        Constraint::Length(8),  // date
        Constraint::Length(2),  // attachment icon
    ];

    let table = Table::new(table_rows, widths)
        .block(
            Block::default()
                .title(format!(" {} ", view.title))
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .row_highlight_style(
            Style::default()
                .bg(theme.selection_bg)
                .fg(theme.selection_fg),
        )
        .column_spacing(1);

    // We use manual highlight via row styles since we handle scroll_offset ourselves
    frame.render_widget(table, area);

    // Scrollbar
    if view.rows.len() > visible_height {
        let mut scrollbar_state =
            ScrollbarState::new(view.rows.len().saturating_sub(visible_height))
                .position(view.scroll_offset);
        frame.render_stateful_widget(
            Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::default().fg(theme.accent)),
            area,
            &mut scrollbar_state,
        );
    }
}

fn build_row<'a>(
    view: &MailListView<'_>,
    row: &MailListRow,
    index: usize,
    theme: &Theme,
) -> Row<'a> {
    let env = &row.representative;
    let is_selected = index == view.selected_index;
    let is_unread = !env.flags.contains(MessageFlags::READ);
    let is_starred = env.flags.contains(MessageFlags::STARRED);
    let is_in_set = view.selected_set.contains(&env.id);
    let selection_marker = match (is_selected, is_in_set) {
        (true, true) => "*",
        (true, false) => ">",
        (false, true) => "+",
        (false, false) => " ",
    };
    let line_number_style = match (is_selected, is_in_set) {
        (true, true) | (true, false) => Style::default().fg(theme.warning).bold(),
        (false, true) => Style::default().fg(theme.accent).bold(),
        (false, false) => Style::default().fg(theme.line_number_fg),
    };

    // Line number
    let line_num_cell = Cell::from(Span::styled(
        format!("{selection_marker}{:>3}", index + 1),
        line_number_style,
    ));

    // Unread indicator
    let unread_cell = Cell::from(Span::styled(
        if is_unread { "N" } else { " " },
        Style::default().fg(theme.accent).bold(),
    ));

    // Star
    let star_cell = Cell::from(Span::styled(
        if is_starred { "★" } else { " " },
        Style::default().fg(theme.warning),
    ));

    let unsubscribe_cell = Cell::from(Span::styled(
        unsubscribe_marker(&env.unsubscribe),
        Style::default().fg(theme.text_muted),
    ));

    // Sender (with thread count badge)
    let (sender_text, thread_count) = sender_parts(row, view.mode);
    let sender_spans: Vec<Span> = if let Some(count) = thread_count {
        vec![
            Span::styled(
                sender_text,
                Style::default().fg(if is_unread {
                    theme.text_primary
                } else {
                    theme.text_secondary
                }),
            ),
            Span::styled(
                format!(" {}", count),
                Style::default().fg(theme.accent_dim).bold(),
            ),
        ]
    } else {
        vec![Span::styled(
            sender_text,
            Style::default().fg(if is_unread {
                theme.text_primary
            } else {
                theme.text_secondary
            }),
        )]
    };
    let sender_cell = Cell::from(Line::from(sender_spans));

    // Subject
    let subject_cell = Cell::from(Span::raw(env.subject.clone()));

    // Date
    let date_str = format_date(&env.date);
    let date_cell = Cell::from(Span::styled(
        date_str,
        Style::default().fg(theme.text_muted),
    ));

    // Attachment
    let attach_cell = Cell::from(Span::styled(
        attachment_marker(env.has_attachments),
        Style::default().fg(theme.success),
    ));

    let base_style = match (is_selected, is_in_set) {
        (true, true) => Style::default()
            .bg(theme.accent)
            .fg(theme.selection_fg)
            .add_modifier(Modifier::BOLD),
        (true, false) => Style::default()
            .bg(theme.selection_bg)
            .fg(theme.selection_fg)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        (false, true) => Style::default()
            .bg(theme.accent_dim)
            .fg(theme.selection_fg)
            .add_modifier(Modifier::BOLD),
        (false, false) if is_unread => theme.unread_style(),
        (false, false) => Style::default().fg(theme.text_secondary),
    };

    Row::new(vec![
        line_num_cell,
        unread_cell,
        star_cell,
        unsubscribe_cell,
        sender_cell,
        subject_cell,
        date_cell,
        attach_cell,
    ])
    .style(base_style)
}

fn sender_parts(row: &MailListRow, mode: MailListMode) -> (String, Option<usize>) {
    let from_raw = row
        .representative
        .from
        .name
        .as_deref()
        .unwrap_or(&row.representative.from.email);
    match mode {
        MailListMode::Threads if row.message_count > 1 => {
            (from_raw.to_string(), Some(row.message_count))
        }
        _ => (from_raw.to_string(), None),
    }
}

fn format_date(date: &chrono::DateTime<Utc>) -> String {
    let local = date.with_timezone(&Local);
    let now = Local::now();

    if local.date_naive() == now.date_naive() {
        local.format("%I:%M%p").to_string()
    } else if local.year() == now.year() {
        local.format("%b %d").to_string()
    } else {
        local.format("%m/%d/%y").to_string()
    }
}

fn attachment_marker(has_attachments: bool) -> &'static str {
    if has_attachments {
        "📎"
    } else {
        "  "
    }
}

fn unsubscribe_marker(unsubscribe: &mxr_core::types::UnsubscribeMethod) -> &'static str {
    if matches!(unsubscribe, mxr_core::types::UnsubscribeMethod::None) {
        " "
    } else {
        "U"
    }
}

#[cfg(test)]
fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

#[cfg(test)]
fn truncate_display(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if display_width(text) <= max_width {
        return text.to_string();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let mut width = 0;
    let mut truncated = String::new();
    for ch in text.chars() {
        let ch_width = UnicodeWidthStr::width(ch.encode_utf8(&mut [0; 4]));
        if width + ch_width > max_width.saturating_sub(3) {
            break;
        }
        truncated.push(ch);
        width += ch_width;
    }
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::id::{AccountId, ThreadId};
    use mxr_core::types::{Address, Envelope, UnsubscribeMethod};

    fn row(message_count: usize, has_attachments: bool) -> MailListRow {
        MailListRow {
            thread_id: ThreadId::new(),
            representative: Envelope {
                id: MessageId::new(),
                account_id: AccountId::new(),
                provider_id: "fake".into(),
                thread_id: ThreadId::new(),
                message_id_header: None,
                in_reply_to: None,
                references: vec![],
                from: Address {
                    name: Some("Matt".into()),
                    email: "matt@example.com".into(),
                },
                to: vec![],
                cc: vec![],
                bcc: vec![],
                subject: "A very long subject line that should not eat the date column".into(),
                date: Utc::now(),
                flags: MessageFlags::empty(),
                snippet: String::new(),
                has_attachments,
                size_bytes: 1024,
                unsubscribe: UnsubscribeMethod::None,
                label_provider_ids: vec![],
            },
            message_count,
            unread_count: message_count,
        }
    }

    #[test]
    fn sender_text_inlines_thread_count_without_brackets() {
        assert_eq!(
            sender_parts(&row(1, false), MailListMode::Threads),
            ("Matt".into(), None)
        );
        assert_eq!(
            sender_parts(&row(4, false), MailListMode::Threads),
            ("Matt".into(), Some(4))
        );
    }

    #[test]
    fn truncate_display_adds_ellipsis() {
        assert_eq!(truncate_display("abcdefghij", 6), "abc...");
    }

    #[test]
    fn attachment_marker_uses_clip_icon() {
        assert_eq!(attachment_marker(true), "📎");
        assert_eq!(attachment_marker(false), "  ");
    }

    #[test]
    fn selection_markers_distinguish_cursor_and_bulk_selection() {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        let first = row(1, false);
        let second = row(1, false);
        let rows = vec![first.clone(), second.clone()];
        let mut selected_set = HashSet::new();
        selected_set.insert(second.representative.id.clone());

        let snapshot = render_to_string(80, 8, |frame| {
            draw_view(
                frame,
                Rect::new(0, 0, 80, 8),
                &MailListView {
                    rows: &rows,
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                    title: "Inbox",
                    selected_set: &selected_set,
                    mode: MailListMode::Threads,
                },
                &Theme::default(),
            );
        });

        assert!(snapshot.contains(">  1"));
        assert!(snapshot.contains("+  2"));
    }
}
