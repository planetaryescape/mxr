//! The Updates lens: notifications as a briefing by source, read at a cut
//! and let go in one key (blueprint 22, phase 4).
//!
//! Renders `Request::GetUpdatesDigest` as served: the cut, the headline,
//! Needs a look, Changed and Routine (folded after a few lines), the
//! "since" strip, and the hidden and expired lines all come from the
//! daemon, and the header line, card and keys from Updates' mode guide.
//! Pure render; wiring lives in `app/updates_actions.rs`.

use mxr_protocol::{
    UpdateLineData, UpdateSectionData, UpdatesDigestData, UpdatesLetGoData, UPDATES_GUIDE,
};
use ratatui::prelude::*;
use ratatui::widgets::*;

use super::now_lens::{marker, section, Body};
use crate::app::{ActivePane, UpdatesPageState, UpdatesRow, UpdatesTuneMenu};
use crate::ui::sanitize::{one_line, truncate};
use crate::ui::todo_lens::wrap;

pub struct UpdatesView<'a> {
    pub page: &'a UpdatesPageState,
    pub selected_index: usize,
    pub active_pane: &'a ActivePane,
}

/// A parcel's place on its track: "[##  ]".
fn track(step: Option<u32>, steps: usize) -> String {
    let filled = step.map_or(0, |step| step as usize + 1).min(steps);
    format!("[{}{}]", "#".repeat(filled), " ".repeat(steps - filled))
}

/// What follows the fact: the delta, a tracker's state, when, and "in To
/// do". Code wrote every part of it.
fn suffix(line: &UpdateLineData) -> String {
    let mut parts = Vec::new();
    if let Some(delta) = &line.delta {
        parts.push(one_line(&delta.text));
    }
    if let Some(tracker) = &line.tracker {
        // The fact already names a parcel's state; the bar shows where it is.
        if tracker.kind == "parcel" && tracker.step.is_some() {
            parts.push(track(tracker.step, tracker.steps.len().max(1)));
        }
        if let Some(detail) = &tracker.detail {
            parts.push(one_line(detail));
        }
    }
    if let Some(time) = &line.time_label {
        parts.push(time.clone());
    }
    if let Some(in_todo) = &line.in_todo {
        parts.push(format!("({})", one_line(in_todo)));
    }
    parts.join(" \u{b7} ")
}

/// One source line: a mark for its section, the source, the fact and
/// what code says about it, and the count for a folded routine line.
pub(crate) fn line_row(
    line: &UpdateLineData,
    selected: bool,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let (mark, mark_style) = match line.section {
        UpdateSectionData::NeedsALook => ("! ", Style::default().fg(theme.warning)),
        UpdateSectionData::Changed => ("\u{25c6} ", Style::default().fg(theme.accent)),
        UpdateSectionData::Routine => ("  ", Style::default()),
    };
    let count = if line.section == UpdateSectionData::Routine && line.count > 1 {
        format!(" {}", line.count)
    } else {
        String::new()
    };
    let avail = width.saturating_sub(2 + 2 + count.chars().count());
    let source_width = (avail / 4).clamp(8, 18).min(avail);
    let rest = avail.saturating_sub(source_width + 2);
    let fact = one_line(&line.fact);
    let extra = suffix(line);
    let text = if extra.is_empty() {
        fact
    } else {
        format!("{fact} \u{b7} {extra}")
    };
    Line::from(vec![
        Span::raw(marker(selected)),
        Span::styled(mark, mark_style),
        Span::styled(
            format!(
                "{:<source_width$}  ",
                truncate(&one_line(&line.source_name), source_width)
            ),
            Style::default().fg(theme.text_primary),
        ),
        Span::styled(
            format!("{:<rest$}", truncate(&text, rest)),
            Style::default().fg(theme.text_secondary),
        ),
        Span::styled(count, Style::default().fg(theme.text_muted)),
    ])
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "── since 16:30: 4 (Notion, GitHub 2) ──".
fn since_title(digest: &UpdatesDigestData) -> String {
    let names: Vec<String> = digest
        .since
        .lines
        .iter()
        .map(|line| match line.count {
            0 | 1 => one_line(&line.source_name),
            n => format!("{} {n}", one_line(&line.source_name)),
        })
        .collect();
    format!(
        "since {}: {} ({})",
        digest.cut.label,
        digest.since.message_count,
        names.join(", ")
    )
}

fn body_for(
    view: &UpdatesView<'_>,
    digest: &UpdatesDigestData,
    width: usize,
    theme: &crate::theme::Theme,
) -> Body {
    let mut body = Body {
        lines: Vec::new(),
        selected_line: 0,
    };
    let muted = Style::default().fg(theme.text_muted);
    let secondary = Style::default().fg(theme.text_secondary);
    let cut = format!(
        "{} \u{b7} {} \u{b7} {} from {}",
        one_line(&digest.cut.title),
        digest.cut.label,
        plural(digest.message_count, "update", "updates"),
        plural(digest.source_count, "source", "sources")
    );
    // The key hint goes first when space runs short; the footer has it.
    let let_go = if digest.let_go_line.is_some()
        && cut.chars().count() + "A let go all".len() + 5 <= width
    {
        "A let go all"
    } else {
        ""
    };
    let gap = width.saturating_sub(cut.chars().count() + let_go.len() + 3);
    body.lines.push(Line::from(vec![
        Span::styled(
            format!("  {}", truncate(&cut, width.saturating_sub(4))),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(gap)),
        Span::styled(let_go, muted),
    ]));
    if !digest.headline.is_empty() {
        for line in wrap(&one_line(&digest.headline), width.saturating_sub(4)) {
            body.text(line, secondary);
        }
    }
    if let Some(empty) = &digest.empty_state {
        body.blank();
        for line in wrap(&one_line(empty), width.saturating_sub(4)) {
            body.text(line, secondary);
        }
    }

    let rows = view.page.rows();
    let mut current: Option<UpdateSectionData> = None;
    for (index, row) in rows.iter().enumerate() {
        let selected = index == view.selected_index;
        match row {
            UpdatesRow::Line(line) => {
                if current != Some(line.section) {
                    current = Some(line.section);
                    let title = match line.section {
                        UpdateSectionData::NeedsALook => "Needs a look",
                        UpdateSectionData::Changed => "Changed",
                        UpdateSectionData::Routine => "Routine",
                    };
                    body.blank();
                    body.lines.push(section(title, None, width, theme));
                }
                body.select(line_row(line, selected, width, theme), selected);
                if let Some(suggested) = &line.todo_suggestion {
                    for text in wrap(&one_line(suggested), width.saturating_sub(8)) {
                        body.text(
                            format!("    {text}"),
                            Style::default()
                                .fg(theme.warning)
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        );
                    }
                }
                if let Some(suggestion) = &line.suggestion {
                    for text in wrap(
                        &format!("{}  K tune", one_line(suggestion)),
                        width.saturating_sub(8),
                    ) {
                        body.text(format!("    {text}"), Style::default().fg(theme.accent));
                    }
                }
            }
            UpdatesRow::MoreRoutine(more) => {
                let folded: u32 = digest
                    .routine
                    .iter()
                    .skip(digest.routine.len() - more)
                    .map(|line| line.count)
                    .sum();
                let text = format!(
                    "> {} \u{b7} {}",
                    plural(*more as u32, "quieter source", "quieter sources"),
                    plural(folded, "update", "updates")
                );
                body.select(
                    Line::from(vec![
                        Span::raw(marker(selected)),
                        Span::styled(truncate(&text, width.saturating_sub(4)), muted),
                    ]),
                    selected,
                );
            }
        }
    }
    if let Some(hidden) = &digest.hidden_line {
        body.blank();
        body.text(one_line(hidden), muted);
    }
    if digest.since.message_count > 0 {
        body.blank();
        body.lines
            .push(section(&since_title(digest), None, width, theme));
    }
    if let Some(expired) = &digest.expired_line {
        body.text(one_line(expired), muted);
    }
    body
}

fn keys_line(page: &UpdatesPageState) -> String {
    let keys: Vec<String> = match &page.guide {
        Some(guide) => guide
            .keys
            .iter()
            .filter(|key| key.key != "?")
            .map(|key| format!("{} {}", key.key, key.verb))
            .collect(),
        None => UPDATES_GUIDE
            .keys
            .iter()
            .filter(|(key, _)| *key != "?")
            .map(|(key, verb)| format!("{key} {verb}"))
            .collect(),
    };
    format!("{}  ? keys", keys.join("  "))
}

/// The selected row's why line, with where its link goes.
fn why_line(view: &UpdatesView<'_>) -> String {
    match view.page.rows().get(view.selected_index) {
        Some(UpdatesRow::Line(line)) => {
            let why = one_line(&line.why);
            match &line.link {
                Some(link) => format!("{why}  L opens {}", one_line(&link.domain)),
                None => why,
            }
        }
        Some(UpdatesRow::MoreRoutine(_)) => "\u{21b5} show every routine source".into(),
        None => String::new(),
    }
}

pub fn draw(frame: &mut Frame, area: Rect, view: &UpdatesView<'_>, theme: &crate::theme::Theme) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let header = view
        .page
        .guide
        .as_ref()
        .map(|guide| guide.header.as_str())
        .or_else(|| {
            view.page
                .digest
                .as_ref()
                .map(|digest| digest.header.as_str())
        })
        .unwrap_or(mxr_protocol::updates_copy::HEADER);
    let block = Block::bordered()
        .title(format!(" Updates \u{2500} {} ", one_line(header)))
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(is_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width as usize;

    let body = match &view.page.digest {
        Some(digest) => body_for(view, digest, width, theme),
        None => {
            let mut body = Body {
                lines: Vec::new(),
                selected_line: 0,
            };
            body.text("Loading Updates…", Style::default().fg(theme.text_muted));
            body
        }
    };
    let footer_height = 2u16;
    let body_height = inner.height.saturating_sub(footer_height) as usize;
    let scroll = body
        .selected_line
        .saturating_sub(body_height.saturating_sub(2));
    let body_area = Rect {
        height: inner.height.saturating_sub(footer_height),
        ..inner
    };
    let footer_area = Rect {
        y: inner.y + inner.height.saturating_sub(footer_height),
        height: footer_height,
        ..inner
    };
    frame.render_widget(
        Paragraph::new(body.lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        body_area,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {}", truncate(&why_line(view), width.saturating_sub(2))),
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
        footer_area,
    );

    if let Some(preview) = &view.page.let_go_preview {
        draw_let_go_preview(frame, area, preview, theme);
    }
    if let Some(menu) = &view.page.tune {
        draw_tune_menu(frame, area, menu, theme);
    }
}

/// Letting go of a digest as the daemon previewed it: Enter lets go of
/// exactly that selection. Shared with Now's Updates card.
pub(crate) fn draw_let_go_preview(
    frame: &mut Frame,
    area: Rect,
    preview: &UpdatesLetGoData,
    theme: &crate::theme::Theme,
) {
    let popup = super::centered_rect(60, 40, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Let go of this digest ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let archived: u32 = preview
        .items
        .iter()
        .filter(|item| item.error.is_none())
        .map(|item| item.archived)
        .sum();
    let provider = preview
        .items
        .first()
        .map_or("the mail server", |item| item.provider.as_str());
    let mut lines = vec![Line::from(Span::styled(
        one_line(&preview.line),
        Style::default().fg(theme.text_primary),
    ))];
    if archived > 0 {
        lines.push(Line::from(Span::styled(
            format!(
                "{} archived in {provider}, since no other mode holds them.",
                plural(archived, "email", "emails")
            ),
            Style::default().fg(theme.text_secondary),
        )));
    }
    lines.push(Line::from(Span::styled(
        "Mail that arrives after the cut stays.",
        Style::default().fg(theme.text_secondary),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter let go  u undo after  Esc keep",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_tune_menu(
    frame: &mut Frame,
    area: Rect,
    menu: &UpdatesTuneMenu,
    theme: &crate::theme::Theme,
) {
    let popup = super::centered_rect_fixed_height(50, 9, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(format!(" Tune {} ", one_line(&menu.source_name)))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut lines: Vec<Line<'static>> = crate::app::TUNE_CHOICES
        .iter()
        .enumerate()
        .map(|(index, (setting, label))| {
            let current = if *setting == menu.current {
                "  (now)"
            } else {
                ""
            };
            Line::from(Span::styled(
                format!(" {} {label}{current}", index + 1),
                Style::default().fg(theme.text_primary),
            ))
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " 1-4 choose  Esc cancel",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_protocol::{
        UpdateDeltaData, UpdateLinkData, UpdateSignalData, UpdateSourceSettingData,
        UpdateTrackerData, UpdatesCutData, UpdatesSinceData,
    };
    use mxr_test_support::render_to_string;

    pub(crate) fn line(
        section: UpdateSectionData,
        source: &str,
        fact: &str,
        count: u32,
    ) -> UpdateLineData {
        UpdateLineData {
            id: format!("{source}|{fact}"),
            section,
            account_id: AccountId::new(),
            source_key: source.to_lowercase().replace(' ', "."),
            source_name: source.into(),
            sender_email: format!("notifications@{}.example", source.to_lowercase()),
            fact: fact.into(),
            fact_source: "subject".into(),
            numbers: Vec::new(),
            delta: None,
            signal: match section {
                UpdateSectionData::NeedsALook => UpdateSignalData::NeedsYou,
                UpdateSectionData::Changed => UpdateSignalData::Changed,
                UpdateSectionData::Routine => UpdateSignalData::Routine,
            },
            count,
            message_ids: vec![MessageId::new()],
            thread_ids: vec![ThreadId::new()],
            latest_message_id: Some(MessageId::new()),
            fact_message_id: Some(MessageId::new()),
            latest_thread_id: Some(ThreadId::new()),
            latest_at: Utc.with_ymd_and_hms(2026, 10, 7, 7, 0, 0).unwrap(),
            time_label: None,
            link: None,
            tracker: None,
            todo_id: None,
            in_todo: None,
            todo_suggestion: None,
            todo_title: format!("Check {source}: {fact}"),
            why: "Here because: automated sender (rule). In the 08:00 digest.".into(),
            setting: UpdateSourceSettingData::EveryDigest,
            suggestion: None,
            provenance: Vec::new(),
        }
    }

    fn cut() -> UpdatesCutData {
        UpdatesCutData {
            at: Utc.with_ymd_and_hms(2026, 10, 7, 8, 0, 0).unwrap(),
            label: "08:00".into(),
            title: "This morning's digest".into(),
            previous_at: Utc.with_ymd_and_hms(2026, 10, 6, 16, 30, 0).unwrap(),
            next_at: Utc.with_ymd_and_hms(2026, 10, 7, 16, 30, 0).unwrap(),
            next_label: "16:30".into(),
            cuts: vec!["08:00".into(), "16:30".into()],
        }
    }

    pub(crate) fn populated() -> UpdatesDigestData {
        let mut google = line(
            UpdateSectionData::NeedsALook,
            "Google",
            "Security alert: New sign-in from Chrome on Windows",
            1,
        );
        google.time_label = Some("06:12".into());
        google.todo_id = Some("todo_1".into());
        google.in_todo = Some("already in To do".into());
        let mut stripe = line(
            UpdateSectionData::NeedsALook,
            "Stripe (stripe.com)",
            "Payout of R 4,210.00 failed: bank declined",
            1,
        );
        stripe.todo_suggestion = Some(
            "Suggested to-do: failed payment from Stripe (stripe.com) (rule). t adds it.".into(),
        );
        let mut parcel = line(
            UpdateSectionData::Changed,
            "Bookshop",
            "Parcel out for delivery",
            1,
        );
        parcel.latest_message_id = None;
        parcel.tracker = Some(UpdateTrackerData {
            kind: "parcel".into(),
            state: "out_for_delivery".into(),
            state_label: "out for delivery".into(),
            outcome: "progress".into(),
            steps: ["ordered", "shipped", "out for delivery", "delivered"]
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            step: Some(2),
            detail: Some("Arriving by Thu 8 Oct \u{b7} DHL".into()),
            delivery_id: Some("d1".into()),
        });
        let mut strava = line(
            UpdateSectionData::Changed,
            "Strava",
            "Your week in running: 21.3 km over 3 runs",
            1,
        );
        strava.delta = Some(UpdateDeltaData {
            raw: "21.3 km".into(),
            previous_raw: "19.0 km".into(),
            change: 12.1,
            text: "up 12% on last week".into(),
            against: Utc.with_ymd_and_hms(2026, 9, 30, 5, 0, 0).unwrap(),
        });
        strava.link = Some(UpdateLinkData {
            url: "https://www.strava.com/athlete/training".into(),
            domain: "strava.com".into(),
        });
        let routine = [
            ("Vercel", "Deployment succeeded for acme-web", 7),
            ("Uptime Robot", "All monitors up", 6),
            ("Linear", "3 issues moved to Done", 5),
            ("Plausible", "Weekly report: 998 visitors", 1),
            ("Notion", "Page edited", 2),
            ("Figma", "New comment", 1),
        ]
        .into_iter()
        .map(|(source, fact, count)| line(UpdateSectionData::Routine, source, fact, count))
        .collect();
        UpdatesDigestData {
            generated_at: Utc.with_ymd_and_hms(2026, 10, 7, 10, 0, 0).unwrap(),
            header: mxr_protocol::updates_copy::HEADER.into(),
            cut: cut(),
            headline: "2 need a look, 2 changed. 22 routine from 6 sources.".into(),
            message_count: 26,
            source_count: 10,
            needs_a_look: vec![google, stripe],
            changed: vec![parcel, strava],
            routine,
            since: UpdatesSinceData {
                label: "arriving for 16:30".into(),
                message_count: 3,
                source_count: 2,
                lines: vec![
                    line(UpdateSectionData::Routine, "Notion", "Page edited", 1),
                    line(UpdateSectionData::Routine, "GitHub", "Run passed", 2),
                ],
            },
            hidden_line: Some("4 updates from muted sources not shown.".into()),
            expired_line: Some("1 expired since you last looked.".into()),
            expired_count: 1,
            expired: Vec::new(),
            let_go_line: Some(
                "Let go of 30 updates from 12 sources; 1 also in To do stays there.".into(),
            ),
            selection_token: "token".into(),
            empty_state: None,
            source_total: 12,
            muted_total: 1,
        }
    }

    fn clear() -> UpdatesDigestData {
        UpdatesDigestData {
            headline: String::new(),
            message_count: 0,
            source_count: 0,
            needs_a_look: Vec::new(),
            changed: Vec::new(),
            routine: Vec::new(),
            since: UpdatesSinceData::default(),
            hidden_line: None,
            expired_line: None,
            expired_count: 0,
            let_go_line: None,
            empty_state: Some("Nothing new since 08:00. Next digest at 16:30.".into()),
            ..populated()
        }
    }

    fn render(page: &UpdatesPageState, width: u16) -> String {
        let theme = crate::theme::Theme::default();
        render_to_string(width, 30, |frame| {
            draw(
                frame,
                Rect::new(0, 0, width, 30),
                &UpdatesView {
                    page,
                    selected_index: 0,
                    active_pane: &ActivePane::MailList,
                },
                &theme,
            );
        })
    }

    fn seen_guide() -> mxr_protocol::ModeGuideData {
        UPDATES_GUIDE.to_data(|_| Some(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()))
    }

    #[test]
    fn updates_lens_populated_at_three_widths() {
        let page = UpdatesPageState {
            digest: Some(populated()),
            guide: Some(seen_guide()),
            ..Default::default()
        };
        for width in [60, 80, 120] {
            let rendered = render(&page, width);
            assert!(rendered.contains("Needs a look"), "{rendered}");
            assert!(rendered.contains("quieter source"), "{rendered}");
            insta::assert_snapshot!(format!("updates_lens_populated_{width}"), rendered);
        }
        // Wide enough, the delta and the To do marker show in full.
        let wide = render(&page, 120);
        assert!(wide.contains("up 12% on last week"), "{wide}");
        assert!(wide.contains("already in To do"), "{wide}");
        assert!(wide.contains("since 08:00: 3 (Notion, GitHub 2)"), "{wide}");
    }

    #[test]
    fn updates_lens_clear_at_three_widths() {
        let page = UpdatesPageState {
            digest: Some(clear()),
            guide: Some(UPDATES_GUIDE.to_data(|_| None)),
            ..Default::default()
        };
        for width in [60, 80, 120] {
            let rendered = render(&page, width);
            assert!(rendered.contains("Nothing new since 08:00"), "{rendered}");
            insta::assert_snapshot!(format!("updates_lens_clear_{width}"), rendered);
        }
    }

    #[test]
    fn routine_folds_until_opened() {
        let mut page = UpdatesPageState {
            digest: Some(populated()),
            ..Default::default()
        };
        // 2 needs + 2 changed + 4 routine + the fold.
        assert_eq!(page.row_count(), 9);
        assert!(matches!(page.rows()[8], UpdatesRow::MoreRoutine(2)));
        page.routine_open = true;
        assert_eq!(page.row_count(), 10);
    }
}
