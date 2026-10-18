//! Explicit task links plus field-wise two-way reconciliation. Only linked
//! tasks participate. No whole-list import, task deletion, or email-body export.

use super::microsoft_todo_cli::{MicrosoftTodoCli, INSTALL_HINT, INSTALL_URL};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{Local, NaiveDate, TimeZone, Utc};
use mxr_protocol::{
    MicrosoftTodoSendData, MicrosoftTodoStatusData, MicrosoftTodoSyncData,
    MicrosoftTodoSyncItemData, ResponseData, TodoEditData,
};
use mxr_store::{TodoMicrosoftExport, TodoRecord, TodoState};
use mxr_todo::linked::{self, LinkedTodoFields};
use mxr_todo::provenance::{FieldProvenance, FieldSource, FieldSources};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use tokio::sync::Mutex;

fn gate() -> &'static Mutex<()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(Mutex::default)
}

pub(super) fn status() -> ResponseData {
    ResponseData::MicrosoftTodoStatus {
        status: MicrosoftTodoStatusData {
            installed: MicrosoftTodoCli::discover().is_some(),
            install_url: INSTALL_URL.into(),
            hint: INSTALL_HINT.into(),
        },
    }
}

#[derive(Serialize, Deserialize)]
struct Payload {
    fields: LinkedTodoFields,
    notes: String,
}

fn payload(record: &TodoRecord) -> Payload {
    let mut notes = format!(
        "From Mixer\n{}\n\nMixer to-do: {}",
        record.reason, record.id
    );
    if let Some(counterparty) = &record.counterparty {
        notes.push_str(&format!("\nFor: {counterparty}"));
    }
    if let Some(message) = &record.source_message_id {
        notes.push_str(&format!("\nSource email ID: {message}"));
    }
    if let Some(url) = &record.action_url {
        notes.push_str(&format!("\nLink from the email (not verified): {url}"));
    }
    Payload {
        fields: LinkedTodoFields::from_record(record, &Local),
        notes,
    }
}

fn fingerprint(payload: &Payload) -> Result<String, HandlerError> {
    Ok(Sha256::digest(serde_json::to_vec(payload)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn reply(id: &str, payload: Payload, dry_run: bool) -> Result<MicrosoftTodoSendData, HandlerError> {
    Ok(MicrosoftTodoSendData {
        dry_run, todo_id: id.into(), fingerprint: fingerprint(&payload)?,
        title: payload.fields.title, due: payload.fields.due, scheduled: payload.fields.scheduled,
        notes: payload.notes, state: "preview".into(), task_id: None,
        summary: "Link this task with Microsoft To Do. Title, due date, schedule and completion will sync both ways. Microsoft-only fields are preserved.".into(),
    })
}

pub(super) async fn link(
    state: &AppState,
    raw_id: &str,
    account_id: Option<&mxr_core::AccountId>,
    dry_run: bool,
    expected: Option<&str>,
) -> HandlerResult {
    if let Some(account_id) = account_id {
        let id = super::todos::resolve_todo_id(state, raw_id).await?;
        let todo = state
            .store
            .get_todo(&id)
            .await?
            .ok_or_else(|| HandlerError::InvalidRequest("No such to-do.".into()))?;
        if todo.account_id != *account_id {
            return Err(HandlerError::InvalidRequest(
                "This to-do does not belong to the selected account.".into(),
            ));
        }
    }
    let cli = MicrosoftTodoCli::discover();
    link_with_cli(state, raw_id, dry_run, expected, cli.as_ref()).await
}

async fn link_with_cli(
    state: &AppState,
    raw_id: &str,
    dry_run: bool,
    expected: Option<&str>,
    cli: Option<&MicrosoftTodoCli>,
) -> HandlerResult {
    let id = super::todos::resolve_todo_id(state, raw_id).await?;
    let record = state
        .store
        .get_todo(&id)
        .await?
        .ok_or_else(|| HandlerError::InvalidRequest("No such to-do.".into()))?;
    let current = payload(&record);
    let mut result = reply(&id, current, dry_run)?;
    if let Some(saved) = state.store.todo_microsoft_export(&id).await? {
        result.task_id = saved.task_id;
        result.state = if saved.state == "accepted" {
            if saved.last_error.is_some() {
                "attention"
            } else {
                "linked"
            }
        } else {
            "unknown"
        }
        .into();
        result.summary = saved.last_error.unwrap_or_else(|| {
            if saved.state == "accepted" { "Linked to Microsoft To Do. Shared fields sync both ways.".into() }
            else { "The handoff is unconfirmed. Check `ms-todo outbox list --format json`; do not create another task.".into() }
        });
        return Ok(ResponseData::MicrosoftTodoSend { result });
    }
    if record.state != TodoState::Open {
        return Err(HandlerError::InvalidRequest(
            "Reopen this to-do before linking it.".into(),
        ));
    }
    if dry_run {
        return Ok(ResponseData::MicrosoftTodoSend { result });
    }
    let cli = cli.ok_or_else(|| HandlerError::InvalidRequest(INSTALL_HINT.into()))?;
    if expected != Some(result.fingerprint.as_str()) {
        return Err(HandlerError::InvalidRequest(
            "Preview this exact task again before linking: its fields may have changed.".into(),
        ));
    }
    state.ensure_effects_allowed()?;
    cli.check_compatible()
        .await
        .map_err(|error| HandlerError::InvalidRequest(error.to_string()))?;
    let outbound = payload(&record);
    let json = serde_json::to_string(&outbound)?;
    if !state
        .store
        .claim_todo_microsoft_export(&record, &json)
        .await?
    {
        return Err(HandlerError::InvalidRequest(
            "This task changed or already has a handoff in progress. Refresh its preview and link status.".into(),
        ));
    }
    let key = format!("mxr:todo:{}:{id}", record.account_id);
    match cli.add(&outbound.fields, &outbound.notes, &key).await {
        Ok((operation_id, task)) => {
            if task
                .fields()
                .map_err(|e| HandlerError::Message(e.to_string()))?
                != outbound.fields
            {
                result.state = "unknown".into();
                result.summary = "ms-todo returned a different task shape. Check its outbox before linking again.".into();
            } else {
                state
                    .store
                    .accept_todo_microsoft_export(
                        &id,
                        &task.id,
                        &operation_id,
                        &serde_json::to_string(&outbound.fields)?,
                    )
                    .await?;
                // Linking is an explicit user decision: reclassification and
                // ageing must not erase the local half of a live task link.
                result.state = "linked".into();
                result.task_id = Some(task.id);
                result.summary = "Linked. Changes sync both ways; ms-todo queues Microsoft updates while offline.".into();
            }
        }
        Err(error) if error.definitely_not_applied() => {
            state.store.release_todo_microsoft_export(&id).await?;
            return Err(HandlerError::InvalidRequest(error.to_string()));
        }
        Err(error) => {
            result.state = "unknown".into();
            result.summary = error.to_string();
        }
    }
    Ok(ResponseData::MicrosoftTodoSend { result })
}

pub(super) async fn sync(
    state: &AppState,
    raw_id: Option<&str>,
    accounts: Option<&[mxr_core::AccountId]>,
    dry_run: bool,
) -> HandlerResult {
    let _guard = gate().lock().await;
    let links = select_links(state, raw_id, accounts).await?;
    if links.is_empty() {
        return Ok(ResponseData::MicrosoftTodoSync {
            sync: MicrosoftTodoSyncData {
                dry_run,
                results: Vec::new(),
            },
        });
    }
    let cli = MicrosoftTodoCli::discover()
        .ok_or_else(|| HandlerError::InvalidRequest(INSTALL_HINT.into()))?;
    sync_links(state, &cli, links, dry_run).await
}

async fn select_links(
    state: &AppState,
    raw_id: Option<&str>,
    accounts: Option<&[mxr_core::AccountId]>,
) -> Result<Vec<TodoMicrosoftExport>, HandlerError> {
    match raw_id {
        Some(raw_id) => {
            let id = super::todos::resolve_todo_id(state, raw_id).await?;
            let todo = state
                .store
                .get_todo(&id)
                .await?
                .ok_or_else(|| HandlerError::InvalidRequest("No such to-do.".into()))?;
            if accounts.is_some_and(|accounts| !accounts.contains(&todo.account_id)) {
                return Err(HandlerError::InvalidRequest(
                    "This to-do does not belong to the selected accounts.".into(),
                ));
            }
            Ok(vec![state
                .store
                .todo_microsoft_export(&id)
                .await?
                .ok_or_else(|| {
                    HandlerError::InvalidRequest(
                        "This to-do is not linked to Microsoft To Do.".into(),
                    )
                })?])
        }
        None => Ok(state
            .store
            .list_todo_microsoft_exports(false, accounts)
            .await?),
    }
}

async fn sync_links(
    state: &AppState,
    cli: &MicrosoftTodoCli,
    links: Vec<TodoMicrosoftExport>,
    dry_run: bool,
) -> HandlerResult {
    if !dry_run {
        state.ensure_effects_allowed()?;
        cli.refresh()
            .await
            .map_err(|e| HandlerError::Message(e.to_string()))?;
    }
    let mut results = Vec::new();
    for link in links {
        match sync_one(state, cli, &link, dry_run).await {
            Ok(result) => results.push(result),
            Err(error) => {
                let result = MicrosoftTodoSyncItemData {
                    todo_id: link.todo_id,
                    state: "paused".into(),
                    summary: error.to_string(),
                    mixer: None,
                    microsoft: None,
                    merged: None,
                };
                results.push(finish(state, result, dry_run, None).await?);
            }
        }
    }
    Ok(ResponseData::MicrosoftTodoSync {
        sync: MicrosoftTodoSyncData { dry_run, results },
    })
}

/// Called by To do's background tick. Installation is optional; absent or
/// unavailable credentials never hold up email sync or other To do work.
pub(crate) async fn background(state: &AppState) {
    let Ok(_guard) = gate().try_lock() else {
        return;
    };
    let Some(cli) = MicrosoftTodoCli::discover() else {
        return;
    };
    if state.ensure_effects_allowed().is_err() {
        return;
    }
    let links = match state.store.list_todo_microsoft_exports(true, None).await {
        Ok(links) if !links.is_empty() => links,
        _ => return,
    };
    if let Err(error) = cli.refresh().await {
        for link in &links {
            let _ = state
                .store
                .note_todo_microsoft_sync(&link.todo_id, None, Some(&error.to_string()))
                .await;
        }
        return;
    }
    // One failing link never stops the rest. The store's attempt time gives
    // each row a retry interval, including conflicts and unavailable peers.
    for link in links.into_iter().take(8) {
        let _ = state
            .store
            .note_todo_microsoft_sync(
                &link.todo_id,
                None,
                Some("Checking the linked Microsoft task…"),
            )
            .await;
        if let Err(error) = sync_one(state, &cli, &link, false).await {
            let _ = state
                .store
                .note_todo_microsoft_sync(&link.todo_id, None, Some(&error.to_string()))
                .await;
        }
    }
}

async fn sync_one(
    state: &AppState,
    cli: &MicrosoftTodoCli,
    link: &TodoMicrosoftExport,
    dry_run: bool,
) -> Result<MicrosoftTodoSyncItemData, HandlerError> {
    let mut result = MicrosoftTodoSyncItemData {
        todo_id: link.todo_id.clone(),
        state: "paused".into(),
        summary: "This handoff needs review before synchronization can resume.".into(),
        mixer: None,
        microsoft: None,
        merged: None,
    };
    let (Some(task_id), Some(base)) = (&link.task_id, &link.baseline_json) else {
        return Ok(result);
    };
    let base: LinkedTodoFields = serde_json::from_str(base)?;
    let remote = match cli.show(task_id).await {
        Ok(remote) => remote,
        Err(error) => {
            result.summary = format!(
                "{}. The Mixer task is unchanged; no replacement task will be created.",
                error
            );
            return finish(state, result, dry_run, None).await;
        }
    };
    if !matches!(remote.sync_state.as_str(), "synced" | "pending") {
        result.summary = "Microsoft To Do has an unresolved write. Check `ms-todo outbox list --format json`; neither task was changed.".into();
        return finish(state, result, dry_run, None).await;
    }
    if remote.recurrence.is_some() {
        result.summary = "This Microsoft task repeats, which Mixer cannot represent. Sync is paused rather than advancing the wrong occurrence.".into();
        return finish(state, result, dry_run, None).await;
    }
    let peer = remote
        .fields()
        .map_err(|e| HandlerError::Message(e.to_string()))?;
    if let Some(uncertain) = &link.uncertain_json {
        let uncertain: LinkedTodoFields = serde_json::from_str(uncertain)?;
        if peer != uncertain {
            result.summary = "A previous sync write is unconfirmed. Check ms-todo's outbox and resolve the shared fields there before resuming; it will not be sent again.".into();
            return finish(state, result, dry_run, None).await;
        }
    }
    let record = state.store.get_todo(&link.todo_id).await?.ok_or_else(|| {
        HandlerError::InvalidRequest("The linked Mixer task no longer exists.".into())
    })?;
    let local = LinkedTodoFields::from_record(&record, &Local);
    result.mixer = Some(wire_fields(&local));
    result.microsoft = Some(wire_fields(&peer));
    let desired = match linked::merge(&base, &local, &peer) {
        Ok(desired) => desired,
        Err(fields) => {
            result.state = "conflict".into();
            result.summary = format!("Changed in both apps: {}. Choose the same value in either app to resume; neither version was overwritten.", fields.join(", "));
            return finish(state, result, dry_run, None).await;
        }
    };
    // Validate the complete inbound plan before an outbound change can happen.
    let edited = local_update(state, &record, &local, &desired)?;
    result.merged = Some(wire_fields(&desired));
    if dry_run {
        result.state = if local == desired && peer == desired {
            "synced"
        } else {
            "changes"
        }
        .into();
        result.summary = if result.state == "synced" {
            "Both tasks agree.".into()
        } else {
            "Shared-field changes would synchronize both ways; Microsoft-only fields stay untouched.".into()
        };
        return Ok(result);
    }
    if peer != desired {
        state
            .store
            .reserve_todo_microsoft_sync(&link.todo_id, Some(&serde_json::to_string(&desired)?))
            .await?;
        if let Err(error) = cli.apply(task_id, &peer, &desired).await {
            if error.definitely_not_applied() {
                state
                    .store
                    .reserve_todo_microsoft_sync(&link.todo_id, None)
                    .await?;
            }
            result.summary = format!("Sync is paused: {error}");
            return finish(state, result, false, None).await;
        }
        let check = cli
            .show(task_id)
            .await
            .map_err(|e| HandlerError::Message(e.to_string()))?;
        if check
            .fields()
            .map_err(|e| HandlerError::Message(e.to_string()))?
            != desired
        {
            result.state = "conflict".into();
            result.summary =
                "Microsoft task changed while syncing. The Mixer task was left unchanged.".into();
            return finish(state, result, false, None).await;
        }
    }
    if local != desired {
        if !state
            .store
            .apply_linked_todo_fields(&record, &edited)
            .await?
        {
            result.state = "conflict".into();
            result.summary = "The Mixer task changed while syncing. Its newer edit was preserved; review both tasks.".into();
            return finish(state, result, false, None).await;
        }
        if local.completed != desired.completed {
            if desired.completed {
                super::records::file_ticked_todos(state, std::slice::from_ref(&edited)).await;
            } else {
                super::records::unfile_todos(state, std::slice::from_ref(&edited)).await;
            }
        }
        super::arrivals::modes_changed(state, &record.account_id);
    }
    result.state = "synced".into();
    result.summary = "Shared fields synchronized. Microsoft-only fields were preserved.".into();
    finish(
        state,
        result,
        false,
        Some(&serde_json::to_string(&desired)?),
    )
    .await
}

async fn finish(
    state: &AppState,
    result: MicrosoftTodoSyncItemData,
    dry_run: bool,
    baseline: Option<&str>,
) -> Result<MicrosoftTodoSyncItemData, HandlerError> {
    if !dry_run {
        let error = (result.state != "synced").then_some(result.summary.as_str());
        state
            .store
            .note_todo_microsoft_sync(&result.todo_id, baseline, error)
            .await?;
    }
    Ok(result)
}

fn wire_fields(fields: &LinkedTodoFields) -> mxr_protocol::MicrosoftTodoFieldsData {
    mxr_protocol::MicrosoftTodoFieldsData {
        title: fields.title.clone(),
        due: fields.due,
        scheduled: fields.scheduled,
        completed: fields.completed,
    }
}

fn local_update(
    state: &AppState,
    record: &TodoRecord,
    local: &LinkedTodoFields,
    desired: &LinkedTodoFields,
) -> Result<TodoRecord, HandlerError> {
    let edits = if local.due != desired.due {
        vec![TodoEditData {
            field: "due".into(),
            value: desired.due.map_or_else(String::new, |day| day.to_string()),
        }]
    } else {
        Vec::new()
    };
    let mut edited = super::todos::apply_edits(state, record.clone(), &edits, Utc::now(), None)?;
    let mut sources = FieldSources::from_json(&edited.field_sources);
    if local.title != desired.title {
        if desired.title.trim().is_empty() || desired.title.len() > 32_768 {
            return Err(HandlerError::InvalidRequest(
                "Microsoft task title is empty or too long; neither version was overwritten."
                    .into(),
            ));
        }
        edited.title = desired.title.clone();
        sources.set(
            "title",
            FieldProvenance::with_evidence(FieldSource::User, "you changed it in Microsoft To Do"),
        );
    }
    if local.scheduled != desired.scheduled {
        edited.scheduled_for = desired
            .scheduled
            .map(|day| local_morning(day, state.snooze_time_prefs().morning_hour))
            .transpose()?;
        sources.set(
            "scheduled_for",
            FieldProvenance::with_evidence(
                FieldSource::User,
                "you scheduled it in Microsoft To Do",
            ),
        );
    }
    if local.completed != desired.completed {
        edited.state = if desired.completed {
            TodoState::Done
        } else {
            TodoState::Open
        };
        edited.done_at = desired.completed.then(Utc::now);
        edited.dismissed_at = None;
        edited.expired_at = None;
        if !desired.completed {
            edited.expired_at_birth = false;
            if matches!(
                edited.catchup,
                Some(mxr_store::TodoCatchup::LetGo | mxr_store::TodoCatchup::Overflow)
            ) {
                edited.catchup = Some(mxr_store::TodoCatchup::Kept);
            }
        }
    }
    edited.field_sources = sources.to_json();
    Ok(edited)
}

fn local_morning(day: NaiveDate, hour: u8) -> Result<chrono::DateTime<Utc>, HandlerError> {
    day.and_hms_opt(u32::from(hour), 0, 0)
        .and_then(|time| Local.from_local_datetime(&time).earliest())
        .map(|time| time.with_timezone(&Utc))
        .ok_or_else(|| {
            HandlerError::InvalidRequest(
                "That Microsoft scheduling date is not representable in the daemon's time zone."
                    .into(),
            )
        })
}

#[cfg(all(test, unix))]
mod tests;
