//! Sorting shows its work (blueprint 22, D119).
//!
//! - Placement: each arrivals row (written by a trigger when sync stores an
//!   inbound message) gets the mode it went to, the rule and reason, and a
//!   "not sure" mark for a rule conflict. The placement is the same rule
//!   live membership uses: the shared classifier, then the thread's shape.
//! - The arrivals line on Now: every arrival first seen since Now was last
//!   opened, counted once by where it is now. The counts, the lists they
//!   open and the track record all read the same rows.
//! - Moves: `X` moves one email (`MoveMessage`), `K` sets the sender's
//!   mode. Both are stored as corrections, take effect in every client at
//!   once and undo exactly (`UndoMove`).
//!
//! Corrections feed placement: a per-email move wins for that email until a
//! newer sender decision, and a sender decision wins over every rule.

use super::conversation_shape::{conversation_shape, human_address, Shape, ShapeInputs};
use super::desk::{self_matcher, Senders};
use super::desk_lanes::is_outbound;
use super::mail_kind::{self, SenderKind};
use super::mode_rules::age_label;
use super::places::scoped_accounts;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::{
    arrivals_copy, ArrivalBucketData, ArrivalCountData, ArrivalItemData, ArrivalListData,
    ArrivalsData, CorrectionData, DaemonEvent, KindRuleData, ModeKindData, MoveChoiceData,
    MoveOutcomeData, NotSureData, ResponseData, SenderKindData,
};
use mxr_store::{
    ArrivalPlacement, ArrivalRow, Correction, DeskMessage, NewCorrection, ScreenerDisposition,
};
use std::collections::{HashMap, HashSet};

/// Arrivals placed per pass, so a large backfill never holds one long read.
const PLACE_BATCH: u32 = 400;
/// The most emails one `GetArrivalModes` may name.
pub(crate) const ARRIVAL_MODES_MAX: usize = 200;
/// A second visit to Now this soon after the first is the same visit, so a
/// double mount or a quick look elsewhere never empties the line.
const SAME_VISIT_SECS: i64 = 60;
/// Undoing a sender's mode through `SetSenderKind` within this long of the
/// move puts the move back instead of logging a second one.
const SENDER_UNDO_WINDOW_SECS: i64 = 10 * 60;
/// The "not sure" mark for someone you've written to who only copied you
/// (D119's U1).
const NOT_SURE_COPIED_KNOWN: &str = "copied_known";

// ---------------------------------------------------------------------------
// Placement
// ---------------------------------------------------------------------------

/// The stored name of a rule.
fn rule_id(rule: KindRuleData) -> String {
    serde_json::to_value(rule)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "person".to_string())
}

/// Where one arrival goes now, by the rules live membership uses.
/// `written_to` reads sent mail directly: on a first sync the contacts
/// table may not have counted it yet.
fn place_message(
    message: &DeskMessage,
    thread: &[DeskMessage],
    senders: &Senders,
    written_to: &HashSet<String>,
    is_self: &dyn Fn(&str) -> bool,
    shape: super::conversation_shape::ShapeConfig,
    spam: bool,
) -> ArrivalPlacement {
    let placed = |mode: &str, rule: String, reason: String, not_sure: Option<String>| {
        ArrivalPlacement {
            message_id: message.id.clone(),
            mode: mode.to_string(),
            rule,
            reason,
            not_sure,
        }
    };
    if spam {
        return placed("spam", "spam".into(), "in Spam".into(), None);
    }
    let mut signals = senders.signals(message, is_self);
    signals.written_to |= written_to.contains(&message.from.email.to_ascii_lowercase());
    let described = mail_kind::describe(&signals);
    let rule = rule_id(described.rule);
    match mail_kind::classify(&signals).kind {
        SenderKind::Denied => placed("screened_out", rule, described.reason, None),
        SenderKind::List => placed("reading", rule, described.reason, None),
        SenderKind::Automated => placed("updates", rule, described.reason, None),
        SenderKind::Person => {
            let person_sender = |m: &DeskMessage| senders.answers(m, is_self);
            let human = |email: &str| human_address(email, &senders.contacts, &senders.screener);
            let kept = |m: &DeskMessage| senders.moves.get(&m.id) == Some(&SenderKind::Person);
            let shape = conversation_shape(
                thread,
                &ShapeInputs {
                    is_self,
                    person_sender: &person_sender,
                    human_address: &human,
                    kept_in_messages: &kept,
                    config: shape,
                },
            );
            if shape == Shape::Copied {
                // Someone you've written to only copied you: the rules
                // disagree, so Now asks (U1).
                let not_sure = (signals.written_to && !signals.addressed)
                    .then(|| NOT_SURE_COPIED_KNOWN.to_string());
                placed(
                    "updates",
                    rule_id(KindRuleData::Copied),
                    mail_kind::COPIED_REASON.to_string(),
                    not_sure,
                )
            } else {
                placed("messages", rule, described.reason, None)
            }
        }
    }
}

/// What placing a batch found.
struct Sorted {
    placements: Vec<ArrivalPlacement>,
    /// Rows that turned out to be mail the account sent: not arrivals.
    sent: Vec<MessageId>,
}

/// Place `wanted` (each message with its thread and spam flag) of one
/// account by today's rules. A message its thread no longer holds (moved
/// or deleted mid-pass) is left for the next pass.
async fn place_all(
    state: &AppState,
    account_id: &AccountId,
    wanted: &[(MessageId, ThreadId, bool)],
) -> Result<Sorted, HandlerError> {
    let mut threads: Vec<ThreadId> = wanted
        .iter()
        .map(|(_, thread, _)| thread.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    threads.sort_by_key(ThreadId::as_str);
    let mut messages = Vec::new();
    for chunk in threads.chunks(super::modes::MEMBERSHIP_MAX_THREADS) {
        messages.extend(
            state
                .store
                .desk_messages_in_threads(account_id, chunk)
                .await?,
        );
    }
    let senders = Senders::load(state, account_id, &messages).await?;
    let is_self = self_matcher(state, account_id).await?;
    let shape = super::conversation_shape::shape_config(state);
    let wanted_ids: HashSet<&MessageId> = wanted.iter().map(|(id, _, _)| id).collect();
    let mut from: Vec<String> = messages
        .iter()
        .filter(|m| wanted_ids.contains(&m.id))
        .map(|m| m.from.email.to_ascii_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    from.sort_unstable();
    let written_to = state.store.addresses_written_to(account_id, &from).await?;
    let by_thread: HashMap<&ThreadId, &[DeskMessage]> = messages
        .chunk_by(|a, b| a.thread_id == b.thread_id)
        .map(|thread| (&thread[0].thread_id, thread))
        .collect();
    let mut sorted = Sorted {
        placements: Vec::new(),
        sent: Vec::new(),
    };
    for (message_id, thread_id, spam) in wanted {
        let Some(thread) = by_thread.get(thread_id) else {
            continue;
        };
        let Some(message) = thread.iter().find(|m| &m.id == message_id) else {
            continue;
        };
        // A row stored with an unknown direction can be your own mail.
        if is_outbound(message, &is_self) {
            sorted.sent.push(message_id.clone());
            continue;
        }
        sorted.placements.push(place_message(
            message,
            thread,
            &senders,
            &written_to,
            &is_self,
            shape,
            *spam,
        ));
    }
    Ok(sorted)
}

/// Place every arrival of `account_id` the daemon has not placed yet.
/// Returns how many were placed.
pub(crate) async fn place_pending(
    state: &AppState,
    account_id: &AccountId,
) -> Result<u32, HandlerError> {
    let started = std::time::Instant::now();
    let mut placed = 0u32;
    loop {
        let pending = state
            .store
            .pending_arrivals(account_id, PLACE_BATCH)
            .await?;
        if pending.is_empty() {
            break;
        }
        let mut sent: Vec<MessageId> = Vec::new();
        let mut wanted: Vec<(MessageId, ThreadId, bool)> = Vec::new();
        for row in pending {
            if row.outbound {
                sent.push(row.message_id);
            } else {
                wanted.push((row.message_id, row.thread_id, row.spam));
            }
        }
        let sorted = place_all(state, account_id, &wanted).await?;
        sent.extend(sorted.sent);
        let dropped = state.store.delete_arrivals(&sent).await?;
        let written = state
            .store
            .set_arrival_placements(&sorted.placements, Utc::now())
            .await?;
        placed = placed.saturating_add(u32::try_from(written).unwrap_or(u32::MAX));
        // No progress: what is left waits for the next sync.
        if written == 0 && dropped == 0 {
            break;
        }
    }
    if placed > 0 {
        tracing::info!(
            account = %account_id,
            placed,
            elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
            "arrivals placed"
        );
        modes_changed(state, account_id);
    }
    Ok(placed)
}

/// Re-place one sender's arrivals after their mode changed.
async fn replace_sender(
    state: &AppState,
    account_id: &AccountId,
    sender_email: &str,
) -> Result<(), HandlerError> {
    let rows = state
        .store
        .arrivals_from_sender(account_id, sender_email)
        .await?;
    replace(state, account_id, rows).await
}

/// Re-place arrivals after a correction: each row's `now_mode` becomes
/// where it goes now, by the same rules as live membership, which honour
/// the moves still in force.
async fn replace(
    state: &AppState,
    account_id: &AccountId,
    rows: Vec<(MessageId, ThreadId)>,
) -> Result<(), HandlerError> {
    if rows.is_empty() {
        return Ok(());
    }
    let wanted: Vec<(MessageId, ThreadId, bool)> = rows
        .into_iter()
        .map(|(message, thread)| (message, thread, false))
        .collect();
    let now_modes: Vec<(MessageId, Option<String>)> = place_all(state, account_id, &wanted)
        .await?
        .placements
        .into_iter()
        .map(|placement| (placement.message_id, Some(placement.mode)))
        .collect();
    state.store.set_arrival_now_modes(&now_modes).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The line
// ---------------------------------------------------------------------------

fn window_keys(account_id: Option<&AccountId>) -> (String, String) {
    let scope = account_id.map_or_else(|| "arrivals".to_string(), |a| format!("arrivals:{a}"));
    (format!("{scope}:visit"), format!("{scope}:since"))
}

pub(super) fn start_of_day<Tz: TimeZone>(now: DateTime<Utc>, tz: &Tz) -> DateTime<Utc> {
    let local = now.with_timezone(tz);
    tz.from_local_datetime(&local.date_naive().and_time(NaiveTime::MIN))
        .earliest()
        .map_or(now - Duration::hours(24), |start| start.with_timezone(&Utc))
}

/// The line's window: since the visit to Now before this one, never more
/// than 24 hours back; the start of today when Now was never opened.
/// `mark_seen` starts a new visit.
async fn window<Tz: TimeZone>(
    state: &AppState,
    account_id: Option<&AccountId>,
    mark_seen: bool,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<DateTime<Utc>, HandlerError> {
    let (visit_key, since_key) = window_keys(account_id);
    if mark_seen {
        let last_visit = state.store.mode_last_viewed(&visit_key).await?;
        let new_visit = last_visit.is_none_or(|at| (now - at).num_seconds() >= SAME_VISIT_SECS);
        if new_visit {
            if let Some(last_visit) = last_visit {
                state.store.set_mode_viewed(&since_key, last_visit).await?;
            }
            state.store.set_mode_viewed(&visit_key, now).await?;
        }
    }
    let since = match state.store.mode_last_viewed(&since_key).await? {
        Some(since) => since.max(now - Duration::hours(24)),
        None => start_of_day(now, tz),
    };
    Ok(since.min(now))
}

/// "08:12" today, "yesterday 22:10", or "Tue 22:10".
fn since_label<Tz: TimeZone>(since: DateTime<Utc>, now: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let local = since.with_timezone(tz);
    let today = now.with_timezone(tz).date_naive();
    if local.date_naive() == today {
        local.format("%H:%M").to_string()
    } else if today.pred_opt() == Some(local.date_naive()) {
        format!("yesterday {}", local.format("%H:%M"))
    } else {
        local.format("%a %H:%M").to_string()
    }
}

/// The window's end: one second past now, so mail stored this second
/// counts.
fn until_of(now: DateTime<Utc>) -> DateTime<Utc> {
    now + Duration::seconds(1)
}

pub(super) async fn get_arrivals(
    state: &AppState,
    account_id: Option<&AccountId>,
    mark_seen: bool,
) -> HandlerResult {
    Ok(ResponseData::Arrivals {
        arrivals: arrivals_at(state, account_id, mark_seen, Utc::now(), &Local).await?,
    })
}

/// The line as it stands at `now` in `tz` (tests move the clock).
pub(super) async fn arrivals_at<Tz: TimeZone>(
    state: &AppState,
    account_id: Option<&AccountId>,
    mark_seen: bool,
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<ArrivalsData, HandlerError>
where
    Tz::Offset: std::fmt::Display,
{
    let started = std::time::Instant::now();
    let accounts = scoped_accounts(state, account_id).await?;
    // Anything sync stored but nobody placed yet (the upgrade's backfill,
    // a pass a restart cut short) is placed before it is counted.
    for account in &accounts {
        place_pending(state, account).await?;
    }
    let since = window(state, account_id, mark_seen, now, tz).await?;
    let until = until_of(now);
    let label = since_label(since, now, tz);
    let counts = state.store.arrival_counts(&accounts, since, until).await?;

    let line_counts: Vec<ArrivalCountData> = ArrivalBucketData::ALL
        .into_iter()
        .filter_map(|bucket| {
            let count = counts.by_mode.get(bucket.id()).copied().unwrap_or(0);
            (count > 0).then(|| ArrivalCountData {
                bucket,
                count,
                label: bucket.label(count),
            })
        })
        .collect();
    // Anything stored under a name the line doesn't know still counts, so
    // the parts can never silently fall short of the total.
    let known: u32 = line_counts.iter().map(|c| c.count).sum();
    let unplaced = counts.total.saturating_sub(known);
    let also: Vec<ArrivalCountData> = [
        (ArrivalBucketData::Todo, counts.also_todo),
        (ArrivalBucketData::Archive, counts.also_archive),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(bucket, count)| ArrivalCountData {
        bucket,
        count,
        label: match bucket {
            ArrivalBucketData::Todo => format!("{count} in To do"),
            _ => format!("{count} in Archive"),
        },
    })
    .collect();
    let latest_at = state.store.latest_arrival_at(&accounts).await?;
    let sorting = counts.by_mode.get("sorting").copied().unwrap_or(0);

    let line = if counts.total == 0 {
        match latest_at {
            Some(latest) => format!(
                "Nothing new since {label}. Latest mail {} ago.",
                age_label(latest, now)
            ),
            None => format!("Nothing new since {label}."),
        }
    } else {
        let mut parts: Vec<String> = line_counts.iter().map(|c| c.label.clone()).collect();
        if unplaced > 0 {
            parts.push(format!("{unplaced} not placed"));
        }
        let mut line = format!(
            "Since {label}: {} arrived. {}.",
            counts.total,
            parts.join(" · ")
        );
        if !also.is_empty() {
            let also_parts: Vec<&str> = also.iter().map(|c| c.label.as_str()).collect();
            line.push_str(&format!(" Also {}.", also_parts.join(", ")));
        }
        line
    };
    // "Accounted for" only once every arrival is placed.
    let clear_line = (counts.total > 0 && sorting == 0 && unplaced == 0).then(|| {
        if counts.total == 1 {
            format!("Clear. The 1 email since {label} is accounted for.")
        } else {
            format!(
                "Clear. All {} emails since {label} are accounted for.",
                counts.total
            )
        }
    });

    let not_sure = not_sure_today(state, &accounts, now, tz).await?;
    let not_sure_line = match not_sure.len() {
        0 => None,
        1 => Some("1 email I wasn't sure about. Where should it go?".to_string()),
        n => Some(format!(
            "{n} emails I wasn't sure about. Where should these go?"
        )),
    };
    let track_record = track_record(state, &accounts, now).await?;
    tracing::debug!(
        accounts = accounts.len(),
        total = counts.total,
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "arrivals composed"
    );
    Ok(ArrivalsData {
        generated_at: now,
        since,
        until,
        since_label: label,
        total: counts.total,
        counts: line_counts,
        also,
        line,
        clear_line,
        latest_at,
        not_sure_hint: (!not_sure.is_empty()).then(|| arrivals_copy::NOT_SURE_HINT.to_string()),
        not_sure,
        not_sure_line,
        track_record,
        never_bury: arrivals_copy::NEVER_BURY.to_string(),
    })
}

/// Today's "Not sure" questions: the first three conflicts first seen
/// today in `tz`, minus those answered. The cap counts asked ones, so an
/// answer never pulls a fourth in.
async fn not_sure_today<Tz: TimeZone>(
    state: &AppState,
    accounts: &[AccountId],
    now: DateTime<Utc>,
    tz: &Tz,
) -> Result<Vec<NotSureData>, HandlerError> {
    let rows = state
        .store
        .not_sure_arrivals(accounts, start_of_day(now, tz), until_of(now))
        .await?;
    Ok(rows
        .into_iter()
        .take(arrivals_copy::NOT_SURE_DAILY_CAP)
        .filter(|row| !row.answered && Some(row.effective.as_str()) == row.mode.as_deref())
        .filter_map(|row| {
            let mode = ModeKindData::parse(&row.effective)?;
            let name = row
                .from_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| row.sender_email.clone());
            Some(NotSureData {
                line: format!(
                    "{name} copied you on \"{}\". {} for now.",
                    row.subject.trim(),
                    mode.name()
                ),
                account_id: row.account_id,
                message_id: row.message_id,
                thread_id: row.thread_id,
                sender_email: row.sender_email,
                sender_name: row.from_name.filter(|name| !name.trim().is_empty()),
                subject: row.subject,
                mode,
                choices: MoveChoiceData::all(),
            })
        })
        .collect())
}

/// "Last week mxr sorted 310 emails; you moved 2." Only once a move
/// exists: before that it could only ever say 0.
async fn track_record(
    state: &AppState,
    accounts: &[AccountId],
    now: DateTime<Utc>,
) -> Result<Option<String>, HandlerError> {
    if !state.store.has_moves(accounts).await? {
        return Ok(None);
    }
    let week = now - Duration::days(7);
    let counts = state
        .store
        .arrival_counts(accounts, week, until_of(now))
        .await?;
    let sorted = counts
        .total
        .saturating_sub(counts.by_mode.get("sorting").copied().unwrap_or(0));
    let moved = state.store.count_moves_since(accounts, week).await?;
    let emails = if sorted == 1 { "email" } else { "emails" };
    Ok(Some(format!(
        "Last week mxr sorted {sorted} {emails}; you moved {moved}."
    )))
}

// ---------------------------------------------------------------------------
// Lists and chips
// ---------------------------------------------------------------------------

fn bucket_of(effective: &str) -> ArrivalBucketData {
    ArrivalBucketData::parse(effective).unwrap_or(ArrivalBucketData::Sorting)
}

/// The Inbox chip: "→ Updates · automated sender", "In Spam".
fn chip(bucket: ArrivalBucketData, reason: Option<&str>, moved: bool) -> String {
    match bucket {
        ArrivalBucketData::Spam => "In Spam".to_string(),
        ArrivalBucketData::ScreenedOut => "Screened out".to_string(),
        ArrivalBucketData::Sorting => "Still sorting".to_string(),
        _ => {
            let name = bucket.mode().map_or("", ModeKindData::name);
            let reason = if moved {
                Some("you moved this email")
            } else {
                reason
            };
            match reason {
                Some(reason) => format!("→ {name} · {reason}"),
                None => format!("→ {name}"),
            }
        }
    }
}

fn item(row: ArrivalRow) -> ArrivalItemData {
    let bucket = bucket_of(&row.effective);
    let arrived_in = row
        .mode
        .as_deref()
        .map_or(ArrivalBucketData::Sorting, bucket_of);
    // The arrival's reason describes where it arrived; after a sender move
    // it would explain the wrong mode.
    let reason = if bucket == arrived_in {
        row.reason.clone()
    } else if row.moved {
        Some("you moved this email".to_string())
    } else {
        Some("you set this sender's mode".to_string())
    };
    ArrivalItemData {
        chip: chip(bucket, reason.as_deref(), row.moved),
        account_id: row.account_id,
        message_id: row.message_id,
        thread_id: row.thread_id,
        sender_email: row.sender_email,
        sender_name: row.from_name.filter(|name| !name.trim().is_empty()),
        subject: row.subject,
        date: row.date,
        first_seen_at: row.first_seen_at,
        arrived_in,
        bucket,
        reason,
        moved: row.moved,
        not_sure: row.not_sure.is_some(),
        unread: row.unread,
        also_todo: row.also_todo,
        also_archive: row.also_archive,
    }
}

pub(super) async fn list_arrivals(
    state: &AppState,
    account_id: Option<&AccountId>,
    bucket: Option<ArrivalBucketData>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    limit: u32,
) -> HandlerResult {
    let now = Utc::now();
    let accounts = scoped_accounts(state, account_id).await?;
    let since = match since {
        Some(since) => since,
        None => window(state, account_id, false, now, &Local).await?,
    };
    let until = until.unwrap_or_else(|| until_of(now));
    if until <= since {
        return Err(HandlerError::InvalidRequest(
            "until must be after since".into(),
        ));
    }
    let (rows, total) = state
        .store
        .list_arrivals(
            &accounts,
            since,
            until,
            bucket.map(ArrivalBucketData::id),
            limit.clamp(1, 1000),
        )
        .await?;
    Ok(ResponseData::ArrivalList {
        list: ArrivalListData {
            since,
            until,
            bucket,
            total,
            items: rows.into_iter().map(item).collect(),
        },
    })
}

pub(super) async fn get_arrival_modes(
    state: &AppState,
    message_ids: &[MessageId],
) -> HandlerResult {
    if message_ids.len() > ARRIVAL_MODES_MAX {
        return Err(HandlerError::InvalidRequest(format!(
            "at most {ARRIVAL_MODES_MAX} emails at once"
        )));
    }
    let mut by_id: HashMap<MessageId, ArrivalRow> = state
        .store
        .arrivals_by_ids(message_ids)
        .await?
        .into_iter()
        .map(|row| (row.message_id.clone(), row))
        .collect();
    Ok(ResponseData::ArrivalModes {
        items: message_ids
            .iter()
            .filter_map(|id| by_id.remove(id))
            .map(item)
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// Moves
// ---------------------------------------------------------------------------

const fn sender_kind_for(mode: ModeKindData) -> Option<SenderKindData> {
    match mode {
        ModeKindData::Messages => Some(SenderKindData::People),
        ModeKindData::Updates => Some(SenderKindData::PaperTrail),
        ModeKindData::Reading => Some(SenderKindData::Reading),
        ModeKindData::Todo | ModeKindData::Archive => None,
    }
}

/// Where a sender kind sends mail, as the ledger names it.
pub(super) const fn bucket_for_kind(kind: SenderKindData) -> ArrivalBucketData {
    match kind {
        SenderKindData::People => ArrivalBucketData::Messages,
        SenderKindData::Reading => ArrivalBucketData::Reading,
        SenderKindData::PaperTrail => ArrivalBucketData::Updates,
        SenderKindData::ScreenedOut => ArrivalBucketData::ScreenedOut,
    }
}

/// Where one email is now: its arrivals row, else placed live.
async fn current_bucket(
    state: &AppState,
    account_id: &AccountId,
    message_id: &MessageId,
    thread_id: &ThreadId,
) -> Result<(ArrivalBucketData, Option<ArrivalRow>), HandlerError> {
    let row = state
        .store
        .arrivals_by_ids(std::slice::from_ref(message_id))
        .await?
        .pop();
    if let Some(row) = row.as_ref().filter(|row| row.mode.is_some()) {
        return Ok((bucket_of(&row.effective), Some(row.clone())));
    }
    let placed = place_all(
        state,
        account_id,
        &[(message_id.clone(), thread_id.clone(), false)],
    )
    .await?;
    let bucket = placed
        .placements
        .first()
        .map_or(ArrivalBucketData::Sorting, |p| bucket_of(&p.mode));
    Ok((bucket, row))
}

/// One `MoveMessage`, as the handler hands it over.
pub(super) struct MoveRequest<'a> {
    pub message_id: &'a MessageId,
    pub mode: ModeKindData,
    pub sender: bool,
    pub dry_run: bool,
    pub source: Option<&'a str>,
}

pub(super) async fn move_message(state: &AppState, request: MoveRequest<'_>) -> HandlerResult {
    let MoveRequest {
        message_id,
        mode,
        sender,
        dry_run,
        source,
    } = request;
    let source = match source.map(str::trim).filter(|s| !s.is_empty()) {
        None => if sender { "sender" } else { "move" }.to_string(),
        Some("not_sure") => "not_sure".to_string(),
        Some(other) => {
            return Err(HandlerError::InvalidRequest(format!(
                "unknown source `{other}`; use not_sure or leave it out"
            )))
        }
    };
    let envelope = state
        .store
        .get_envelope(message_id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest(format!("No message {message_id}.")))?;
    let account_id = envelope.account_id.clone();
    let sender_email = envelope.from.email.trim().to_ascii_lowercase();
    let (from, row) = current_bucket(state, &account_id, message_id, &envelope.thread_id).await?;
    let now = Utc::now();
    let outcome = |copy: String,
                   ask_sender: Option<String>,
                   correction_id: Option<i64>,
                   aspect_id: Option<String>| MoveOutcomeData {
        account_id: account_id.clone(),
        message_id: message_id.clone(),
        thread_id: envelope.thread_id.clone(),
        sender_email: sender_email.clone(),
        from,
        to: mode,
        sender,
        dry_run,
        hint: ask_sender
            .is_some()
            .then(|| arrivals_copy::MOVE_HINT.to_string()),
        copy,
        ask_sender,
        correction_id,
        aspect_id,
    };
    let correction = |scope: &str| NewCorrection {
        account_id: account_id.clone(),
        scope: scope.to_string(),
        message_id: Some(message_id.clone()),
        sender_email: sender_email.clone(),
        from_mode: from.id().to_string(),
        to_mode: mode.id().to_string(),
        rule: row.as_ref().and_then(|row| row.rule.clone()),
        source: source.clone(),
        created_at: now,
        prior_moved_to: None,
        prior_moved_at: None,
        prior_disposition: None,
        aspect_id: None,
    };

    if sender {
        let kind = sender_kind_for(mode).ok_or_else(|| {
            HandlerError::InvalidRequest(
                "A sender's mail can go to Messages, Updates or Reading.".into(),
            )
        })?;
        if dry_run {
            return Ok(moved(outcome(
                format!("Would send all mail from {sender_email} to {}.", mode.name()),
                None,
                None,
                None,
            )));
        }
        let prior = state
            .store
            .get_screener_decision(&account_id, &sender_email)
            .await?
            .map(|decision| decision.disposition.as_db_str().to_string())
            .unwrap_or_default();
        super::places::apply_sender_kind(state, &account_id, &sender_email, Some(kind)).await?;
        let id = state
            .store
            .insert_correction(&NewCorrection {
                prior_disposition: Some(prior),
                ..correction("sender")
            })
            .await?;
        // Sender-level corrections win over this sender's earlier email
        // moves; a later email move still wins for that email.
        state
            .store
            .supersede_sender_moves(&account_id, &sender_email, id)
            .await?;
        replace_sender(state, &account_id, &sender_email).await?;
        modes_changed(state, &account_id);
        return Ok(moved(outcome(
            format!("All mail from {sender_email} goes to {}.", mode.name()),
            None,
            Some(id),
            None,
        )));
    }

    match mode {
        ModeKindData::Messages | ModeKindData::Updates | ModeKindData::Reading => {
            let to = ArrivalBucketData::from_mode(mode);
            if from == to {
                // Keeping it where it is answers a Not-sure question; for a
                // plain move it changes nothing.
                if source == "not_sure" && !dry_run {
                    state.store.ensure_arrival(message_id).await?;
                    let id = state.store.insert_correction(&correction("email")).await?;
                    modes_changed(state, &account_id);
                    return Ok(moved(outcome(
                        format!("Kept in {}.", mode.name()),
                        None,
                        Some(id),
                        None,
                    )));
                }
                return Ok(moved(outcome(
                    format!("Already in {}.", mode.name()),
                    None,
                    None,
                    None,
                )));
            }
            if dry_run {
                return Ok(moved(outcome(
                    format!("Would move to {}.", mode.name()),
                    None,
                    None,
                    None,
                )));
            }
            state.store.ensure_arrival(message_id).await?;
            let prior = state
                .store
                .arrivals_by_ids(std::slice::from_ref(message_id))
                .await?
                .pop();
            let (prior_moved_to, prior_moved_at) = prior
                .map(|row| (row.moved_to, row.moved_at))
                .unwrap_or_default();
            state
                .store
                .set_arrival_move(message_id, Some(mode.id()), Some(now))
                .await?;
            let id = state
                .store
                .insert_correction(&NewCorrection {
                    prior_moved_to,
                    prior_moved_at,
                    ..correction("email")
                })
                .await?;
            modes_changed(state, &account_id);
            Ok(moved(outcome(
                format!("Moved to {}.", mode.name()),
                Some(format!("{} (K)", arrivals_copy::ALWAYS_FOR_SENDER)),
                Some(id),
                None,
            )))
        }
        ModeKindData::Todo => {
            let title = envelope.subject.trim();
            let title = if title.is_empty() {
                "Follow up on this email"
            } else {
                title
            };
            let response =
                super::todos::create(state, message_id, title, None, None, None, dry_run).await?;
            let todo_id = match response {
                ResponseData::TodoChange { change } => {
                    change.changed.first().map(|todo| todo.id.clone())
                }
                _ => None,
            };
            aspect_move(state, AspectMove {
                account_id: &account_id,
                message_id,
                dry_run,
                aspect_id: todo_id,
                correction: correction("email"),
                copy: ("Would add to To do.", "Added to To do."),
            })
            .await
            .map(|(copy, id, aspect)| moved(outcome(copy, None, id, aspect)))
        }
        ModeKindData::Archive => {
            let response = super::records::file(state, message_id, None, dry_run).await?;
            let record_id = match response {
                ResponseData::RecordChange { change } => {
                    change.records.first().map(|record| record.id.clone())
                }
                _ => None,
            };
            aspect_move(state, AspectMove {
                account_id: &account_id,
                message_id,
                dry_run,
                aspect_id: record_id,
                correction: correction("email"),
                copy: ("Would file in Archive.", "Filed in Archive."),
            })
            .await
            .map(|(copy, id, aspect)| moved(outcome(copy, None, id, aspect)))
        }
    }
}

fn moved(outcome: MoveOutcomeData) -> ResponseData {
    ResponseData::MessageMoved { outcome }
}

fn modes_changed(state: &AppState, account_id: &AccountId) {
    crate::chimes::emit_daemon_event(
        state,
        DaemonEvent::ModesChanged {
            account_id: Some(account_id.clone()),
        },
    );
}

struct AspectMove<'a> {
    account_id: &'a AccountId,
    message_id: &'a MessageId,
    dry_run: bool,
    aspect_id: Option<String>,
    correction: NewCorrection,
    /// The dry run's copy, then the run's.
    copy: (&'static str, &'static str),
}

/// To do and Archive add the email there (D119: they are aspects, never a
/// primary mode), and log the correction with what was made, for undo.
async fn aspect_move(
    state: &AppState,
    request: AspectMove<'_>,
) -> Result<(String, Option<i64>, Option<String>), HandlerError> {
    if request.dry_run {
        return Ok((request.copy.0.to_string(), None, None));
    }
    state.store.ensure_arrival(request.message_id).await?;
    let id = state
        .store
        .insert_correction(&NewCorrection {
            aspect_id: request.aspect_id.clone(),
            ..request.correction
        })
        .await?;
    modes_changed(state, request.account_id);
    Ok((request.copy.1.to_string(), Some(id), request.aspect_id))
}

pub(super) async fn undo_move(state: &AppState, correction_id: i64) -> HandlerResult {
    let correction = state
        .store
        .get_correction(correction_id)
        .await?
        .ok_or_else(|| {
            HandlerError::InvalidRequest(format!("No correction {correction_id}."))
        })?;
    // Stamping first makes a double undo (two clients, a double press) a
    // no-op instead of reverting twice.
    if !state
        .store
        .mark_correction_undone(correction_id, Utc::now())
        .await?
    {
        return Ok(ResponseData::MoveUndone {
            correction_id,
            copy: "Already undone.".to_string(),
        });
    }
    revert(state, &correction).await?;
    modes_changed(state, &correction.fields.account_id);
    let back = ArrivalBucketData::parse(&correction.fields.from_mode)
        .map_or("where it was".to_string(), |bucket| {
            bucket.mode().map_or_else(
                || bucket.label(1).trim_start_matches("1 ").to_string(),
                |mode| mode.name().to_string(),
            )
        });
    Ok(ResponseData::MoveUndone {
        correction_id,
        copy: format!("Moved back to {back}."),
    })
}

async fn revert(state: &AppState, correction: &Correction) -> Result<(), HandlerError> {
    let fields = &correction.fields;
    if fields.scope == "sender" {
        let previous = fields
            .prior_disposition
            .as_deref()
            .and_then(ScreenerDisposition::from_db_str)
            .and_then(mail_kind::kind_for);
        super::places::apply_sender_kind(state, &fields.account_id, &fields.sender_email, previous)
            .await?;
        state.store.restore_superseded_moves(correction.id).await?;
        replace_sender(state, &fields.account_id, &fields.sender_email).await?;
        return Ok(());
    }
    let Some(message_id) = fields.message_id.as_ref() else {
        return Ok(());
    };
    match ModeKindData::parse(&fields.to_mode) {
        Some(ModeKindData::Todo) => {
            if let Some(todo) = &fields.aspect_id {
                state
                    .store
                    .set_todos_state(
                        std::slice::from_ref(todo),
                        &[mxr_store::TodoState::Open],
                        mxr_store::TodoState::Dismissed,
                        Utc::now(),
                    )
                    .await?;
            }
        }
        Some(ModeKindData::Archive) => {
            if let Some(record) = &fields.aspect_id {
                super::records::dismiss(state, std::slice::from_ref(record), false, false).await?;
            }
        }
        Some(_) if fields.from_mode != fields.to_mode => {
            state
                .store
                .set_arrival_move(
                    message_id,
                    fields.prior_moved_to.as_deref(),
                    fields.prior_moved_at,
                )
                .await?;
            if let Some(envelope) = state.store.get_envelope(message_id).await? {
                replace(
                    state,
                    &fields.account_id,
                    vec![(message_id.clone(), envelope.thread_id)],
                )
                .await?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// After `SetSenderKind` (the Screener, a sender row's `K`): log the move,
/// or recognise it as the undo of the move just made, then re-place the
/// sender's arrivals and tell every client.
pub(super) async fn after_sender_kind(
    state: &AppState,
    account_id: &AccountId,
    sender_email: &str,
    previous: Option<SenderKindData>,
    kind: Option<SenderKindData>,
) -> Result<(), HandlerError> {
    let now = Utc::now();
    let latest = state
        .store
        .latest_sender_correction(account_id, sender_email)
        .await?;
    let undoes_latest = latest.as_ref().is_some_and(|latest| {
        (now - latest.fields.created_at).num_seconds() <= SENDER_UNDO_WINDOW_SECS
            && latest
                .fields
                .prior_disposition
                .as_deref()
                .map(|prior| {
                    ScreenerDisposition::from_db_str(prior).and_then(mail_kind::kind_for)
                })
                == Some(kind)
    });
    if undoes_latest {
        if let Some(latest) = latest {
            state.store.mark_correction_undone(latest.id, now).await?;
            state.store.restore_superseded_moves(latest.id).await?;
        }
    } else if previous != kind {
        let name = |kind: Option<SenderKindData>| {
            kind.map_or("auto", |kind| bucket_for_kind(kind).id())
                .to_string()
        };
        let id = state
            .store
            .insert_correction(&NewCorrection {
                account_id: account_id.clone(),
                scope: "sender".to_string(),
                message_id: None,
                sender_email: sender_email.to_string(),
                from_mode: name(previous),
                to_mode: name(kind),
                rule: None,
                source: "sender".to_string(),
                created_at: now,
                prior_moved_to: None,
                prior_moved_at: None,
                prior_disposition: Some(
                    previous
                        .map(|kind| mail_kind::disposition_for(kind).as_db_str().to_string())
                        .unwrap_or_default(),
                ),
                aspect_id: None,
            })
            .await?;
        if kind.is_some() {
            state
                .store
                .supersede_sender_moves(account_id, sender_email, id)
                .await?;
        }
    }
    replace_sender(state, account_id, sender_email).await?;
    modes_changed(state, account_id);
    Ok(())
}

pub(super) async fn list_corrections(
    state: &AppState,
    account_id: Option<&AccountId>,
    limit: u32,
) -> HandlerResult {
    let accounts = scoped_accounts(state, account_id).await?;
    let corrections = state
        .store
        .list_corrections(&accounts, limit.clamp(1, 1000))
        .await?
        .into_iter()
        .map(|correction| CorrectionData {
            id: correction.id,
            account_id: correction.fields.account_id,
            scope: correction.fields.scope,
            message_id: correction.fields.message_id,
            sender_email: correction.fields.sender_email,
            from_mode: correction.fields.from_mode,
            to_mode: correction.fields.to_mode,
            rule: correction.fields.rule,
            source: correction.fields.source,
            created_at: correction.fields.created_at,
            undone_at: correction.undone_at,
        })
        .collect();
    Ok(ResponseData::Corrections { corrections })
}

/// The accounts a message-scoped arrivals request touches.
pub(super) async fn correction_accounts(
    state: &AppState,
    correction_id: i64,
) -> Result<Vec<AccountId>, HandlerError> {
    Ok(state
        .store
        .get_correction(correction_id)
        .await?
        .map(|correction| vec![correction.fields.account_id])
        .unwrap_or_default())
}

