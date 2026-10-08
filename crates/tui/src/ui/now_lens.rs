//! The Now lens: the front page, at most ten things in four fixed
//! sections (People, Due soon, the Updates card, the evening Reading
//! pick), each capped by the daemon with its "and N more" line.
//!
//! Renders `Request::GetNow` as served: the caps, counts, why lines, the
//! overload line and the clear state all come from the daemon, and the
//! header line from Now's mode guide. Hints show in the status line
//! (`app/hints.rs`). Pure render; wiring lives in `app/now_actions.rs`.

use mxr_protocol::{ArrivalsData, NowData, NowPersonData, NowTodoData};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ActivePane, NowPageState};
use crate::ui::sanitize::{one_line, truncate};
use crate::ui::todo_lens::wrap;

/// Shown while the first run sorts history (blueprint 22, first run).
const SORTING_LINE: &str = "Sorting your mail, newest first. Now fills in within a few minutes.";

pub struct NowView<'a> {
    pub page: &'a NowPageState,
    pub selected_index: usize,
    pub active_pane: &'a ActivePane,
    /// The Updates card's start in local time ("08:00"), formatted by the
    /// caller so the render stays independent of the machine's zone.
    pub updates_since: Option<String>,
}

/// "22h", "3d", "40m": how long a person has waited.
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

/// A section rule: "── People ───────────── and 8 more in Messages".
pub(crate) fn section(
    title: &str,
    right: Option<&str>,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let left = format!(" \u{2500}\u{2500} {title} ");
    let right = right
        .map(|text| format!(" {} ", one_line(text)))
        .unwrap_or_default();
    let used = left.chars().count() + right.chars().count();
    let fill = "\u{2500}".repeat(width.saturating_sub(used + 1));
    Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(fill, Style::default().fg(theme.text_muted)),
        Span::styled(right, Style::default().fg(theme.text_secondary)),
    ])
}

pub(crate) struct Body {
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) selected_line: usize,
}

impl Body {
    pub(crate) fn text(&mut self, text: impl Into<String>, style: Style) {
        self.lines.push(Line::from(Span::styled(
            format!("  {}", text.into()),
            style,
        )));
    }

    pub(crate) fn blank(&mut self) {
        self.lines.push(Line::from(""));
    }

    pub(crate) fn select(&mut self, line: Line<'static>, selected: bool) {
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

pub(crate) fn marker(selected: bool) -> &'static str {
    if selected {
        "\u{258c} "
    } else {
        "  "
    }
}

fn person_line(
    person: &NowPersonData,
    selected: bool,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let row = &person.row;
    let who = one_line(
        row.counterparty_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&row.counterparty_email),
    );
    let age = age_label(row.age_seconds);
    let avail = width.saturating_sub(2 + 1 + age.chars().count() + 1);
    let who_width = (avail / 3).clamp(8, 22).min(avail);
    let subject_width = avail.saturating_sub(who_width + 2);
    let subject = truncate(&one_line(&row.subject), subject_width);
    Line::from(vec![
        Span::raw(marker(selected)),
        Span::styled(
            format!("{:<who_width$}  ", truncate(&who, who_width)),
            Style::default().fg(theme.text_primary),
        ),
        Span::styled(
            format!("{subject:<subject_width$} "),
            Style::default().fg(theme.text_secondary),
        ),
        Span::styled(age, Style::default().fg(theme.text_muted)),
    ])
}

fn todo_line(
    item: &NowTodoData,
    selected: bool,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let todo = &item.todo;
    let when = one_line(&todo.when_label);
    let who = one_line(
        todo.person_label
            .as_deref()
            .or(todo.counterparty.as_deref())
            .unwrap_or(""),
    );
    let avail = width.saturating_sub(2 + 4);
    let when_width = when.chars().count().min(avail.saturating_sub(12));
    let who_width = if avail > when_width + 40 {
        who.chars().count().min(18)
    } else {
        0
    };
    let title_width = avail.saturating_sub(when_width + who_width + 3);
    let mut spans = vec![
        Span::raw(marker(selected)),
        Span::styled("[ ] ", Style::default().fg(theme.text_muted)),
        Span::styled(
            format!(
                "{:<title_width$} ",
                truncate(&one_line(&todo.title), title_width)
            ),
            Style::default().fg(theme.text_primary),
        ),
        Span::styled(
            truncate(&when, when_width),
            Style::default().fg(theme.text_muted),
        ),
    ];
    if who_width > 0 {
        spans.push(Span::styled(
            format!("  {}", truncate(&who, who_width)),
            Style::default().fg(theme.text_secondary),
        ));
    }
    Line::from(spans)
}

fn body_for(view: &NowView<'_>, now: &NowData, width: usize, theme: &crate::theme::Theme) -> Body {
    let mut body = Body {
        lines: Vec::new(),
        selected_line: 0,
    };
    let muted = Style::default().fg(theme.text_muted);
    let secondary = Style::default().fg(theme.text_secondary);
    if !now.headline.is_empty() {
        body.lines.push(Line::from(Span::styled(
            format!("  {}", one_line(&now.headline)),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )));
    }
    if !now.first_run.complete {
        body.text(SORTING_LINE, secondary);
    }
    let arrivals = view.page.arrivals.as_ref();
    let clear = now.item_count == 0;
    let mut index = 0usize;
    if let Some(arrivals) = arrivals {
        arrivals_lines(&mut body, view, arrivals, clear, width, theme, &mut index);
    }
    if clear {
        if let Some(empty) = &now.empty_state {
            // The arrivals line already said "Clear."; keep only when the
            // next to-do surfaces.
            let empty = one_line(empty);
            let empty = match arrivals.and_then(|a| a.clear_line.as_ref()) {
                Some(_) => empty
                    .strip_prefix(mxr_protocol::now_copy::CLEAR)
                    .map_or(empty.as_str(), str::trim)
                    .to_string(),
                None => empty,
            };
            if !empty.is_empty() {
                body.blank();
                for line in wrap(&empty, width.saturating_sub(4)) {
                    body.text(line, secondary);
                }
            }
        }
    }

    if !now.people.rows.is_empty() {
        body.blank();
        body.lines.push(section(
            "People",
            now.people.more_line.as_deref(),
            width,
            theme,
        ));
        if let Some(overload) = &now.people.overload_line {
            for line in wrap(&one_line(overload), width.saturating_sub(4)) {
                body.text(line, Style::default().fg(theme.accent));
            }
        }
        for person in &now.people.rows {
            let selected = index == view.selected_index;
            body.select(person_line(person, selected, width, theme), selected);
            if let Some(question) = &person.new_sender {
                let choices = question
                    .choices
                    .iter()
                    .enumerate()
                    .map(|(i, choice)| format!("{} {}", i + 1, choice.label))
                    .collect::<Vec<_>>()
                    .join("  ");
                for line in wrap(
                    &format!("{}  {choices}", one_line(&question.question)),
                    width.saturating_sub(6),
                ) {
                    body.text(format!("  {line}"), Style::default().fg(theme.accent));
                }
            }
            index += 1;
        }
    }
    if !now.due_soon.todos.is_empty() {
        body.blank();
        body.lines.push(section(
            "Due soon",
            now.due_soon.more_line.as_deref(),
            width,
            theme,
        ));
        for item in &now.due_soon.todos {
            let selected = index == view.selected_index;
            body.select(todo_line(item, selected, width, theme), selected);
            index += 1;
        }
    }
    if let Some(card) = &now.updates {
        body.blank();
        let title = match &view.updates_since {
            Some(cut) => format!("Updates {cut} digest"),
            None => "Updates".to_string(),
        };
        body.lines
            .push(section(&title, card.more_line.as_deref(), width, theme));
        let selected = index == view.selected_index;
        let headline = if card.headline.is_empty() {
            &card.line
        } else {
            &card.headline
        };
        body.select(
            Line::from(vec![
                Span::raw(marker(selected)),
                Span::styled(
                    truncate(&one_line(headline), width.saturating_sub(4)),
                    Style::default().fg(theme.text_primary),
                ),
            ]),
            selected,
        );
        for line in &card.lines {
            body.lines.push(super::updates_lens::line_row(
                line,
                false,
                width.saturating_sub(2),
                theme,
            ));
        }
        body.text("  \u{21b5} open  A let go of this digest", muted);
        index += 1;
    }
    if let Some(pick) = &now.reading {
        body.blank();
        body.lines.push(section("For tonight", None, width, theme));
        let selected = index == view.selected_index;
        let from = one_line(pick.sender_name.as_deref().unwrap_or(&pick.sender_email));
        let from_width = from.chars().count().min(24);
        let subject_width = width.saturating_sub(2 + from_width + 3);
        body.select(
            Line::from(vec![
                Span::raw(marker(selected)),
                Span::styled(
                    format!(
                        "{:<subject_width$} ",
                        truncate(&one_line(&pick.subject), subject_width)
                    ),
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled(truncate(&from, from_width), secondary),
            ]),
            selected,
        );
    }
    if let Some(not_now) = &now.not_now {
        body.blank();
        body.text(one_line(not_now), muted);
    }
    if let Some(track) = arrivals.and_then(|a| a.track_record.as_ref()) {
        body.blank();
        body.text(one_line(track), muted);
    }
    body
}

/// The arrivals line under the headline, muted and with no count of its
/// own to chase, then the day's Not-sure questions. Selectable: Enter on
/// the line lists the emails behind it; a mode key answers a question.
fn arrivals_lines(
    body: &mut Body,
    view: &NowView<'_>,
    arrivals: &ArrivalsData,
    clear: bool,
    width: usize,
    theme: &crate::theme::Theme,
    index: &mut usize,
) {
    let muted = Style::default().fg(theme.text_muted);
    let line = match (&arrivals.clear_line, clear) {
        (Some(clear_line), true) => clear_line,
        _ => &arrivals.line,
    };
    let selected = *index == view.selected_index;
    let mut wrapped = wrap(&one_line(line), width.saturating_sub(4)).into_iter();
    if let Some(first) = wrapped.next() {
        body.select(
            Line::from(vec![
                Span::raw(marker(selected)),
                Span::styled(first, muted),
            ]),
            selected,
        );
    }
    for rest in wrapped {
        body.text(rest, muted);
    }
    *index += 1;
    if arrivals.not_sure.is_empty() {
        return;
    }
    body.blank();
    body.lines.push(section("Not sure", None, width, theme));
    if let Some(ask) = &arrivals.not_sure_line {
        body.text(one_line(ask), Style::default().fg(theme.text_secondary));
    }
    let choices = crate::app::MOVE_CHOICES
        .iter()
        .map(|(key, mode)| format!("{key} {}", mode.name()))
        .collect::<Vec<_>>()
        .join("  ");
    for question in &arrivals.not_sure {
        let selected = *index == view.selected_index;
        body.select(
            Line::from(vec![
                Span::raw(marker(selected)),
                Span::styled(
                    truncate(&one_line(&question.line), width.saturating_sub(4)),
                    Style::default().fg(theme.text_primary),
                ),
            ]),
            selected,
        );
        if selected {
            body.text(format!("  {choices}"), Style::default().fg(theme.accent));
        }
        *index += 1;
    }
}

fn keys_line(page: &NowPageState) -> String {
    let rest: Vec<String> = match &page.guide {
        Some(guide) => guide
            .keys
            .iter()
            .filter(|key| !matches!(key.key.as_str(), "Enter" | "?"))
            .map(|key| format!("{} {}", key.key, key.verb))
            .collect(),
        None => mxr_protocol::NOW_GUIDE
            .keys
            .iter()
            .filter(|(key, _)| !matches!(*key, "Enter" | "?"))
            .map(|(key, verb)| format!("{key} {verb}"))
            .collect(),
    };
    format!("\u{21b5} open in its mode  {}  ? keys", rest.join("  "))
}

pub fn draw(frame: &mut Frame, area: Rect, view: &NowView<'_>, theme: &crate::theme::Theme) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let header = view
        .page
        .guide
        .as_ref()
        .map(|guide| guide.header.as_str())
        .or_else(|| view.page.now.as_ref().map(|now| now.header.as_str()))
        .unwrap_or(mxr_protocol::now_copy::HEADER);
    let block = Block::bordered()
        .title(format!(" Now \u{2500} {} ", one_line(header)))
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(is_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = inner.width as usize;

    let body = match &view.page.now {
        Some(now) => body_for(view, now, width, theme),
        None => {
            let mut body = Body {
                lines: Vec::new(),
                selected_line: 0,
            };
            body.text("Loading Now…", Style::default().fg(theme.text_muted));
            body
        }
    };
    let rows = view.page.rows();
    let selected = rows.get(view.selected_index).copied();

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
    let why = selected.map(|row| one_line(row.why())).unwrap_or_default();
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
        footer_area,
    );

    if let Some(preview) = &view.page.digest_preview {
        super::updates_lens::draw_let_go_preview(frame, area, preview, theme);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_protocol::{
        DeskLaneKind, DeskRowData, NowDueData, NowPeopleData, NowUpdatesCardData,
        ScreenerChoiceData, ScreenerQuestionData, SenderKindData, TodoFirstRunData, NOW_GUIDE,
    };
    use mxr_test_support::render_to_string;

    fn person(name: &str, subject: &str, hours: i64) -> NowPersonData {
        let thread_id = ThreadId::new();
        NowPersonData {
            row: DeskRowData {
                lane: DeskLaneKind::Owed,
                account_id: AccountId::new(),
                thread_id,
                message_id: MessageId::new(),
                message_ids: Vec::new(),
                counterparty_email: format!(
                    "{}@example.com",
                    name.to_lowercase().replace(' ', ".")
                ),
                counterparty_name: Some(name.into()),
                subject: subject.into(),
                reason: "replied to your message".into(),
                since: Utc::now(),
                age_seconds: hours * 3600,
                usual_seconds: None,
                usual_samples: 0,
                overdue: false,
                unread: true,
                starred: false,
                commitment_id: None,
                back_at: None,
            },
            why: format!("From Messages: your turn with {name}, {hours}h."),
            new_sender: None,
        }
    }

    pub(crate) fn populated() -> NowData {
        let mut iris = person("Iris Chen", "Does the incident note read right?", 24);
        iris.new_sender = Some(ScreenerQuestionData {
            account_id: AccountId::new(),
            sender_email: "iris.chen@example.com".into(),
            question: "New sender. Keep in Messages?".into(),
            choices: ["Messages", "Updates", "Reading", "Block"]
                .iter()
                .zip([
                    SenderKindData::People,
                    SenderKindData::PaperTrail,
                    SenderKindData::Reading,
                    SenderKindData::ScreenedOut,
                ])
                .map(|(label, kind)| ScreenerChoiceData {
                    kind,
                    label: (*label).into(),
                })
                .collect(),
        });
        let mut tax = crate::ui::todo_lens::tests::council_tax();
        tax.thread_id = Some(ThreadId::new());
        let mut lease = crate::ui::todo_lens::tests::todo("todo_9", "Sign lease renewal");
        lease.when_label = "act by Mon 13 Oct \u{b7} due Wed 15 Oct".into();
        lease.person_label = Some("Sam".into());
        NowData {
            generated_at: Utc::now(),
            header: NOW_GUIDE.header.into(),
            headline: "Friday afternoon. 3 people, 2 things to act on.".into(),
            people: NowPeopleData {
                rows: vec![
                    person("Maya Ortiz", "Can you send the launch checklist?", 22),
                    person("Sam Okafor", "Are you around Thursday?", 3),
                    iris,
                ],
                total: 11,
                more_line: Some("and 8 more in Messages".into()),
                overload_line: Some(
                    "11 people are waiting on you. The three below are furthest past your usual pace."
                        .into(),
                ),
            },
            due_soon: NowDueData {
                todos: [tax, lease]
                    .into_iter()
                    .map(|todo| NowTodoData {
                        why: format!("From To do: {}.", todo.when_label),
                        todo,
                    })
                    .collect(),
                total: 4,
                more_line: Some("and 2 more in To do".into()),
            },
            updates: Some(NowUpdatesCardData {
                message_count: 23,
                source_count: 9,
                top_sources: Vec::new(),
                line: "23 updates from 9 sources. Most from GitHub, Vercel and Stripe.".into(),
                since: Utc::now(),
                thread_ids: vec![ThreadId::new()],
                early: false,
                title: "This morning's digest".into(),
                cut_label: "08:00".into(),
                headline: "1 needs a look, 2 changed. 23 routine from 9 sources.".into(),
                lines: {
                    use crate::ui::updates_lens::tests::line;
                    use mxr_protocol::UpdateSectionData;
                    let mut google = line(
                        UpdateSectionData::NeedsALook,
                        "Google",
                        "New sign-in from Chrome on Windows",
                        1,
                    );
                    google.in_todo = Some("already in To do".into());
                    vec![
                        google,
                        line(UpdateSectionData::Changed, "Strava", "21.3 km over 3 runs", 1),
                    ]
                },
                more_line: Some("+23 routine".into()),
                selection_token: "token".into(),
                let_go_line: Some("Let go of 26 updates from 12 sources.".into()),
            }),
            reading: None,
            not_now: Some("Not now: Reading 6 this week".into()),
            item_count: 6,
            empty_state: None,
            next_at: None,
            first_run: TodoFirstRunData {
                complete: true,
                scanned: 10,
                reached: None,
            },
            coming_up: Vec::new(),
        }
    }

    pub(crate) fn clear() -> NowData {
        NowData {
            headline: "Friday afternoon. Nothing needs you.".into(),
            people: NowPeopleData::default(),
            due_soon: NowDueData::default(),
            updates: None,
            not_now: None,
            item_count: 0,
            empty_state: Some("Clear. The next to-do surfaces Mon 19 Oct 09:00.".into()),
            ..populated()
        }
    }

    pub(crate) fn page(now: NowData, hints_seen: bool) -> NowPageState {
        NowPageState {
            now: Some(now),
            guide: Some(NOW_GUIDE.to_data(|_| hints_seen.then(Utc::now))),
            ..NowPageState::default()
        }
    }

    fn render_at(page: &NowPageState, width: u16, selected_index: usize) -> String {
        render_to_string(width, 34, |frame| {
            draw(
                frame,
                Rect::new(0, 0, width, 34),
                &NowView {
                    page,
                    selected_index,
                    active_pane: &ActivePane::MailList,
                    updates_since: Some("08:00".into()),
                },
                &crate::theme::Theme::default(),
            );
        })
    }

    #[test]
    fn now_shows_four_fixed_sections_with_their_more_lines_at_every_width() {
        let page = page(populated(), true);
        for width in [60u16, 80, 120] {
            let rendered = render_at(&page, width, 0);
            let people = rendered.find("People").expect("People section");
            let due = rendered.find("Due soon").expect("Due soon section");
            let updates = rendered.find("Updates 08:00 digest").expect("Updates card");
            assert!(
                people < due && due < updates,
                "{width}: fixed order\n{rendered}"
            );
            assert!(rendered.contains("Maya Ortiz"), "{width}\n{rendered}");
            assert!(rendered.contains("[ ] Pay council"), "{width}\n{rendered}");
            assert!(
                rendered.contains("New sender. Keep in Messages?"),
                "{width}"
            );
            assert!(rendered.contains("Not now: Reading 6 this week"), "{width}");
            insta::assert_snapshot!(format!("now_lens_populated_{width}"), rendered);
        }
        let wide = render_at(&page, 120, 0);
        assert!(wide.contains("and 8 more in Messages"), "{wide}");
        assert!(wide.contains("and 2 more in To do"), "{wide}");
        assert!(!wide.contains("early version"), "{wide}");
        assert!(wide.contains("+23 routine"), "{wide}");
        assert!(wide.contains("(already in To do)"), "{wide}");
        assert!(
            wide.contains("From Messages: your turn with Maya Ortiz"),
            "{wide}"
        );
    }

    #[test]
    fn a_clear_now_says_when_the_next_thing_surfaces() {
        let page = page(clear(), true);
        for width in [60u16, 80, 120] {
            let rendered = render_at(&page, width, 0);
            assert!(
                rendered.contains("Clear. The next to-do surfaces"),
                "{width}\n{rendered}"
            );
            assert!(
                !rendered.contains("People"),
                "{width}: empty sections disappear"
            );
            insta::assert_snapshot!(format!("now_lens_clear_{width}"), rendered);
        }
    }

    #[test]
    fn no_card_teaches_at_the_top_even_before_any_hint_is_seen() {
        let unseen = page(populated(), false);
        for width in [60u16, 80, 120] {
            let rendered = render_at(&unseen, width, 0);
            assert!(
                !rendered.contains("Now shows at most ten things"),
                "{width}\n{rendered}"
            );
            assert!(!rendered.contains("Esc close"), "{width}");
        }
    }

    #[test]
    fn mail_text_cannot_reach_the_terminal_as_control_sequences() {
        let mut now = populated();
        now.people.rows[0].row.subject = "Hi\u{1b}[31m\nthere\u{202e}".into();
        let rendered = render_at(&page(now, true), 120, 0);
        assert!(!rendered.contains('\u{1b}'));
        assert!(!rendered.contains('\u{202e}'));
    }

    fn arrivals(questions: usize) -> mxr_protocol::ArrivalsData {
        use mxr_protocol::{ArrivalBucketData, ArrivalCountData, ModeKindData, MoveChoiceData};
        let now = Utc::now();
        let count = |bucket, count: u32, label: &str| ArrivalCountData {
            bucket,
            count,
            label: label.into(),
        };
        let not_sure: Vec<mxr_protocol::NotSureData> = (0..questions)
            .map(|i| mxr_protocol::NotSureData {
                account_id: AccountId::new(),
                message_id: MessageId::new(),
                thread_id: ThreadId::new(),
                sender_email: "maya@example.com".into(),
                sender_name: Some("Maya Ortiz".into()),
                subject: format!("Q{i} plan"),
                mode: ModeKindData::Updates,
                line: format!("Maya Ortiz copied you on \"Q{i} plan\". Updates for now."),
                choices: MoveChoiceData::all(),
            })
            .collect();
        mxr_protocol::ArrivalsData {
            generated_at: now,
            since: now,
            until: now,
            since_label: "08:12".into(),
            total: 50,
            counts: vec![
                count(ArrivalBucketData::Messages, 8, "8 Messages"),
                count(ArrivalBucketData::Updates, 10, "10 Updates"),
                count(ArrivalBucketData::Reading, 31, "31 Reading"),
                count(ArrivalBucketData::Spam, 1, "1 spam"),
            ],
            also: vec![count(ArrivalBucketData::Todo, 2, "2 in To do")],
            line: "Since 08:12: 50 arrived. 8 Messages \u{b7} 10 Updates \u{b7} 31 Reading \u{b7} 1 spam. Also 2 in To do.".into(),
            clear_line: Some("Clear. All 50 emails since 08:12 are accounted for.".into()),
            latest_at: Some(now),
            not_sure_line: (questions > 0).then(|| {
                format!("{questions} emails I wasn't sure about. Where should these go?")
            }),
            not_sure_hint: None,
            not_sure,
            track_record: Some("Last week mxr sorted 310 emails; you moved 2.".into()),
            never_bury: String::new(),
        }
    }

    #[test]
    fn the_arrivals_line_sits_under_the_headline_with_its_questions_and_track_record() {
        let mut with_line = page(populated(), true);
        with_line.arrivals = Some(arrivals(2));
        for width in [60u16, 80, 120] {
            let rendered = render_at(&with_line, width, 1);
            let line = rendered.find("Since 08:12").expect("the line");
            let not_sure = rendered.find("Not sure").expect("the questions");
            let people = rendered.find("People").expect("People");
            assert!(line < not_sure && not_sure < people, "{width}\n{rendered}");
            // The selected question shows its keys; the other doesn't.
            assert_eq!(
                rendered.matches("m Messages").count(),
                1,
                "{width}\n{rendered}"
            );
            insta::assert_snapshot!(format!("now_lens_arrivals_{width}"), rendered);
        }
        let wide = render_at(&with_line, 120, 0);
        assert!(wide.contains("Since 08:12: 50 arrived."), "{wide}");
        assert!(wide.contains("2 emails I wasn't sure about"), "{wide}");
        assert!(wide.contains("Maya Ortiz copied you on \"Q0 plan\""), "{wide}");
        assert!(
            wide.contains("Last week mxr sorted 310 emails; you moved 2."),
            "{wide}"
        );
        assert!(wide.contains("Every email since then"), "the footer explains the line");
    }

    #[test]
    fn a_clear_now_says_every_arrival_is_accounted_for_once() {
        let mut clear_now = page(clear(), true);
        clear_now.arrivals = Some(arrivals(0));
        let rendered = render_at(&clear_now, 120, 0);
        assert!(
            rendered.contains("Clear. All 50 emails since 08:12 are accounted for."),
            "{rendered}"
        );
        assert!(!rendered.contains("Since 08:12:"), "{rendered}");
        assert!(
            rendered.contains("The next to-do surfaces"),
            "when the next thing surfaces stays: {rendered}"
        );
        assert_eq!(rendered.matches("Clear.").count(), 1, "{rendered}");
        assert!(!rendered.contains("Not sure"), "{rendered}");
    }

    #[test]
    fn ages_read_short() {
        assert_eq!(age_label(30), "1m");
        assert_eq!(age_label(22 * 3600), "22h");
        assert_eq!(age_label(3 * 86_400), "3d");
    }
}
