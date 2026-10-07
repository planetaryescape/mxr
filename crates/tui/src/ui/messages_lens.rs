//! The Messages lens: people you talk with on the left, in the daemon's
//! four bands, and the selected person's page on the right, with the topic
//! strip, the selected conversation as new text and the composer line.
//!
//! Renders `ListMessages` and `GetPerson` as served: the bands, order,
//! previews, new text and "trimmed" markers come from the daemon, and the
//! header and empty states from Messages' guide; hints show in the status
//! line (`app/hints.rs`). A
//! narrow terminal shows one pane at a time, the focused one. Pure render;
//! wiring lives in `app/messages_actions.rs`.

use chrono::{DateTime, Datelike, FixedOffset, Utc};
use mxr_protocol::{
    ConversationData, ConversationMessageData, MessageLayoutData, MessagesBandData,
    MessagesPreviewKindData, MessagesRowData, MessagesTopicData, ModeGuideData, PersonPageData,
    TopicStateData,
};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ActivePane, MessagesFocus, MessagesItem, MessagesPageState};
use crate::ui::sanitize::{one_line, truncate};
use crate::ui::todo_lens::wrap;

/// Below this width the lens shows one pane at a time.
const TWO_PANE_MIN_WIDTH: u16 = 90;

pub struct MessagesView<'a> {
    pub page: &'a MessagesPageState,
    pub selected_index: usize,
    pub active_pane: &'a ActivePane,
    /// "Now" for ages and day labels, and the zone to show times in, so the
    /// render stays independent of the machine's clock and zone.
    pub now: DateTime<Utc>,
    pub offset: FixedOffset,
    /// Seconds left on a Got it countdown, when one runs.
    pub ack_seconds_left: Option<u64>,
}

/// "16h", "3d", "40m".
fn age_label(seconds: i64) -> String {
    let minutes = seconds.max(60) / 60;
    if minutes < 60 {
        format!("{minutes}m")
    } else if minutes < 48 * 60 {
        format!("{}h", minutes / 60)
    } else {
        format!("{}d", minutes / (24 * 60))
    }
}

/// "18:02", "Yesterday 18:02", "Tue 18:02", "12 Mar".
fn when(at: DateTime<Utc>, view: &MessagesView<'_>) -> String {
    let local = at.with_timezone(&view.offset);
    let today = view.now.with_timezone(&view.offset).date_naive();
    let days = (today - local.date_naive()).num_days();
    match days {
        i64::MIN..=0 => local.format("%H:%M").to_string(),
        1 => local.format("Yesterday %H:%M").to_string(),
        2..=6 => local.format("%a %H:%M").to_string(),
        _ if local.year() == today.year() => local.format("%-d %b").to_string(),
        _ => local.format("%-d %b %Y").to_string(),
    }
}

/// The list's short day: "15:00", "Yesterday", "Tue", "12 Mar".
fn list_day(at: DateTime<Utc>, view: &MessagesView<'_>) -> String {
    let local = at.with_timezone(&view.offset);
    let today = view.now.with_timezone(&view.offset).date_naive();
    match (today - local.date_naive()).num_days() {
        i64::MIN..=0 => local.format("%H:%M").to_string(),
        1 => "Yesterday".to_string(),
        2..=6 => local.format("%a").to_string(),
        _ if local.year() == today.year() => local.format("%-d %b").to_string(),
        _ => local.format("%-d %b %Y").to_string(),
    }
}

fn band_title(title: &str, theme: &crate::theme::Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {title}"),
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::BOLD),
    ))
}

fn marker(selected: bool) -> &'static str {
    if selected {
        "\u{258c}"
    } else {
        " "
    }
}

struct Body {
    lines: Vec<Line<'static>>,
    selected_line: usize,
}

impl Body {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            selected_line: 0,
        }
    }

    fn text(&mut self, indent: usize, text: impl Into<String>, style: Style) {
        self.lines.push(Line::from(Span::styled(
            format!("{}{}", " ".repeat(indent), text.into()),
            style,
        )));
    }

    fn blank(&mut self) {
        self.lines.push(Line::from(""));
    }

    fn select(&mut self, line: Line<'static>, selected: bool) {
        if selected {
            self.selected_line = self.lines.len();
            let spans = line
                .spans
                .into_iter()
                .map(|span| {
                    let style = span.style.add_modifier(Modifier::REVERSED);
                    span.style(style)
                })
                .collect::<Vec<_>>();
            self.lines.push(Line::from(spans));
        } else {
            self.lines.push(line);
        }
    }
}

/// A row's name line: name on the left, the age and a dot when it's your
/// turn on the right.
fn row_line(
    row: &MessagesRowData,
    selected: bool,
    width: usize,
    view: &MessagesView<'_>,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let age = match row.band {
        MessagesBandData::YourTurn => {
            age_label((view.now - row.turn_since.unwrap_or(row.last_at)).num_seconds())
        }
        _ => list_day(row.last_at, view),
    };
    let dot = if row.your_turn { " *" } else { "  " };
    let right = format!("{age}{dot}");
    let name_width = width.saturating_sub(1 + right.chars().count() + 1);
    let name_style = if row.your_turn || row.unread {
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_primary)
    };
    Line::from(vec![
        Span::raw(marker(selected)),
        Span::styled(
            format!(
                "{:<name_width$} ",
                truncate(&one_line(&row.title), name_width)
            ),
            name_style,
        ),
        Span::styled(right, Style::default().fg(theme.text_muted)),
    ])
}

fn preview_line(
    row: &MessagesRowData,
    width: usize,
    theme: &crate::theme::Theme,
) -> Option<Line<'static>> {
    let preview = row.preview.as_ref()?;
    let text = match preview.kind {
        MessagesPreviewKindData::Ask => format!("\u{201c}{}\u{201d}", one_line(&preview.text)),
        _ => one_line(&preview.text),
    };
    Some(Line::from(Span::styled(
        format!("   {}", truncate(&text, width.saturating_sub(4))),
        Style::default().fg(theme.text_secondary),
    )))
}

fn list_body(view: &MessagesView<'_>, width: usize, theme: &crate::theme::Theme) -> Body {
    let mut body = Body::new();
    let page = view.page;
    let Some(messages) = &page.messages else {
        body.text(
            1,
            "Loading Messages\u{2026}",
            Style::default().fg(theme.text_muted),
        );
        return body;
    };
    let secondary = Style::default().fg(theme.text_secondary);
    if messages.your_turn.is_empty() {
        if let Some(empty) = &messages.empty_state {
            for line in wrap(&one_line(empty), width.saturating_sub(2)) {
                body.text(1, line, secondary);
            }
        }
        for lapsed in &messages.lapsed {
            for line in wrap(&one_line(&lapsed.line), width.saturating_sub(4)) {
                body.text(3, line, Style::default().fg(theme.text_muted));
            }
        }
    }
    let items = page.items();
    let mut band: Option<MessagesBandData> = None;
    for (index, item) in items.iter().enumerate() {
        let selected = index == view.selected_index;
        match item {
            MessagesItem::Row(row) => {
                let shown_band = if band == Some(MessagesBandData::Quiet) {
                    MessagesBandData::Quiet
                } else {
                    row.band
                };
                if band != Some(shown_band) && shown_band != MessagesBandData::Quiet {
                    if !body.lines.is_empty() {
                        body.blank();
                    }
                    body.lines.push(band_title(
                        match shown_band {
                            MessagesBandData::YourTurn => "YOUR TURN",
                            MessagesBandData::Pinned => "PINNED",
                            MessagesBandData::Recent => "RECENT",
                            MessagesBandData::Quiet => "QUIET",
                        },
                        theme,
                    ));
                }
                band = Some(shown_band);
                body.select(row_line(row, selected, width, view, theme), selected);
                if shown_band != MessagesBandData::Pinned && shown_band != MessagesBandData::Quiet {
                    if let Some(line) = preview_line(row, width, theme) {
                        body.lines.push(line);
                    }
                }
            }
            MessagesItem::QuietToggle(count) => {
                if !body.lines.is_empty() {
                    body.blank();
                }
                let arrow = if page.quiet_open { "v" } else { ">" };
                let label = format!("QUIET ({count})");
                let fill = width.saturating_sub(label.chars().count() + 4);
                body.select(
                    Line::from(vec![
                        Span::raw(marker(selected)),
                        Span::styled(
                            label,
                            Style::default()
                                .fg(theme.text_muted)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{}{arrow}", " ".repeat(fill)),
                            Style::default().fg(theme.text_muted),
                        ),
                    ]),
                    selected,
                );
                band = Some(MessagesBandData::Quiet);
            }
        }
    }
    body
}

/// A topic's name on a person's page: "with Ruth: Pricing copy" for a group.
pub(crate) fn topic_label(topic: &MessagesTopicData) -> String {
    if topic.with.is_empty() {
        one_line(&topic.subject)
    } else {
        format!(
            "with {}: {}",
            topic.with.join(", "),
            one_line(&topic.subject)
        )
    }
}

/// The topic strip: "Topics: [Contract renewal *] Launch checklist ·
/// with Ruth: Pricing copy", wrapped.
fn topic_strip(
    page: &PersonPageData,
    width: usize,
    theme: &crate::theme::Theme,
) -> Vec<Line<'static>> {
    let selected = page.conversation.as_ref().map(|c| &c.thread_id);
    let labels: Vec<String> = page
        .topics
        .iter()
        .map(|topic| {
            let mut label = topic_label(topic);
            if topic.state == TopicStateData::YourTurn {
                label.push_str(" *");
            }
            if Some(&topic.thread_id) == selected {
                format!("[{label}]")
            } else {
                label
            }
        })
        .collect();
    wrap(
        &format!("Topics: {}", labels.join(" \u{b7} ")),
        width.saturating_sub(2),
    )
    .into_iter()
    .map(|line| {
        Line::from(Span::styled(
            format!(" {line}"),
            Style::default().fg(theme.text_secondary),
        ))
    })
    .collect()
}

fn message_lines(
    body: &mut Body,
    message: &ConversationMessageData,
    expanded: bool,
    width: usize,
    view: &MessagesView<'_>,
    theme: &crate::theme::Theme,
) {
    let who = if message.from_me {
        "You".to_string()
    } else {
        one_line(message.from.name.as_deref().unwrap_or(&message.from.email))
    };
    let header = format!("{who} \u{b7} {}", when(message.date, view));
    let text = match &message.ask_quote {
        Some(quote) => message
            .text
            .replacen(quote.as_str(), &format!("\u{bb}{quote}\u{ab}"), 1),
        None => message.text.clone(),
    };
    let paragraphs: Vec<&str> = text
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .collect();
    let compact = message.layout == MessageLayoutData::Compact;
    let mine_right = compact && message.from_me;
    let text_width = if compact {
        (width * 2 / 3).max(20).min(width.saturating_sub(2))
    } else {
        width.saturating_sub(2)
    };
    // A closed letter shows its first paragraph, and the ask when it is
    // further down, so the eye lands on what they asked.
    let shown: Vec<&str> = if compact || expanded {
        paragraphs.clone()
    } else {
        paragraphs
            .iter()
            .enumerate()
            .filter(|(index, p)| *index == 0 || p.contains('\u{bb}'))
            .map(|(_, p)| *p)
            .collect()
    };
    let place = |line: String| {
        if mine_right {
            let pad = width.saturating_sub(line.chars().count() + 1);
            format!("{}{line}", " ".repeat(pad))
        } else {
            format!(" {line}")
        }
    };
    body.lines.push(Line::from(Span::styled(
        place(header),
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::BOLD),
    )));
    for (index, paragraph) in shown.iter().enumerate() {
        if index > 0 {
            body.blank();
        }
        for line in wrap(&one_line(paragraph), text_width) {
            let style = if line.contains('\u{bb}') || line.contains('\u{ab}') {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            body.lines
                .push(Line::from(Span::styled(place(line), style)));
        }
    }
    let mut notes: Vec<String> = Vec::new();
    let more = paragraphs.len().saturating_sub(shown.len());
    if more > 0 {
        let noun = if more == 1 { "paragraph" } else { "paragraphs" };
        notes.push(format!("[+{more} {noun}]"));
    }
    for attachment in &message.attachments {
        notes.push(format!(
            "{} {}K",
            one_line(&attachment.filename),
            attachment.size_bytes.div_ceil(1024)
        ));
    }
    if let Some(label) = &message.trimmed_label {
        notes.push(format!("({label}, o as sent)"));
    }
    if !notes.is_empty() {
        for line in wrap(&notes.join("  "), width.saturating_sub(2)) {
            body.lines.push(Line::from(Span::styled(
                place(line),
                Style::default().fg(theme.text_muted),
            )));
        }
    }
    body.blank();
}

fn conversation_lines(
    body: &mut Body,
    conversation: &ConversationData,
    width: usize,
    view: &MessagesView<'_>,
    theme: &crate::theme::Theme,
) {
    if conversation.earlier_count > 0 {
        body.text(
            1,
            format!("{} earlier messages", conversation.earlier_count),
            Style::default().fg(theme.text_muted),
        );
    }
    for message in &conversation.messages {
        let expanded = view.page.expanded.contains(&message.message_id);
        message_lines(body, message, expanded, width, view, theme);
    }
}

fn person_body(
    page: &PersonPageData,
    width: usize,
    view: &MessagesView<'_>,
    theme: &crate::theme::Theme,
) -> Body {
    let mut body = Body::new();
    for line in wrap(&one_line(&page.relationship_line), width.saturating_sub(2)) {
        body.text(1, line, Style::default().fg(theme.text_secondary));
    }
    for suggestion in &page.merge_suggestions {
        for line in wrap(
            &format!(
                "Same person? {} ({}). mxr messages merge --dry-run",
                one_line(&suggestion.name),
                suggestion.addresses.join(", ")
            ),
            width.saturating_sub(2),
        ) {
            body.text(1, line, Style::default().fg(theme.accent));
        }
    }
    body.lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(width),
        Style::default().fg(theme.text_muted),
    )));
    body.lines.extend(topic_strip(page, width, theme));
    body.lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(width),
        Style::default().fg(theme.text_muted),
    )));
    match &page.conversation {
        Some(conversation) => conversation_lines(&mut body, conversation, width, view, theme),
        None => body.text(
            1,
            "No conversation to show.",
            Style::default().fg(theme.text_muted),
        ),
    }
    body
}

/// The footer keys, with their verbs, from the guide.
fn keys_line(page: &MessagesPageState) -> String {
    let wanted = ["r", ".", "e", "t", "b", "c"];
    let verb = |key: &str| -> Option<String> {
        match &page.guide {
            Some(guide) => guide
                .keys
                .iter()
                .find(|k| k.key == key)
                .map(|k| k.verb.clone()),
            None => mxr_protocol::MESSAGES_GUIDE
                .keys
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_string()),
        }
    };
    let mut parts: Vec<String> = wanted
        .iter()
        .filter_map(|key| verb(key).map(|v| format!("{key} {v}")))
        .collect();
    parts.push("[ ] topic".into());
    parts.push("? help".into());
    parts.join("  ")
}

fn render_scrolled(frame: &mut Frame, area: Rect, body: Body) {
    let height = area.height as usize;
    let scroll = body.selected_line.saturating_sub(height.saturating_sub(3));
    frame.render_widget(
        Paragraph::new(body.lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        area,
    );
}

fn draw_list(
    frame: &mut Frame,
    area: Rect,
    view: &MessagesView<'_>,
    focused: bool,
    theme: &crate::theme::Theme,
) {
    let header = view
        .page
        .guide
        .as_ref()
        .map(|guide| guide.header.as_str())
        .or_else(|| view.page.messages.as_ref().map(|m| m.header.as_str()))
        .unwrap_or(mxr_protocol::messages_copy::HEADER);
    let block = Block::bordered()
        .title(format!(" Messages \u{2500} {} ", one_line(header)))
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_scrolled(frame, inner, list_body(view, inner.width as usize, theme));
}

fn draw_person(
    frame: &mut Frame,
    area: Rect,
    view: &MessagesView<'_>,
    focused: bool,
    theme: &crate::theme::Theme,
) {
    let row = view.page.row_at(view.selected_index);
    let page = row.and_then(|row| view.page.page_for_row(row));
    let title = match (row, page) {
        (Some(row), Some(page)) => format!(
            " {} \u{b7} {} ",
            one_line(&row.title),
            one_line(&page.header_line)
        ),
        (Some(row), None) => format!(" {} ", one_line(&row.title)),
        _ => " Messages ".to_string(),
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width as usize;
    let composer_height = 1u16;
    let body_area = Rect {
        height: inner.height.saturating_sub(composer_height),
        ..inner
    };
    let composer_area = Rect {
        y: inner.y + inner.height.saturating_sub(composer_height),
        height: composer_height,
        ..inner
    };
    let body = match page {
        Some(page) => person_body(page, width, view, theme),
        None => {
            let mut body = Body::new();
            let line = if row.is_some() {
                "Loading\u{2026}"
            } else {
                "Pick a person on the left."
            };
            body.text(1, line, Style::default().fg(theme.text_muted));
            body
        }
    };
    // The newest message sits at the bottom, above the composer.
    let overflow = body.lines.len().saturating_sub(body_area.height as usize);
    frame.render_widget(
        Paragraph::new(body.lines).scroll((u16::try_from(overflow).unwrap_or(u16::MAX), 0)),
        body_area,
    );
    let composer = page
        .and_then(|page| page.conversation.as_ref())
        .map(|conversation| format!("> {}", one_line(&conversation.composer.label)))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", truncate(&composer, width.saturating_sub(2))),
            Style::default().fg(theme.accent),
        ))),
        composer_area,
    );
}

pub fn draw(frame: &mut Frame, area: Rect, view: &MessagesView<'_>, theme: &crate::theme::Theme) {
    let pane_focused = *view.active_pane == ActivePane::MailList;
    let footer_height = 2u16;
    let main = Rect {
        height: area.height.saturating_sub(footer_height),
        ..area
    };
    let footer = Rect {
        y: area.y + area.height.saturating_sub(footer_height),
        height: footer_height,
        ..area
    };
    let person_focus = view.page.focus == MessagesFocus::Person;
    if area.width >= TWO_PANE_MIN_WIDTH {
        let list_width = (area.width * 2 / 5).clamp(34, 48);
        let [left, right] =
            Layout::horizontal([Constraint::Length(list_width), Constraint::Min(20)]).areas(main);
        draw_list(frame, left, view, pane_focused && !person_focus, theme);
        draw_person(frame, right, view, pane_focused && person_focus, theme);
    } else if person_focus {
        draw_person(frame, main, view, pane_focused, theme);
    } else {
        draw_list(frame, main, view, pane_focused, theme);
    }

    let width = footer.width as usize;
    let first = match (&view.page.ack, view.ack_seconds_left) {
        (Some(ack), Some(left)) => format!(
            " Got it to {} in {left}s \u{b7} u undo: {}",
            one_line(&ack.to),
            one_line(&ack.plan.text)
        ),
        _ => view
            .page
            .row_at(view.selected_index)
            .map(|row| format!(" {}", one_line(&row.why)))
            .unwrap_or_default(),
    };
    let first_style = if view.page.ack.is_some() {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_secondary)
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(truncate(&first, width), first_style)),
            Line::from(Span::styled(
                format!(
                    " {}",
                    truncate(&keys_line(view.page), width.saturating_sub(2))
                ),
                Style::default().fg(theme.text_muted),
            )),
        ]),
        footer,
    );
}

#[cfg(test)]
pub(crate) mod tests;
