//! `Request::GetNow`: the front page (blueprint 22). Four fixed sections,
//! never reordered: People (from Messages, the desk's lanes), Due soon
//! (To do's Now band, by act-by), one Updates card, and one Reading pick
//! from 17:00. At most three items each, so Now never shows more than ten
//! things, and nothing past its relevancy window enters it. The caps live
//! here so every client shows the same Now.

use super::desk::{compose_desk, desk_lane};
use super::mode_rules::{
    age_label, day_part, more_line, now_headline, overload_line, reading_fade, updates_line,
    EVENING_HOUR,
};
use super::modes::{inbox_modes, place_threads, InboxModes};
use super::places::{scoped_accounts, Placed};
use super::todo_view::{bands, day_label, start_of_week};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Local, TimeZone, Timelike, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::MessageFlags;
use mxr_protocol::{
    now_copy, DeskLaneKind, DeskRowData, ModeKindData, NowData, NowDueData, NowPeopleData,
    NowPersonData, NowReadingPickData, NowTodoData, NowUpdateSourceData, NowUpdatesCardData,
    ResponseData, ScreenerQuestionData, TodoData, TodoNextData, UpdatesDigestData, NOW_SECTION_CAP,
};
use std::collections::{HashMap, HashSet};

/// What Now and the rail are built from, read once.
pub(super) struct NowSnapshot {
    pub owed: Vec<DeskRowData>,
    pub people_new: Vec<DeskRowData>,
    pub waiting: Vec<DeskRowData>,
    /// To do's Now band, by act-by, inside its relevancy window.
    pub due_now: Vec<TodoData>,
    pub next_surface: Option<TodoNextData>,
    pub inbox: InboxModes,
    /// Updates' latest digest, for the card and the rail's count.
    pub digest: UpdatesDigestData,
}

impl NowSnapshot {
    /// Work only: people whose turn it is with you, and things to act on.
    pub(super) fn badge(&self) -> u32 {
        distinct_people([&self.owed, &self.people_new]) + count(self.due_now.len())
    }

    /// Everyone in Messages, waiting on them included: the rail's count.
    /// People counts persons, not conversations.
    pub(super) fn people_total(&self) -> u32 {
        distinct_people([&self.owed, &self.people_new, &self.waiting])
    }

    /// People on Now: whose turn it is with you, never who you wait on.
    fn your_turn_total(&self) -> u32 {
        distinct_people([&self.owed, &self.people_new])
    }

    /// Person rows in People's order (You owe, then New from people), each
    /// person once, at their most pressing conversation.
    fn person_rows(&self) -> impl Iterator<Item = &DeskRowData> {
        let mut seen = HashSet::new();
        self.owed
            .iter()
            .chain(&self.people_new)
            .filter(move |row| seen.insert(row.counterparty_email.to_ascii_lowercase()))
    }
}

/// How many different people the rows are with.
fn distinct_people<const N: usize>(lanes: [&Vec<DeskRowData>; N]) -> u32 {
    let people: HashSet<String> = lanes
        .into_iter()
        .flatten()
        .map(|row| row.counterparty_email.to_ascii_lowercase())
        .collect();
    count(people.len())
}

pub(super) fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

pub(super) async fn snapshot<Tz: TimeZone>(
    state: &AppState,
    account_id: Option<&AccountId>,
    accounts: &[AccountId],
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<NowSnapshot, HandlerError>
where
    Tz::Offset: std::fmt::Display,
{
    let desk = compose_desk(state, account_id, now).await?;
    let lane = |kind| desk_lane(&desk.rows, kind, u32::MAX).rows;
    // Hard rule: nothing past its relevancy window enters Now.
    let records = state
        .store
        .list_runway_todos(account_id, start_of_week(now, tz))
        .await?
        .into_iter()
        .filter(|record| record.relevant_until.is_none_or(|until| until >= now))
        .collect();
    let bands = bands(records, now, tz);
    // To do's own Now band keeps late rows after the ones still in time;
    // Now puts them first, so its cap of three never hides one.
    let mut due_now = bands.now;
    due_now.sort_by_key(|todo| (!todo.overdue, todo.act_by_at.is_none(), todo.act_by_at));
    let mut inbox = inbox_modes(state, accounts).await?;
    let scope = super::updates_digest::Scope {
        account_id: account_id.cloned(),
        ..Default::default()
    };
    // The digest is all Now and the rail read of Updates' mail.
    let updates = std::mem::take(&mut inbox.updates);
    let (digest, _, _) = Box::pin(super::updates::digest_from(
        state, accounts, updates, &scope, now, tz,
    ))
    .await?;
    Ok(NowSnapshot {
        owed: lane(DeskLaneKind::Owed),
        people_new: lane(DeskLaneKind::PeopleNew),
        waiting: lane(DeskLaneKind::Waiting),
        due_now,
        next_surface: bands.next_surface,
        inbox,
        digest,
    })
}

pub(super) async fn get_now(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    Ok(ResponseData::Now {
        now: now_at(state, account_id, Utc::now(), &Local).await?,
    })
}

/// Now as it stands at `now` in `tz` (tests move the clock).
pub(super) async fn now_at<Tz: TimeZone>(
    state: &AppState,
    account_id: Option<&AccountId>,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<NowData, HandlerError>
where
    Tz::Offset: std::fmt::Display,
{
    let started = std::time::Instant::now();
    let accounts = scoped_accounts(state, account_id).await?;
    let snapshot = snapshot(state, account_id, &accounts, now, tz).await?;
    let asks = sender_questions(state, &snapshot, now).await?;

    let people = people_section(&snapshot, &asks, now);
    let due_soon = due_section(&snapshot);
    let updates = updates_card(&snapshot.digest);
    let evening = now.with_timezone(tz).hour() >= EVENING_HOUR;
    let reading = evening
        .then(|| reading_pick(&snapshot.inbox.reading, now))
        .flatten();
    let not_now = (!evening)
        .then(|| {
            let week = now - Duration::days(7);
            let issues = snapshot
                .inbox
                .reading
                .iter()
                .filter(|item| item.message.date >= week)
                .count();
            (issues > 0).then(|| format!("Not now: Reading {issues} this week"))
        })
        .flatten();

    let item_count = count(
        people.rows.len()
            + due_soon.todos.len()
            + usize::from(updates.is_some())
            + usize::from(reading.is_some()),
    );
    let first_run = super::todos::first_run(state, account_id).await?;
    let next_at = snapshot.next_surface.as_ref().map(|next| next.at);
    let empty_state = (item_count == 0).then(|| {
        if !first_run.complete {
            now_copy::NEVER_HAD_ANY.to_string()
        } else {
            match &snapshot.next_surface {
                Some(next) => format!(
                    "{} The next to-do surfaces {} {}.",
                    now_copy::CLEAR,
                    day_label(next.at, tz),
                    next.at.with_timezone(tz).format("%H:%M")
                ),
                None => now_copy::CLEAR.to_string(),
            }
        }
    });
    let headline = now_headline(
        &day_part(now, tz),
        distinct_people([&snapshot.owed, &snapshot.people_new]),
        due_soon.total,
    );
    tracing::debug!(
        accounts = accounts.len(),
        item_count,
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "now composed"
    );
    Ok(NowData {
        generated_at: now,
        header: now_copy::HEADER.to_string(),
        headline,
        people,
        due_soon,
        updates,
        reading,
        not_now,
        item_count,
        empty_state,
        next_at,
        first_run,
        coming_up: super::records::coming_up(state, &accounts, now, NOW_SECTION_CAP).await?,
    })
}

/// The new-sender question for each shown person row, from membership's
/// own first-time check (D117), so Now asks exactly when Messages would.
async fn sender_questions(
    state: &AppState,
    snapshot: &NowSnapshot,
    now: DateTime<Utc>,
) -> Result<HashMap<ThreadId, ScreenerQuestionData>, HandlerError> {
    let mut by_account: HashMap<AccountId, Vec<ThreadId>> = HashMap::new();
    for row in snapshot.person_rows().take(NOW_SECTION_CAP) {
        by_account
            .entry(row.account_id.clone())
            .or_default()
            .push(row.thread_id.clone());
    }
    let mut asks = HashMap::new();
    for (account, threads) in by_account {
        for placement in place_threads(state, &account, &threads, now).await? {
            if let Some(question) = placement.data.new_sender {
                asks.insert(placement.data.thread_id, question);
            }
        }
    }
    Ok(asks)
}

fn people_section(
    snapshot: &NowSnapshot,
    asks: &HashMap<ThreadId, ScreenerQuestionData>,
    now: DateTime<Utc>,
) -> NowPeopleData {
    let total = snapshot.your_turn_total();
    let rows: Vec<NowPersonData> = snapshot
        .person_rows()
        .take(NOW_SECTION_CAP)
        .map(|row| {
            let new_sender = asks.get(&row.thread_id).cloned();
            NowPersonData {
                why: format!(
                    "From Messages: {}, {}.",
                    super::mode_rules::messages_summary(row),
                    age_label(row.since, now)
                ),
                row: row.clone(),
                new_sender,
            }
        })
        .collect();
    NowPeopleData {
        more_line: more_line(total, rows.len(), ModeKindData::Messages),
        overload_line: overload_line(total, distinct_people([&snapshot.owed]), rows.len()),
        rows,
        total,
    }
}

fn due_section(snapshot: &NowSnapshot) -> NowDueData {
    let total = count(snapshot.due_now.len());
    let todos: Vec<NowTodoData> = snapshot
        .due_now
        .iter()
        .take(NOW_SECTION_CAP)
        .map(|todo| NowTodoData {
            why: format!("From To do: {}.", todo.when_label),
            todo: todo.clone(),
        })
        .collect();
    NowDueData {
        more_line: more_line(total, todos.len(), ModeKindData::Todo),
        todos,
        total,
    }
}

/// One card, whatever the count: the latest digest's headline, up to
/// three lines that need a look or changed, and how much waits behind
/// them. No card when the digest is empty.
fn updates_card(digest: &UpdatesDigestData) -> Option<NowUpdatesCardData> {
    let all: Vec<_> = digest
        .needs_a_look
        .iter()
        .chain(&digest.changed)
        .chain(&digest.routine)
        .collect();
    // Parcels on their way aren't a digest to read: once its mail is let
    // go, the card leaves Now unless a tracker went wrong.
    if all.is_empty() || (digest.message_count == 0 && digest.needs_a_look.is_empty()) {
        return None;
    }
    let lines: Vec<_> = digest
        .needs_a_look
        .iter()
        .chain(&digest.changed)
        .take(NOW_SECTION_CAP)
        .cloned()
        .collect();
    let routine: u32 = digest.routine.iter().map(|line| line.count).sum();
    let other_lines = digest.needs_a_look.len() + digest.changed.len() - lines.len();
    let more_line = match (other_lines, routine) {
        (0, 0) => None,
        (0, routine) => Some(format!("+{routine} routine")),
        (more, 0) => Some(format!("+{more} more")),
        (more, routine) => Some(format!("+{more} more, {routine} routine")),
    };
    // The busiest sources, for the one-line summary the card had before.
    let mut ranked: Vec<NowUpdateSourceData> = Vec::new();
    for line in &all {
        if line.sender_email.is_empty() {
            continue;
        }
        match ranked
            .iter_mut()
            .find(|s| s.sender_email == line.sender_email)
        {
            Some(source) => source.count += line.count,
            None => ranked.push(NowUpdateSourceData {
                sender_email: line.sender_email.clone(),
                sender_name: Some(line.source_name.clone()),
                count: line.count,
            }),
        }
    }
    ranked.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.sender_email.cmp(&b.sender_email))
    });
    ranked.truncate(NOW_SECTION_CAP);
    let names: Vec<String> = ranked
        .iter()
        .filter_map(|source| source.sender_name.clone())
        .collect();
    let mut seen = HashSet::new();
    let thread_ids = all
        .iter()
        .flat_map(|line| line.thread_ids.iter().cloned())
        .filter(|thread| seen.insert(thread.clone()))
        .collect();
    Some(NowUpdatesCardData {
        line: updates_line(digest.message_count, digest.source_count, &names),
        since: digest.cut.at,
        thread_ids,
        message_count: digest.message_count,
        source_count: digest.source_count,
        top_sources: ranked,
        early: false,
        title: digest.cut.title.clone(),
        cut_label: digest.cut.label.clone(),
        headline: digest.headline.clone(),
        lines,
        more_line,
        selection_token: digest.selection_token.clone(),
        let_go_line: digest.let_go_line.clone(),
    })
}

/// The evening's one thing to read: the newest unread issue still inside
/// its source's fade, from the source you read most of (read share, then
/// how many you've read).
fn reading_pick(reading: &[Placed], now: DateTime<Utc>) -> Option<NowReadingPickData> {
    struct Source<'a> {
        read: usize,
        total: usize,
        newest_unread: Option<&'a Placed>,
        dates: Vec<DateTime<Utc>>,
    }
    let mut sources: HashMap<String, Source<'_>> = HashMap::new();
    // Newest first already, so the first unread seen is the newest.
    for item in reading {
        let source = sources
            .entry(item.message.from_email.to_ascii_lowercase())
            .or_insert(Source {
                read: 0,
                total: 0,
                newest_unread: None,
                dates: Vec::new(),
            });
        source.total += 1;
        source.dates.push(item.message.date);
        if item.message.flags.contains(MessageFlags::READ) {
            source.read += 1;
        } else if source.newest_unread.is_none() {
            source.newest_unread = Some(item);
        }
    }
    let (source, item) = sources
        .into_values()
        .filter_map(|source| {
            let item = source.newest_unread?;
            let fade = reading_fade(source.dates.clone());
            (item.message.date >= now - fade).then_some((source, item))
        })
        .max_by(|(a, a_item), (b, b_item)| {
            // read/total compared without floats: a.read * b.total vs b.read * a.total.
            (a.read * b.total)
                .cmp(&(b.read * a.total))
                .then(a.read.cmp(&b.read))
                .then(a_item.message.date.cmp(&b_item.message.date))
        })?;
    let message = &item.message;
    let name = message
        .from_name
        .clone()
        .filter(|name| !name.trim().is_empty());
    let label = name.clone().unwrap_or_else(|| message.from_email.clone());
    let why = if source.read == 0 {
        format!("From Reading: the newest from {label}.")
    } else {
        format!(
            "From Reading: you've read {} of the {} from {label} in your inbox.",
            source.read, source.total
        )
    };
    Some(NowReadingPickData {
        account_id: message.account_id.clone(),
        message_id: message.id.clone(),
        thread_id: message.thread_id.clone(),
        sender_email: message.from_email.to_ascii_lowercase(),
        sender_name: name,
        subject: message.subject.clone(),
        date: message.date,
        why,
    })
}
