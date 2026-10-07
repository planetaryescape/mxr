//! The To do lens: things email asked you to do, as a runway ordered by
//! when to act, never a list of subject lines.
//!
//! Renders `Request::GetTodoRunway` in four bands (Now, Coming up by week,
//! Whenever, Done this week) with each row as an instruction: verb and
//! object, who, how much, a runway bar and "act by Wed 7 · due Fri 9".
//! The footer prints where Enter goes before it is pressed. The header
//! line and the empty states come from the daemon's mode guide; hints show
//! in the status line (`app/hints.rs`). Pure render; wiring lives in
//! `app/todo_actions.rs`.

use mxr_protocol::{TodoChangeData, TodoData};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::app::{ActivePane, TodoPageState, TodoPanel, TodoPromptKind, TodoPromptState};
use crate::ui::sanitize::{one_line, truncate};

pub struct TodoView<'a> {
    pub page: &'a TodoPageState,
    pub selected_index: usize,
    pub active_pane: &'a ActivePane,
}

/// Cells in the runway bar.
const BAR_CELLS: usize = 10;

/// The runway bar: it fills from the day the row showed up to the day it's
/// due. One colour, the accent; past the deadline the row says "was due"
/// instead of drawing a bar.
pub(crate) fn runway_bar(todo: &TodoData) -> Option<String> {
    if todo.overdue {
        return None;
    }
    let fill = todo.runway?;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a clamped fraction of ten cells"
    )]
    let filled = (fill.clamp(0.0, 1.0) * BAR_CELLS as f64).round() as usize;
    Some(format!(
        "{}{}",
        "\u{2593}".repeat(filled),
        "\u{2591}".repeat(BAR_CELLS - filled)
    ))
}

/// Who the row is for: "you promised Priya" on a promise, else the payee.
fn who(todo: &TodoData) -> String {
    one_line(
        todo.person_label
            .as_deref()
            .or(todo.counterparty.as_deref())
            .unwrap_or(""),
    )
}

/// The footer's first line: where the row's link goes (Enter opens the
/// email, never the link), then why the row is here.
pub(crate) fn trust_line(todo: &TodoData) -> String {
    let mut parts = Vec::new();
    if let Some(domain) = todo
        .action
        .as_ref()
        .and_then(|action| action.domain.as_ref())
    {
        parts.push(format!("link goes to {}", one_line(domain)));
    }
    parts.push(one_line(&todo.why));
    if let Some(looks_done) = &todo.looks_done {
        parts.push(format!("Looks done: {}", one_line(&looks_done.reason)));
    }
    parts.join(" \u{b7} ")
}

/// What Enter does on this row, named for its action and destination.
pub(crate) fn primary_label(todo: &TodoData) -> String {
    match &todo.action {
        Some(action) => one_line(&action.label),
        None => "open email".to_string(),
    }
}

fn keys_line(page: &TodoPageState, selected: Option<&TodoData>) -> String {
    match page.panel {
        TodoPanel::Runway => {
            let enter = selected.map_or_else(|| "do it".to_string(), primary_label);
            // The mode's own key table, so the footer says what web and CLI say.
            let rest: Vec<String> = match &page.guide {
                Some(guide) => guide
                    .keys
                    .iter()
                    .filter(|key| !matches!(key.key.as_str(), "Enter" | "t" | "u" | "?"))
                    .map(|key| format!("{} {}", key.key, key.verb))
                    .collect(),
                None => mxr_protocol::TODO_GUIDE
                    .keys
                    .iter()
                    .filter(|(key, _)| !matches!(*key, "Enter" | "t" | "u" | "?"))
                    .map(|(key, verb)| format!("{key} {verb}"))
                    .collect(),
            };
            let rest = rest.join("  ");
            format!("\u{21b5} {enter}  {rest}  ? keys")
        }
        TodoPanel::Expired => "\u{21b5} restore  o email  Esc back to the runway".to_string(),
        TodoPanel::Catchup => {
            "\u{21b5} keep  e let go  A let go of all  o email  Esc back".to_string()
        }
    }
}

/// Greedy word wrap for the cards and catch-up copy, which are plain text.
/// Now's lens uses it too.
pub(crate) fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(16);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn band(title: &str, theme: &crate::theme::Theme) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {title}"),
        Style::default()
            .fg(theme.text_muted)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Column widths for a row, chosen from the lens width alone so bars and
/// dates line up down a band. The title and the whole date come first;
/// the bar, the amount and then who give way as the lens narrows.
struct RowColumns {
    title: usize,
    who: usize,
    amount: usize,
    bar: bool,
    when: usize,
}

impl RowColumns {
    /// Long enough for "shows up Mon 19 Oct · act by Mon 26 Oct".
    const WHEN: usize = 40;
    const TITLE_MIN: usize = 14;
    /// The title grows to this before who and the amount get room.
    const TITLE_PREFERRED: usize = 28;
    const TITLE_MAX: usize = 48;
    const WHO_MAX: usize = 22;
    const AMOUNT: usize = 9;

    fn for_width(width: usize) -> Self {
        // The marker, and the gap after the title.
        let avail = width.saturating_sub(2 + 2);
        let when = Self::WHEN.min(avail.saturating_sub(Self::TITLE_MIN));
        let mut left = avail.saturating_sub(when + Self::TITLE_MIN);
        let bar = left > BAR_CELLS;
        if bar {
            left -= BAR_CELLS + 1;
        }
        let grown = left.min(Self::TITLE_PREFERRED - Self::TITLE_MIN);
        left -= grown;
        let amount = if left >= Self::AMOUNT + 2 {
            left -= Self::AMOUNT + 2;
            Self::AMOUNT
        } else {
            0
        };
        let who = if left >= 9 {
            let who = Self::WHO_MAX.min(left - 1);
            left -= who + 1;
            who
        } else {
            0
        };
        let title = (Self::TITLE_MIN + grown + left).min(Self::TITLE_MAX);
        Self {
            title,
            who,
            amount,
            bar,
            when,
        }
    }
}

/// One row as a sentence: title, who, amount, bar and dates. `dim` for
/// Coming up and Whenever, which aren't asking for anything yet.
fn row_line(
    todo: &TodoData,
    selected: bool,
    dim: bool,
    width: usize,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    let marker = if selected { "\u{258c} " } else { "  " };
    let who = who(todo);
    let amount = todo
        .amount
        .as_ref()
        .map(|amount| one_line(&amount.display))
        .unwrap_or_default();
    let bar = runway_bar(todo);
    let when = one_line(&todo.when_label);
    let cols = RowColumns::for_width(width);
    let when = truncate(&when, cols.when);
    let text = if dim {
        theme.text_secondary
    } else {
        theme.text_primary
    };
    let title_width = cols.title;
    let mut spans = vec![
        Span::raw(marker),
        Span::styled(
            format!(
                "{:<title_width$}  ",
                truncate(&one_line(&todo.title), title_width)
            ),
            Style::default().fg(text),
        ),
    ];
    if cols.who > 0 {
        let who_width = cols.who;
        spans.push(Span::styled(
            format!("{:<who_width$} ", truncate(&who, who_width)),
            Style::default().fg(theme.text_secondary),
        ));
    }
    if cols.amount > 0 {
        let amount_width = cols.amount;
        spans.push(Span::styled(
            format!("{amount:>amount_width$}  "),
            Style::default().fg(text),
        ));
    }
    if cols.bar {
        spans.push(Span::styled(
            format!("{:<width$} ", bar.unwrap_or_default(), width = BAR_CELLS),
            Style::default().fg(if dim { theme.accent_dim } else { theme.accent }),
        ));
    }
    // Overdue is a fact in the row's own words, never red.
    spans.push(Span::styled(when, Style::default().fg(theme.text_muted)));
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
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            selected_line: 0,
        }
    }

    fn push(&mut self, line: Line<'static>) {
        self.lines.push(line);
    }

    fn text(&mut self, text: impl Into<String>, style: Style) {
        self.lines.push(Line::from(Span::styled(
            format!("  {}", text.into()),
            style,
        )));
    }

    fn rows(
        &mut self,
        todos: &[TodoData],
        next_index: &mut usize,
        view: &TodoView<'_>,
        dim: bool,
        width: usize,
        theme: &crate::theme::Theme,
    ) {
        for todo in todos {
            let selected = *next_index == view.selected_index;
            if selected {
                self.selected_line = self.lines.len();
            }
            self.push(row_line(todo, selected, dim, width, theme));
            *next_index += 1;
        }
    }
}

fn runway_body(view: &TodoView<'_>, width: usize, theme: &crate::theme::Theme) -> Body {
    let mut body = Body::new();
    let page = view.page;
    let muted = Style::default().fg(theme.text_muted);
    let secondary = Style::default().fg(theme.text_secondary);
    let Some(runway) = &page.runway else {
        body.text("Loading To do…", muted);
        return body;
    };
    if !runway.headline.is_empty() {
        body.push(Line::from(Span::styled(
            format!("  {}", one_line(&runway.headline)),
            Style::default()
                .fg(theme.text_primary)
                .add_modifier(Modifier::BOLD),
        )));
    }
    if runway.catchup_count > 0 {
        body.push(Line::from(""));
        let things = if runway.catchup_count == 1 {
            "thing"
        } else {
            "things"
        };
        body.text(
            format!(
                "Catch up: {} {things} from before mxr sorted your mail might still need you \u{b7} C go through them",
                runway.catchup_count
            ),
            Style::default().fg(theme.accent),
        );
    }
    // The daemon sets the empty state whenever Now is empty: what lands
    // here, or when the next thing shows up.
    if runway.now.is_empty() && runway.empty_state.is_some() {
        body.push(Line::from(""));
        let empty = runway
            .empty_state
            .as_deref()
            .map(one_line)
            .unwrap_or_default();
        for line in wrap(&empty, width.saturating_sub(4)) {
            body.text(line, secondary);
        }
        // Never had any: say how to add one by hand.
        if let Some(guide) = page
            .guide
            .as_ref()
            .filter(|guide| empty == guide.never_had_any)
        {
            body.text(guide.add_one.replace('`', ""), muted);
        }
    }

    let mut index = 0usize;
    if !runway.now.is_empty() {
        body.push(Line::from(""));
        body.push(band("NOW", theme));
        body.rows(&runway.now, &mut index, view, false, width, theme);
    }
    if !runway.coming_up.is_empty() {
        body.push(Line::from(""));
        body.push(band("COMING UP", theme));
        for week in &runway.coming_up {
            body.text(one_line(&week.label), muted);
            body.rows(&week.todos, &mut index, view, true, width, theme);
        }
    }
    if !runway.later.is_empty() {
        body.push(Line::from(""));
        body.push(band("LATER", theme));
        body.rows(&runway.later, &mut index, view, true, width, theme);
    }
    if !runway.whenever.is_empty() {
        body.push(Line::from(""));
        body.push(band(
            &format!("WHENEVER ({})", runway.whenever.len()),
            theme,
        ));
        body.rows(&runway.whenever, &mut index, view, true, width, theme);
    }
    if !runway.done_this_week.is_empty() {
        body.push(Line::from(""));
        body.push(band(
            &format!("DONE THIS WEEK ({})", runway.done_this_week.len()),
            theme,
        ));
    }
    // Counted when the lens opened; later refreshes count from then.
    let expired = page.expired_on_open.max(runway.expired_since_last_looked);
    if expired > 0 {
        body.push(Line::from(""));
        body.text(
            format!("{expired} expired since you last looked \u{b7} E open the list"),
            muted,
        );
    }
    body
}

fn expired_body(view: &TodoView<'_>, width: usize, theme: &crate::theme::Theme) -> Body {
    let mut body = Body::new();
    body.push(band("EXPIRED", theme));
    body.text(
        "Past their window or let go. Enter puts one back on the runway.",
        Style::default().fg(theme.text_secondary),
    );
    body.push(Line::from(""));
    if view.page.expired.is_empty() {
        body.text(
            "Nothing has expired.",
            Style::default().fg(theme.text_muted),
        );
    }
    let mut index = 0usize;
    body.rows(&view.page.expired, &mut index, view, false, width, theme);
    body
}

fn catchup_body(view: &TodoView<'_>, width: usize, theme: &crate::theme::Theme) -> Body {
    let mut body = Body::new();
    let secondary = Style::default().fg(theme.text_secondary);
    let Some(catchup) = &view.page.catchup else {
        body.text(
            "Loading the catch-up…",
            Style::default().fg(theme.text_muted),
        );
        return body;
    };
    body.push(Line::from(Span::styled(
        format!("  {}", one_line(&catchup.title)),
        Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD),
    )));
    if !catchup.todos.is_empty() {
        for line in wrap(&catchup.why, width.saturating_sub(4)) {
            body.text(line, secondary);
        }
    }
    if let Some(line) = &catchup.already_over_line {
        body.text(one_line(line), Style::default().fg(theme.text_muted));
    }
    body.push(Line::from(""));
    let mut index = 0usize;
    body.rows(&catchup.todos, &mut index, view, false, width, theme);
    if catchup.overflow_count > 0 {
        body.push(Line::from(""));
        body.text(
            format!(
                "{} more didn't fit and are in the Expired list.",
                catchup.overflow_count
            ),
            Style::default().fg(theme.text_muted),
        );
    }
    body
}

pub fn draw(frame: &mut Frame, area: Rect, view: &TodoView<'_>, theme: &crate::theme::Theme) {
    let is_focused = *view.active_pane == ActivePane::MailList;
    let header = view.page.guide.as_ref().map_or(
        "Things email asked you to do, ordered by when to act.",
        |g| g.header.as_str(),
    );
    let block = Block::bordered()
        .title(format!(" To do \u{2500} {} ", one_line(header)))
        .border_type(BorderType::Rounded)
        .border_style(theme.border_style(is_focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let body = match view.page.panel {
        TodoPanel::Runway => runway_body(view, width, theme),
        TodoPanel::Expired => expired_body(view, width, theme),
        TodoPanel::Catchup => catchup_body(view, width, theme),
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
    let trust = selected.map(trust_line).unwrap_or_default();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {}", truncate(&trust, width.saturating_sub(2))),
                Style::default().fg(theme.text_secondary),
            )),
            Line::from(Span::styled(
                format!(" {}", keys_line(view.page, selected)),
                Style::default().fg(theme.text_muted),
            )),
        ]),
        footer_area,
    );

    draw_catchup_preview(frame, area, view.page.catchup_preview.as_ref(), theme);
}

/// "Let go of all" as the daemon previewed it: Enter lets go of exactly
/// these rows.
fn draw_catchup_preview(
    frame: &mut Frame,
    area: Rect,
    preview: Option<&TodoChangeData>,
    theme: &crate::theme::Theme,
) {
    let Some(preview) = preview else {
        return;
    };
    let popup = super::centered_rect(60, 50, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(" Let go of all ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let width = inner.width as usize;
    let mut lines = vec![Line::from(Span::styled(
        one_line(&preview.summary),
        Style::default().fg(theme.text_primary),
    ))];
    lines.push(Line::from(""));
    for todo in &preview.changed {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                truncate(&one_line(&todo.title), width.saturating_sub(4))
            ),
            Style::default().fg(theme.text_secondary),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "They go to the Expired list. Enter let go of these  u undo after  Esc keep them",
        Style::default().fg(theme.text_muted),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// The `Z`, `,` and `t` prompt. Drawn over any screen: `t` works from
/// the mail list and the reader too.
pub fn draw_prompt(
    frame: &mut Frame,
    area: Rect,
    prompt: Option<&TodoPromptState>,
    theme: &crate::theme::Theme,
) {
    let Some(prompt) = prompt else {
        return;
    };
    let (title, lead, hint) = match prompt.kind {
        TodoPromptKind::Schedule { .. } => (
            " Schedule ",
            "When should it show up? The due date stays as it is.",
            "Type a time: mon 9am \u{b7} in 3d \u{b7} fri",
        ),
        TodoPromptKind::Edit { .. } => (
            " Edit ",
            "Correct a field as field=value. The row is yours from then on.",
            "title=Pay rent \u{b7} due=fri 9 oct \u{b7} amount=142.00 \u{b7} counterparty=Camden",
        ),
        TodoPromptKind::Create { .. } => (
            " Make a to-do ",
            "What to do, starting with the verb. It lands in To do (g x).",
            "Reply to Priya \u{b7} Send the signed form \u{b7} Pay the invoice",
        ),
    };
    let popup = super::centered_rect(56, 40, area);
    frame.render_widget(Clear, popup);
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .style(Style::default().bg(theme.modal_bg));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(2),
            Constraint::Length(1),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(lead)
            .style(Style::default().fg(theme.text_secondary))
            .wrap(Wrap { trim: true }),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(format!(" \u{203a} {}_", prompt.input))
            .style(Style::default().fg(theme.text_primary)),
        chunks[1],
    );
    let (lines, style) = if let Some(error) = &prompt.error {
        (vec![error.clone()], Style::default().fg(theme.error))
    } else if matches!(prompt.kind, TodoPromptKind::Schedule { .. })
        && !prompt.input.trim().is_empty()
    {
        (prompt.time.lines(), Style::default().fg(theme.text_primary))
    } else {
        (
            vec![hint.to_string()],
            Style::default().fg(theme.text_muted),
        )
    };
    frame.render_widget(
        Paragraph::new(lines.into_iter().map(Line::from).collect::<Vec<_>>())
            .style(style)
            .wrap(Wrap { trim: false }),
        chunks[2],
    );
    frame.render_widget(
        Paragraph::new("Enter save  Esc cancel").style(Style::default().fg(theme.text_muted)),
        chunks[3],
    );
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::id::{AccountId, MessageId};
    use mxr_protocol::{
        TodoActionData, TodoActionKindData, TodoAmountData, TodoFirstRunData, TodoRunwayData,
        TodoStateData, TodoWeekData, TODO_GUIDE,
    };
    use mxr_test_support::render_to_string;

    pub(crate) fn todo(id: &str, title: &str) -> TodoData {
        let now = Utc::now();
        TodoData {
            id: id.into(),
            account_id: AccountId::new(),
            kind: "other".into(),
            verb: "do".into(),
            title: title.into(),
            counterparty: None,
            person_label: None,
            amount: None,
            due_at: None,
            due_words: None,
            act_by_at: None,
            surface_at: None,
            scheduled_for: None,
            relevant_until: None,
            state: TodoStateData::Open,
            catchup: None,
            origin: "rule".into(),
            why: "Here because: \"payment due 9 October\" (rule).".into(),
            next: None,
            when_label: "no date".into(),
            overdue: false,
            runway: None,
            action: None,
            looks_done: None,
            fields: Vec::new(),
            user_touched: false,
            thread_id: None,
            source_message_id: Some(MessageId::new()),
            source_date: None,
            surfaced_at: None,
            expired_at: None,
            done_at: None,
            dismissed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub(crate) fn council_tax() -> TodoData {
        let mut bill = todo("todo_1", "Pay council tax");
        bill.kind = "bill".into();
        bill.counterparty = Some("Camden Council".into());
        bill.amount = Some(TodoAmountData {
            minor: 14_200,
            currency: "GBP".into(),
            display: "\u{a3}142.00".into(),
        });
        bill.when_label = "act by Wed 7 Oct \u{b7} due Fri 9 Oct".into();
        bill.runway = Some(0.6);
        bill.action = Some(TodoActionData {
            kind: TodoActionKindData::OpenEmail,
            label: "Open email to pay".into(),
            message_id: bill.source_message_id.clone(),
            url: "https://www.camden.gov.uk/pay".into(),
            domain: Some("camden.gov.uk".into()),
            trusted: false,
        });
        bill
    }

    pub(crate) fn runway(now: Vec<TodoData>, coming: Vec<TodoData>) -> TodoRunwayData {
        TodoRunwayData {
            generated_at: Utc::now(),
            header: TODO_GUIDE.header.into(),
            headline: if now.is_empty() {
                String::new()
            } else {
                format!(
                    "{} things need you. Pay council tax first, act by Wed.",
                    now.len()
                )
            },
            empty_state: None,
            now,
            coming_up: if coming.is_empty() {
                Vec::new()
            } else {
                vec![TodoWeekData {
                    week_start: chrono::NaiveDate::from_ymd_opt(2026, 10, 12).unwrap(),
                    label: "wk of 12 Oct".into(),
                    todos: coming,
                }]
            },
            later: Vec::new(),
            whenever: Vec::new(),
            done_this_week: Vec::new(),
            catchup_count: 0,
            expired_since_last_looked: 0,
            next_surface: None,
            first_run: TodoFirstRunData {
                complete: true,
                scanned: 10,
                reached: None,
            },
        }
    }

    pub(crate) fn page(runway: TodoRunwayData, hints_seen: bool) -> TodoPageState {
        TodoPageState {
            runway: Some(runway),
            guide: Some(TODO_GUIDE.to_data(|_| hints_seen.then(Utc::now))),
            ..TodoPageState::default()
        }
    }

    fn render(page: &TodoPageState, selected_index: usize) -> String {
        render_to_string(120, 30, |frame| {
            draw(
                frame,
                Rect::new(0, 0, 120, 30),
                &TodoView {
                    page,
                    selected_index,
                    active_pane: &ActivePane::MailList,
                },
                &crate::theme::Theme::default(),
            );
        })
    }

    fn overdue_failed_payment() -> TodoData {
        let mut late = todo("todo_2", "Pay water bill");
        late.counterparty = Some("Thames Water".into());
        late.when_label = "was due Wed 30 Sep".into();
        late.overdue = true;
        late.runway = Some(1.0);
        late
    }

    fn renewal() -> TodoData {
        let mut renewal = todo("todo_3", "Renew car insurance");
        renewal.counterparty = Some("Admiral".into());
        renewal.when_label = "shows up Mon 19 Oct \u{b7} act by Mon 26 Oct".into();
        renewal
    }

    #[test]
    fn rows_read_as_instructions_on_a_runway_in_bands() {
        let mut runway = runway(
            vec![council_tax(), overdue_failed_payment()],
            vec![renewal()],
        );
        runway.whenever.push(todo("todo_4", "Send the signed form"));
        runway
            .done_this_week
            .push(todo("todo_5", "Verify your email"));
        runway.expired_since_last_looked = 3;
        let rendered = render(&page(runway, true), 0);
        insta::assert_snapshot!("todo_lens_runway", rendered);
        assert!(rendered.contains("To do \u{2500} Things email asked you to do"));
        assert!(rendered.contains("NOW"));
        assert!(rendered.contains("COMING UP"));
        assert!(rendered.contains("wk of 12 Oct"));
        assert!(rendered.contains("WHENEVER (1)"));
        assert!(rendered.contains("DONE THIS WEEK (1)"));
        assert!(rendered.contains("Pay council tax"));
        assert!(rendered.contains("Camden Council"));
        assert!(rendered.contains("\u{a3}142.00"));
        assert!(rendered.contains("\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2593}\u{2591}\u{2591}\u{2591}\u{2591} act by Wed 7 Oct \u{b7} due Fri 9 Oct"));
        assert!(rendered.contains("3 expired since you last looked \u{b7} E open the list"));
        // The footer says where Enter goes before it's pressed.
        assert!(rendered.contains("link goes to camden.gov.uk \u{b7} Here because"));
        assert!(rendered.contains("\u{21b5} Open email to pay  e tick off  Z schedule"));
        assert!(!rendered.contains('\u{2014}'), "no em dashes");
    }

    #[test]
    fn overdue_reads_was_due_with_no_bar_and_no_red() {
        let page = page(runway(vec![overdue_failed_payment()], vec![]), true);
        let rendered = render(&page, 0);
        let line = rendered
            .lines()
            .find(|line| line.contains("Pay water bill"))
            .unwrap();
        assert!(line.contains("was due Wed 30 Sep"), "{line}");
        assert!(
            !line.contains('\u{2593}'),
            "no bar past the deadline: {line}"
        );
        // Draw again and check no cell is painted in the theme's error red.
        let theme = crate::theme::Theme::default();
        let backend = ratatui::backend::TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                draw(
                    frame,
                    Rect::new(0, 0, 120, 30),
                    &TodoView {
                        page: &page,
                        selected_index: 0,
                        active_pane: &ActivePane::MailList,
                    },
                    &theme,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert!(buffer.content.iter().all(|cell| cell.fg != theme.error));
    }

    #[test]
    fn a_row_about_a_link_says_open_email_and_names_the_domain() {
        let rendered = render(&page(runway(vec![council_tax()], vec![]), true), 0);
        assert!(rendered.contains("link goes to camden.gov.uk \u{b7} Here because"));
        assert!(rendered.contains("\u{21b5} Open email to pay"));
    }

    #[test]
    fn no_card_teaches_at_the_top_even_before_any_hint_is_seen() {
        let unseen = render(&page(runway(vec![council_tax()], vec![]), false), 0);
        assert!(!unseen.contains("Each row is one thing to do"), "{unseen}");
        assert!(!unseen.contains("Esc close"), "{unseen}");
    }

    #[test]
    fn empty_states_teach_the_job_or_say_when_the_next_thing_shows_up() {
        let mut never = runway(vec![], vec![]);
        never.empty_state = Some(TODO_GUIDE.never_had_any.into());
        let rendered = render(&page(never, false), 0);
        assert!(
            rendered.contains("When an email asks you to pay"),
            "{rendered}"
        );
        assert!(rendered.contains("Press t on any email to add one yourself."));

        let mut clear = runway(vec![], vec![renewal()]);
        clear.empty_state =
            Some("Nothing needs you. Next: renew car insurance shows up Mon 19 Oct.".into());
        let rendered = render(&page(clear, true), 0);
        assert!(
            rendered.contains("Nothing needs you. Next: renew car insurance shows up Mon 19 Oct.")
        );
        assert!(!rendered.contains("Press t on any email"));
    }

    #[test]
    fn the_catch_up_line_and_panel_offer_keep_or_let_go() {
        let mut with_catchup = runway(vec![council_tax()], vec![]);
        with_catchup.catchup_count = 2;
        let rendered = render(&page(with_catchup.clone(), true), 0);
        assert!(rendered.contains("Catch up: 2 things from before mxr sorted your mail"));

        let mut catchup_page = page(with_catchup, true);
        catchup_page.panel = TodoPanel::Catchup;
        catchup_page.catchup = Some(mxr_protocol::TodoCatchupData {
            title: "Catch up: 2 things from the last two weeks might still need you.".into(),
            why: mxr_protocol::todo_copy::CATCH_UP_WHY.into(),
            window_days: 14,
            todos: vec![
                todo("todo_6", "Sign Tenancy renewal"),
                overdue_failed_payment(),
            ],
            overflow_count: 0,
            already_over: Vec::new(),
            already_over_line: Some("Already over, so not shown: 1 past invite.".into()),
            first_run: TodoFirstRunData {
                complete: true,
                scanned: 10,
                reached: None,
            },
        });
        let rendered = render(&catchup_page, 0);
        insta::assert_snapshot!("todo_lens_catchup", rendered);
        assert!(rendered.contains("Catch up: 2 things from the last two weeks"));
        assert!(rendered.contains("Already over, so not shown: 1 past invite."));
        assert!(rendered.contains("\u{21b5} keep  e let go  A let go of all"));

        catchup_page.catchup_preview = Some(TodoChangeData {
            dry_run: true,
            action: "let_go".into(),
            changed: catchup_page.catchup.as_ref().unwrap().todos.clone(),
            unchanged: Vec::new(),
            summary: "Would let go of 2.".into(),
        });
        let rendered = render(&catchup_page, 0);
        assert!(rendered.contains("Would let go of 2."));
        assert!(rendered.contains("Enter let go of these"));
    }

    #[test]
    fn mail_text_cannot_reach_the_terminal_as_control_sequences() {
        let mut hostile = council_tax();
        hostile.title = "Pay\u{1b}[2J now".into();
        hostile.why = "Here because\u{1b}]0;pwned\u{7}".into();
        let rendered = render(&page(runway(vec![hostile], vec![]), true), 0);
        assert!(
            !rendered
                .chars()
                .any(|c| matches!(c as u32, 0x00..=0x09 | 0x0B..=0x1F | 0x7F..=0x9F)),
            "{rendered:?}"
        );
    }

    fn render_at(page: &TodoPageState, width: u16) -> String {
        render_to_string(width, 30, |frame| {
            draw(
                frame,
                Rect::new(0, 0, width, 30),
                &TodoView {
                    page,
                    selected_index: 0,
                    active_pane: &ActivePane::MailList,
                },
                &crate::theme::Theme::default(),
            );
        })
    }

    #[test]
    fn narrow_lenses_keep_the_title_and_the_whole_date() {
        let mut coming = renewal();
        coming.when_label = "shows up Mon 19 Oct \u{b7} act by Mon 26 Oct".into();
        let page = page(runway(vec![council_tax()], vec![coming]), true);
        for width in [60u16, 80] {
            let rendered = render_at(&page, width);
            for (title, when) in [
                ("Pay council", "act by Wed 7 Oct \u{b7} due Fri 9 Oct"),
                ("Renew car", "shows up Mon 19 Oct \u{b7} act by Mon 26 Oct"),
            ] {
                // The row, not the headline that also names it.
                let line = rendered
                    .lines()
                    .find(|line| line.contains(title) && !line.contains("need you"))
                    .unwrap_or_else(|| panic!("{width}: no row for {title}\n{rendered}"));
                assert!(line.contains(when), "{width}: the date is cut: {line}");
            }
        }
        // At 80 columns the bar still fits; at 60 it gives way to the date.
        let wide = render_at(&page, 80);
        let bill = wide
            .lines()
            .find(|line| line.contains("Pay council") && !line.contains("need you"))
            .unwrap();
        assert!(bill.contains('\u{2593}'), "{bill}");
    }
}
