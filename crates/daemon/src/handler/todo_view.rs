//! To do rows as clients draw them: the runway's bands, each row's labels,
//! its why line, its one button and every field's provenance. Built here
//! so the CLI, TUI and web show the same words.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use mxr_protocol::{
    todo_copy, TodoActionData, TodoAmountData, TodoData, TodoFieldData, TodoGateData,
    TodoLooksDoneData, TodoNextData, TodoStateData, TodoWeekData,
};
use mxr_store::{TodoRecord, TodoState};
use mxr_todo::action_link::{button_label, Gate};
use mxr_todo::money::format_amount;
use mxr_todo::provenance::FieldSources;
use mxr_todo::TodoKind;

/// How far ahead Coming up looks; dated rows past it are Later.
pub(super) const COMING_UP_DAYS: i64 = 30;

/// "Wed 7 Oct", in the daemon's zone.
pub(super) fn day_label<Tz: TimeZone>(at: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at.with_timezone(tz).format("%a %-d %b").to_string()
}

fn time_label<Tz: TimeZone>(at: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    at.with_timezone(tz).format("%a %-d %b %H:%M").to_string()
}

pub(super) fn state_data(state: TodoState) -> TodoStateData {
    match state {
        TodoState::Open => TodoStateData::Open,
        TodoState::Done => TodoStateData::Done,
        TodoState::Dismissed => TodoStateData::Dismissed,
        TodoState::Expired => TodoStateData::Expired,
    }
}

/// When the row shows up: the user's own date wins.
pub(super) fn effective_surface(record: &TodoRecord) -> Option<DateTime<Utc>> {
    record.scheduled_for.or(record.surface_at)
}

pub(super) fn to_data(record: &TodoRecord, now: DateTime<Utc>) -> TodoData {
    to_data_in(record, now, &Local)
}

pub(super) fn to_data_in<Tz: TimeZone>(record: &TodoRecord, now: DateTime<Utc>, tz: &Tz) -> TodoData
where
    Tz::Offset: std::fmt::Display,
{
    let kind = TodoKind::parse(&record.kind).unwrap_or(TodoKind::Other);
    let fields = FieldSources::from_json(&record.field_sources);
    let overdue = record.due_at.is_some_and(|due| due < now);
    TodoData {
        id: record.id.clone(),
        account_id: record.account_id.clone(),
        kind: record.kind.clone(),
        verb: record.verb.clone(),
        title: record.title.clone(),
        counterparty: record.counterparty.clone(),
        person_label: (kind == TodoKind::Promise)
            .then(|| {
                record
                    .counterparty
                    .as_ref()
                    .map(|person| format!("you promised {person}"))
            })
            .flatten(),
        amount: record
            .amount_minor
            .zip(record.currency.as_ref())
            .map(|(minor, currency)| TodoAmountData {
                minor,
                currency: currency.clone(),
                display: format_amount(minor, currency),
            }),
        due_at: record.due_at,
        due_words: record.due_words.clone(),
        act_by_at: record.act_by_at,
        surface_at: record.surface_at,
        scheduled_for: record.scheduled_for,
        relevant_until: record.relevant_until,
        state: state_data(record.state),
        catchup: record.catchup.map(|catchup| catchup.as_str().to_string()),
        origin: record.origin.clone(),
        why: why_line(record),
        next: next_line(kind, record),
        when_label: when_label(record, kind, now, tz),
        overdue,
        runway: runway(record, now),
        action: action(record, kind),
        looks_done: record
            .looks_done_reason
            .as_ref()
            .map(|reason| TodoLooksDoneData {
                message_id: record.looks_done_message_id.clone(),
                reason: reason.clone(),
            }),
        fields: fields
            .0
            .iter()
            .map(|(field, provenance)| TodoFieldData {
                field: field.clone(),
                source: provenance.source.as_str().to_string(),
                checked: provenance.checked,
                evidence: provenance.evidence.clone(),
                source_label: provenance.source.describe().to_string(),
            })
            .collect(),
        user_touched: record.user_touched(),
        thread_id: record.thread_id.clone(),
        source_message_id: record.source_message_id.clone(),
        source_date: record.source_date,
        surfaced_at: record.surfaced_at,
        expired_at: record.expired_at,
        done_at: record.done_at,
        dismissed_at: record.dismissed_at,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn why_line(record: &TodoRecord) -> String {
    format!(
        "Here because: {}.",
        record.reason.trim().trim_end_matches('.')
    )
}

/// What happens next, only where it's true of this build.
fn next_line(kind: TodoKind, record: &TodoRecord) -> Option<String> {
    if record.state != TodoState::Open {
        return None;
    }
    match kind {
        TodoKind::Bill | TodoKind::PaymentFailed => {
            Some("When the receipt arrives, the row says it looks done.".to_string())
        }
        TodoKind::Verify => Some("Lets go by itself once the link expires.".to_string()),
        TodoKind::Rsvp if record.origin == "ics" => {
            Some("Ticks itself off when you answer the invite.".to_string())
        }
        TodoKind::Renewal | TodoKind::Sign => {
            Some("When the confirmation arrives, the row says it looks done.".to_string())
        }
        _ => None,
    }
}

fn when_label<Tz: TimeZone>(
    record: &TodoRecord,
    kind: TodoKind,
    now: DateTime<Utc>,
    tz: &Tz,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    if let Some(done) = record.done_at.filter(|_| record.state == TodoState::Done) {
        return format!("done {}", day_label(done, tz));
    }
    let due = record.due_at;
    let act_by = record.act_by_at;
    if let Some(surface) = effective_surface(record).filter(|surface| *surface > now) {
        let shows = format!("shows up {}", day_label(surface, tz));
        return match act_by.or(due) {
            Some(act_by) => format!("{shows} · act by {}", day_label(act_by, tz)),
            None => shows,
        };
    }
    let same_day = |a: DateTime<Utc>, b: DateTime<Utc>| {
        a.with_timezone(tz).date_naive() == b.with_timezone(tz).date_naive()
    };
    match (kind, act_by, due) {
        (TodoKind::Verify, _, Some(expiry)) => format!("link expires {}", time_label(expiry, tz)),
        (_, _, Some(due)) if due < now => format!("was due {}", day_label(due, tz)),
        (_, Some(act_by), Some(due)) if !same_day(act_by, due) => {
            format!(
                "act by {} · due {}",
                day_label(act_by, tz),
                day_label(due, tz)
            )
        }
        (_, _, Some(due)) => format!("act by {}", day_label(due, tz)),
        (_, Some(_), None) => "act now".to_string(),
        (_, None, None) => "no date".to_string(),
    }
}

/// The bar fills from the day it showed up to the deadline.
fn runway(record: &TodoRecord, now: DateTime<Utc>) -> Option<f64> {
    let start = effective_surface(record)?;
    let end = record.due_at?;
    let span = (end - start).num_seconds();
    if span <= 0 {
        return Some(1.0);
    }
    #[expect(clippy::cast_precision_loss, reason = "a progress fraction")]
    let fill = (now - start).num_seconds() as f64 / span as f64;
    Some(fill.clamp(0.0, 1.0))
}

fn action(record: &TodoRecord, kind: TodoKind) -> Option<TodoActionData> {
    let url = record.action_url.clone()?;
    let gate = record
        .action_gate
        .as_deref()
        .and_then(|gate| serde_json::from_str::<Gate>(gate).ok());
    let trusted = record.action_trusted;
    Some(TodoActionData {
        label: button_label(kind, record.action_domain.as_deref(), trusted),
        url,
        domain: record.action_domain.clone(),
        trusted,
        gate: gate.map(|gate| TodoGateData {
            dmarc_pass: gate.dmarc_pass,
            domain_match: gate.domain_match,
            prior_mail: gate.prior_mail,
        }),
        untrusted_reason: (!trusted).then(|| {
            gate.and_then(Gate::failure)
                .unwrap_or("the sender couldn't be checked")
                .to_string()
        }),
    })
}

/// Now's order: by act-by, promises first on the same day, and rows past
/// their deadline after the ones still in time.
pub(super) fn now_order(a: &TodoRecord, b: &TodoRecord, now: DateTime<Utc>) -> std::cmp::Ordering {
    let late = |record: &TodoRecord| record.due_at.is_some_and(|due| due < now);
    let promise_last = |record: &TodoRecord| u8::from(record.kind != "promise");
    late(a)
        .cmp(&late(b))
        .then_with(|| match (a.act_by_at, b.act_by_at) {
            (Some(x), Some(y)) => x.date_naive().cmp(&y.date_naive()),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
        .then_with(|| promise_last(a).cmp(&promise_last(b)))
        .then_with(|| a.act_by_at.cmp(&b.act_by_at))
        .then_with(|| a.id.cmp(&b.id))
}

/// The open and recently done rows sorted into bands.
#[derive(Default)]
pub(super) struct Bands {
    pub now: Vec<TodoData>,
    pub coming_up: Vec<TodoWeekData>,
    pub later: Vec<TodoData>,
    pub whenever: Vec<TodoData>,
    pub done_this_week: Vec<TodoData>,
    pub next_surface: Option<TodoNextData>,
    /// The first row in Now, for the headline.
    pub first_now: Option<TodoRecord>,
}

pub(super) fn bands<Tz: TimeZone>(records: Vec<TodoRecord>, now: DateTime<Utc>, tz: &Tz) -> Bands
where
    Tz::Offset: std::fmt::Display,
{
    let week_start = start_of_week(now, tz);
    let horizon = now + Duration::days(COMING_UP_DAYS);
    let mut now_rows = Vec::new();
    let mut coming = Vec::new();
    let mut later = Vec::new();
    let mut whenever = Vec::new();
    let mut done = Vec::new();
    for record in records {
        match record.state {
            TodoState::Done => {
                if record.done_at.is_some_and(|at| at >= week_start) {
                    done.push(record);
                }
            }
            TodoState::Open if record.catchup == Some(mxr_store::TodoCatchup::Pending) => {}
            TodoState::Open => match effective_surface(&record) {
                None => whenever.push(record),
                Some(at) if at <= now => now_rows.push(record),
                Some(at) if at <= horizon => coming.push(record),
                Some(_) => later.push(record),
            },
            TodoState::Dismissed | TodoState::Expired => {}
        }
    }
    now_rows.sort_by(|a, b| now_order(a, b, now));
    coming.sort_by_key(|record| (effective_surface(record), record.id.clone()));
    later.sort_by_key(|record| (effective_surface(record), record.id.clone()));
    whenever.sort_by(|a, b| {
        b.source_date
            .cmp(&a.source_date)
            .then_with(|| a.id.cmp(&b.id))
    });
    done.sort_by_key(|record| std::cmp::Reverse(record.done_at));

    let next_surface = coming.first().or_else(|| later.first()).and_then(|record| {
        let at = effective_surface(record)?;
        Some(TodoNextData {
            todo_id: record.id.clone(),
            title: record.title.clone(),
            at,
            label: day_label(at, tz),
        })
    });
    let mut weeks: Vec<TodoWeekData> = Vec::new();
    for record in &coming {
        let Some(at) = effective_surface(record) else {
            continue;
        };
        let monday = monday_of(at.with_timezone(tz).date_naive());
        let data = to_data_in(record, now, tz);
        match weeks.last_mut() {
            Some(week) if week.week_start == monday => week.todos.push(data),
            _ => weeks.push(TodoWeekData {
                week_start: monday,
                label: format!("wk of {}", monday.format("%-d %b")),
                todos: vec![data],
            }),
        }
    }
    Bands {
        first_now: now_rows.first().cloned(),
        now: now_rows
            .iter()
            .map(|record| to_data_in(record, now, tz))
            .collect(),
        coming_up: weeks,
        later: later
            .iter()
            .map(|record| to_data_in(record, now, tz))
            .collect(),
        whenever: whenever
            .iter()
            .map(|record| to_data_in(record, now, tz))
            .collect(),
        done_this_week: done
            .iter()
            .map(|record| to_data_in(record, now, tz))
            .collect(),
        next_surface,
    }
}

fn monday_of(day: NaiveDate) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().num_days_from_monday()))
}

/// Monday 00:00 of this week in `tz`.
pub(super) fn start_of_week<Tz: TimeZone>(now: DateTime<Utc>, tz: &Tz) -> DateTime<Utc> {
    let monday = monday_of(now.with_timezone(tz).date_naive());
    mxr_todo::timing::start_of_day(monday, tz, 0)
}

/// "3 things need you. Pay council tax first, act by Wed." The act-by part
/// only when it's still ahead: a failed payment's is already now.
pub(super) fn headline<Tz: TimeZone>(
    count: usize,
    first: Option<&TodoRecord>,
    now: DateTime<Utc>,
    tz: &Tz,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let Some(first) = first else {
        return String::new();
    };
    let act_by = first
        .act_by_at
        .filter(|at| *at > now)
        .map(|at| at.with_timezone(tz).format(", act by %a").to_string())
        .unwrap_or_default();
    if count == 1 {
        format!("1 thing needs you. {}{act_by}.", first.title)
    } else {
        format!("{count} things need you. {} first{act_by}.", first.title)
    }
}

/// The empty state when Now has nothing: what lands here, or when the next
/// thing does.
pub(super) fn empty_state(never_had_any: bool, next: Option<&TodoNextData>) -> String {
    if never_had_any {
        return todo_copy::NEVER_HAD_ANY.to_string();
    }
    match next {
        Some(next) => format!(
            "{} Next: {} shows up {}.",
            todo_copy::CLEAR_FOR_NOW,
            mxr_todo::text::lower_first(&next.title),
            next.label
        ),
        None => todo_copy::CLEAR_FOR_NOW.to_string(),
    }
}

/// "1 past invite", "50 past invites": a kind the first run found already
/// over, for the catch-up's line.
pub(super) fn already_over_label(kind: &str, count: i64) -> String {
    let (one, many) = match kind {
        "bill" => ("old bill", "old bills"),
        "payment_failed" => ("old payment problem", "old payment problems"),
        "renewal" => ("past renewal", "past renewals"),
        "document" => ("past document renewal", "past document renewals"),
        "rsvp" => ("past invite", "past invites"),
        "verify" => ("expired link", "expired links"),
        "sign" => ("old signing request", "old signing requests"),
        "promise" => ("older promise", "older promises"),
        _ => ("past to-do", "past to-dos"),
    };
    if count == 1 {
        one.to_string()
    } else {
        many.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::London;
    use mxr_core::id::AccountId;

    fn at(m: u32, d: u32, h: u32) -> DateTime<Utc> {
        London
            .with_ymd_and_hms(2026, m, d, h, 0, 0)
            .single()
            .expect("valid time")
            .with_timezone(&Utc)
    }

    fn record(id: &str, kind: &str) -> TodoRecord {
        TodoRecord {
            id: id.to_string(),
            account_id: AccountId::from_provider_id("fake", "me@example.com"),
            thread_id: None,
            source_message_id: None,
            source_date: Some(at(9, 28, 8)),
            kind: kind.to_string(),
            verb: "pay".to_string(),
            doc_type: None,
            title: "Pay council tax".to_string(),
            counterparty: Some("Camden Council".to_string()),
            sender_domain: Some("camden.gov.uk".to_string()),
            amount_minor: Some(14200),
            currency: Some("GBP".to_string()),
            due_at: Some(at(10, 9, 23)),
            due_words: Some("payment due 9 October".to_string()),
            act_by_at: Some(at(10, 7, 23)),
            surface_at: Some(at(10, 4, 9)),
            scheduled_for: None,
            action_url: Some("https://www.camden.gov.uk/pay".to_string()),
            action_domain: Some("camden.gov.uk".to_string()),
            action_trusted: true,
            action_gate: Some(
                r#"{"dmarc_pass":true,"domain_match":true,"prior_mail":true}"#.to_string(),
            ),
            relevant_until: Some(at(10, 23, 23)),
            window_source: Some("rule".to_string()),
            state: TodoState::Open,
            expired_at: None,
            expired_at_birth: false,
            catchup: None,
            looks_done_message_id: None,
            looks_done_reason: None,
            origin: "rule".to_string(),
            reason: "\"payment due 9 October\" (rule)".to_string(),
            field_sources: r#"{"due_at":{"source":"rule","evidence":"payment due 9 October"}}"#
                .to_string(),
            user_edited: false,
            commitment_id: None,
            rules_version: 1,
            dedup_key: id.to_string(),
            surfaced_at: None,
            created_at: at(9, 28, 8),
            updated_at: at(9, 28, 8),
            done_at: None,
            dismissed_at: None,
        }
    }

    #[test]
    fn a_surfaced_bill_reads_as_an_instruction_with_its_dates_and_button() {
        let data = to_data_in(&record("a", "bill"), at(10, 5, 10), &London);
        assert_eq!(data.when_label, "act by Wed 7 Oct · due Fri 9 Oct");
        assert_eq!(
            data.amount.as_ref().map(|a| a.display.as_str()),
            Some("£142.00")
        );
        assert_eq!(data.why, "Here because: \"payment due 9 October\" (rule).");
        let action = data.action.expect("action");
        assert_eq!(action.label, "Pay on camden.gov.uk");
        assert!(action.untrusted_reason.is_none());
        assert_eq!(data.fields[0].source_label, "a pattern in the email");
    }

    #[test]
    fn past_the_deadline_reads_was_due_and_sorts_after_rows_in_time() {
        let mut late = record("late", "bill");
        late.due_at = Some(at(10, 2, 23));
        late.act_by_at = late.due_at;
        let data = to_data_in(&late, at(10, 5, 10), &London);
        assert_eq!(data.when_label, "was due Fri 2 Oct");
        assert!(data.overdue);
        let in_time = record("in-time", "bill");
        assert_eq!(
            now_order(&late, &in_time, at(10, 5, 10)),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn untrusted_link_says_open_email_and_why() {
        let mut lookalike = record("b", "bill");
        lookalike.action_trusted = false;
        lookalike.action_domain = Some("camden-gov.uk".to_string());
        lookalike.action_gate =
            Some(r#"{"dmarc_pass":true,"domain_match":false,"prior_mail":false}"#.to_string());
        let action = to_data_in(&lookalike, at(10, 5, 10), &London)
            .action
            .expect("action");
        assert_eq!(action.label, "Open email to pay");
        assert_eq!(action.domain.as_deref(), Some("camden-gov.uk"));
        assert_eq!(
            action.untrusted_reason.as_deref(),
            Some("the link goes to a different domain from the sender's")
        );
    }

    #[test]
    fn bands_split_by_surface_time_and_group_coming_up_by_week() {
        let now = at(10, 5, 10);
        let surfaced = record("now", "bill");
        let mut next_week = record("next", "renewal");
        next_week.title = "Renew car insurance".to_string();
        next_week.surface_at = Some(at(10, 12, 9));
        let mut undated = record("whenever", "sign");
        undated.surface_at = None;
        undated.due_at = None;
        undated.act_by_at = None;
        let mut pending = record("pending", "promise");
        pending.catchup = Some(mxr_store::TodoCatchup::Pending);
        let bands = bands(vec![surfaced, next_week, undated, pending], now, &London);
        assert_eq!(bands.now.len(), 1);
        assert_eq!(bands.coming_up.len(), 1);
        assert_eq!(bands.coming_up[0].label, "wk of 12 Oct");
        assert_eq!(bands.whenever.len(), 1);
        let next = bands.next_surface.expect("next");
        assert_eq!(
            empty_state(false, Some(&next)),
            "Nothing needs you. Next: renew car insurance shows up Mon 12 Oct."
        );
        assert_eq!(
            headline(1, bands.first_now.as_ref(), now, &London),
            "1 thing needs you. Pay council tax, act by Wed."
        );
    }
}
