use crate::app::{ActivePane, MailListMode, MailListRow};
use crate::theme::Theme;
use chrono::{Datelike, Local, Utc};
use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;
use throbber_widgets_tui::{Throbber, BRAILLE_SIX};
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
    pub loading_message: Option<&'a str>,
    pub loading_throbber: Option<&'a throbber_widgets_tui::ThrobberState>,
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
            loading_message: None,
            loading_throbber: None,
        },
        theme,
    );
}

pub fn draw_view(frame: &mut Frame, area: Rect, view: &MailListView<'_>, theme: &Theme) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let border_style = theme.border_style(is_focused);

    let visible_height = area.height.saturating_sub(2) as usize;

    if view.rows.is_empty() {
        if let (Some(message), Some(throbber)) = (view.loading_message, view.loading_throbber) {
            let block = Block::default()
                .title(format!(" {} ", view.title))
                .borders(Borders::ALL)
                .border_style(border_style);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(45),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Percentage(45),
                ])
                .split(inner);
            frame.render_widget(
                Paragraph::new(
                    Throbber::default()
                        .throbber_set(BRAILLE_SIX)
                        .throbber_style(Style::default().fg(theme.accent))
                        .to_symbol_span(throbber),
                )
                .alignment(Alignment::Center),
                chunks[1],
            );
            frame.render_widget(
                Paragraph::new(message)
                    .style(Style::default().fg(theme.text_muted))
                    .alignment(Alignment::Center),
                chunks[2],
            );
            return;
        }
    }

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
    let base_style = row_base_style(theme, is_selected, is_in_set, is_unread);
    let row_fg = base_style.fg.unwrap_or(theme.text_primary);
    let row_secondary_fg = if is_selected || is_in_set {
        row_fg
    } else if is_unread {
        theme.text_primary
    } else {
        theme.text_secondary
    };
    let row_muted_fg = if is_selected || is_in_set {
        row_fg
    } else {
        theme.text_muted
    };
    let row_marker_fg = if is_selected || is_in_set {
        row_fg
    } else {
        theme.line_number_fg
    };
    let selection_marker = match (is_selected, is_in_set) {
        (true, true) => "*",
        (true, false) => ">",
        (false, true) => "+",
        (false, false) => " ",
    };
    let line_number_style = match (is_selected, is_in_set) {
        (true, true) | (true, false) => Style::default().fg(row_fg).bold(),
        (false, true) => Style::default().fg(row_fg).bold(),
        (false, false) => Style::default().fg(row_marker_fg),
    };

    // Line number
    let line_num_cell = Cell::from(Span::styled(
        format!("{selection_marker}{:>3}", index + 1),
        line_number_style,
    ));

    // Unread indicator
    let unread_cell = Cell::from(Span::styled(
        if is_unread { "N" } else { " " },
        Style::default().fg(row_fg).bold(),
    ));

    // Star
    let star_cell = Cell::from(Span::styled(
        if is_starred { "★" } else { " " },
        Style::default().fg(row_fg),
    ));

    let unsubscribe_cell = Cell::from(Span::styled(
        unsubscribe_marker(&env.unsubscribe),
        Style::default().fg(row_muted_fg),
    ));

    // Sender (with thread count badge)
    let (sender_text, thread_count) = sender_parts(row, view.mode);
    let sender_spans: Vec<Span> = if let Some(count) = thread_count {
        vec![
            Span::styled(sender_text, Style::default().fg(row_secondary_fg)),
            Span::styled(format!(" {}", count), Style::default().fg(row_fg).bold()),
        ]
    } else {
        vec![Span::styled(
            sender_text,
            Style::default().fg(row_secondary_fg),
        )]
    };
    let sender_cell = Cell::from(Line::from(sender_spans));

    // Subject
    let subject_cell = Cell::from(Span::raw(env.subject.clone()));

    // Date
    let date_str = format_date(&env.date);
    let date_cell = Cell::from(Span::styled(date_str, Style::default().fg(row_muted_fg)));

    // Attachment
    let attach_cell = Cell::from(Span::styled(
        attachment_marker(env.has_attachments),
        Style::default().fg(row_fg),
    ));

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

fn row_base_style(theme: &Theme, is_selected: bool, is_in_set: bool, is_unread: bool) -> Style {
    match (is_selected, is_in_set) {
        (true, true) => {
            let bg = blend_bg(theme.selection_bg, theme.accent, 96);
            let fg = contrast_foreground(bg, theme.selection_fg);
            Style::default()
                .bg(bg)
                .fg(fg)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        }
        (true, false) => {
            let fg = contrast_foreground(theme.selection_bg, theme.selection_fg);
            Style::default()
                .bg(theme.selection_bg)
                .fg(fg)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        }
        (false, true) => {
            let bg = blend_bg(theme.selection_bg, theme.accent_dim, 72);
            let fg = contrast_foreground(bg, theme.selection_fg);
            Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD)
        }
        (false, false) if is_unread => theme.unread_style(),
        (false, false) => Style::default().fg(theme.text_secondary),
    }
}

fn contrast_foreground(bg: Color, fallback: Color) -> Color {
    let Some((r, g, b)) = color_rgb(bg) else {
        return fallback;
    };
    let luminance = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
    if luminance >= 140 {
        Color::Black
    } else {
        Color::White
    }
}

fn blend_bg(base: Color, tint: Color, tint_weight: u8) -> Color {
    let Some((base_r, base_g, base_b)) = color_rgb(base) else {
        return base;
    };
    let Some((tint_r, tint_g, tint_b)) = color_rgb(tint) else {
        return base;
    };
    let tint_weight = u16::from(tint_weight);
    let base_weight = 255u16.saturating_sub(tint_weight);
    let mix = |base: u8, tint: u8| -> u8 {
        (((u16::from(base) * base_weight) + (u16::from(tint) * tint_weight)) / 255) as u8
    };
    Color::Rgb(
        mix(base_r, tint_r),
        mix(base_g, tint_g),
        mix(base_b, tint_b),
    )
}

fn color_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Reset => None,
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((205, 49, 49)),
        Color::Green => Some((13, 188, 121)),
        Color::Yellow => Some((229, 229, 16)),
        Color::Blue => Some((36, 114, 200)),
        Color::Magenta => Some((188, 63, 188)),
        Color::Cyan => Some((17, 168, 205)),
        Color::Gray => Some((229, 229, 229)),
        Color::DarkGray => Some((102, 102, 102)),
        Color::LightRed => Some((241, 76, 76)),
        Color::LightGreen => Some((35, 209, 139)),
        Color::LightYellow => Some((245, 245, 67)),
        Color::LightBlue => Some((59, 142, 234)),
        Color::LightMagenta => Some((214, 112, 214)),
        Color::LightCyan => Some((41, 184, 219)),
        Color::White => Some((255, 255, 255)),
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Indexed(idx) => Some(indexed_color_rgb(idx)),
    }
}

fn indexed_color_rgb(idx: u8) -> (u8, u8, u8) {
    match idx {
        0 => (0, 0, 0),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),
        16..=231 => {
            let idx = idx - 16;
            let r = idx / 36;
            let g = (idx % 36) / 6;
            let b = idx % 6;
            let to_channel = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (to_channel(r), to_channel(g), to_channel(b))
        }
        232..=255 => {
            let shade = 8 + (idx - 232) * 10;
            (shade, shade, shade)
        }
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
                    loading_message: None,
                    loading_throbber: None,
                },
                &Theme::default(),
            );
        });

        assert!(snapshot.contains(">  1"));
        assert!(snapshot.contains("+  2"));
    }
    #[test]
    fn bulk_selection_uses_tinted_background_for_contrast() {
        let theme = Theme::default();
        let style = row_base_style(
            &Theme {
                selection_bg: Color::Rgb(40, 44, 52),
                accent_dim: Color::Rgb(160, 200, 255),
                ..theme
            },
            false,
            true,
            false,
        );

        assert_eq!(style.bg, Some(Color::Rgb(73, 88, 109)));
        assert_eq!(style.fg, Some(Color::White));
    }

    #[test]
    fn loading_placeholder_renders_when_mailbox_is_loading() {
        use mxr_test_support::render_to_string;
        use throbber_widgets_tui::ThrobberState;

        let throbber = ThrobberState::default();
        let rendered = render_to_string(60, 8, |frame| {
            draw_view(
                frame,
                Rect::new(0, 0, 60, 8),
                &MailListView {
                    rows: &[],
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                    title: "Inbox",
                    selected_set: &HashSet::new(),
                    mode: MailListMode::Threads,
                    loading_message: Some("Loading selected account..."),
                    loading_throbber: Some(&throbber),
                },
                &Theme::default(),
            );
        });

        assert!(rendered.contains("Loading selected account..."));
    }
}
