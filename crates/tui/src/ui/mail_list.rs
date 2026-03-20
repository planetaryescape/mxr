use crate::app::{ActivePane, MailListMode, MailListRow};
use chrono::{Datelike, Local, Utc};
use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;
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

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    rows: &[MailListRow],
    selected_index: usize,
    scroll_offset: usize,
    active_pane: &ActivePane,
    title: &str,
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
    );
}

pub fn draw_view(frame: &mut Frame, area: Rect, view: &MailListView<'_>) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let visible_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(2) as usize;
    let compact = inner_width < 72;
    let ultra_compact = inner_width < 48;

    let items: Vec<ListItem> = view
        .rows
        .iter()
        .enumerate()
        .skip(view.scroll_offset)
        .take(visible_height)
        .map(|(i, row)| render_row(view, row, i, inner_width, compact, ultra_compact))
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(" {} ", view.title))
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    frame.render_widget(list, area);
}

fn render_row(
    view: &MailListView<'_>,
    row: &MailListRow,
    index: usize,
    inner_width: usize,
    compact: bool,
    ultra_compact: bool,
) -> ListItem<'static> {
    let env = &row.representative;
    let is_selected = index == view.selected_index;
    let is_unread = !env.flags.contains(MessageFlags::READ);
    let is_starred = env.flags.contains(MessageFlags::STARRED);
    let is_answered = env.flags.contains(MessageFlags::ANSWERED);
    let is_in_set = view.selected_set.contains(&env.id);
    let right_text = format_right_column(env, compact);
    let date_width = display_width(&right_text).max(if compact { 7 } else { 12 });
    let line_number_width = if ultra_compact { 0 } else { 4 };
    let selection_width = usize::from(!view.selected_set.is_empty());
    let flags_width = if compact { 5 } else { 6 };
    let gap_width = 3;

    let reserved = selection_width + line_number_width + flags_width + gap_width + date_width;
    let available_text = inner_width.saturating_sub(reserved);
    let min_sender_width = if ultra_compact { 8 } else { 12 };
    let min_subject_width = if ultra_compact { 8 } else { 14 };
    let preferred_sender_width = if ultra_compact {
        available_text / 2
    } else if compact {
        18
    } else {
        22
    };
    let mut sender_width = preferred_sender_width.min(available_text.saturating_sub(min_subject_width));
    sender_width = sender_width.max(min_sender_width.min(available_text));
    let subject_width = available_text.saturating_sub(sender_width);

    let (sender_text, thread_count) = sender_parts(row, view.mode);
    let thread_count_width = thread_count
        .map(|count| display_width(&format!(" {}", count)))
        .unwrap_or(0);
    let subject_text = if ultra_compact {
        truncate_display(&env.subject, subject_width)
    } else {
        pad_right_display(&truncate_display(&env.subject, subject_width), subject_width)
    };

    let mut spans: Vec<Span> = Vec::new();

    if !view.selected_set.is_empty() {
        spans.push(if is_in_set {
            Span::styled("*", Style::default().fg(Color::Magenta).bold())
        } else {
            Span::raw(" ")
        });
    }

    if !ultra_compact {
        spans.push(Span::styled(
            pad_left_display(&(index + 1).to_string(), 3) + " ",
            Style::default().fg(Color::Rgb(80, 80, 80)),
        ));
    }

    spans.push(Span::styled(
        if is_unread { "N" } else { " " },
        Style::default().fg(Color::Cyan).bold(),
    ));
    spans.push(Span::styled(
        if is_starred { "★" } else { " " },
        Style::default().fg(Color::Yellow),
    ));
    if !compact {
        spans.push(Span::styled(
            if is_answered { "A" } else { " " },
            Style::default().fg(Color::Blue),
        ));
    }
    spans.push(Span::styled(
        attachment_marker(env.has_attachments),
        Style::default().fg(Color::Green),
    ));
    spans.push(Span::raw(" "));
    let sender_cell = pad_right_display(
        &sender_text,
        sender_width.saturating_sub(thread_count_width),
    );
    spans.push(Span::styled(
        sender_cell,
        Style::default().fg(if is_unread { Color::White } else { Color::Gray }),
    ));
    if let Some(thread_count) = thread_count {
        spans.push(Span::styled(
            format!(" {}", thread_count),
            Style::default().fg(Color::LightBlue).bold(),
        ));
    }
    spans.push(Span::raw(" "));
    spans.push(Span::raw(subject_text));
    spans.push(Span::styled(
        pad_left_display(&right_text, date_width),
        Style::default().fg(Color::Rgb(100, 100, 110)),
    ));

    let line = Line::from(spans);
    let base_style = if is_selected {
        Style::default().bg(Color::Rgb(50, 50, 60)).fg(Color::White)
    } else if is_in_set {
        Style::default().bg(Color::Rgb(40, 30, 50)).fg(Color::White)
    } else if is_unread {
        Style::default().fg(Color::White).bold()
    } else {
        Style::default().fg(Color::Gray)
    };

    ListItem::new(line).style(base_style)
}

fn format_right_column(env: &mxr_core::Envelope, compact: bool) -> String {
    let date_str = format_date(&env.date);
    if compact {
        date_str
    } else {
        format!("{} {}", date_str, format_size(env.size_bytes))
    }
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

fn format_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("({:.0}M)", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("({:.0}K)", bytes as f64 / 1024.0)
    } else {
        format!("({}B)", bytes)
    }
}

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn attachment_marker(has_attachments: bool) -> &'static str {
    if has_attachments { "📎" } else { "  " }
}

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

fn pad_right_display(text: &str, width: usize) -> String {
    let truncated = truncate_display(text, width);
    let padding = width.saturating_sub(display_width(&truncated));
    format!("{truncated}{}", " ".repeat(padding))
}

fn pad_left_display(text: &str, width: usize) -> String {
    let truncated = truncate_display(text, width);
    let padding = width.saturating_sub(display_width(&truncated));
    format!("{}{truncated}", " ".repeat(padding))
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
        assert_eq!(sender_parts(&row(1, false), MailListMode::Threads), ("Matt".into(), None));
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
}
