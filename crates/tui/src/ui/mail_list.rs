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

    // Compute the subject column's effective character width so
    // `format_subject_line` can decide whether there's room for the
    // snippet preview. Sum of fixed column widths + 9 inter-column
    // spaces (Table::column_spacing(1) between 10 cells) + 2 border
    // chars. The attachment chip widened to 8 to fit "📎 99K". The
    // link chip is 2 chars (a single `🔗` glyph; the heavy tier uses a
    // brighter color rather than a double glyph to keep the column narrow).
    let show_triage = view.rows.iter().any(|row| row.triage_verdict.is_some());
    let triage_width = if show_triage { 8 } else { 0 };
    let fixed_columns_width = 4 + 1 + 2 + 2 + triage_width + 22 + 8 + 8 + 2;
    let column_count = if show_triage { 10 } else { 9 };
    let column_spacing_total: u16 = column_count - 1;
    let border_total: u16 = 2;
    let subject_max_width = area
        .width
        .saturating_sub(fixed_columns_width)
        .saturating_sub(column_spacing_total)
        .saturating_sub(border_total) as usize;

    let table_rows: Vec<Row> = view
        .rows
        .iter()
        .enumerate()
        .skip(view.scroll_offset)
        .take(visible_height)
        .map(|(i, row)| build_row(view, row, i, theme, subject_max_width, show_triage))
        .collect();

    let mut widths = vec![
        Constraint::Length(4), // line number
        Constraint::Length(1), // unread indicator
        Constraint::Length(2), // star
        Constraint::Length(2), // list markers (unsubscribe/reply-later)
    ];
    if show_triage {
        widths.push(Constraint::Length(8)); // triage verdict token
    }
    widths.extend([
        Constraint::Length(22), // sender
        Constraint::Fill(1),    // subject (+ snippet preview)
        Constraint::Length(8),  // date
        Constraint::Length(8),  // attachment chip ("📎 99K")
        Constraint::Length(2),  // link chip (`🔗` for Some / Heavy, blank for None)
    ]);

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

/// In thread aggregate mode with multiple messages, show `+N` beside the
/// subject where N counts distinct participant addresses (normalized email,
/// From/To/Cc union) excluding the representative message From address.
fn thread_participation_chip(row: &MailListRow, mode: MailListMode) -> Option<String> {
    if mode != MailListMode::Threads {
        return None;
    }
    if row.message_count <= 1 || row.other_participant_count == 0 {
        return None;
    }
    Some(format!("+{}", row.other_participant_count))
}

fn build_row<'a>(
    view: &MailListView<'_>,
    row: &MailListRow,
    index: usize,
    theme: &Theme,
    subject_max_width: usize,
    show_triage: bool,
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
        (false, false) if row.pending_mutation => "!",
        (false, false) => " ",
    };
    let line_number_style = match (is_selected, is_in_set) {
        (true, true | false) => Style::default().fg(row_fg).bold(),
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

    let list_markers_cell = Cell::from(Span::styled(
        list_markers(row),
        Style::default().fg(row_muted_fg),
    ));

    let triage_cell = Cell::from(Span::styled(
        row.triage_verdict.clone().unwrap_or_default(),
        triage_style(row.triage_verdict.as_deref(), row_fg, theme),
    ));

    // Sender (with thread count badge)
    let (sender_text, thread_count) = sender_parts(row, view.mode);
    let sender_spans: Vec<Span> = if let Some(count) = thread_count {
        vec![
            Span::styled(sender_text, Style::default().fg(row_secondary_fg)),
            Span::styled(
                format!(" {}", format_thread_count_badge(count)),
                Style::default().fg(row_fg).bold(),
            ),
        ]
    } else {
        vec![Span::styled(
            sender_text,
            Style::default().fg(row_secondary_fg),
        )]
    };
    let sender_cell = Cell::from(Line::from(sender_spans));

    // Subject + participation chip (`+N` other participants per delight plan).
    let participation_chip = thread_participation_chip(row, view.mode);
    let chip_budget = participation_chip
        .as_ref()
        .map_or(0, |s| s.chars().count() + 1);
    // Remaining width goes to trailing snippet after " · "; format_subject_line
    // allocates subject first, then snippet when there's room.
    let (subject_text, snippet_preview) = format_subject_line(
        &env.subject,
        &env.snippet,
        subject_max_width.saturating_sub(chip_budget),
    );
    let mut subject_chunks: Vec<Span> = vec![Span::styled(
        subject_text,
        Style::default().fg(row_secondary_fg),
    )];
    if let Some(chip) = participation_chip {
        subject_chunks.push(Span::styled(
            format!(" {chip}"),
            Style::default().fg(theme.accent_dim),
        ));
    }
    let subject_cell = if let Some(snippet) = snippet_preview {
        subject_chunks.extend([
            Span::styled(" · ", Style::default().fg(row_muted_fg)),
            Span::styled(snippet, Style::default().fg(row_muted_fg)),
        ]);
        Cell::from(Line::from(subject_chunks))
    } else {
        Cell::from(Line::from(subject_chunks))
    };

    // Date
    let date_str = format_date(&env.date);
    let date_cell = Cell::from(Span::styled(date_str, Style::default().fg(row_muted_fg)));

    // Attachment chip: paperclip + size readout when present.
    let attach_text = format_attachment_chip(
        env.has_attachments,
        u32::try_from(env.size_bytes).unwrap_or(u32::MAX),
    );
    let attach_cell = if attach_text.is_empty() {
        Cell::from(Span::styled("  ", Style::default().fg(row_fg)))
    } else {
        Cell::from(Span::styled(attach_text, Style::default().fg(row_fg)))
    };

    // Link chip: tri-state glyph driven by `Envelope::link_density()`.
    // Heavy uses the accent color so newsletter-shaped mail stands out from
    // ordinary link-bearing mail without claiming a second column.
    let link_cell = match env.link_density() {
        mxr_core::types::LinkDensity::None => {
            Cell::from(Span::styled("  ", Style::default().fg(row_fg)))
        }
        mxr_core::types::LinkDensity::Some => {
            Cell::from(Span::styled("🔗", Style::default().fg(row_muted_fg)))
        }
        mxr_core::types::LinkDensity::Heavy => Cell::from(Span::styled(
            "🔗",
            Style::default()
                .fg(theme.accent)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )),
    };

    let mut cells = vec![line_num_cell, unread_cell, star_cell, list_markers_cell];
    if show_triage {
        cells.push(triage_cell);
    }
    cells.extend([sender_cell, subject_cell, date_cell, attach_cell, link_cell]);

    Row::new(cells).style(base_style)
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
fn triage_style(verdict: Option<&str>, selected_fg: Color, theme: &Theme) -> Style {
    match verdict {
        Some("ACTION") => Style::default().fg(Color::Red).bold(),
        Some("FYI") => Style::default().fg(Color::Blue),
        Some("ROUTINE") => Style::default().fg(theme.text_muted),
        Some(_) => Style::default().fg(selected_fg),
        None => Style::default().fg(theme.text_muted),
    }
}

fn sender_parts(row: &MailListRow, mode: MailListMode) -> (String, Option<usize>) {
    // Reserve trailing chars in the 22-char sender column for the thread
    // count badge (" ↔99" worst case = 4 chars + leading space). Without
    // the reservation a long display name would visually collide with the badge.
    let max_width = match mode {
        MailListMode::Threads if row.message_count > 1 => 17,
        _ => 22,
    };
    let from_text = format_sender(&row.representative.from, max_width);
    match mode {
        MailListMode::Threads if row.message_count > 1 => (from_text, Some(row.message_count)),
        _ => (from_text, None),
    }
}

pub fn format_thread_count_badge(message_count: usize) -> String {
    if message_count > 1 {
        format!("↔{message_count}")
    } else {
        String::new()
    }
}

fn format_date(date: &chrono::DateTime<Utc>) -> String {
    format_date_relative(date, &Utc::now())
}

/// Format a sender address for the inbox row. Prefers the display name
/// when present and non-empty; otherwise falls back to the email
/// address. Truncates to `max_width` characters with a trailing ellipsis
/// when the text is longer.
pub fn format_sender(address: &mxr_core::types::Address, max_width: usize) -> String {
    let raw = match address.name.as_deref() {
        Some(name) if !name.trim().is_empty() => name,
        _ => address.email.as_str(),
    };
    truncate_with_ellipsis(raw, max_width)
}

fn truncate_with_ellipsis(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let char_count = text.chars().count();
    if char_count <= max_width {
        return text.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let mut result: String = text.chars().take(max_width - 1).collect();
    result.push('…');
    result
}

/// Format the attachment chip for the inbox row. Returns an empty string
/// when there are no attachments, otherwise a paperclip glyph followed
/// by a short size readout (`"📎 45K"`).
///
/// The size is the message's total size — a reasonable proxy for the
/// attachment heft, since envelopes with attachments are dominated by
/// attachment bytes.
pub fn format_attachment_chip(has_attachments: bool, size_bytes: u32) -> String {
    if !has_attachments {
        return String::new();
    }
    format!("📎 {}", format_byte_size(size_bytes))
}

fn format_byte_size(bytes: u32) -> String {
    const KIB: u32 = 1024;
    const MIB: u32 = 1024 * 1024;
    if bytes >= MIB {
        format!("{}M", bytes / MIB)
    } else if bytes >= KIB {
        format!("{}K", bytes / KIB)
    } else {
        format!("{bytes}B")
    }
}

/// Compose the subject line with an inline snippet preview, fit to
/// `max_width` characters. Returns `(subject_text, Option<snippet_text>)`
/// so the renderer can style each part differently (subject prominent,
/// snippet dim).
///
/// The snippet is omitted entirely when the row doesn't have enough
/// horizontal space to display something useful — preferring "no
/// snippet" over "·" with one truncated character.
pub fn format_subject_line(
    subject: &str,
    snippet: &str,
    max_width: usize,
) -> (String, Option<String>) {
    if max_width == 0 {
        return (String::new(), None);
    }
    let subject_chars = subject.chars().count();
    if snippet.trim().is_empty() {
        return (truncate_with_ellipsis(subject, max_width), None);
    }
    if subject_chars >= max_width {
        return (truncate_with_ellipsis(subject, max_width), None);
    }
    // Reserve room for the " · " separator (3 chars) plus a meaningful
    // snippet head — at least 4 chars of snippet text.
    const SEPARATOR_WIDTH: usize = 3;
    const MIN_SNIPPET_WIDTH: usize = 4;
    let after_subject = max_width - subject_chars;
    if after_subject < SEPARATOR_WIDTH + MIN_SNIPPET_WIDTH {
        return (subject.to_string(), None);
    }
    let snippet_room = after_subject - SEPARATOR_WIDTH;
    let snippet_text = truncate_with_ellipsis(snippet.trim(), snippet_room);
    (subject.to_string(), Some(snippet_text))
}

/// Format an email date relative to `now`. Used by the inbox row to give
/// the user a "how long ago?" hint at a glance — a relative time ladder
/// rather than a wall-clock or locale-formatted timestamp.
///
/// Ladder:
/// - within the last minute: `"now"`
/// - within the last hour: `"5m"`
/// - within the last 24 hours: `"3h"`
/// - within the last 7 days: short weekday name (`"Tue"`)
/// - same year, older: month + day (`"Mar 4"`)
/// - different year (or future date): month/day/year (`"03/04/23"`)
pub fn format_date_relative(date: &chrono::DateTime<Utc>, now: &chrono::DateTime<Utc>) -> String {
    let elapsed = now.signed_duration_since(*date);
    if elapsed.num_seconds() < 0 {
        // Future date — defer to absolute formatting; relative tense
        // doesn't apply.
        return date.with_timezone(&Local).format("%m/%d/%y").to_string();
    }
    let seconds = elapsed.num_seconds();
    let minutes = elapsed.num_minutes();
    let hours = elapsed.num_hours();
    let days = elapsed.num_days();

    if seconds < 60 {
        return "now".to_string();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    if hours < 24 {
        return format!("{hours}h");
    }
    if days < 7 {
        return date.with_timezone(&Local).format("%a").to_string();
    }
    let local = date.with_timezone(&Local);
    let now_local = now.with_timezone(&Local);
    if local.year() == now_local.year() {
        local.format("%b %-d").to_string()
    } else {
        local.format("%m/%d/%y").to_string()
    }
}

fn list_markers(row: &MailListRow) -> String {
    let mut marker = String::with_capacity(2);
    if !matches!(
        row.representative.unsubscribe,
        mxr_core::types::UnsubscribeMethod::None
    ) {
        marker.push('U');
    }
    if row.reply_later {
        marker.push('R');
    }
    if row.open_commitment_count > 0 {
        marker.push(commitment_marker(row.open_commitment_count));
    }
    if marker.is_empty() {
        " ".to_string()
    } else {
        marker
    }
}

fn commitment_marker(count: u32) -> char {
    char::from_digit(count.min(9), 10).unwrap_or('C')
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
        row_with_participants(message_count, 0, has_attachments)
    }

    fn row_with_participants(
        message_count: usize,
        other_participant_count: usize,
        has_attachments: bool,
    ) -> MailListRow {
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
                link_count: 0,
                body_word_count: 0,
                label_provider_ids: vec![],
                keywords: std::collections::BTreeSet::new(),
            },
            message_count,
            unread_count: message_count,
            other_participant_count,
            open_commitment_count: 0,
            triage_verdict: None,
            reply_later: false,
            pending_mutation: false,
        }
    }

    #[test]
    fn sender_text_inlines_thread_count_with_conversation_marker() {
        assert_eq!(
            sender_parts(&row(1, false), MailListMode::Threads),
            ("Matt".into(), None)
        );
        assert_eq!(
            sender_parts(&row(4, false), MailListMode::Threads),
            ("Matt".into(), Some(4))
        );
        assert_eq!(format_thread_count_badge(4), "↔4");
        assert_eq!(format_thread_count_badge(1), "");
    }

    #[test]
    fn row_shows_thread_participation_chip_only_when_multi_message() {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        let wide = |row: MailListRow| {
            render_to_string(120, 6, |frame| {
                draw_view(
                    frame,
                    Rect::new(0, 0, 120, 6),
                    &MailListView {
                        rows: &[row],
                        selected_index: 0,
                        scroll_offset: 0,
                        active_pane: &ActivePane::MailList,
                        title: "Inbox",
                        selected_set: &HashSet::new(),
                        mode: MailListMode::Threads,
                        loading_message: None,
                        loading_throbber: None,
                    },
                    &Theme::default(),
                );
            })
        };

        let mut one = row_with_participants(1, 5, false);
        one.representative.subject = "solo".into();
        assert!(
            !wide(one).contains("+"),
            "single-message thread omits +N chip even if other_participant_count leftover"
        );

        let mut none_other = row_with_participants(4, 0, false);
        none_other.representative.subject = "monologue".into();
        assert!(
            !wide(none_other).contains("+"),
            "multi-message thread with no other participants omits chip"
        );

        let mut with_chip = row_with_participants(3, 2, false);
        with_chip.representative.subject = "Planning".into();
        let rendered = wide(with_chip);
        assert!(
            rendered.contains("+2"),
            "multi-message thread shows +N for distinct other participants"
        );
    }

    #[test]
    fn row_renders_reply_later_marker() {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        let mut row = row(1, false);
        row.reply_later = true;
        row.representative.subject = "Needs response".into();
        let rows = vec![row];

        let rendered = render_to_string(100, 6, |frame| {
            draw_view(
                frame,
                Rect::new(0, 0, 100, 6),
                &MailListView {
                    rows: &rows,
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                    title: "Inbox",
                    selected_set: &HashSet::new(),
                    mode: MailListMode::Messages,
                    loading_message: None,
                    loading_throbber: None,
                },
                &Theme::default(),
            );
        });

        assert!(
            rendered.contains(" R "),
            "reply-later rows should render an R marker in the list marker column"
        );
    }

    #[test]
    fn row_renders_open_commitment_marker() {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        let mut row = row(1, false);
        row.open_commitment_count = 2;
        row.representative.subject = "Follow-up".into();
        let rows = vec![row];

        let rendered = render_to_string(100, 6, |frame| {
            draw_view(
                frame,
                Rect::new(0, 0, 100, 6),
                &MailListView {
                    rows: &rows,
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                    title: "Inbox",
                    selected_set: &HashSet::new(),
                    mode: MailListMode::Messages,
                    loading_message: None,
                    loading_throbber: None,
                },
                &Theme::default(),
            );
        });

        assert!(
            rendered.contains(" 2 "),
            "rows with open commitments should render a count marker; got:\n{rendered}"
        );
    }

    #[test]
    fn truncate_display_adds_ellipsis() {
        assert_eq!(truncate_display("abcdefghij", 6), "abc...");
    }

    /// build_row should compose subjects with `format_subject_line`
    /// (snippet preview after a separator) and attachments with
    /// `format_attachment_chip` (paperclip + size). Reachable through the
    /// rendered string of a focused row.
    #[test]
    fn row_renders_snippet_preview_and_attachment_chip() {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        let mut row = row(1, true);
        // Use a short subject so there's room for the snippet preview at
        // 120-col width.
        row.representative.subject = "Status update".into();
        row.representative.snippet = "Shipping plan ready for review".into();
        // 2.5 MiB → format_attachment_chip emits "📎 2M".
        row.representative.size_bytes = 2 * 1024 * 1024 + 512 * 1024;
        let rows = vec![row];

        let snapshot = render_to_string(120, 6, |frame| {
            draw_view(
                frame,
                Rect::new(0, 0, 120, 6),
                &MailListView {
                    rows: &rows,
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                    title: "Inbox",
                    selected_set: &HashSet::new(),
                    mode: MailListMode::Threads,
                    loading_message: None,
                    loading_throbber: None,
                },
                &Theme::default(),
            );
        });

        assert!(
            snapshot.contains("Shipping plan"),
            "row must surface the snippet preview when there's room; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains(" · "),
            "row must use the ' · ' separator from format_subject_line; got:\n{snapshot}",
        );
        assert!(
            snapshot.contains("2M"),
            "row must surface the size chip from format_attachment_chip; got:\n{snapshot}",
        );
    }

    #[test]
    fn row_renders_thread_count_badge_for_conversations() {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        let mut row = row(4, false);
        row.representative.subject = "Planning".into();
        row.representative.snippet = "Latest reply".into();
        let rows = vec![row];

        let snapshot = render_to_string(100, 6, |frame| {
            draw_view(
                frame,
                Rect::new(0, 0, 100, 6),
                &MailListView {
                    rows: &rows,
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                    title: "Inbox",
                    selected_set: &HashSet::new(),
                    mode: MailListMode::Threads,
                    loading_message: None,
                    loading_throbber: None,
                },
                &Theme::default(),
            );
        });

        assert!(
            snapshot.contains("↔4"),
            "conversation rows must surface a thread-count badge; got:\n{snapshot}",
        );
    }

    /// Narrow renders should fall back gracefully — no snippet, no panic.
    /// The subject still appears, attachment chip still renders the
    /// paperclip but may drop the size depending on column width.
    #[test]
    fn row_omits_snippet_when_terminal_too_narrow() {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        let mut row = row(1, false);
        row.representative.subject = "Hello".into();
        row.representative.snippet = "this snippet should not appear".into();
        let rows = vec![row];

        // 60 cols is the narrow case the formatter is designed to drop
        // the snippet for, leaving the subject alone.
        let snapshot = render_to_string(60, 6, |frame| {
            draw_view(
                frame,
                Rect::new(0, 0, 60, 6),
                &MailListView {
                    rows: &rows,
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                    title: "Inbox",
                    selected_set: &HashSet::new(),
                    mode: MailListMode::Threads,
                    loading_message: None,
                    loading_throbber: None,
                },
                &Theme::default(),
            );
        });

        assert!(
            !snapshot.contains("this snippet"),
            "narrow row must drop the snippet preview; got:\n{snapshot}",
        );
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
