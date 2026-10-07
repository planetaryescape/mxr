//! The Archive lens: records built from mail under an answer box, never a
//! list of emails.
//!
//! Renders `Request::ListRecords` as a ledger by month with each month's
//! count and totals, one row per record (date, issuer, what, amount,
//! reference, PDF), and `Request::AnswerFromRecords` as one answer line
//! with the field asked for and where it came from. The header line and
//! the empty state come from the daemon's mode guide and ledger; hints show
//! in the status line (`app/hints.rs`). Pure render; wiring lives in
//! `app/records_actions.rs`.

use mxr_protocol::{RecordData, RecordExportData, RecordMonthData};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ActivePane, RecordsPageState, RECORD_KIND_CHIPS};
use crate::ui::sanitize::{one_line, truncate};
use crate::ui::todo_lens::wrap;

pub struct RecordsView<'a> {
    pub page: &'a RecordsPageState,
    pub selected_index: usize,
    pub active_pane: &'a ActivePane,
}

/// "03 Mar". Record days are stored at midday UTC, so UTC reads the same
/// day the daemon meant in every zone.
fn day(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%d %b").to_string()
}

fn date_long(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%-d %b %Y").to_string()
}

/// The amount as the ledger shows it, with `·?` while nobody confirmed it.
fn amount_cell(record: &RecordData) -> String {
    let Some(amount) = &record.amount else {
        return String::new();
    };
    let unchecked = record
        .unchecked_fields
        .iter()
        .any(|field| field == "amount");
    format!(
        "{}{}",
        one_line(&amount.display),
        if unchecked { " \u{b7}?" } else { "  " }
    )
}

/// Column widths from the lens width alone, so amounts line up down a
/// month. The reference and the issuer give way first as the lens
/// narrows; the date, what and the amount stay.
struct Columns {
    issuer: usize,
    title: usize,
    amount: usize,
    reference: usize,
    pdf: bool,
}

impl Columns {
    const DATE: usize = 6;
    const AMOUNT: usize = 14;
    const REFERENCE: usize = 12;
    const ISSUER: usize = 16;
    const TITLE_MIN: usize = 12;

    fn for_width(width: usize) -> Self {
        // Marker, date and the gaps between columns.
        let mut left = width.saturating_sub(2 + Self::DATE + 2 + Self::AMOUNT + 1);
        let pdf = left > Self::TITLE_MIN + 6;
        if pdf {
            left -= 4;
        }
        let reference = if left >= Self::TITLE_MIN + Self::ISSUER + Self::REFERENCE + 3 {
            left -= Self::REFERENCE + 1;
            Self::REFERENCE
        } else {
            0
        };
        let issuer = if left >= Self::TITLE_MIN + 10 {
            let issuer = Self::ISSUER.min(left - Self::TITLE_MIN - 1);
            left -= issuer + 1;
            issuer
        } else {
            0
        };
        Self {
            issuer,
            title: left.max(Self::TITLE_MIN),
            amount: Self::AMOUNT,
            reference,
            pdf,
        }
    }
}

fn row_line(
    record: &RecordData,
    selected: bool,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let cols = Columns::for_width(width);
    let marker = if selected { "\u{258c} " } else { "  " };
    let what = record
        .title
        .as_deref()
        .unwrap_or(record.kind_label.as_str());
    let mut spans = vec![
        Span::raw(marker),
        Span::styled(
            format!("{:<6}  ", record.date.map(day).unwrap_or_default()),
            Style::default().fg(theme.text_muted),
        ),
    ];
    if cols.issuer > 0 {
        let issuer_width = cols.issuer;
        spans.push(Span::styled(
            format!(
                "{:<issuer_width$} ",
                truncate(
                    &one_line(record.issuer.as_deref().unwrap_or("-")),
                    issuer_width
                )
            ),
            Style::default().fg(theme.text_primary),
        ));
    }
    let title_width = cols.title;
    spans.push(Span::styled(
        format!("{:<title_width$} ", truncate(&one_line(what), title_width)),
        Style::default().fg(theme.text_secondary),
    ));
    let amount_width = cols.amount;
    spans.push(Span::styled(
        format!("{:>amount_width$}", amount_cell(record)),
        Style::default().fg(theme.text_primary),
    ));
    if cols.reference > 0 {
        let reference_width = cols.reference;
        spans.push(Span::styled(
            format!(
                " {:<reference_width$}",
                truncate(
                    &one_line(record.reference.as_deref().unwrap_or("")),
                    reference_width
                )
            ),
            Style::default().fg(theme.text_muted),
        ));
    }
    if cols.pdf {
        spans.push(Span::styled(
            if record.pdf.is_some() { " pdf" } else { "    " },
            Style::default().fg(theme.accent),
        ));
    }
    if selected {
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }
    Line::from(spans)
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

/// "── 2025 · March ──────── 3 records  £1,412.40"
fn month_line(
    label: &str,
    count: u32,
    totals: &str,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let left = format!(" \u{2500}\u{2500} {label} ");
    let right = format!(
        " {} {}  {totals} ",
        count,
        if count == 1 { "record" } else { "records" }
    );
    let fill = width
        .saturating_sub(left.chars().count() + right.chars().count())
        .max(1);
    Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(theme.text_secondary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "\u{2500}".repeat(fill),
            Style::default().fg(theme.border_unfocused),
        ),
        Span::styled(right, Style::default().fg(theme.text_secondary)),
    ])
}

fn chips_line(page: &RecordsPageState, theme: &crate::theme::Theme) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    for (index, (name, _)) in RECORD_KIND_CHIPS.iter().enumerate() {
        let text = if index == page.kind_chip {
            format!("[{name}] ")
        } else {
            format!("{name} ")
        };
        spans.push(Span::styled(
            text,
            Style::default().fg(if index == page.kind_chip {
                theme.accent
            } else {
                theme.text_muted
            }),
        ));
    }
    // Only the narrowing that is on: "[ ] year" and "p issuer" say how.
    let mut narrowed = Vec::new();
    if let Some(year) = page.filter.year {
        narrowed.push(format!("year: {year}"));
    }
    if let Some(issuer) = page.filter.issuer.as_deref() {
        narrowed.push(format!("issuer: {}", one_line(issuer)));
    }
    if !narrowed.is_empty() {
        spans.push(Span::styled(
            format!("  {}", narrowed.join("  ")),
            Style::default().fg(theme.text_secondary),
        ));
    }
    Line::from(spans)
}

fn answer_lines(
    body: &mut Body,
    page: &RecordsPageState,
    width: usize,
    theme: &crate::theme::Theme,
) {
    let Some(answer) = &page.answer else {
        return;
    };
    let secondary = Style::default().fg(theme.text_secondary);
    let muted = Style::default().fg(theme.text_muted);
    if let Some(list) = &answer.list {
        body.lines.push(Line::from(Span::styled(
            format!(
                " {}",
                truncate(&one_line(&list.header), width.saturating_sub(4))
            ),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )));
        if let (Some(first), Some(last)) = (list.first, list.last) {
            body.text(
                format!("{} to {}", date_long(first), date_long(last)),
                secondary,
            );
        }
        body.text(
            if list.issuer.is_some() {
                "p issuer page  Esc clear search"
            } else {
                "Esc clear search"
            },
            muted,
        );
    } else if let Some(card) = &answer.answer {
        let record = &card.record;
        let provenance = card.provenance.as_ref().map_or_else(String::new, |field| {
            if field.checked {
                format!("\u{2713}{}", field.source)
            } else {
                format!("\u{b7}? {}", field.source)
            }
        });
        let mut context = Vec::new();
        context.extend(record.issuer.as_deref().map(one_line));
        context.extend(record.title.as_deref().map(one_line));
        context.extend(record.date.map(date_long));
        body.lines.push(Line::from(vec![
            Span::styled(
                format!(" \u{25b6} {}  ", one_line(&card.label)),
                Style::default().fg(theme.text_secondary),
            ),
            Span::styled(
                one_line(&card.value),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {provenance}"), secondary),
        ]));
        body.text(
            truncate(&context.join(" \u{b7} "), width.saturating_sub(4)),
            secondary,
        );
        if let Some(group) = &record.group {
            body.text(
                format!(
                    "Part of {} \"{}\" ({})",
                    group.kind,
                    one_line(&group.title),
                    group.count
                ),
                secondary,
            );
        }
        body.text(
            format!(
                "y copy  {}  o email  Esc clear",
                if record.pdf.is_some() {
                    "\u{21b5} pdf"
                } else {
                    "\u{21b5} card"
                }
            ),
            muted,
        );
        if !answer.also.is_empty() {
            let also: Vec<String> = answer
                .also
                .iter()
                .map(|other| {
                    let name = one_line(
                        other
                            .title
                            .as_deref()
                            .or(other.issuer.as_deref())
                            .unwrap_or(""),
                    );
                    match &other.reference {
                        Some(reference) => format!("{name} ({})", one_line(reference)),
                        None => name,
                    }
                })
                .collect();
            body.text(
                truncate(
                    &format!("Also matching: {}", also.join(", ")),
                    width.saturating_sub(4),
                ),
                muted,
            );
        }
        if answer.matching > 1 {
            body.text(format!("a show all {} matches", answer.matching), muted);
        }
    } else if let Some(fallback) = &answer.fallback {
        for line in wrap(&one_line(&fallback.note), width.saturating_sub(4)) {
            body.text(line, secondary);
        }
        if let Some(found) = &fallback.answer {
            for line in wrap(&one_line(&found.text), width.saturating_sub(4))
                .into_iter()
                .take(3)
            {
                body.text(line, Style::default().fg(theme.text_primary));
            }
            for citation in found.citations.iter().take(3) {
                body.text(
                    truncate(
                        &format!("{}  {}", day(citation.date), one_line(&citation.subject)),
                        width.saturating_sub(4),
                    ),
                    muted,
                );
            }
        }
        if let Some(error) = &fallback.error {
            body.text(
                format!("Searching all mail failed: {}", one_line(error)),
                muted,
            );
        }
    }
    body.lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(width),
        Style::default().fg(theme.border_unfocused),
    )));
}

fn ledger_body(view: &RecordsView<'_>, width: usize, theme: &crate::theme::Theme) -> Body {
    let page = view.page;
    let mut body = Body {
        lines: Vec::new(),
        selected_line: 0,
    };
    let muted = Style::default().fg(theme.text_muted);
    let secondary = Style::default().fg(theme.text_secondary);
    let total = page.ledger.as_ref().map_or(0, |ledger| ledger.total);
    let query = if page.asking {
        format!("/ {}_", page.query)
    } else if page.query.is_empty() {
        "/ ask: \"lisbon booking ref\"".to_string()
    } else {
        format!("/ {}", page.query)
    };
    body.lines.push(Line::from(vec![
        Span::styled(
            format!(
                " Archive  {total} {}   ",
                if total == 1 { "record" } else { "records" }
            ),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate(&one_line(&query), width.saturating_sub(24)),
            Style::default().fg(if page.asking {
                theme.accent
            } else {
                theme.text_muted
            }),
        ),
    ]));
    let Some(ledger) = &page.ledger else {
        body.text("Loading Archive…", muted);
        return body;
    };
    if let Some(first_run) = &ledger.first_run {
        body.text(one_line(&first_run.line), muted);
    }
    body.lines.push(Line::from(""));
    answer_lines(&mut body, page, width, theme);
    // A list answer stands in for the ledger: the kind chips and the
    // issuer filter don't narrow it.
    if let Some(list) = page.listed_matches() {
        month_rows(&mut body, &list.records, &list.months, view, width, theme);
        if (list.count as usize) > list.records.len() {
            body.text(
                format!(
                    "{} of {} shown. Narrow the query to see the rest.",
                    list.records.len(),
                    list.count
                ),
                muted,
            );
        }
        return body;
    }
    if !ledger.coming_up.is_empty() {
        body.text("Coming up", secondary.add_modifier(Modifier::BOLD));
        for moment in &ledger.coming_up {
            body.text(
                truncate(
                    &format!("  {}", one_line(&moment.label)),
                    width.saturating_sub(4),
                ),
                Style::default().fg(theme.text_primary),
            );
        }
        body.lines.push(Line::from(""));
    }
    body.lines.push(chips_line(page, theme));
    if let Some(issuer) = &ledger.issuer {
        let totals: Vec<String> = issuer
            .totals
            .iter()
            .map(|total| one_line(&total.display))
            .collect();
        body.text(
            format!(
                "{}: {} records, {}  \u{b7} Esc every issuer",
                one_line(&issuer.name),
                issuer.count,
                totals.join(" and ")
            ),
            secondary,
        );
    }
    if let Some(empty) = &ledger.empty_state {
        body.lines.push(Line::from(""));
        for line in wrap(&one_line(empty), width.saturating_sub(4)) {
            body.text(line, secondary);
        }
        return body;
    }
    month_rows(
        &mut body,
        &ledger.records,
        &ledger.months,
        view,
        width,
        theme,
    );
    if (ledger.matching as usize) > ledger.records.len() {
        body.text(
            format!(
                "{} of {} shown. Narrow with [ ] year, p issuer or g f kind.",
                ledger.records.len(),
                ledger.matching
            ),
            muted,
        );
    }
    body
}

fn month_rows(
    body: &mut Body,
    records: &[RecordData],
    months: &[RecordMonthData],
    view: &RecordsView<'_>,
    width: usize,
    theme: &crate::theme::Theme,
) {
    let mut current: Option<String> = None;
    for (index, record) in records.iter().enumerate() {
        let month = record.date.map(|at| at.format("%Y-%m").to_string());
        if month != current {
            if let Some(header) = months
                .iter()
                .find(|header| Some(&header.month) == month.as_ref())
            {
                let totals = if header.totals.is_empty() {
                    "no amounts".to_string()
                } else {
                    header
                        .totals
                        .iter()
                        .map(|total| one_line(&total.display))
                        .collect::<Vec<_>>()
                        .join(" + ")
                };
                body.lines.push(month_line(
                    &one_line(&header.label),
                    header.count,
                    &totals,
                    width,
                    theme,
                ));
            }
            current = month;
        }
        let selected = index == view.selected_index;
        if selected {
            body.selected_line = body.lines.len();
        }
        body.lines.push(row_line(record, selected, width, theme));
        for extra in [&record.stage_line, &record.detail_line]
            .into_iter()
            .flatten()
        {
            body.lines.push(Line::from(Span::styled(
                format!(
                    "          {}",
                    truncate(&one_line(extra), width.saturating_sub(12))
                ),
                Style::default().fg(theme.text_muted),
            )));
        }
    }
}

fn keys_line(page: &RecordsPageState) -> String {
    if page.asking {
        return "Type what you remember  \u{21b5} ask  Esc stop typing".to_string();
    }
    let keys: Vec<String> = match &page.guide {
        Some(guide) => guide
            .keys
            .iter()
            .filter(|key| !matches!(key.key.as_str(), "e" | "u" | "?" | "t"))
            .map(|key| format!("{} {}", key.key, key.verb))
            .collect(),
        None => mxr_protocol::ARCHIVE_GUIDE
            .keys
            .iter()
            .filter(|(key, _)| !matches!(*key, "e" | "u" | "?" | "t"))
            .map(|(key, verb)| format!("{key} {verb}"))
            .collect(),
    };
    format!("{}  g f kind  ? keys", keys.join("  "))
}

pub fn draw(frame: &mut Frame, area: Rect, view: &RecordsView<'_>, theme: &crate::theme::Theme) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let header = view
        .page
        .guide
        .as_ref()
        .map_or(mxr_protocol::archive_copy::HEADER, |guide| {
            guide.header.as_str()
        });
    let block = Block::bordered()
        .title(format!(" Archive \u{2500} {} ", one_line(header)))
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(is_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let body = ledger_body(view, width, theme);
    let footer_height = 2u16;
    let body_height = inner.height.saturating_sub(footer_height) as usize;
    let scroll = body
        .selected_line
        .saturating_sub(body_height.saturating_sub(3));
    frame.render_widget(
        Paragraph::new(body.lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        Rect {
            height: inner.height.saturating_sub(footer_height),
            ..inner
        },
    );
    let selected = view.page.rows().get(view.selected_index);
    let why = selected
        .map(|record| one_line(&record.why))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {}", truncate(&why, width.saturating_sub(2))),
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(Span::styled(
                format!(
                    " {}",
                    truncate(&keys_line(view.page), width.saturating_sub(2))
                ),
                Style::default().fg(theme.text_muted),
            )),
        ]),
        Rect {
            y: inner.y + inner.height.saturating_sub(footer_height),
            height: footer_height,
            ..inner
        },
    );

    if let Some(record) = &view.page.card {
        draw_card(frame, area, record, theme);
    }
    if let Some(export) = &view.page.export_preview {
        draw_export_preview(frame, area, export, theme);
    }
}

/// The record card: every field with where it came from, its documents
/// and the emails it was built from.
pub(crate) fn card_text(record: &RecordData, width: usize) -> Vec<(String, bool)> {
    let mut lines = vec![(
        format!(
            "{} \u{b7} {}",
            one_line(record.issuer.as_deref().unwrap_or("-")),
            one_line(&record.kind_label)
        ),
        true,
    )];
    if let Some(title) = &record.title {
        lines.push((one_line(title), false));
    }
    lines.push((String::new(), false));
    for field in &record.fields {
        if matches!(field.field.as_str(), "issuer" | "title") {
            continue;
        }
        // An open dot: money or a date nobody has confirmed.
        let dot = if field.checked { " " } else { "\u{25cb}" };
        let evidence = field
            .evidence
            .as_deref()
            .filter(|evidence| *evidence != field.value)
            .map(|evidence| format!(": \"{}\"", one_line(evidence)))
            .unwrap_or_default();
        lines.push((
            truncate(
                &format!(
                    "{dot} {:<12} {:<20} ({}{evidence})",
                    one_line(&field.label),
                    one_line(&field.value),
                    one_line(&field.source_label)
                ),
                width,
            ),
            false,
        ));
    }
    if let Some(stage) = &record.stage_line {
        lines.push((String::new(), false));
        lines.push((one_line(stage), false));
    }
    if let Some(group) = &record.group {
        lines.push((
            format!(
                "Part of {} \"{}\" ({})",
                group.kind,
                one_line(&group.title),
                group.count
            ),
            false,
        ));
    }
    if !record.documents.is_empty() {
        lines.push((String::new(), false));
        for document in &record.documents {
            lines.push((
                truncate(
                    &format!(
                        "{}  {} KB{}",
                        one_line(&document.filename),
                        document.size_bytes / 1024,
                        if document.on_disk {
                            ""
                        } else {
                            "  downloads when opened"
                        }
                    ),
                    width,
                ),
                false,
            ));
        }
    }
    if !record.sources.is_empty() {
        lines.push((String::new(), false));
        lines.push((format!("From {} emails", record.sources.len()), true));
        for source in &record.sources {
            lines.push((
                truncate(
                    &format!(
                        "{}  {:<12} {}",
                        day(source.date),
                        source.stage,
                        one_line(&source.subject)
                    ),
                    width,
                ),
                false,
            ));
        }
    }
    if let Some(others) = record.issuer_records.filter(|count| *count > 0) {
        lines.push((
            format!(
                "Also from {}: {others} more records  p issuer page",
                one_line(record.issuer.as_deref().unwrap_or("-"))
            ),
            false,
        ));
    }
    lines.push((String::new(), false));
    for line in wrap(&one_line(&record.why), width) {
        lines.push((line, false));
    }
    lines
}

fn draw_card(frame: &mut Frame, area: Rect, record: &RecordData, theme: &crate::theme::Theme) {
    let popup = super::centered_rect(76, 80, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Record ")
        .title_bottom(
            " y copy ref  Y copy amount  \u{21b5} pdf  o email  , fix  v mark checked  X not a record  Esc close ",
        )
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let lines: Vec<Line> = card_text(record, inner.width.saturating_sub(1) as usize)
        .into_iter()
        .map(|(text, strong)| {
            Line::from(Span::styled(
                text,
                if strong {
                    Style::default()
                        .fg(theme.text_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_secondary)
                },
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_export_preview(
    frame: &mut Frame,
    area: Rect,
    export: &RecordExportData,
    theme: &crate::theme::Theme,
) {
    let popup = super::centered_rect(60, 45, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Export records ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut lines = vec![Line::from(Span::styled(
        one_line(&export.summary),
        Style::default().fg(theme.text_primary),
    ))];
    lines.push(Line::from(""));
    for kind in &export.by_kind {
        lines.push(Line::from(Span::styled(
            format!("  {:<10} {}", one_line(&kind.label), kind.count),
            Style::default().fg(theme.text_secondary),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Writes a CSV to your Downloads folder. Enter export  Esc cancel",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// The `,` prompt and the `T` pass menu, drawn over any screen: `T` works
/// from the mail list and the reader.
pub fn draw_overlays(
    frame: &mut Frame,
    area: Rect,
    page: &RecordsPageState,
    theme: &crate::theme::Theme,
) {
    if let Some(prompt) = &page.prompt {
        let popup = super::centered_rect(60, 40, area);
        frame.render_widget(Clear, popup);
        let block = Block::bordered()
            .title(" Fix a field ")
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent))
            .style(Style::default().bg(theme.modal_bg));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let mut lines = vec![
            Line::from(Span::styled(
                "Correct a field as field=value. Your value wins on every re-run.",
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!(" \u{203a} {}_", prompt.input),
                Style::default().fg(theme.text_primary),
            )),
            Line::from(""),
        ];
        if let Some(error) = &prompt.error {
            lines.push(Line::from(Span::styled(
                one_line(error),
                Style::default().fg(theme.error),
            )));
        } else if let Some(preview) = &prompt.preview {
            lines.push(Line::from(Span::styled(
                one_line(&preview.message),
                Style::default().fg(theme.text_primary),
            )));
            lines.push(Line::from(Span::styled(
                "Enter apply  Esc cancel",
                Style::default().fg(theme.text_muted),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "amount=\u{a3}12.50 \u{b7} date=3 March 2025 \u{b7} issuer=Amazon \u{b7} kind=receipt",
                Style::default().fg(theme.text_muted),
            )));
            lines.push(Line::from(Span::styled(
                "Enter preview  Esc cancel",
                Style::default().fg(theme.text_muted),
            )));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }
    if let Some(menu) = &page.pass_menu {
        let popup = super::centered_rect(60, 45, area);
        frame.render_widget(Clear, popup);
        let block = Block::bordered()
            .title(" Pass to a mode ")
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.accent))
            .style(Style::default().bg(theme.modal_bg));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let width = inner.width.saturating_sub(1) as usize;
        let mut lines = Vec::new();
        match &menu.preview {
            None => {
                lines.push(Line::from(Span::styled(
                    "  a  Archive: file as a record",
                    Style::default().fg(theme.text_primary),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "a or Enter preview  Esc cancel",
                    Style::default().fg(theme.text_muted),
                )));
            }
            Some(preview) => {
                lines.push(Line::from(Span::styled(
                    one_line(&preview.message),
                    Style::default().fg(theme.text_primary),
                )));
                if let Some(record) = preview.records.first() {
                    lines.push(Line::from(""));
                    for (text, _) in card_text(record, width).into_iter().take(12) {
                        lines.push(Line::from(Span::styled(
                            text,
                            Style::default().fg(theme.text_secondary),
                        )));
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Enter file in Archive  Esc cancel",
                    Style::default().fg(theme.text_muted),
                )));
            }
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

#[cfg(test)]
pub(crate) mod tests;
