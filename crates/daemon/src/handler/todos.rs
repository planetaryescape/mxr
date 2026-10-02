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
    TodoEditData, TodoFirstRunData, TodoKindCountData, TodoRunwayData, TodoStateActionData,
    TodoStateData,
};
use mxr_store::{CommitmentStatus, TodoCatchup, TodoRecord, TodoState};
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
const FIRST_RUN_PAGES_PER_TICK: u32 = 20;
const FIRST_RUN_PAGE_SIZE: u32 = 500;

pub(crate) async fn pass_config(
    state: &AppState,
    now: DateTime<Utc>,
) -> Result<PassConfig<Local>, HandlerError> {
    let cfg = state.config_snapshot();
    Ok(PassConfig {
        now,
        tz: Local,
        morning_hour: state.snooze_time_prefs().morning_hour,
        catchup_days: cfg.todo.catchup_days,
        catchup_max: CATCHUP_MAX,
        trusted_authserv: trusted_authserv(state, &cfg).await?,
    })
}

/// The authserv-ids each account's provider stamps on the
/// `Authentication-Results` it adds. Gmail (over its API or IMAP) is
/// `mx.google.com`; other IMAP providers come from
/// `[todo] trusted_authserv_ids`; Outlook's results carry no authserv-id,
/// so its accounts get none and their pay links are never one click.
async fn trusted_authserv(
    state: &AppState,
    cfg: &mxr_config::MxrConfig,
) -> Result<HashMap<AccountId, Vec<String>>, HandlerError> {
    use mxr_config::SyncProviderConfig;
    use mxr_core::types::ProviderKind;
    const GOOGLE: &str = "mx.google.com";
    let mut trusted = HashMap::new();
    for account in state.store.list_accounts().await? {
        let Some(backend) = account.sync_backend else {
            continue;
        };
        let ids = match backend.provider_kind {
            // The fake provider stands in for Gmail in the demo and tests.
            ProviderKind::Gmail | ProviderKind::Fake => vec![GOOGLE.to_string()],
            ProviderKind::Imap => {
                let gmail_host = matches!(
                    cfg.accounts.get(&backend.config_key).and_then(|account| account.sync.as_ref()),
                    Some(SyncProviderConfig::Imap { host, .. })
                        if host.ends_with("gmail.com") || host.ends_with("googlemail.com")
                );
                let mut ids = cfg.todo.trusted_authserv_ids.clone();
                if gmail_host {
                    ids.push(GOOGLE.to_string());
                }
                ids
            }
            ProviderKind::Smtp | ProviderKind::OutlookPersonal | ProviderKind::OutlookWork => {
                Vec::new()
            }
        };
        trusted.insert(account.id, ids);
    }
    Ok(trusted)
}

// ---------------------------------------------------------------------------
// Background work
// ---------------------------------------------------------------------------

/// Classify newly synced mail. Never errors the caller; failures are
/// logged, like the delivery scan.
pub(crate) async fn scan_messages(state: &AppState, message_ids: &[MessageId]) {
    if !state.config_snapshot().todo.enabled || message_ids.is_empty() {
        return;
    }
    let cfg = match pass_config(state, Utc::now()).await {
        Ok(cfg) => cfg,
        Err(error) => {
            tracing::warn!(%error, "to-do scan skipped: no account trust settings");
            return;
        }
    };
    match pass::scan_messages(&state.store, &cfg, message_ids).await {
        Ok(summary) if summary.created + summary.updated + summary.looks_done > 0 => {
            tracing::info!(
                created = summary.created,
                updated = summary.updated,
                catchup = summary.catchup,
                looks_done = summary.looks_done,
                "post-sync to-do scan"
            );
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "to-do scan failed"),
    }
}

/// One background tick: advance each account's first run, mirror changed
/// promises, then sweep. Returns whether a first run is still going, so
/// the loop can come back sooner.
pub(crate) async fn tick(
    state: &AppState,
    now: DateTime<Utc>,
    promise_fingerprints: &mut HashMap<AccountId, String>,
) -> Result<bool, HandlerError> {
    if !state.config_snapshot().todo.enabled {
        return Ok(false);
    }
    let cfg = pass_config(state, now).await?;
    let mut running = false;
    for account in state.store.list_accounts().await? {
        if !account.enabled {
            continue;
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
        let fingerprint = state.store.promise_fingerprint(&account.id).await?;
        if promise_fingerprints.get(&account.id) != Some(&fingerprint) {
            match pass::sync_promises(&state.store, &cfg, &account.id, !progress.complete).await {
                Ok(_) => {
                    promise_fingerprints.insert(account.id.clone(), fingerprint);
                }
                Err(error) => {
                    tracing::warn!(account = %account.id, %error, "to-do promise mirror failed");
                }
            }
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
    Ok(TodoRunwayData {
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
    })
}

async fn first_run(
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
    let todos = state
        .store
        .list_todos_in_state(account_id, store_state(todo_state), limit)
        .await?
        .iter()
        .map(|record| to_data(record, now))
        .collect();
    Ok(ResponseData::Todos { todos })
}

pub(super) async fn get_todo(state: &AppState, raw_id: &str) -> HandlerResult {
    let id = resolve_todo_id(state, raw_id).await?;
    let record = state
        .store
        .get_todo(&id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest(format!("No to-do {raw_id}.")))?;
    Ok(ResponseData::Todo {
        todo: to_data(&record, Utc::now()),
    })
}

pub(super) async fn get_catchup(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    let now = Utc::now();
    let todos: Vec<_> = state
        .store
        .list_catchup_todos(account_id)
        .await?
        .iter()
        .map(|record| to_data(record, now))
        .collect();
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
        state.store.get_todos(&changed_ids).await?
    };
    Ok(change(dry_run, verb, done_word, &changed, unchanged, now))
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

fn apply_edits(
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
        action_trusted: false,
        action_gate: None,
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
    let (keep, (selected, unchanged)) = match decision {
        TodoCatchupDecisionData::Keep { todo_ids } => {
            let ids = resolve_todo_ids(state, todo_ids).await?;
            (true, select(state, &ids, pending).await?)
        }
        TodoCatchupDecisionData::LetGo { todo_ids } => {
            let ids = resolve_todo_ids(state, todo_ids).await?;
            (false, select(state, &ids, pending).await?)
        }
        TodoCatchupDecisionData::LetGoAll => (
            false,
            (
                state.store.list_catchup_todos(account_id).await?,
                Vec::new(),
            ),
        ),
    };
    let changed = if dry_run {
        selected
            .into_iter()
            .map(|mut record| {
                if keep {
                    record.catchup = Some(TodoCatchup::Kept);
                    record.user_edited = true;
                } else {
                    record.catchup = Some(TodoCatchup::LetGo);
                    record.state = TodoState::Expired;
                    record.expired_at = Some(now);
                }
                record
            })
            .collect::<Vec<_>>()
    } else {
        let ids: Vec<String> = selected.iter().map(|record| record.id.clone()).collect();
        let changed_ids = if keep {
            state.store.keep_catchup_todos(&ids, now).await?
        } else {
            state.store.let_go_catchup_todos(&ids, now).await?
        };
        state.store.get_todos(&changed_ids).await?
    };
    let (verb, word) = if keep {
        ("keep", "Kept")
    } else {
        ("let_go", "Let go of")
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

fn resolve_phrase(
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
