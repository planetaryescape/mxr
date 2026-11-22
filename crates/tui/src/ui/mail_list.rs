use crate::app::{ActivePane, MailListMode, MailListRow, RowGists};
use crate::theme::Theme;
use crate::ui::account_mark::AccountMarks;
use crate::ui::sanitize::strip_control_chars;
use chrono::{Datelike, Local, Utc};
use mxr_core::id::MessageId;
use mxr_core::types::MessageFlags;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::collections::HashSet;
use throbber_widgets_tui::{Throbber, BRAILLE_SIX};

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
    pub row_gists: &'a RowGists,
    pub account_marks: &'a AccountMarks,
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

    let show_triage = view.rows.iter().any(|row| row.triage_verdict.is_some());
    let triage_width = if show_triage { 8 } else { 0 };
    let chip_width = view.account_marks.column_width() as u16;
    let sender_width: u16 = if area.width < 100 { 14 } else { 22 };
    let fixed_columns_width = 4 + 1 + 2 + 2 + triage_width + sender_width + 8 + 8 + 2 + chip_width;
    let column_count: u16 = if show_triage { 10 } else { 9 } + u16::from(chip_width > 0);
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
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(2),
    ];
    if show_triage {
        widths.push(Constraint::Length(8));
    }
    widths.extend([
        Constraint::Length(sender_width),
        Constraint::Fill(1), // subject (+ snippet preview)
    ]);
    if chip_width > 0 {
        widths.push(Constraint::Length(chip_width));
    }
    widths.extend([
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(2),
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

    frame.render_widget(table, area);

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

    let line_num_cell = Cell::from(Span::styled(
        format!("{selection_marker}{:>3}", index + 1),
        line_number_style,
    ));

    let unread_cell = Cell::from(Span::styled(
        if is_unread { "N" } else { " " },
        Style::default().fg(row_fg).bold(),
    ));

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

    let safe_subject = strip_control_chars(&env.subject);
    let wake_line = row
        .wake_at
        .map(|wake_at| format!("Back {}", format_wake_long(&wake_at)));
    let snippet = match &wake_line {
        Some(line) => line.as_str(),
        None => view
            .row_gists
            .get(&row.thread_id)
            .map_or(env.snippet.as_str(), crate::app::RowGist::line),
    };
    let safe_snippet = strip_control_chars(snippet);

    let (sender_text_raw, thread_count) = sender_parts(row, view.mode);
    let sender_text = strip_control_chars(&sender_text_raw).into_owned();
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

    let participation_chip = thread_participation_chip(row, view.mode);
    let chip_budget = participation_chip
        .as_ref()
        .map_or(0, |s| s.chars().count() + 1)
        + row.mode_chip.map_or(0, |mode| mode.chars().count() + 2);
    let (subject_text, snippet_preview) = format_subject_line(
        safe_subject.as_ref(),
        safe_snippet.as_ref(),
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
    if let Some(mode) = row.mode_chip {
        subject_chunks.push(Span::styled(
            format!("  {mode}"),
            Style::default().fg(row_muted_fg),
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

    let date_str = match row.wake_at {
        Some(wake_at) => format_wake(&wake_at, &Utc::now()),
        None => format_date(&env.date),
    };
    let date_cell = Cell::from(Span::styled(date_str, Style::default().fg(row_muted_fg)));

    let attach_text = format_attachment_chip(
        env.has_attachments,
        u32::try_from(env.size_bytes).unwrap_or(u32::MAX),
    );
    let attach_cell = if attach_text.is_empty() {
        Cell::from(Span::styled("  ", Style::default().fg(row_fg)))
    } else {
        Cell::from(Span::styled(attach_text, Style::default().fg(row_fg)))
    };

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
    cells.extend([sender_cell, subject_cell]);
    if let Some(spans) = view.account_marks.row_cell(&env.account_id, theme) {
        cells.push(Cell::from(Line::from(spans)));
    }
    cells.extend([date_cell, attach_cell, link_cell]);

    Row::new(cells).style(base_style)
}

fn row_base_style(theme: &Theme, is_selected: bool, is_in_set: bool, is_unread: bool) -> Style {
    match (is_selected, is_in_set) {
        (true, true) => {
            let bg = blend_bg(theme.selection_bg, theme.accent, 96);
            let fg = contrast_foreground(bg, theme);
            Style::default()
                .bg(bg)
                .fg(fg)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        }
        (true, false) => {
            let fg = contrast_foreground(theme.selection_bg, theme);
            Style::default()
                .bg(theme.selection_bg)
                .fg(fg)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        }
        (false, true) => {
            let bg = blend_bg(theme.selection_bg, theme.accent_dim, 72);
            let fg = contrast_foreground(bg, theme);
            Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD)
        }
        (false, false) if is_unread => theme.unread_style(),
        (false, false) => Style::default().fg(theme.text_secondary),
    }
}

fn contrast_foreground(bg: Color, theme: &Theme) -> Color {
    let Some((r, g, b)) = color_rgb(bg) else {
        return theme.selection_fg;
    };
    let luminance = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
    if luminance >= 140 {
        theme.contrast_dark
    } else {
        theme.contrast_light
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

pub(crate) fn color_rgb(color: Color) -> Option<(u8, u8, u8)> {
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

pub(crate) fn format_wake(wake_at: &chrono::DateTime<Utc>, now: &chrono::DateTime<Utc>) -> String {
    let ahead = wake_at.signed_duration_since(*now);
    if ahead.num_seconds() <= 0 {
        return "now".to_string();
    }
    let local = wake_at.with_timezone(&Local);
    let now_local = now.with_timezone(&Local);
    if local.date_naive() == now_local.date_naive() {
        return local.format("%H:%M").to_string();
    }
    if ahead.num_days() < 6 {
        return local.format("%a").to_string();
    }
    if local.year() == now_local.year() {
        local.format("%b %-d").to_string()
    } else {
        local.format("%m/%d/%y").to_string()
    }
}

pub(crate) fn format_wake_long(wake_at: &chrono::DateTime<Utc>) -> String {
    wake_at
        .with_timezone(&Local)
        .format("%a %-d %b, %H:%M")
        .to_string()
}

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

pub fn format_date_relative(date: &chrono::DateTime<Utc>, now: &chrono::DateTime<Utc>) -> String {
    let elapsed = now.signed_duration_since(*date);
    if elapsed.num_seconds() < 0 {
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
            mode_chip: None,
            wake_at: None,
        }
    }

    fn marked_account(
        key: &str,
        enabled: bool,
        color: mxr_core::AccountColor,
        tag: &str,
    ) -> (AccountId, mxr_protocol::AccountSummaryData) {
        let account_id = AccountId::new();
        let summary = mxr_protocol::AccountSummaryData {
            account_id: account_id.clone(),
            key: Some(key.into()),
            name: key.into(),
            email: format!("{key}@example.com"),
            provider_kind: "imap".into(),
            sync_kind: Some("imap".into()),
            send_kind: None,
            enabled,
            is_default: false,
            source: mxr_protocol::AccountSourceData::Config,
            editable: mxr_protocol::AccountEditModeData::Full,
            sync: None,
            send: None,
            capabilities: Default::default(),
            identity: Some(mxr_protocol::AccountIdentityData {
                color,
                tag: tag.into(),
            }),
        };
        (account_id, summary)
    }

    fn two_account_rows(first: &AccountId, second: &AccountId) -> Vec<MailListRow> {
        let mut first_row = row(1, false);
        first_row.representative.subject = "Quarterly plan".into();
        first_row.representative.account_id = first.clone();
        let mut second_row = row(1, false);
        second_row.representative.subject = "Venue booking".into();
        second_row.representative.account_id = second.clone();
        vec![first_row, second_row]
    }

    fn render_marked_rows(
        rows: &[MailListRow],
        marks: &crate::ui::account_mark::AccountMarks,
    ) -> String {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        render_to_string(100, 6, |frame| {
            draw_view(
                frame,
                Rect::new(0, 0, 100, 6),
                &MailListView {
                    rows,
                    selected_index: 0,
                    scroll_offset: 0,
                    active_pane: &ActivePane::MailList,
                    title: "All accounts",
                    selected_set: &HashSet::new(),
                    mode: MailListMode::Messages,
                    loading_message: None,
                    loading_throbber: None,
                    row_gists: &RowGists::new(),
                    account_marks: marks,
                },
                &Theme::default(),
            );
        })
    }

    #[test]
    fn mixed_accounts_mark_each_row_with_its_chip() {
        use crate::ui::account_mark::{AccountMarks, ChipDepth};
        use mxr_core::AccountColor;

        let (first_id, work) = marked_account("work", true, AccountColor::Sky, "W");
        let (second_id, home) = marked_account("home", true, AccountColor::Amber, "PE");
        let rows = two_account_rows(&first_id, &second_id);
        let marks = AccountMarks::from_accounts(&[work, home], false, ChipDepth::Truecolor);

        let rendered = render_marked_rows(&rows, &marks);
        insta::assert_snapshot!("mail_list_account_chips_mixed", rendered);
    }

    #[test]
    fn row_renders_snippet_preview_and_attachment_chip() {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        let mut row = row(1, true);
        row.representative.subject = "Status update".into();
        row.representative.snippet = "Shipping plan ready for review".into();
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
                    row_gists: &RowGists::new(),
                    account_marks: &AccountMarks::default(),
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
    fn subject_escape_sequence_stripped_in_row() {
        use mxr_test_support::render_to_string;
        use std::collections::HashSet;

        let mut row = row(1, false);
        row.representative.subject = "Meeting\x1b[2J notes".into();
        row.representative.snippet = String::new();
        let rows = vec![row];

        let rendered = render_to_string(120, 6, |frame| {
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
                    row_gists: &RowGists::new(),
                    account_marks: &AccountMarks::default(),
                },
                &Theme::default(),
            );
        });

        assert!(
            !rendered.contains('\x1b'),
            "ESC byte must not survive to the rendered row; got:\n{rendered:?}"
        );
        assert!(
            rendered.contains("Meeting"),
            "printable subject text must still appear; got:\n{rendered}"
        );
        assert!(
            rendered.contains("notes"),
            "text after the sequence must still appear; got:\n{rendered}"
        );
    }
}
