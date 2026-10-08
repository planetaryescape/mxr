//! The Reading lens: newsletters as an edition, with a reader beside it.
//!
//! Left, the edition `Request::GetReadingEdition` serves: three bands
//! (since your last visit, earlier this week, fading), each item as a
//! headline with source, minutes and how much you read it, a digest's
//! links under it, and "You left off here" between what's new and what
//! you've seen. Nothing is counted but Later. Right, the item under the
//! cursor in a 72-column reader with a progress line and the Issue and
//! Article texts. Also draws the let-go-of-everything and unsubscribe
//! previews. Pure render; wiring lives in `app/reading_actions.rs`.

use mxr_protocol::{
    ReadingBandData, ReadingItemData, ReadingParagraphData, ReadingUnsubscribeData,
};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{
    ActivePane, ReadingConfirm, ReadingPageState, ReadingRow, ReadingView, READING_LINKS_SHOWN,
};
use crate::ui::sanitize::{one_line, truncate};
use crate::ui::todo_lens::wrap;

pub struct ReadingLensView<'a> {
    pub page: &'a ReadingPageState,
    pub selected_index: usize,
    pub active_pane: &'a ActivePane,
}

/// The reader's measure.
pub(crate) const READER_WIDTH: usize = 72;
/// Below this the lens shows the edition or the reader, not both.
const TWO_PANE_MIN_WIDTH: u16 = 100;
const PROGRESS_CELLS: usize = 20;
const DEFAULT_WPM: u32 = 230;

const LIST_KEYS: &str = "Enter read \u{b7} L article \u{b7} b later \u{b7} e let go \u{b7} D unsubscribe \u{b7} A let go all \u{b7} B Later shelf \u{b7} ? help";
const READER_KEYS: &str = "Esc back \u{b7} j/k scroll \u{b7} h highlight \u{b7} L article \u{b7} b later \u{b7} e let go \u{b7} D unsubscribe \u{b7} R original \u{b7} o email";

/// As many whole "key verb" items as fit in `width`.
fn fit_keys(keys: &str, width: usize) -> String {
    let mut out = String::new();
    for item in keys.split(" \u{b7} ") {
        let next = if out.is_empty() {
            item.to_string()
        } else {
            format!("{out} \u{b7} {item}")
        };
        if next.chars().count() > width {
            break;
        }
        out = next;
    }
    out
}

fn dot() -> &'static str {
    " \u{b7} "
}

/// "Long Reads Weekly · 3 min · you read 8 of 10 · 6 links".
fn meta_line(item: &ReadingItemData) -> String {
    let mut parts = vec![one_line(&item.source), format!("{} min", item.minutes)];
    if let Some(engagement) = &item.engagement {
        parts.push(one_line(engagement));
    }
    if !item.links.is_empty() {
        parts.push(format!("{} links", item.links.len()));
    }
    if item.on_later {
        parts.push("on Later".into());
    }
    parts.join(dot())
}

fn band_label(band: ReadingBandData, label: &str, note: Option<&str>) -> String {
    let label = label.to_uppercase();
    match (band, note) {
        (ReadingBandData::Fading, Some(note)) => format!("{label}  {}", one_line(note)),
        _ => label,
    }
}

struct Body {
    lines: Vec<Line<'static>>,
    selected_line: usize,
}

impl Body {
    fn text(&mut self, text: impl Into<String>, style: Style) {
        self.lines.push(Line::from(Span::styled(
            format!("  {}", text.into()),
            style,
        )));
    }
}

fn marked(spans: Vec<Span<'static>>, selected: bool) -> Line<'static> {
    let mut spans = spans;
    if selected {
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }
    Line::from(spans)
}

fn edition_body(view: &ReadingLensView<'_>, width: usize, theme: &crate::theme::Theme) -> Body {
    let mut body = Body {
        lines: Vec::new(),
        selected_line: 0,
    };
    let page = view.page;
    let muted = Style::default().fg(theme.text_muted);
    let secondary = Style::default().fg(theme.text_secondary);
    let Some(edition) = &page.edition else {
        body.text("Loading Reading\u{2026}", muted);
        return body;
    };
    if page.later_shelf {
        return later_body(view, &edition.later, width, theme);
    }
    if let Some(empty) = &edition.empty {
        body.lines.push(Line::from(""));
        for line in wrap(&one_line(&empty.line), width.saturating_sub(4)) {
            body.text(line, secondary);
        }
    }
    let text_width = width.saturating_sub(4);
    let mut index = 0usize;
    for (band_index, band) in edition.bands.iter().enumerate() {
        body.lines.push(Line::from(""));
        body.lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                truncate(
                    &band_label(band.band, &band.label, band.note.as_deref()),
                    text_width
                )
            ),
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::BOLD),
        )));
        let fading = band.band == ReadingBandData::Fading;
        let text = if fading {
            theme.text_secondary
        } else {
            theme.text_primary
        };
        for item in &band.items {
            let selected = index == view.selected_index;
            if selected {
                body.selected_line = body.lines.len();
            }
            let marker = if selected { "\u{258c} " } else { "  " };
            let mut title = Style::default().fg(text);
            if item.lead {
                title = title.add_modifier(Modifier::BOLD);
            }
            body.lines.push(marked(
                vec![
                    Span::raw(marker),
                    Span::styled(truncate(&one_line(&item.title), text_width), title),
                ],
                selected,
            ));
            body.text(truncate(&meta_line(item), text_width), secondary);
            if let Some(offer) = &item.unsubscribe_offer {
                body.text(
                    truncate(
                        &format!("{}{}D unsubscribe", one_line(offer), dot()),
                        text_width,
                    ),
                    muted,
                );
            }
            index += 1;
            for link in item.links.iter().take(READING_LINKS_SHOWN) {
                let selected = index == view.selected_index;
                if selected {
                    body.selected_line = body.lines.len();
                }
                let via = if link.tracked { "via " } else { "" };
                let domain = format!("  {via}{}", one_line(&link.domain));
                let room = text_width
                    .saturating_sub(4)
                    .saturating_sub(domain.chars().count());
                let marker = if selected { "\u{258c} " } else { "  " };
                body.lines.push(marked(
                    vec![
                        Span::raw(marker),
                        Span::styled("  \u{203a} ", Style::default().fg(theme.accent)),
                        Span::styled(
                            truncate(&one_line(&link.title), room.max(8)),
                            Style::default().fg(text),
                        ),
                        Span::styled(domain, muted),
                    ],
                    selected,
                ));
                index += 1;
            }
            if item.links.len() > READING_LINKS_SHOWN {
                body.text(
                    format!("  + {} more", item.links.len() - READING_LINKS_SHOWN),
                    muted,
                );
            }
        }
        if band_index == 0 && edition.left_off_here {
            body.lines.push(Line::from(""));
            let label = format!(" {} ", mxr_protocol::reading_copy::LEFT_OFF);
            let side = text_width.saturating_sub(label.chars().count()) / 2;
            body.text(
                format!(
                    "{}{label}{}",
                    "\u{2500}".repeat(side),
                    "\u{2500}".repeat(side)
                ),
                muted,
            );
        }
    }
    body
}

/// The Later shelf: what you kept, newest first. Later never fades; an
/// item kept over 30 days asks once if you still want it.
fn later_body(
    view: &ReadingLensView<'_>,
    later: &[ReadingItemData],
    width: usize,
    theme: &crate::theme::Theme,
) -> Body {
    let mut body = Body {
        lines: Vec::new(),
        selected_line: 0,
    };
    let muted = Style::default().fg(theme.text_muted);
    let secondary = Style::default().fg(theme.text_secondary);
    let text_width = width.saturating_sub(4);
    body.lines.push(Line::from(""));
    body.lines.push(Line::from(Span::styled(
        format!(
            "  LATER  {}",
            truncate("Later never fades. Esc back to the edition.", text_width)
        ),
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::BOLD),
    )));
    if later.is_empty() {
        body.lines.push(Line::from(""));
        body.text("Nothing saved. b keeps an item here.", secondary);
    }
    for (index, item) in later.iter().enumerate() {
        let selected = index == view.selected_index;
        if selected {
            body.selected_line = body.lines.len();
        }
        let marker = if selected { "\u{258c} " } else { "  " };
        body.lines.push(marked(
            vec![
                Span::raw(marker),
                Span::styled(
                    truncate(&one_line(&item.title), text_width),
                    Style::default().fg(theme.text_primary),
                ),
            ],
            selected,
        ));
        let mut meta = vec![one_line(&item.source), format!("{} min", item.minutes)];
        if item.article_cached {
            meta.push("saved to read offline".into());
        }
        body.text(truncate(&meta.join(dot()), text_width), secondary);
        body.text(truncate(&one_line(&item.why), text_width), muted);
        if item.still_want_it {
            body.text(
                format!("{} b lets it go", mxr_protocol::reading_copy::STILL_WANT_IT),
                Style::default().fg(theme.accent),
            );
        }
    }
    body
}

/// Minutes left at the reader's pace from how far down they are.
pub(crate) fn minutes_left(words: u32, progress: f64, wpm: u32) -> u32 {
    let left = (f64::from(words) * (1.0 - progress.clamp(0.0, 1.0))).ceil();
    if left <= 0.0 {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a non-negative word count well inside u32"
    )]
    let left = left as u32;
    left.div_ceil(wpm.max(1)).max(1)
}

fn progress_bar(progress: f64) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a clamped fraction of twenty cells"
    )]
    let filled = (progress.clamp(0.0, 1.0) * PROGRESS_CELLS as f64).round() as usize;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a clamped percentage"
    )]
    let percent = (progress.clamp(0.0, 1.0) * 100.0).round() as u32;
    format!(
        "{}{} {percent}%",
        "\u{2593}".repeat(filled),
        "\u{2591}".repeat(PROGRESS_CELLS - filled)
    )
}

/// The text the reader shows, as paragraphs, before wrapping.
fn reader_paragraphs(page: &ReadingPageState) -> Vec<ReadingParagraphData> {
    let Some(detail) = &page.reader else {
        return Vec::new();
    };
    if let Some(original) = &page.original_text {
        return original
            .split("\n\n")
            .filter(|text| !text.trim().is_empty())
            .map(|text| ReadingParagraphData {
                kind: "text".into(),
                text: text.to_string(),
            })
            .collect();
    }
    match (page.view, &detail.article) {
        (ReadingView::Article, Some(article)) => article.paragraphs.clone(),
        (ReadingView::Article, None) => {
            let domain = detail.item.domain.as_deref().unwrap_or("its site");
            let text = match &detail.article_error {
                Some(error) => format!(
                    "The article couldn't be read here: {error}. Open it in a browser: {}",
                    detail.item.url.as_deref().unwrap_or_default()
                ),
                None => format!("Not fetched yet. L fetches it from {domain}."),
            };
            vec![ReadingParagraphData {
                kind: "text".into(),
                text,
            }]
        }
        (ReadingView::Issue, _) => detail.paragraphs.clone(),
    }
}

/// The reader's wrapped lines at `width`, and which paragraph each line
/// belongs to (`None` for the header and the end block), so a highlight
/// takes the paragraph at the top of the view.
pub(crate) fn reader_lines(
    page: &ReadingPageState,
    width: usize,
    theme: &crate::theme::Theme,
) -> (Vec<Line<'static>>, Vec<Option<usize>>) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut owners: Vec<Option<usize>> = Vec::new();
    let Some(detail) = &page.reader else {
        lines.push(Line::from(Span::styled(
            "  Enter reads the item under the cursor.",
            Style::default().fg(theme.text_muted),
        )));
        owners.push(None);
        return (lines, owners);
    };
    let width = width.clamp(16, READER_WIDTH);
    let item = &detail.item;
    let mut push = |line: Line<'static>, owner: Option<usize>| {
        lines.push(line);
        owners.push(owner);
    };
    for line in wrap(&one_line(&item.title), width) {
        push(
            Line::from(Span::styled(
                line,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )),
            None,
        );
    }
    let progress = page.progress();
    let words = match (page.view, &detail.article) {
        (ReadingView::Article, Some(article)) => article.words,
        _ => item.words,
    };
    let left = minutes_left(words, progress, detail.pace_wpm.max(DEFAULT_WPM / 2));
    push(
        Line::from(Span::styled(
            truncate(
                &format!(
                    "{}{}{}{}{left} min left",
                    one_line(&item.source),
                    dot(),
                    item.arrived_at
                        .with_timezone(&chrono::Local)
                        .format("%a %-d %b"),
                    dot()
                ),
                width,
            ),
            Style::default().fg(theme.text_secondary),
        )),
        None,
    );
    push(
        Line::from(Span::styled(
            progress_bar(progress),
            Style::default().fg(theme.accent),
        )),
        None,
    );
    if item.url.is_some() {
        let (issue, article) = match page.view {
            ReadingView::Issue => ("[Issue]", "Article"),
            ReadingView::Article => ("Issue", "[Article]"),
        };
        let fetched = detail.article.as_ref().map_or_else(String::new, |article| {
            format!("{}fetched from {}", dot(), article.contacted.join(", "))
        });
        push(
            Line::from(Span::styled(
                truncate(&format!("{issue}  {article}{fetched}"), width),
                Style::default().fg(theme.text_muted),
            )),
            None,
        );
    }
    if page.original_text.is_some() {
        push(
            Line::from(Span::styled(
                "The email's own text (R for the reader)",
                Style::default().fg(theme.text_muted),
            )),
            None,
        );
    }
    push(Line::from(""), None);
    for (index, paragraph) in reader_paragraphs(page).iter().enumerate() {
        let (prefix, style) = match paragraph.kind.as_str() {
            "heading" => (
                "",
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            "list_item" => ("- ", Style::default().fg(theme.text_primary)),
            "quote" => ("> ", Style::default().fg(theme.text_secondary)),
            _ => ("", Style::default().fg(theme.text_primary)),
        };
        let text = format!("{prefix}{}", one_line(&paragraph.text));
        for line in wrap(&text, width) {
            push(Line::from(Span::styled(line, style)), Some(index));
        }
        push(Line::from(""), None);
    }
    push(
        Line::from(Span::styled(
            "\u{2500}\u{2500} end \u{2500}\u{2500}",
            Style::default().fg(theme.text_muted),
        )),
        None,
    );
    for line in wrap(
        &format!(
            "From this source: {}.",
            one_line(&detail.source_data.evidence)
        ),
        width,
    ) {
        push(
            Line::from(Span::styled(
                line,
                Style::default().fg(theme.text_secondary),
            )),
            None,
        );
    }
    if !detail.highlights.is_empty() {
        push(
            Line::from(Span::styled(
                format!("{} highlight(s) saved", detail.highlights.len()),
                Style::default().fg(theme.text_muted),
            )),
            None,
        );
    }
    (lines, owners)
}

/// The paragraph at the top of the reader, for `h`.
pub(crate) fn paragraph_at_top(page: &ReadingPageState) -> Option<String> {
    let (_, owners) = reader_lines(page, READER_WIDTH, &crate::theme::Theme::default());
    let paragraphs = reader_paragraphs(page);
    owners
        .iter()
        .skip(usize::from(page.scroll))
        .find_map(|owner| *owner)
        .and_then(|index| paragraphs.get(index))
        .map(|paragraph| paragraph.text.clone())
}

/// How many lines the reader holds, for progress.
pub(crate) fn reader_line_count(page: &ReadingPageState) -> u16 {
    let (lines, _) = reader_lines(page, READER_WIDTH, &crate::theme::Theme::default());
    u16::try_from(lines.len()).unwrap_or(u16::MAX)
}

fn draw_list(
    frame: &mut Frame,
    area: Rect,
    view: &ReadingLensView<'_>,
    theme: &crate::theme::Theme,
) {
    let width = area.width as usize;
    let body = edition_body(view, width, theme);
    let height = area.height as usize;
    let scroll = body.selected_line.saturating_sub(height.saturating_sub(3));
    frame.render_widget(
        Paragraph::new(body.lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        area,
    );
}

fn draw_reader(
    frame: &mut Frame,
    area: Rect,
    page: &ReadingPageState,
    focused: bool,
    theme: &crate::theme::Theme,
) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = (inner.width as usize).saturating_sub(2).min(READER_WIDTH);
    let (lines, _) = reader_lines(page, width, theme);
    // Centre the measure in a wide pane.
    let pad = (inner.width as usize).saturating_sub(width) / 2;
    let column = Rect {
        x: inner.x + u16::try_from(pad).unwrap_or(0),
        width: u16::try_from(width + 1)
            .unwrap_or(inner.width)
            .min(inner.width),
        ..inner
    };
    frame.render_widget(Paragraph::new(lines).scroll((page.scroll, 0)), column);
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    view: &ReadingLensView<'_>,
    theme: &crate::theme::Theme,
) {
    let page = view.page;
    let is_focused = *view.active_pane == ActivePane::MailList;
    let header = page
        .guide
        .as_ref()
        .map_or(mxr_protocol::reading_copy::HEADER, |guide| {
            guide.header.as_str()
        });
    let later = page
        .edition
        .as_ref()
        .map_or(0, |edition| edition.later_count);
    let block = Block::bordered()
        .title(format!(" Reading \u{2500} {} ", one_line(header)))
        .title(
            Line::from(format!(" Later {later} "))
                .right_aligned()
                .style(Style::default().fg(theme.accent)),
        )
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(is_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let footer_height = 2u16;
    let body_area = Rect {
        height: inner.height.saturating_sub(footer_height),
        ..inner
    };
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(footer_height),
        height: footer_height,
        ..inner
    };
    let reading = page.reader_focused && page.reader.is_some();
    if inner.width >= TWO_PANE_MIN_WIDTH {
        let list_width = (inner.width * 2 / 5).max(34);
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(list_width), Constraint::Min(20)])
            .split(body_area);
        draw_list(frame, chunks[0], view, theme);
        draw_reader(frame, chunks[1], page, reading, theme);
    } else if reading {
        draw_reader(frame, body_area, page, true, theme);
    } else {
        draw_list(frame, body_area, view, theme);
    }

    let width = inner.width as usize;
    let rows = page.rows();
    let why = rows
        .get(view.selected_index)
        .map(|row| one_line(&row.issue().why))
        .unwrap_or_default();
    let keys = if reading { READER_KEYS } else { LIST_KEYS };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {}", truncate(&why, width.saturating_sub(2))),
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(Span::styled(
                format!(" {}", fit_keys(keys, width.saturating_sub(2))),
                Style::default().fg(theme.text_muted),
            )),
        ]),
        footer_area,
    );

    draw_confirm(frame, area, page.confirm.as_ref(), page, theme);
}

fn method_line(method: ReadingUnsubscribeData) -> &'static str {
    match method {
        ReadingUnsubscribeData::OneClick => {
            "Method: one-click (the sender is told directly, no browser)"
        }
        ReadingUnsubscribeData::Link => "Method: a page the sender asks you to open",
        ReadingUnsubscribeData::Mailto => "Method: an email to the list's address",
        ReadingUnsubscribeData::None => "Method: none found in the headers or the body",
    }
}

fn draw_confirm(
    frame: &mut Frame,
    area: Rect,
    confirm: Option<&ReadingConfirm>,
    page: &ReadingPageState,
    theme: &crate::theme::Theme,
) {
    let Some(confirm) = confirm else {
        return;
    };
    let height = match confirm {
        ReadingConfirm::LetGoAll { thread_ids, .. } => u16::try_from(thread_ids.len())
            .unwrap_or(u16::MAX)
            .saturating_add(8),
        ReadingConfirm::Unsubscribe { .. } => 15,
    };
    let popup = super::centered_rect_fixed_height(70, height.min(area.height), area);
    frame.render_widget(Clear, popup);
    let (title, lines) = match confirm {
        ReadingConfirm::LetGoAll { thread_ids, items } => {
            let titles: Vec<String> = page
                .rows()
                .iter()
                .filter_map(|row| match row {
                    ReadingRow::Item(item) if thread_ids.contains(&item.thread_id) => {
                        Some(one_line(&item.title))
                    }
                    _ => None,
                })
                .collect();
            let archived = items.iter().filter(|item| item.archived > 0).count();
            let mut lines = vec![Line::from(Span::styled(
                format!("Let go of {} in Reading?", thread_ids.len()),
                Style::default().fg(theme.text_primary),
            ))];
            lines.push(Line::from(""));
            for title in titles {
                lines.push(Line::from(Span::styled(
                    format!("  {}", truncate(&title, 56)),
                    Style::default().fg(theme.text_secondary),
                )));
            }
            lines.push(Line::from(""));
            if archived > 0 {
                let provider = items
                    .first()
                    .map_or("the provider", |item| item.provider.as_str());
                lines.push(Line::from(Span::styled(
                    format!("{archived} no other mode holds will be archived in {provider}."),
                    Style::default().fg(theme.text_secondary),
                )));
            }
            lines.push(Line::from(Span::styled(
                "Enter let go of these \u{b7} u undo after \u{b7} Esc keep them",
                Style::default().fg(theme.text_muted),
            )));
            (" Let go of everything shown ", lines)
        }
        ReadingConfirm::Unsubscribe {
            target,
            message_count,
            ..
        } => {
            let issues = if *message_count == 1 {
                "issue"
            } else {
                "issues"
            };
            let lines = vec![
                Line::from(Span::styled(
                    format!("Unsubscribe from {}?", one_line(&target.source)),
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("  {}.", one_line(&target.evidence)),
                    Style::default().fg(theme.text_secondary),
                )),
                Line::from(Span::styled(
                    format!("  {}", method_line(target.method)),
                    Style::default().fg(theme.text_secondary),
                )),
                Line::from(Span::styled(
                    "  u just unsubscribe \u{b7} keep what you have",
                    Style::default().fg(theme.text_secondary),
                )),
                Line::from(Span::styled(
                    format!("  a unsubscribe and clear {message_count} {issues} \u{b7} marks them read and archives them"),
                    Style::default().fg(theme.text_secondary),
                )),
                Line::from(Span::styled(
                    format!("  {}", mxr_protocol::reading_copy::UNSUBSCRIBE_IRREVERSIBLE),
                    Style::default().fg(theme.warning),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Enter or u just unsubscribe \u{b7} a clear its issues \u{b7} Esc keep",
                    Style::default().fg(theme.text_muted),
                )),
            ];
            (" Unsubscribe ", lines)
        }
    };
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

#[cfg(test)]
pub(crate) mod tests;
