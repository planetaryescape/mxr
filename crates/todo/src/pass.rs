//! Running detection over mail and keeping the rows right over time.
//!
//! * [`scan_messages`] classifies newly synced mail (the post-sync fan-out).
//! * [`run_first_run`] classifies an account's history newest first,
//!   resumably, applying every window as it goes: what is already over is
//!   written expired and never surfaces, and undated recent items form one
//!   catch-up batch of at most `catchup_max`.
//! * [`sweep`] completes answered RSVPs, expires rows past their window and
//!   claims the rows whose surface time has come.
//!
//! The same placement rules serve all of them, so a re-run or a rule
//! change can add at most one more catch-up batch, never a flood.

use crate::action_link::registrable_domain;
use crate::detect::{from_invite, Detection, Origin};
use crate::provenance::{FieldProvenance, FieldSource, FieldSources};
use crate::text::clip;
use crate::timing::{lead_time, place, window, Placement, TimingInput};
use crate::{TodoKind, RULES_VERSION};
use chrono::{DateTime, TimeZone, Utc};
use mxr_core::id::{AccountId, MessageId};
use mxr_core::types::MessageFlags;
use mxr_store::{Store, TodoCatchup, TodoRecord, TodoScanRow, TodoState, TodoUpsert};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

/// What a pass needs besides the store.
#[derive(Debug, Clone)]
pub struct PassConfig<Tz> {
    pub now: DateTime<Utc>,
    /// The user's zone: due days end at midnight here, rows surface at the
    /// start of working hours here.
    pub tz: Tz,
    pub morning_hour: u8,
    /// How far back undated items may join the first run's catch-up.
    pub catchup_days: u32,
    pub catchup_max: u32,
}

/// Counts only: no titles, amounts or due words, so a summary is safe to
/// log and to record as activity.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PassSummary {
    pub scanned: u64,
    /// Messages whose subject or snippet earned a read of the body.
    pub bodies_read: u64,
    pub created: u64,
    pub updated: u64,
    pub reopened: u64,
    pub catchup: u64,
    pub catchup_overflow: u64,
    /// Found already over, by kind.
    pub expired_at_birth: BTreeMap<String, u64>,
    pub looks_done: u64,
    /// Open promise rows dismissed because their commitment is not a
    /// promise you made in mail you sent.
    pub not_promises: u64,
}

impl PassSummary {
    fn add(&mut self, other: &Self) {
        self.scanned += other.scanned;
        self.bodies_read += other.bodies_read;
        self.created += other.created;
        self.updated += other.updated;
        self.reopened += other.reopened;
        self.catchup += other.catchup;
        self.catchup_overflow += other.catchup_overflow;
        self.looks_done += other.looks_done;
        self.not_promises += other.not_promises;
        for (kind, count) in &other.expired_at_birth {
            *self.expired_at_birth.entry(kind.clone()).or_default() += count;
        }
    }
}

/// Where an account's first run has got to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirstRunProgress {
    pub complete: bool,
    pub scanned: i64,
    /// The date of the oldest message classified so far.
    pub reached: Option<DateTime<Utc>>,
    pub summary: PassSummary,
    /// Messages classified in this tick, for bounded reconciliation with
    /// later cached evidence after a task is created.
    pub scanned_message_ids: Vec<MessageId>,
}

/// Classify newly synced messages. Each account's first-run state decides
/// whether a recent undated item joins the catch-up or goes straight in.
pub async fn scan_messages<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    message_ids: &[MessageId],
) -> anyhow::Result<PassSummary>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let rows = store.list_todo_scan_rows(message_ids).await?;
    let mut summary = PassSummary::default();
    let mut in_progress: HashMap<AccountId, bool> = HashMap::new();
    for row in &rows {
        let run_in_progress = match in_progress.get(&row.account_id) {
            Some(value) => *value,
            None => {
                let value = run_in_progress(store, &row.account_id).await?;
                in_progress.insert(row.account_id.clone(), value);
                value
            }
        };
        summary.add(&classify_or_skip(store, cfg, row, run_in_progress).await);
    }
    if summary.catchup > 0 {
        summary.catchup_overflow += store.trim_todo_catchup(cfg.catchup_max, cfg.now).await?;
    }
    Ok(summary)
}

async fn run_in_progress(store: &Store, account_id: &AccountId) -> anyhow::Result<bool> {
    Ok(store
        .get_todo_run(account_id)
        .await?
        .is_none_or(|run| run.rules_version != RULES_VERSION || run.completed_at.is_none()))
}

/// Classify up to `max_pages` pages of the account's history, newest
/// first, from where the run got to. Starts a run when there is none for
/// this rules version; completes it, mirroring promises, when the history
/// runs out.
pub async fn run_first_run<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    account_id: &AccountId,
    page_size: u32,
    max_pages: u32,
) -> anyhow::Result<FirstRunProgress>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let mut run = match store.get_todo_run(account_id).await? {
        Some(run) if run.rules_version == RULES_VERSION => run,
        _ => {
            store
                .start_todo_run(account_id, RULES_VERSION, cfg.now)
                .await?;
            store
                .get_todo_run(account_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("to-do run vanished after starting"))?
        }
    };
    let mut summary = PassSummary::default();
    if run.completed_at.is_some() {
        return Ok(FirstRunProgress {
            complete: true,
            scanned: run.scanned,
            reached: run.cursor.map(|(date, _)| date),
            summary,
            scanned_message_ids: Vec::new(),
        });
    }
    let mut scanned_message_ids = Vec::new();
    for _ in 0..max_pages {
        let page = store
            .list_messages_for_todo_scan(account_id, run.cursor.as_ref(), page_size)
            .await?;
        let Some(last) = page.last() else {
            summary.catchup_overflow += store.trim_todo_catchup(cfg.catchup_max, cfg.now).await?;
            store.complete_todo_run(account_id, cfg.now).await?;
            return Ok(FirstRunProgress {
                complete: true,
                scanned: run.scanned,
                reached: run.cursor.map(|(date, _)| date),
                summary,
                scanned_message_ids,
            });
        };
        let ids: Vec<_> = page.iter().map(|row| row.id.clone()).collect();
        let stored = store.asks_for_messages(account_id, &ids).await?;
        if page.iter().any(|row| !stored.contains_key(&row.id)) {
            return Ok(FirstRunProgress {
                complete: false,
                scanned: run.scanned,
                reached: run.cursor.map(|(date, _)| date),
                summary,
                scanned_message_ids,
            });
        }
        let cursor = (last.date, last.id.clone());
        let mut page_summary = PassSummary::default();
        for row in &page {
            scanned_message_ids.push(row.id.clone());
            page_summary.add(&classify_or_skip(store, cfg, row, true).await);
        }
        if page_summary.catchup > 0 {
            page_summary.catchup_overflow +=
                store.trim_todo_catchup(cfg.catchup_max, cfg.now).await?;
        }
        summary.add(&page_summary);
        store
            .advance_todo_run(account_id, &cursor, page.len() as i64)
            .await?;
        run.scanned += page.len() as i64;
        run.cursor = Some(cursor);
    }
    Ok(FirstRunProgress {
        complete: false,
        scanned: run.scanned,
        reached: run.cursor.map(|(date, _)| date),
        summary,
        scanned_message_ids,
    })
}

/// [`classify`], logging a failure and moving on, so one unreadable message
/// can't stall the first run or the post-sync scan.
async fn classify_or_skip<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    row: &TodoScanRow,
    run_in_progress: bool,
) -> PassSummary
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    match classify(store, cfg, row, run_in_progress).await {
        Ok(summary) => summary,
        Err(error) => {
            tracing::warn!(message = %row.id, %error, "to-do classification skipped a message");
            PassSummary {
                scanned: 1,
                ..PassSummary::default()
            }
        }
    }
}

/// Read one message and write what it holds: a to-do, or a confirmation
/// of one.
async fn classify<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    row: &TodoScanRow,
    run_in_progress: bool,
) -> anyhow::Result<PassSummary>
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let mut summary = PassSummary {
        scanned: 1,
        ..PassSummary::default()
    };
    let flags = MessageFlags::from_bits_truncate(row.flags);
    if flags.intersects(MessageFlags::SPAM | MessageFlags::TRASH) {
        return Ok(summary);
    }
    if row.outbound {
        summary.looks_done += offer_promises_kept(store, cfg, row).await?;
    }
    let stored = store
        .asks_for_messages(&row.account_id, std::slice::from_ref(&row.id))
        .await?;
    let Some(answer) = stored.get(&row.id) else {
        return Ok(summary);
    };
    let Some(envelope) = store.get_envelope(&row.id).await? else {
        return Ok(summary);
    };
    let mut detections = crate::asks::items::detections(answer, &envelope);
    // Calendar PARTSTAT retains its explicit pending/answered semantics.
    if !row.outbound && !detections.iter().any(|(_, d)| d.kind == TodoKind::Rsvp) {
        if let Some(invite) = invite_detection(store, row).await? {
            detections.push((usize::MAX, invite));
        }
    }
    let mut current_keys = Vec::new();
    for (_, detection) in detections {
        let (record, placement) = stored_record(cfg, row, detection, run_in_progress);
        current_keys.push(record.reason.clone());
        record_outcome(
            &mut summary,
            &record,
            placement,
            store.upsert_asks_todo(&record, answer).await?,
        );
    }
    store.withdraw_asks_todos(answer, &current_keys).await?;
    Ok(summary)
}

/// The same identity and timing projection serves backfill and new mail.
pub fn stored_record<Tz>(
    cfg: &PassConfig<Tz>,
    row: &TodoScanRow,
    detection: Detection,
    run_in_progress: bool,
) -> (TodoRecord, Placement)
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let identity = detection.object_key.clone();
    let (mut record, placement) = build_record(cfg, row, detection, run_in_progress);
    record.origin = "asks".into();
    let due_period = record.due_at.map_or_else(
        || {
            row.date
                .with_timezone(&cfg.tz)
                .format("sent:%Y-%m")
                .to_string()
        },
        |due| due.with_timezone(&cfg.tz).format("due:%Y-%m").to_string(),
    );
    let kind_identity = if matches!(record.kind.as_str(), "bill" | "payment_failed") {
        "payment"
    } else {
        record.kind.as_str()
    };
    record.dedup_key = format!(
        "asks|{kind_identity}|{}|{due_period}",
        mxr_store::assessment_fingerprint(&identity)
    );
    (record, placement)
}

fn record_outcome(
    summary: &mut PassSummary,
    record: &TodoRecord,
    placement: Placement,
    upsert: TodoUpsert,
) {
    match upsert {
        TodoUpsert::Inserted { .. } => {
            summary.created += 1;
            match placement {
                Placement::CatchUp => summary.catchup += 1,
                Placement::ExpiredAtBirth(_) => {
                    *summary
                        .expired_at_birth
                        .entry(record.kind.clone())
                        .or_default() += 1;
                }
                Placement::Open => {}
            }
        }
        TodoUpsert::Updated { reopened, .. } => {
            summary.updated += 1;
            summary.reopened += u64::from(reopened);
        }
        TodoUpsert::Unchanged => {}
    }
}

/// A calendar invite still waiting for the user's answer.
async fn invite_detection(store: &Store, row: &TodoScanRow) -> anyhow::Result<Option<Detection>> {
    let Some(calendar) = store.invite_awaiting_reply(&row.id).await? else {
        return Ok(None);
    };
    let starts = calendar.starts_at.as_deref().and_then(parse_ics_time);
    let organizer = calendar
        .organizer
        .as_ref()
        .map(|person| person.name.clone().unwrap_or_else(|| person.email.clone()));
    Ok(Some(from_invite(
        calendar.summary.as_deref(),
        organizer.as_deref(),
        &row.from_email,
        starts,
    )))
}

/// An ICS date or date-time: `20261015T180000Z`, `20261015T180000` (read
/// as UTC: no zone is carried here) or `20261015`.
pub fn parse_ics_time(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    let naive = chrono::NaiveDateTime::parse_from_str(value.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
        .ok()
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(value, "%Y%m%d")
                .ok()
                .and_then(|day| day.and_hms_opt(0, 0, 0))
        })
        .or_else(|| {
            DateTime::parse_from_rfc3339(value)
                .ok()
                .map(|at| at.naive_utc())
        })?;
    Some(Utc.from_utc_datetime(&naive))
}

fn build_record<Tz>(
    cfg: &PassConfig<Tz>,
    row: &TodoScanRow,
    detection: Detection,
    run_in_progress: bool,
) -> (TodoRecord, Placement)
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let input = TimingInput {
        kind: detection.kind,
        doc_type: detection.doc_type.as_deref(),
        arrived: row.date,
        due: detection.due.as_ref().map(|due| due.at),
        event_start: detection.event_start,
    };
    let timing = lead_time(&input, &cfg.tz, cfg.morning_hour);
    let window = window(&input);
    let placement = place(
        cfg.now,
        &window,
        input.due,
        row.date,
        cfg.catchup_days,
        run_in_progress,
    );
    let mut fields = detection.fields;
    set_timing_fields(&mut fields, &timing, &window);
    let window_source = window.until.map(|_| {
        match (detection.due.is_some(), detection.origin) {
            (true, Origin::Schema) => "schema",
            (_, Origin::Ics) => "ics",
            (true, Origin::Rule) => "rule",
            (false, _) => "default",
        }
        .to_string()
    });
    let dedup_key = dedup_key(
        &detection.kind,
        detection.sender_domain.as_deref(),
        row,
        &detection.object_key,
        detection.due.as_ref().map(|d| d.at),
        &cfg.tz,
    );
    let (state, catchup, expired_at, expired_at_birth) = placed_state(placement, cfg.now);
    let record = TodoRecord {
        id: new_todo_id(),
        account_id: row.account_id.clone(),
        thread_id: Some(row.thread_id.clone()),
        source_message_id: Some(row.id.clone()),
        source_date: Some(row.date),
        kind: detection.kind.as_str().to_string(),
        verb: detection.verb,
        doc_type: detection.doc_type,
        title: {
            let title = crate::text::strip_urls(&detection.title);
            if title.is_empty() {
                detection.title
            } else {
                title
            }
        },
        counterparty: detection.counterparty,
        sender_domain: detection.sender_domain,
        amount_minor: detection.amount.as_ref().map(|amount| amount.minor),
        currency: detection
            .amount
            .as_ref()
            .map(|amount| amount.currency.clone()),
        due_at: detection.due.as_ref().map(|due| due.at),
        due_words: detection.due.as_ref().map(|due| clip(&due.words, 120)),
        act_by_at: timing.act_by,
        surface_at: timing.surface_at,
        scheduled_for: None,
        action_url: detection.link.as_ref().map(|link| link.url.clone()),
        action_domain: detection
            .link
            .as_ref()
            .and_then(|link| registrable_domain(&link.host)),
        relevant_until: window.until,
        window_source,
        state,
        expired_at,
        expired_at_birth,
        catchup,
        looks_done_message_id: None,
        looks_done_reason: None,
        origin: detection.origin.as_str().to_string(),
        reason: detection.reason,
        field_sources: fields.to_json(),
        user_edited: false,
        commitment_id: None,
        rules_version: RULES_VERSION,
        dedup_key,
        surfaced_at: None,
        created_at: cfg.now,
        updated_at: cfg.now,
        done_at: None,
        dismissed_at: None,
    };
    (record, placement)
}

/// The state columns a new row starts with for its placement.
fn placed_state(
    placement: Placement,
    now: DateTime<Utc>,
) -> (TodoState, Option<TodoCatchup>, Option<DateTime<Utc>>, bool) {
    match placement {
        Placement::Open => (TodoState::Open, None, None, false),
        Placement::CatchUp => (TodoState::Open, Some(TodoCatchup::Pending), None, false),
        Placement::ExpiredAtBirth(_) => (TodoState::Expired, None, Some(now), true),
    }
}

fn set_timing_fields(
    fields: &mut FieldSources,
    timing: &crate::timing::Timing,
    window: &crate::timing::Window,
) {
    if timing.act_by.is_some() {
        fields.set(
            "act_by_at",
            FieldProvenance::with_evidence(FieldSource::Table, timing.act_by_rule),
        );
    }
    if timing.surface_at.is_some() {
        fields.set(
            "surface_at",
            FieldProvenance::with_evidence(FieldSource::Table, timing.surface_rule),
        );
    }
    if window.until.is_some() {
        fields.set(
            "relevant_until",
            FieldProvenance::with_evidence(FieldSource::Table, window.rule),
        );
    }
}

/// One row per thing: a reminder for the same bill (same sender, object
/// and due day) updates the row; next month's bill is a new one. Kinds
/// without a money cycle key on the conversation.
fn dedup_key<Tz>(
    kind: &TodoKind,
    sender_domain: Option<&str>,
    row: &TodoScanRow,
    object_key: &str,
    due: Option<DateTime<Utc>>,
    tz: &Tz,
) -> String
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let thread = row.thread_id.as_str();
    match kind {
        TodoKind::Bill | TodoKind::PaymentFailed | TodoKind::Renewal | TodoKind::Document => {
            let bucket = due.map_or_else(
                || row.date.with_timezone(tz).format("sent:%Y-%m").to_string(),
                |due| due.with_timezone(tz).format("due:%Y-%m-%d").to_string(),
            );
            format!(
                "{kind}|{}|{object_key}|{bucket}",
                sender_domain.unwrap_or(&thread)
            )
        }
        _ => format!("{kind}|{thread}|{object_key}"),
    }
}

/// Random rather than time-ordered, so the short form clients print (the
/// first few characters after `todo_`) is unique even for rows made in the
/// same millisecond.
pub fn new_todo_id() -> String {
    format!("todo_{}", uuid::Uuid::new_v4().simple())
}

/// A message you sent in a thread where you promised something offers the
/// promise as kept. "Sent something" isn't "kept the promise", so it only
/// offers.
async fn offer_promises_kept<Tz>(
    store: &Store,
    cfg: &PassConfig<Tz>,
    row: &TodoScanRow,
) -> anyhow::Result<u64> {
    let mut offered = 0;
    for todo in store
        .list_open_promise_todos_for_thread(&row.account_id, &row.thread_id)
        .await?
    {
        if todo.source_date.is_some_and(|source| row.date <= source) {
            continue;
        }
        let reason = format!("you wrote to them {}", row.date.format("%-d %b"));
        if store
            .set_todo_looks_done(&todo.id, &row.id, &reason, cfg.now)
            .await?
        {
            offered += 1;
        }
    }
    Ok(offered)
}

/// What one sweep did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SweepSummary {
    pub rsvp_answered: u64,
    pub expired: u64,
    /// Rows whose surface time came, claimed now and announced once.
    pub surfaced: Vec<String>,
}

/// Completes answered RSVPs, expires rows past their window, and claims
/// rows whose surface time has come.
pub async fn sweep(store: &Store, now: DateTime<Utc>) -> anyhow::Result<SweepSummary> {
    Ok(SweepSummary {
        rsvp_answered: store.complete_answered_rsvp_todos(now).await?,
        expired: store.expire_lapsed_todos(now).await?,
        surfaced: store.claim_surfaced_todos(now).await?,
    })
}
