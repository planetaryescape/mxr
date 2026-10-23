//! To do: IPC handlers, the post-sync scan and the background tick.
//!
//! Detection, lead times, windows and the first run live in `mxr-todo`;
//! this module runs them against the daemon's store and clock and serves
//! the runway. Every mutation takes `dry_run`, and the preview selects
//! rows with the same predicate the write uses, so what was previewed is
//! what changes.

use super::time::resolve_in_zone;
use super::todo_view::{already_over_label, bands, empty_state, headline, start_of_week, to_data};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Local, Utc};
use mxr_core::id::{AccountId, MessageId};
use mxr_protocol::{
    todo_copy, ResponseData, TodoCatchupData, TodoCatchupDecisionData, TodoChangeData,
    TodoEditData, TodoFirstRunData, TodoKindCountData, TodoResolutionActionData, TodoRunwayData,
    TodoStateActionData, TodoStateData,
};
use mxr_store::{
    CommitmentStatus, TodoCatchup, TodoRecord, TodoResolutionAction, TodoResolutionDates,
    TodoResolutionSnapshot, TodoResolutionStatus, TodoState,
};
use mxr_todo::pass::{self, PassConfig};
use mxr_todo::provenance::{FieldProvenance, FieldSource, FieldSources};
use mxr_todo::timing::{lead_time, window, TimingInput};
use mxr_todo::TodoKind;
use std::collections::HashMap;

/// The mode name `mode_views` records To do's last look under.
const MODE: &str = "todo";
/// The catch-up shows at most this many rows (blueprint 22: half the ~50
/// overdue tasks at which people stop opening a task app).
pub(crate) const CATCHUP_MAX: u32 = 25;
/// First-run pages classified per background tick, so a big mailbox is
/// sorted in steps rather than one long hold on the writer.
const FIRST_RUN_PAGES_PER_TICK: u32 = 1;
const FIRST_RUN_PAGE_SIZE: u32 = 100;

pub(crate) fn pass_config(state: &AppState, now: DateTime<Utc>) -> PassConfig<Local> {
    let cfg = state.config_snapshot();
    PassConfig {
        now,
        tz: Local,
        morning_hour: state.snooze_time_prefs().morning_hour,
        catchup_days: cfg.todo.catchup_days,
        catchup_max: CATCHUP_MAX,
    }
}

// ---------------------------------------------------------------------------
// Background work
// ---------------------------------------------------------------------------

/// The to-do scan as of now, for tests.
#[cfg(test)]
pub(crate) async fn scan_messages(state: &AppState, message_ids: &[MessageId]) {
    scan_messages_at(state, message_ids, Utc::now()).await;
}

/// Classify newly synced mail as of `now`. Never errors the caller; failures
/// are logged, like the delivery scan. A due date in mail is a day that ends
/// at midnight, so the full classifier eval pins `now` to grade the same mail
/// the same way at any hour.
pub(crate) async fn scan_messages_at(
    state: &AppState,
    message_ids: &[MessageId],
    now: DateTime<Utc>,
) {
    if !state.config_snapshot().todo.enabled || message_ids.is_empty() {
        return;
    }
    let cfg = pass_config(state, now);
    let envelopes = match state.store.list_envelopes_by_ids(message_ids).await {
        Ok(envelopes) => envelopes,
        Err(error) => {
            tracing::warn!(%error, "asks source lookup failed");
            return;
        }
    };
    let accounts: std::collections::HashSet<_> =
        envelopes.iter().map(|e| e.account_id.clone()).collect();
    for account in accounts {
        if let Err(error) =
            super::assessments::reconcile_messages(state, &account, message_ids).await
        {
            tracing::warn!(%error, "asks materialization pending before to-do scan");
            return;
        }
    }
    match pass::scan_messages(&state.store, &cfg, message_ids).await {
        Ok(summary) => {
            super::todo_resolution::scan_messages(state, message_ids).await;
            if summary.created + summary.updated + summary.looks_done == 0 {
                return;
            }
            tracing::info!(
                created = summary.created,
                updated = summary.updated,
                catchup = summary.catchup,
                looks_done = summary.looks_done,
                "post-sync to-do scan"
            );
        }
        Err(error) => tracing::warn!(%error, "to-do scan failed"),
    }
}

/// One background tick: advance each account's first run, mirror changed
/// promises, then sweep. Returns whether a first run is still going, so
/// the loop can come back sooner.
pub(crate) async fn tick(
    state: &AppState,
    now: DateTime<Utc>,
    _promise_fingerprints: &mut HashMap<AccountId, String>,
) -> Result<bool, HandlerError> {
    if !state.config_snapshot().todo.enabled {
        return Ok(false);
    }
    let cfg = pass_config(state, now);
    let mut running = false;
    for account in state.store.list_accounts().await? {
        if !account.enabled {
            continue;
        }
        if !super::asks_upgrade::page(state, &account.id, now).await? {
            running = true;
            continue;
        }
        let run = state.store.get_todo_run(&account.id).await?;
        if run.as_ref().is_none_or(|run| {
            run.rules_version != mxr_todo::RULES_VERSION || run.completed_at.is_none()
        }) {
            let cursor = run
                .as_ref()
                .filter(|run| run.rules_version == mxr_todo::RULES_VERSION)
                .and_then(|run| run.cursor.as_ref());
            let page = state
                .store
                .list_messages_for_todo_scan(&account.id, cursor, FIRST_RUN_PAGE_SIZE)
                .await?;
            let ids: Vec<_> = page.iter().map(|row| row.id.clone()).collect();
            if let Err(error) =
                super::assessments::reconcile_messages(state, &account.id, &ids).await
            {
                tracing::warn!(%error, "first-run asks pending");
                running = true;
                continue;
            }
        }
        // One account's failure never stops the others or the sweep.
        let progress = match pass::run_first_run(
            &state.store,
            &cfg,
            &account.id,
            FIRST_RUN_PAGE_SIZE,
            FIRST_RUN_PAGES_PER_TICK,
        )
        .await
        {
            Ok(progress) => progress,
            Err(error) => {
                tracing::warn!(account = %account.id, %error, "to-do first run failed");
                continue;
            }
        };
        if !progress.scanned_message_ids.is_empty() {
            super::todo_resolution::scan_messages(state, &progress.scanned_message_ids).await;
        }
        running |= !progress.complete;
        if progress.summary.created > 0 {
            tracing::info!(
                account = %account.id,
                complete = progress.complete,
                scanned = progress.scanned,
                created = progress.summary.created,
                catchup = progress.summary.catchup,
                expired_at_birth = progress.summary.expired_at_birth.values().sum::<u64>(),
                "to-do first run"
            );
        }
    }
    let swept = pass::sweep(&state.store, now)
        .await
        .map_err(|error| error.to_string())?;
    if swept.expired + swept.rsvp_answered > 0 || !swept.surfaced.is_empty() {
        tracing::info!(
            expired = swept.expired,
            rsvp_answered = swept.rsvp_answered,
            surfaced = swept.surfaced.len(),
            "to-do sweep"
        );
    }
    running |= super::todo_resolution::drain(state, now).await?;
    Ok(running)
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

pub(super) async fn get_runway(
    state: &AppState,
    account_id: Option<&AccountId>,
    mark_seen: bool,
) -> HandlerResult {
    Ok(ResponseData::TodoRunway {
        runway: runway_at(state, account_id, mark_seen, Utc::now()).await?,
    })
}

pub(super) async fn runway_at(
    state: &AppState,
    account_id: Option<&AccountId>,
    mark_seen: bool,
    now: DateTime<Utc>,
) -> Result<TodoRunwayData, HandlerError> {
    let records = state
        .store
        .list_runway_todos(account_id, start_of_week(now, &Local))
        .await?;
    let catchup_count = state.store.count_catchup_todos(account_id).await?;
    // Seen per scope: opening one account's To do leaves the others' counts.
    let seen_key =
        account_id.map_or_else(|| MODE.to_string(), |account| format!("{MODE}:{account}"));
    let last_seen = state.store.mode_last_viewed(&seen_key).await?;
    let expired_since = state
        .store
        .count_todos_expired_since(account_id, last_seen.unwrap_or(DateTime::UNIX_EPOCH))
        .await?;
    if mark_seen {
        state.store.set_mode_viewed(&seen_key, now).await?;
    }
    let never_had_any = records.is_empty()
        && catchup_count == 0
        && state
            .store
            .count_todos_in_state(account_id, TodoState::Done)
            .await?
            == 0;
    let bands = bands(records, now, &Local);
    let headline = headline(bands.now.len(), bands.first_now.as_ref(), now, &Local);
    let empty_state = bands
        .now
        .is_empty()
        .then(|| empty_state(never_had_any, bands.next_surface.as_ref()));
    let might_need_you = state
        .store
        .uncertain_asks(account_id)
        .await?
        .into_iter()
        .map(
            |(thread_id, subject, asks)| mxr_protocol::TodoUncertainData {
                thread_id,
                subject,
                asks: asks.summary(),
            },
        )
        .collect();
    let mut runway = TodoRunwayData {
        might_need_you,
        generated_at: now,
        header: todo_copy::HEADER.to_string(),
        headline,
        empty_state,
        now: bands.now,
        coming_up: bands.coming_up,
        later: bands.later,
        whenever: bands.whenever,
        done_this_week: bands.done_this_week,
        catchup_count: u32::try_from(catchup_count).unwrap_or(u32::MAX),
        expired_since_last_looked: u32::try_from(expired_since).unwrap_or(u32::MAX),
        next_surface: bands.next_surface,
        first_run: first_run(state, account_id).await?,
    };
    attach_resolutions(state, &mut runway.now).await?;
    attach_resolutions(state, &mut runway.later).await?;
    attach_resolutions(state, &mut runway.whenever).await?;
    attach_resolutions(state, &mut runway.done_this_week).await?;
    for week in &mut runway.coming_up {
        attach_resolutions(state, &mut week.todos).await?;
    }
    Ok(runway)
}

async fn attach_resolutions(
    state: &AppState,
    todos: &mut [mxr_protocol::TodoData],
) -> Result<(), HandlerError> {
    for todo in todos {
        todo.resolution = state
            .store
            .latest_todo_resolution(&todo.id, &todo.account_id)
            .await?
            .map(|suggestion| super::todo_resolution::resolution_data(&suggestion));
    }
    Ok(())
}

pub(super) async fn first_run(
    state: &AppState,
    account_id: Option<&AccountId>,
) -> Result<TodoFirstRunData, HandlerError> {
    let mut data = TodoFirstRunData {
        complete: true,
        scanned: 0,
        reached: None,
    };
    for account in state.store.list_accounts().await? {
        if !account.enabled || account_id.is_some_and(|id| *id != account.id) {
            continue;
        }
        match state.store.get_todo_run(&account.id).await? {
            Some(run) if run.rules_version == mxr_todo::RULES_VERSION => {
                data.complete &= run.completed_at.is_some();
                data.scanned += run.scanned;
                if let Some((date, _)) = run.cursor {
                    data.reached = Some(data.reached.map_or(date, |reached| reached.min(date)));
                }
            }
            _ => data.complete = false,
        }
    }
    Ok(data)
}

pub(super) async fn list_todos(
    state: &AppState,
    account_id: Option<&AccountId>,
    todo_state: TodoStateData,
    limit: u32,
) -> HandlerResult {
    let now = Utc::now();
    let records = state
        .store
        .list_todos_in_state(account_id, store_state(todo_state), limit)
        .await?;
    let mut todos = records
        .iter()
        .map(|record| to_data(record, now))
        .collect::<Vec<_>>();
    attach_resolutions(state, &mut todos).await?;
    Ok(ResponseData::Todos { todos })
}

pub(super) async fn get_todo(state: &AppState, raw_id: &str) -> HandlerResult {
    let id = resolve_todo_id(state, raw_id).await?;
    let record = state
        .store
        .get_todo(&id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest(format!("No to-do {raw_id}.")))?;
    let mut todo = to_data(&record, Utc::now());
    attach_resolutions(state, std::slice::from_mut(&mut todo)).await?;
    Ok(ResponseData::Todo { todo })
}

pub(super) async fn get_catchup(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    let now = Utc::now();
    let records = state.store.list_catchup_todos(account_id).await?;
    let mut todos = records
        .iter()
        .map(|record| to_data(record, now))
        .collect::<Vec<_>>();
    attach_resolutions(state, &mut todos).await?;
    let window_days = state.config_snapshot().todo.catchup_days;
    let already_over: Vec<TodoKindCountData> = state
        .store
        .count_todos_expired_at_birth_by_kind(account_id)
        .await?
        .into_iter()
        .map(|(kind, count)| TodoKindCountData {
            label: already_over_label(&kind, count),
            kind,
            count: u32::try_from(count).unwrap_or(u32::MAX),
        })
        .collect();
    let already_over_line = (!already_over.is_empty()).then(|| {
        format!(
            "Already over, so not shown: {}.",
            already_over
                .iter()
                .map(|kind| format!("{} {}", kind.count, kind.label))
                .collect::<Vec<_>>()
                .join(", ")
        )
    });
    let span = if window_days == 14 {
        "the last two weeks".to_string()
    } else {
        format!("the last {window_days} days")
    };
    let title = match todos.len() {
        0 => "Nothing to catch up on.".to_string(),
        1 => format!("Catch up: 1 thing from {span} might still need you."),
        n => format!("Catch up: {n} things from {span} might still need you."),
    };
    Ok(ResponseData::TodoCatchup {
        catchup: TodoCatchupData {
            title,
            why: todo_copy::CATCH_UP_WHY.to_string(),
            window_days,
            overflow_count: u32::try_from(state.store.count_catchup_overflow(account_id).await?)
                .unwrap_or(u32::MAX),
            todos,
            already_over,
            already_over_line,
            first_run: first_run(state, account_id).await?,
        },
    })
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

pub(super) async fn set_state(
    state: &AppState,
    raw_ids: &[String],
    action: TodoStateActionData,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let ids = resolve_todo_ids(state, raw_ids).await?;
    let (from, to, verb, done_word): (&[TodoState], TodoState, &str, &str) = match action {
        TodoStateActionData::Done => (&[TodoState::Open], TodoState::Done, "done", "Ticked off"),
        TodoStateActionData::Undo => (
            &[TodoState::Done, TodoState::Dismissed, TodoState::Expired],
            TodoState::Open,
            "undo",
            "Reopened",
        ),
        TodoStateActionData::Dismiss => (
            &[TodoState::Open, TodoState::Done, TodoState::Expired],
            TodoState::Dismissed,
            "dismiss",
            "Marked not a to-do:",
        ),
    };
    let (selected, unchanged) = select(state, &ids, |record| from.contains(&record.state)).await?;
    let changed = if dry_run {
        selected
            .into_iter()
            .map(|record| simulate_state(record, to, now))
            .collect::<Vec<_>>()
    } else {
        let selected_ids: Vec<String> = selected.iter().map(|record| record.id.clone()).collect();
        let changed_ids = state
            .store
            .set_todos_state(&selected_ids, from, to, now)
            .await?;
        for record in selected
            .iter()
            .filter(|record| changed_ids.contains(&record.id))
        {
            mirror_promise(state, record, to, now).await?;
        }
        let changed = state.store.get_todos(&changed_ids).await?;
        // A ticked-off bill is a record in Archive; reopening it unfiles
        // what the tick-off filed.
        match to {
            TodoState::Done => {
                super::records::file_ticked_todos(state, &changed).await;
            }
            _ => super::records::unfile_todos(state, &changed).await,
        }
        changed
    };
    Ok(change(dry_run, verb, done_word, &changed, unchanged, now))
}

pub(super) async fn correct(
    state: &AppState,
    raw_id: &str,
    reason: mxr_protocol::TodoCorrectionData,
    edits: &[TodoEditData],
    time_zone: Option<&str>,
    dry_run: bool,
) -> HandlerResult {
    use mxr_protocol::TodoCorrectionData as Reason;
    let now = Utc::now();
    let id = resolve_todo_id(state, raw_id).await?;
    let before = state
        .store
        .get_todo(&id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest("To-do no longer exists".into()))?;
    if (reason == Reason::WrongDetails) == edits.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "Wrong details needs saved field edits; the other corrections take no edits".into(),
        ));
    }
    let mut after = if reason == Reason::WrongDetails {
        apply_edits(state, before.clone(), edits, now, time_zone)?
    } else {
        before.clone()
    };
    match reason {
        Reason::NotATodo | Reason::NotMine => {
            after.state = TodoState::Dismissed;
            after.dismissed_at = Some(now);
        }
        Reason::AlreadyDone => {
            after.state = TodoState::Done;
            after.done_at = Some(now);
        }
        Reason::WrongDetails => {}
    }
    let token = uuid::Uuid::new_v4().to_string();
    let mut fields = FieldSources::from_json(&after.field_sources);
    fields.set(
        "correction",
        FieldProvenance::with_evidence(FieldSource::User, format!("{}:{token}", reason.as_str())),
    );
    after.field_sources = fields.to_json();
    after.user_edited = true;
    after.updated_at = now;
    let saved = state
        .store
        .correct_todo(&token, reason.as_str(), &before, &after, dry_run, now)
        .await?;
    let mut response = change(
        dry_run,
        reason.as_str(),
        "Corrected",
        &[saved],
        Vec::new(),
        now,
    );
    if let ResponseData::TodoChange { change } = &mut response {
        change.correction_undo = (!dry_run).then_some(token);
    }
    Ok(response)
}

pub(super) async fn undo_correction(
    state: &AppState,
    raw_id: &str,
    token: &str,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let id = resolve_todo_id(state, raw_id).await?;
    let record = state
        .store
        .undo_todo_correction(&id, token, dry_run, now)
        .await?;
    Ok(change(
        dry_run,
        "undo_correction",
        "Restored",
        &[record],
        Vec::new(),
        now,
    ))
}

pub(super) async fn resolve_suggestion(
    state: &AppState,
    raw_todo_id: &str,
    suggestion_id: &str,
    action: TodoResolutionActionData,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let todo_id = resolve_todo_id(state, raw_todo_id).await?;
    let record = state
        .store
        .get_todo(&todo_id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest(format!("No to-do {raw_todo_id}.")))?;
    let suggestion = state
        .store
        .latest_todo_resolution(&todo_id, &record.account_id)
        .await?
        .filter(|suggestion| suggestion.id == suggestion_id)
        .ok_or_else(|| {
            HandlerError::InvalidRequest(
                "That resolution offer is no longer current. Refresh the to-do and review the newer evidence.".into(),
            )
        })?;
    if action == TodoResolutionActionData::Accept
        && suggestion.status == TodoResolutionStatus::Accepted
    {
        return Ok(ResponseData::TodoChange {
            change: TodoChangeData {
                correction_undo: None,
                dry_run: false,
                action: "resolution_accept".into(),
                changed: Vec::new(),
                unchanged: Vec::new(),
                summary: "This resolution was already accepted; nothing changed again.".into(),
            },
        });
    }
    let expects_pending = !matches!(action, TodoResolutionActionData::Undo);
    if expects_pending != (suggestion.status == TodoResolutionStatus::Pending) {
        return Err(HandlerError::InvalidRequest(format!(
            "This resolution offer is {} and cannot be {}.",
            suggestion.status.as_str(),
            match action {
                TodoResolutionActionData::Preview => "previewed",
                TodoResolutionActionData::Accept => "accepted",
                TodoResolutionActionData::Dismiss => "dismissed",
                TodoResolutionActionData::Undo => "undone",
            }
        )));
    }
    if !state.store.validate_resolution_source(&suggestion).await?
        || !resolution_quote_is_current(state, &suggestion).await?
    {
        return Err(HandlerError::InvalidRequest(
            "The source email is no longer available; refresh this to-do.".into(),
        ));
    }
    let task_is_current = if action == TodoResolutionActionData::Undo {
        state
            .store
            .validate_todo_resolution_undo(&suggestion)
            .await?
    } else {
        state
            .store
            .validate_todo_resolution_task(&suggestion)
            .await?
    };
    if matches!(
        action,
        TodoResolutionActionData::Preview
            | TodoResolutionActionData::Accept
            | TodoResolutionActionData::Undo
    ) && !task_is_current
    {
        return Err(HandlerError::InvalidRequest(
            "The to-do changed after this offer was created. Refresh and review the current task."
                .into(),
        ));
    }
    let preview = dry_run || matches!(action, TodoResolutionActionData::Preview);
    let mut prepared_dates = None;
    if suggestion.action == TodoResolutionAction::ReviseDate {
        let due_at = suggestion.proposed_due_at.ok_or_else(|| {
            HandlerError::InvalidRequest("The proposed date is no longer valid.".into())
        })?;
        let due_words = suggestion.proposed_due_words.clone().ok_or_else(|| {
            HandlerError::InvalidRequest("The proposed date has no source words.".into())
        })?;
        let mut proposed = record.clone();
        proposed.due_at = Some(due_at);
        proposed.due_words = Some(due_words.clone());
        let mut fields = FieldSources::from_json(&proposed.field_sources);
        fields.set(
            "due_at",
            FieldProvenance::with_evidence(
                if suggestion.source_method == "model" {
                    FieldSource::Model
                } else {
                    FieldSource::Rule
                },
                suggestion.source_quote.clone(),
            ),
        );
        retime_record(
            &mut proposed,
            &mut fields,
            state.snooze_time_prefs().morning_hour,
        );
        prepared_dates = Some(TodoResolutionDates {
            due_at,
            due_words,
            act_by_at: proposed.act_by_at,
            surface_at: proposed.surface_at,
            relevant_until: proposed.relevant_until,
            field_sources: fields.to_json(),
        });
    }
    if preview {
        let mut projected = record.clone();
        let projected_status = match action {
            TodoResolutionActionData::Preview => TodoResolutionStatus::Pending,
            TodoResolutionActionData::Accept => TodoResolutionStatus::Accepted,
            TodoResolutionActionData::Dismiss => TodoResolutionStatus::Dismissed,
            TodoResolutionActionData::Undo => TodoResolutionStatus::Undone,
        };
        match action {
            TodoResolutionActionData::Preview | TodoResolutionActionData::Accept
                if suggestion.action == TodoResolutionAction::Complete =>
            {
                projected.state = TodoState::Done;
                projected.done_at = Some(now);
                projected.user_edited = true;
                projected.updated_at = now;
            }
            TodoResolutionActionData::Preview | TodoResolutionActionData::Accept => {
                if let Some(dates) = &prepared_dates {
                    apply_resolution_dates(&mut projected, dates, now);
                }
            }
            TodoResolutionActionData::Undo => {
                let snapshot: TodoResolutionSnapshot = suggestion
                    .snapshot_json
                    .as_deref()
                    .ok_or_else(|| {
                        HandlerError::InvalidRequest(
                            "The accepted resolution has no undo snapshot.".into(),
                        )
                    })
                    .and_then(|snapshot| {
                        serde_json::from_str(snapshot).map_err(|error| {
                            HandlerError::InvalidRequest(format!(
                                "The undo snapshot is invalid: {error}"
                            ))
                        })
                    })?;
                projected.due_at = snapshot.due_at;
                projected.due_words = snapshot.due_words;
                projected.act_by_at = snapshot.act_by_at;
                projected.surface_at = snapshot.surface_at;
                projected.relevant_until = snapshot.relevant_until;
                projected.window_source = snapshot.window_source;
                projected.field_sources = snapshot.field_sources;
                projected.state = snapshot.state;
                projected.done_at = snapshot.done_at;
                projected.expired_at = snapshot.expired_at;
                projected.dismissed_at = snapshot.dismissed_at;
                projected.user_edited = snapshot.user_edited;
                projected.updated_at = now;
            }
            TodoResolutionActionData::Dismiss => {}
        }
        let mut todo = to_data(&projected, now);
        let mut resolution = super::todo_resolution::resolution_data(&suggestion);
        resolution.status = projected_status.as_str().into();
        todo.resolution = Some(resolution);
        let summary = match (action, suggestion.action) {
            (TodoResolutionActionData::Undo, _) => {
                format!(
                    "Would undo the accepted resolution for “{}”. Nothing changed yet.",
                    record.title
                )
            }
            (TodoResolutionActionData::Dismiss, _) => {
                format!(
                    "Would dismiss the resolution offer for “{}”. Nothing changed yet.",
                    record.title
                )
            }
            (_, TodoResolutionAction::Complete) => {
                format!(
                    "Would complete “{}” based on the quoted email. Nothing changed yet.",
                    record.title
                )
            }
            (_, TodoResolutionAction::ReviseDate) => format!(
                "Would change the due date for “{}” from {} to {}. Nothing changed yet.",
                record.title,
                record.due_at.map_or_else(
                    || "no date".to_string(),
                    |date| date
                        .with_timezone(&Local)
                        .format("%a %-d %b %Y")
                        .to_string()
                ),
                prepared_dates
                    .as_ref()
                    .map(|dates| dates
                        .due_at
                        .with_timezone(&Local)
                        .format("%a %-d %b %Y")
                        .to_string())
                    .unwrap_or_default(),
            ),
        };
        return Ok(ResponseData::TodoChange {
            change: TodoChangeData {
                correction_undo: None,
                dry_run: true,
                action: "resolution_preview".into(),
                changed: vec![todo],
                unchanged: Vec::new(),
                summary,
            },
        });
    }
    let changed = match action {
        TodoResolutionActionData::Dismiss => {
            state
                .store
                .dismiss_todo_resolution(suggestion_id, &todo_id, &record.account_id, now)
                .await?
        }
        TodoResolutionActionData::Accept => {
            state
                .store
                .accept_todo_resolution(
                    suggestion_id,
                    &todo_id,
                    &record.account_id,
                    prepared_dates.as_ref(),
                    now,
                )
                .await?
        }
        TodoResolutionActionData::Undo => {
            state
                .store
                .undo_todo_resolution(suggestion_id, &todo_id, &record.account_id, now)
                .await?
        }
        TodoResolutionActionData::Preview => false,
    };
    if !changed {
        return Err(HandlerError::InvalidRequest(
            "The to-do changed after this offer was created. Refresh and review the current task."
                .into(),
        ));
    }
    let updated = state
        .store
        .get_todo(&todo_id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest("The to-do no longer exists.".into()))?;
    match action {
        TodoResolutionActionData::Accept if suggestion.action == TodoResolutionAction::Complete => {
            mirror_promise(state, &updated, TodoState::Done, now).await?;
            super::records::file_ticked_todos(state, std::slice::from_ref(&updated)).await;
        }
        TodoResolutionActionData::Undo if suggestion.action == TodoResolutionAction::Complete => {
            mirror_promise(state, &updated, updated.state, now).await?;
            if updated.state == TodoState::Done {
                super::records::file_ticked_todos(state, std::slice::from_ref(&updated)).await;
            } else {
                super::records::unfile_todos(state, std::slice::from_ref(&updated)).await;
            }
        }
        _ => {}
    }
    let mut todo = to_data(&updated, now);
    todo.resolution = state
        .store
        .latest_todo_resolution(&todo_id, &record.account_id)
        .await?
        .map(|current| super::todo_resolution::resolution_data(&current));
    Ok(ResponseData::TodoChange {
        change: TodoChangeData {
            correction_undo: None,
            dry_run: false,
            action: match action {
                TodoResolutionActionData::Dismiss => "resolution_dismiss",
                TodoResolutionActionData::Accept => "resolution_accept",
                TodoResolutionActionData::Undo => "resolution_undo",
                TodoResolutionActionData::Preview => "resolution_preview",
            }
            .into(),
            changed: vec![todo],
            unchanged: Vec::new(),
            summary: match action {
                TodoResolutionActionData::Dismiss => "Dismissed the resolution offer.".into(),
                TodoResolutionActionData::Accept => "Accepted the resolution offer.".into(),
                TodoResolutionActionData::Undo => "Undid the resolution offer.".into(),
                TodoResolutionActionData::Preview => unreachable!(),
            },
        },
    })
}

fn apply_resolution_dates(
    record: &mut TodoRecord,
    dates: &TodoResolutionDates,
    now: DateTime<Utc>,
) {
    record.due_at = Some(dates.due_at);
    record.due_words = Some(dates.due_words.clone());
    record.act_by_at = dates.act_by_at;
    record.surface_at = dates.surface_at;
    record.relevant_until = dates.relevant_until;
    record.window_source = Some("table".into());
    record.field_sources = dates.field_sources.clone();
    record.user_edited = true;
    record.updated_at = now;
}

async fn resolution_quote_is_current(
    state: &AppState,
    suggestion: &mxr_store::TodoResolutionSuggestion,
) -> Result<bool, HandlerError> {
    let Some(envelope) = state
        .store
        .get_envelope(&suggestion.source_message_id)
        .await?
    else {
        return Ok(false);
    };
    if envelope.account_id != suggestion.account_id || envelope.date != suggestion.source_date {
        return Ok(false);
    }
    match suggestion.quote_location.as_str() {
        "subject" => Ok(envelope.subject.contains(&suggestion.source_quote)),
        "body" => {
            let Some(body) = state.store.get_body(&suggestion.source_message_id).await? else {
                return Ok(false);
            };
            let text = mxr_reader::clean(
                body.text_plain.as_deref(),
                body.text_html.as_deref(),
                &mxr_reader::ReaderConfig::default(),
            )
            .content;
            Ok(text.contains(&suggestion.source_quote))
        }
        _ => Ok(false),
    }
}

/// A promise row's state goes back to its commitment, so the desk and
/// `mxr commitments` agree and the mirror doesn't undo the user's choice.
async fn mirror_promise(
    state: &AppState,
    record: &TodoRecord,
    to: TodoState,
    now: DateTime<Utc>,
) -> Result<(), HandlerError> {
    let Some(commitment_id) = &record.commitment_id else {
        return Ok(());
    };
    let (status, resolved_at) = match to {
        TodoState::Done => (CommitmentStatus::Resolved, Some(now)),
        TodoState::Open => (CommitmentStatus::Open, None),
        TodoState::Dismissed | TodoState::Expired => return Ok(()),
    };
    state
        .store
        .set_contact_commitment_status(&record.account_id, commitment_id, status, resolved_at)
        .await?;
    Ok(())
}

fn simulate_state(mut record: TodoRecord, to: TodoState, now: DateTime<Utc>) -> TodoRecord {
    record.state = to;
    record.updated_at = now;
    match to {
        TodoState::Done => record.done_at = Some(now),
        TodoState::Dismissed => record.dismissed_at = Some(now),
        TodoState::Expired => record.expired_at = Some(now),
        TodoState::Open => {
            record.done_at = None;
            record.dismissed_at = None;
            record.expired_at = None;
            record.user_edited = true;
            if matches!(
                record.catchup,
                Some(TodoCatchup::LetGo | TodoCatchup::Overflow)
            ) {
                record.catchup = Some(TodoCatchup::Kept);
            }
        }
    }
    record
}

pub(super) async fn schedule(
    state: &AppState,
    raw_id: &str,
    when: Option<&str>,
    time_zone: Option<&str>,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let id = resolve_todo_id(state, raw_id).await?;
    let at = match when.map(str::trim).filter(|when| !when.is_empty()) {
        Some(phrase) => Some(resolve_phrase(state, phrase, now, time_zone)?),
        None => None,
    };
    let (selected, unchanged) = select(state, std::slice::from_ref(&id), |record| {
        record.state == TodoState::Open
    })
    .await?;
    let changed = if dry_run {
        selected
            .into_iter()
            .map(|mut record| {
                record.scheduled_for = at;
                record
            })
            .collect::<Vec<_>>()
    } else {
        let mut changed = Vec::new();
        for record in selected {
            if state.store.schedule_todo(&record.id, at, now).await? {
                changed.extend(state.store.get_todo(&record.id).await?);
            }
        }
        changed
    };
    let word = if at.is_some() {
        "Scheduled"
    } else {
        "Cleared the date on"
    };
    Ok(change(dry_run, "schedule", word, &changed, unchanged, now))
}

pub(super) async fn update(
    state: &AppState,
    raw_id: &str,
    edits: &[TodoEditData],
    time_zone: Option<&str>,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let id = resolve_todo_id(state, raw_id).await?;
    let record = state
        .store
        .get_todo(&id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest(format!("No to-do {raw_id}.")))?;
    if edits.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "Say what to change, like title=\"Pay council tax\" or due=\"fri 9 oct\".".to_string(),
        ));
    }
    let edited = apply_edits(state, record, edits, now, time_zone)?;
    let changed = if dry_run {
        vec![edited]
    } else {
        state.store.update_todo_by_user(&edited, now).await?;
        state
            .store
            .get_todo(&edited.id)
            .await?
            .into_iter()
            .collect()
    };
    Ok(change(
        dry_run,
        "edit",
        "Changed",
        &changed,
        Vec::new(),
        now,
    ))
}

pub(super) fn apply_edits(
    state: &AppState,
    mut record: TodoRecord,
    edits: &[TodoEditData],
    now: DateTime<Utc>,
    time_zone: Option<&str>,
) -> Result<TodoRecord, HandlerError> {
    let mut fields = FieldSources::from_json(&record.field_sources);
    let mut retime = false;
    for edit in edits {
        let value = edit.value.trim();
        let field = edit.field.trim().to_ascii_lowercase();
        let yours = FieldProvenance::with_evidence(FieldSource::User, "you set it");
        match field.as_str() {
            "title" => {
                if value.is_empty() {
                    return Err(HandlerError::InvalidRequest(
                        "A to-do needs a title.".to_string(),
                    ));
                }
                record.title = mxr_todo::text::clip(value, 120);
                fields.set("title", yours);
            }
            "due" | "due_at" => {
                record.due_at = if value.is_empty() {
                    None
                } else {
                    Some(end_of_named_day(resolve_phrase(
                        state, value, now, time_zone,
                    )?))
                };
                record.due_words = None;
                record.window_source = Some("user".to_string());
                fields.set("due_at", yours);
                retime = true;
            }
            "amount" => {
                if value.is_empty() {
                    record.amount_minor = None;
                    record.currency = None;
                } else {
                    let amount = parse_amount(value, record.currency.as_deref())?;
                    record.amount_minor = Some(amount.0);
                    record.currency = Some(amount.1);
                }
                fields.set("amount", yours);
            }
            "counterparty" | "who" => {
                record.counterparty = (!value.is_empty()).then(|| mxr_todo::text::clip(value, 60));
                fields.set("counterparty", yours);
            }
            "kind" => {
                let kind = TodoKind::parse(value).ok_or_else(|| {
                    HandlerError::InvalidRequest(format!(
                        "Unknown kind \"{value}\". Use one of: {}.",
                        TodoKind::ALL.map(TodoKind::as_str).join(", ")
                    ))
                })?;
                record.kind = kind.as_str().to_string();
                record.verb = kind.default_verb().to_string();
                fields.set("kind", yours);
                retime = true;
            }
            other => {
                return Err(HandlerError::InvalidRequest(format!(
                    "Can't edit \"{other}\". Edit title, due, amount, counterparty or kind."
                )))
            }
        }
    }
    if retime {
        retime_record(
            &mut record,
            &mut fields,
            state.snooze_time_prefs().morning_hour,
        );
    }
    record.field_sources = fields.to_json();
    record.user_edited = true;
    record.updated_at = now;
    Ok(record)
}

/// Act-by, surface time and window again from the table, after the user
/// changed the due date or the kind.
fn retime_record(record: &mut TodoRecord, fields: &mut FieldSources, morning_hour: u8) {
    let kind = TodoKind::parse(&record.kind).unwrap_or(TodoKind::Other);
    let input = TimingInput {
        kind,
        doc_type: record.doc_type.as_deref(),
        arrived: record.source_date.unwrap_or(record.created_at),
        due: record.due_at,
        event_start: None,
    };
    let timing = lead_time(&input, &Local, morning_hour);
    let window = window(&input);
    record.act_by_at = timing.act_by;
    record.surface_at = timing.surface_at;
    record.relevant_until = window.until;
    for (field, value, rule) in [
        ("act_by_at", timing.act_by.is_some(), timing.act_by_rule),
        (
            "surface_at",
            timing.surface_at.is_some(),
            timing.surface_rule,
        ),
        ("relevant_until", window.until.is_some(), window.rule),
    ] {
        if value {
            fields.set(
                field,
                FieldProvenance::with_evidence(FieldSource::Table, rule),
            );
        } else {
            fields.0.remove(field);
        }
    }
}

pub(super) async fn create(
    state: &AppState,
    message_id: &MessageId,
    title: &str,
    kind: Option<&str>,
    due: Option<&str>,
    time_zone: Option<&str>,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    let title = title.trim();
    if title.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "Say what to do, like --title \"Send the signed form\".".to_string(),
        ));
    }
    let kind = match kind.map(str::trim).filter(|kind| !kind.is_empty()) {
        Some(kind) => TodoKind::parse(kind).ok_or_else(|| {
            HandlerError::InvalidRequest(format!(
                "Unknown kind \"{kind}\". Use one of: {}.",
                TodoKind::ALL.map(TodoKind::as_str).join(", ")
            ))
        })?,
        None => TodoKind::Other,
    };
    let row = state
        .store
        .list_todo_scan_rows(std::slice::from_ref(message_id))
        .await?
        .pop()
        .ok_or_else(|| HandlerError::InvalidRequest(format!("No message {message_id}.")))?;
    let counterparty = if row.outbound {
        state
            .store
            .get_envelope(message_id)
            .await?
            .and_then(|envelope| envelope.to.into_iter().next())
            .map(|to| to.name.unwrap_or(to.email))
    } else {
        Some(
            row.from_name
                .clone()
                .unwrap_or_else(|| row.from_email.clone()),
        )
    };
    let due_at = match due.map(str::trim).filter(|due| !due.is_empty()) {
        Some(phrase) => Some(end_of_named_day(resolve_phrase(
            state, phrase, now, time_zone,
        )?)),
        None => None,
    };
    let mut fields = FieldSources::default();
    let yours = || FieldProvenance::with_evidence(FieldSource::User, "you set it");
    fields.set("title", yours());
    fields.set("kind", yours());
    if due_at.is_some() {
        fields.set("due_at", yours());
    }
    let id = pass::new_todo_id();
    let mut record = TodoRecord {
        id: id.clone(),
        account_id: row.account_id.clone(),
        thread_id: Some(row.thread_id.clone()),
        source_message_id: Some(row.id.clone()),
        source_date: Some(row.date),
        kind: kind.as_str().to_string(),
        verb: title
            .split_whitespace()
            .next()
            .unwrap_or_else(|| kind.default_verb())
            .to_ascii_lowercase(),
        doc_type: None,
        title: mxr_todo::text::clip(title, 120),
        counterparty,
        sender_domain: mxr_todo::action_link::email_domain(&row.from_email),
        amount_minor: None,
        currency: None,
        due_at,
        due_words: None,
        act_by_at: None,
        surface_at: None,
        scheduled_for: None,
        action_url: None,
        action_domain: None,
        relevant_until: None,
        window_source: due_at.map(|_| "user".to_string()),
        state: TodoState::Open,
        expired_at: None,
        expired_at_birth: false,
        catchup: None,
        looks_done_message_id: None,
        looks_done_reason: None,
        origin: "manual".to_string(),
        reason: "you added it".to_string(),
        field_sources: String::new(),
        user_edited: true,
        commitment_id: None,
        rules_version: mxr_todo::RULES_VERSION,
        dedup_key: format!("manual|{id}"),
        surfaced_at: None,
        created_at: now,
        updated_at: now,
        done_at: None,
        dismissed_at: None,
    };
    retime_record(
        &mut record,
        &mut fields,
        state.snooze_time_prefs().morning_hour,
    );
    record.field_sources = fields.to_json();
    if !dry_run {
        state.store.insert_todo(&record).await?;
    }
    Ok(change(
        dry_run,
        "create",
        "Added",
        &[record],
        Vec::new(),
        now,
    ))
}

/// What a catch-up decision does to the rows it selects.
#[derive(Clone, Copy)]
enum Choice {
    Keep,
    LetGo,
    Undecide,
}

pub(super) async fn set_catchup(
    state: &AppState,
    account_id: Option<&AccountId>,
    decision: &TodoCatchupDecisionData,
    dry_run: bool,
) -> HandlerResult {
    let now = Utc::now();
    // The account the request is scoped to bounds the ids it may touch.
    let pending = |record: &TodoRecord| {
        record.state == TodoState::Open
            && record.catchup == Some(TodoCatchup::Pending)
            && account_id.is_none_or(|account| record.account_id == *account)
    };
    let decided = |record: &TodoRecord| {
        matches!(
            (record.state, record.catchup),
            (TodoState::Open, Some(TodoCatchup::Kept))
                | (TodoState::Expired, Some(TodoCatchup::LetGo))
        ) && account_id.is_none_or(|account| record.account_id == *account)
    };
    let (choice, (selected, unchanged)) = match decision {
        TodoCatchupDecisionData::Keep { todo_ids } => {
            let ids = resolve_todo_ids(state, todo_ids).await?;
            (Choice::Keep, select(state, &ids, pending).await?)
        }
        TodoCatchupDecisionData::LetGo { todo_ids } => {
            let ids = resolve_todo_ids(state, todo_ids).await?;
            (Choice::LetGo, select(state, &ids, pending).await?)
        }
        TodoCatchupDecisionData::LetGoAll => (
            Choice::LetGo,
            (
                state.store.list_catchup_todos(account_id).await?,
                Vec::new(),
            ),
        ),
        TodoCatchupDecisionData::Undecide { todo_ids } => {
            let ids = resolve_todo_ids(state, todo_ids).await?;
            (Choice::Undecide, select(state, &ids, decided).await?)
        }
    };
    let changed = if dry_run {
        selected
            .into_iter()
            .map(|mut record| {
                match choice {
                    Choice::Keep => {
                        record.catchup = Some(TodoCatchup::Kept);
                        record.user_edited = true;
                    }
                    Choice::LetGo => {
                        record.catchup = Some(TodoCatchup::LetGo);
                        record.state = TodoState::Expired;
                        record.expired_at = Some(now);
                    }
                    Choice::Undecide => {
                        record.catchup = Some(TodoCatchup::Pending);
                        record.state = TodoState::Open;
                        record.expired_at = None;
                    }
                }
                record
            })
            .collect::<Vec<_>>()
    } else {
        let ids: Vec<String> = selected.iter().map(|record| record.id.clone()).collect();
        let changed_ids = match choice {
            Choice::Keep => state.store.keep_catchup_todos(&ids, now).await?,
            Choice::LetGo => state.store.let_go_catchup_todos(&ids, now).await?,
            Choice::Undecide => state.store.undecide_catchup_todos(&ids, now).await?,
        };
        state.store.get_todos(&changed_ids).await?
    };
    let (verb, word) = match choice {
        Choice::Keep => ("keep", "Kept"),
        Choice::LetGo => ("let_go", "Let go of"),
        Choice::Undecide => ("undecide", "Back in the catch-up:"),
    };
    Ok(change(dry_run, verb, word, &changed, unchanged, now))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// The rows among `ids` that `applies` to, and the ids it doesn't. One
/// predicate for the preview and the write, so they select the same rows.
async fn select(
    state: &AppState,
    ids: &[String],
    applies: impl Fn(&TodoRecord) -> bool,
) -> Result<(Vec<TodoRecord>, Vec<String>), HandlerError> {
    let mut records = state.store.get_todos(ids).await?;
    records.sort_by_key(|record| ids.iter().position(|id| *id == record.id));
    let (selected, skipped): (Vec<_>, Vec<_>) =
        records.into_iter().partition(|record| applies(record));
    Ok((
        selected,
        skipped.into_iter().map(|record| record.id).collect(),
    ))
}

fn change(
    dry_run: bool,
    action: &str,
    word: &str,
    changed: &[TodoRecord],
    unchanged: Vec<String>,
    now: DateTime<Utc>,
) -> ResponseData {
    let count = changed.len();
    let things = if count == 1 {
        "1 to-do".to_string()
    } else {
        format!("{count} to-dos")
    };
    let summary = if dry_run {
        format!(
            "Would change {things} ({}). Nothing changed yet.",
            word.trim_end_matches(':').to_lowercase()
        )
    } else {
        format!("{word} {things}.")
    };
    ResponseData::TodoChange {
        change: TodoChangeData {
            correction_undo: None,
            dry_run,
            action: action.to_string(),
            changed: changed.iter().map(|record| to_data(record, now)).collect(),
            unchanged,
            summary,
        },
    }
}

/// A full id, or a prefix of one that matches exactly one row.
pub(super) async fn resolve_todo_id(state: &AppState, raw: &str) -> Result<String, HandlerError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(HandlerError::InvalidRequest("Give a to-do id.".to_string()));
    }
    if state.store.get_todo(raw).await?.is_some() {
        return Ok(raw.to_string());
    }
    let prefixed;
    let prefix = if raw.starts_with("todo_") {
        raw
    } else {
        prefixed = format!("todo_{raw}");
        &prefixed
    };
    let matches = state.store.find_todo_ids_by_prefix(prefix, 2).await?;
    match matches.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err(HandlerError::InvalidRequest(format!(
            "No to-do matches {raw}."
        ))),
        _ => Err(HandlerError::InvalidRequest(format!(
            "{raw} matches more than one to-do; use more of the id."
        ))),
    }
}

pub(super) async fn resolve_todo_ids(
    state: &AppState,
    raw: &[String],
) -> Result<Vec<String>, HandlerError> {
    if raw.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "Give at least one to-do id.".to_string(),
        ));
    }
    let mut ids = Vec::with_capacity(raw.len());
    for id in raw {
        let id = resolve_todo_id(state, id).await?;
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    Ok(ids)
}

pub(super) fn resolve_phrase(
    state: &AppState,
    phrase: &str,
    now: DateTime<Utc>,
    time_zone: Option<&str>,
) -> Result<DateTime<Utc>, HandlerError> {
    match resolve_in_zone(state, phrase, Some(now), time_zone)? {
        Ok(resolution) => Ok(resolution.at),
        Err(error) => Err(HandlerError::InvalidRequest(error.message)),
    }
}

/// A due date means the whole named day.
fn end_of_named_day(at: DateTime<Utc>) -> DateTime<Utc> {
    mxr_todo::dates::end_of_day(at.with_timezone(&Local).date_naive(), &Local)
}

fn parse_amount(value: &str, current: Option<&str>) -> Result<(i64, String), HandlerError> {
    if let Some((_, amount)) = mxr_todo::money::find_amounts(value).into_iter().next() {
        return Ok((amount.minor, amount.currency));
    }
    let currency = current.ok_or_else(|| {
        HandlerError::InvalidRequest("Say the currency too, like £142.00.".to_string())
    })?;
    let number: f64 = value.replace(',', "").parse().map_err(|_| {
        HandlerError::InvalidRequest(format!("\"{value}\" isn't an amount, like £142.00."))
    })?;
    if !number.is_finite() || number <= 0.0 {
        return Err(HandlerError::InvalidRequest(format!(
            "\"{value}\" isn't an amount, like £142.00."
        )));
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a money amount rounded to minor units"
    )]
    let minor = (number * 100.0).round() as i64;
    Ok((minor, currency.to_string()))
}

fn store_state(state: TodoStateData) -> TodoState {
    match state {
        TodoStateData::Open => TodoState::Open,
        TodoStateData::Done => TodoState::Done,
        TodoStateData::Dismissed => TodoState::Dismissed,
        TodoStateData::Expired => TodoState::Expired,
    }
}

/// The accounts the given to-dos belong to, for the account allowlist.
pub(super) async fn todo_accounts(
    state: &AppState,
    raw_ids: &[String],
) -> Result<Vec<AccountId>, HandlerError> {
    let ids = resolve_todo_ids(state, raw_ids).await?;
    let mut accounts = Vec::new();
    for record in state.store.get_todos(&ids).await? {
        if !accounts.contains(&record.account_id) {
            accounts.push(record.account_id);
        }
    }
    Ok(accounts)
}
