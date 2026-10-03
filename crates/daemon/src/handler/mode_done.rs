//! `Request::SetModeDone`: done here, per mode (blueprint 22, "Handoff
//! names where the item went").
//!
//! - Messages, Updates, Reading: the thread's done mark in that mode, at
//!   the watermark of the messages this plan read, so a later message
//!   brings it back to that mode only.
//! - To do: its open rows on the thread are ticked off.
//! - Archive has no done: records stay.
//!
//! When no other mode holds the thread (Archive never does) and
//! `modes.archive_on_last_done` is on, the thread is archived and marked
//! read at the provider too. Each outcome carries the handoff copy.
//!
//! One plan serves the preview and the run, so a dry run lists exactly
//! what the run changes. Messages go through the shared mutation path; the
//! run saves one undo entry holding the messages' prior labels and read
//! state plus the marks, rows and promises as they were.

use super::mode_rules::{due_detail, handoff_copy, provider_name, Handoff, StillIn};
use super::modes::{mark_name, place_threads, threads_by_account, Placement};
use super::mutations::{apply_mutation_batch, UNDO_WINDOW_SECS};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Local, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::MessageFlags;
use mxr_protocol::{
    AccountMutationResultData, ModeDoneOutcomeData, ModeKindData, MutationCommand, ResponseData,
};
use mxr_store::{
    CommitmentPrior, CommitmentStatus, DeskUndo, ModeDoneMark, TodoRecord, TodoState,
    TodoTickPrior, UndoEntry, UndoEntrySnapshot, UndoableMutationKind,
};
use std::collections::{HashMap, HashSet};

/// What done in one mode will do to one thread.
struct Plan {
    thread_id: ThreadId,
    account_id: Option<AccountId>,
    mode: ModeKindData,
    mark: Option<ModeDoneMark>,
    /// To do: the open rows to tick off.
    todos: Vec<TodoRecord>,
    still_in: Vec<StillIn>,
    /// In the inbox, archived and marked read: the last mode let go.
    archive: Vec<MessageId>,
    /// Of those, the ones unread.
    unread: usize,
    /// No mode holds it but the setting keeps it in the inbox.
    left_in_inbox: bool,
    /// To do on a record: the toast says it is filed in Archive.
    filed: bool,
    provider: &'static str,
    error: Option<String>,
}

impl Plan {
    fn failed(thread_id: &ThreadId, mode: ModeKindData, error: impl Into<String>) -> Self {
        Self {
            thread_id: thread_id.clone(),
            account_id: None,
            mode,
            mark: None,
            todos: Vec::new(),
            still_in: Vec::new(),
            archive: Vec::new(),
            unread: 0,
            left_in_inbox: false,
            filed: false,
            provider: provider_name(None),
            error: Some(error.into()),
        }
    }

    fn outcome(&self) -> ModeDoneOutcomeData {
        let copy = if self.error.is_some() {
            String::new()
        } else {
            handoff_copy(&Handoff {
                mode: self.mode,
                still_in: &self.still_in,
                archived: !self.archive.is_empty(),
                left_in_inbox: self.left_in_inbox,
                filed: self.filed,
                provider: self.provider,
            })
        };
        ModeDoneOutcomeData {
            thread_id: self.thread_id.clone(),
            account_id: self.account_id.clone(),
            mode: self.mode,
            marked: self.mark.is_some() && self.error.is_none(),
            todos_ticked: self.todos.iter().map(|todo| todo.id.clone()).collect(),
            still_in: self.still_in.iter().map(|still| still.mode).collect(),
            archived: u32::try_from(self.archive.len()).unwrap_or(u32::MAX),
            marked_read: u32::try_from(self.unread).unwrap_or(u32::MAX),
            provider: self.provider.to_string(),
            copy,
            error: self.error.clone(),
        }
    }
}

pub(super) async fn set_mode_done(
    state: &AppState,
    thread_ids: &[ThreadId],
    mode: ModeKindData,
    dry_run: bool,
) -> HandlerResult {
    if mode == ModeKindData::Archive {
        return Err(HandlerError::InvalidRequest(
            "Archive has no done: records stay filed.".into(),
        ));
    }
    if thread_ids.is_empty() {
        return Err(HandlerError::InvalidRequest("no threads given".into()));
    }
    let plans = plan(state, thread_ids, mode, Utc::now()).await?;
    if dry_run {
        return Ok(ResponseData::ModeDone {
            items: plans.iter().map(Plan::outcome).collect(),
            dry_run: true,
            mutation_id: None,
            undo_unavailable: false,
        });
    }
    run(state, plans).await
}

async fn plan(
    state: &AppState,
    thread_ids: &[ThreadId],
    mode: ModeKindData,
    now: DateTime<Utc>,
) -> Result<Vec<Plan>, HandlerError> {
    let archive_on_last_done = state.config_snapshot().modes.archive_on_last_done;
    let mut placed: HashMap<ThreadId, Placement> = HashMap::new();
    for (account, threads) in threads_by_account(state, thread_ids).await? {
        for placement in place_threads(state, &account, &threads, now).await? {
            placed.insert(placement.data.thread_id.clone(), placement);
        }
    }
    let mut seen: HashSet<&ThreadId> = HashSet::new();
    Ok(thread_ids
        .iter()
        .map(|thread_id| {
            // One plan per thread, so each message is changed and
            // snapshotted for undo once.
            if !seen.insert(thread_id) {
                return Plan::failed(thread_id, mode, "this thread is already in the request");
            }
            match placed.get(thread_id) {
                Some(placement) => plan_one(placement, mode, archive_on_last_done, now),
                None => Plan::failed(thread_id, mode, "conversation not found"),
            }
        })
        .collect())
}

fn plan_one(
    placement: &Placement,
    mode: ModeKindData,
    archive_on_last_done: bool,
    now: DateTime<Utc>,
) -> Plan {
    let data = &placement.data;
    let provider = provider_name(placement.provider.as_ref());
    if mode == ModeKindData::Todo && placement.todos.is_empty() {
        let mut plan = Plan::failed(&data.thread_id, mode, "no open to-do on this conversation");
        plan.account_id = Some(data.account_id.clone());
        plan.provider = provider;
        return plan;
    }
    let mark = mark_name(mode).map(|name| ModeDoneMark {
        account_id: data.account_id.clone(),
        thread_id: data.thread_id.clone(),
        mode: name.to_string(),
        // Storage order, as the marks are read: anything stored after
        // this plan brings the thread back to this mode.
        through_seq: placement
            .messages
            .iter()
            .map(|m| m.seq)
            .max()
            .unwrap_or_default(),
        through_count: i64::try_from(placement.messages.len()).unwrap_or(i64::MAX),
    });
    let still_in: Vec<StillIn> = data
        .modes
        .iter()
        .map(|entry| entry.mode)
        .filter(|held| *held != mode && held.holds_inbox())
        .map(|held| StillIn {
            mode: held,
            detail: (held == ModeKindData::Todo)
                .then(|| {
                    placement
                        .todos
                        .iter()
                        .filter_map(|todo| todo.due_at)
                        .min()
                        .map(|due| due_detail(due, now, &Local))
                })
                .flatten(),
        })
        .collect();
    let live_in_inbox: Vec<_> = placement
        .messages
        .iter()
        .filter(|m| m.in_inbox && !m.trashed)
        .collect();
    let last = still_in.is_empty();
    let (archive, unread) = if last && archive_on_last_done {
        (
            live_in_inbox.iter().map(|m| m.id.clone()).collect(),
            live_in_inbox
                .iter()
                .filter(|m| !m.flags.contains(MessageFlags::READ))
                .count(),
        )
    } else {
        (Vec::new(), 0)
    };
    Plan {
        thread_id: data.thread_id.clone(),
        account_id: Some(data.account_id.clone()),
        mode,
        mark,
        todos: if mode == ModeKindData::Todo {
            placement.todos.clone()
        } else {
            Vec::new()
        },
        left_in_inbox: last && !archive_on_last_done && !live_in_inbox.is_empty(),
        filed: mode == ModeKindData::Todo
            && data
                .modes
                .iter()
                .any(|entry| entry.mode == ModeKindData::Archive),
        still_in,
        archive,
        unread,
        provider,
        error: None,
    }
}

async fn run(state: &AppState, mut plans: Vec<Plan>) -> HandlerResult {
    let mutation_id = uuid::Uuid::now_v7().to_string();
    let archive_ids: Vec<MessageId> = plans
        .iter()
        .filter(|plan| plan.error.is_none())
        .flat_map(|plan| plan.archive.iter().cloned())
        .collect();

    let mut snapshots: Vec<UndoEntrySnapshot> = Vec::new();
    // Messages the archive fully changed; a failed one is undone but its
    // thread isn't done.
    let mut changed: HashSet<MessageId> = HashSet::new();
    let mut account_errors: HashMap<AccountId, String> = HashMap::new();
    let mut batch_error: Option<String> = None;
    if !archive_ids.is_empty() {
        let cmd = MutationCommand::ReadAndArchive {
            message_ids: archive_ids,
        };
        match apply_mutation_batch(state, &cmd, &mutation_id, None).await {
            Ok(batch) => {
                changed.extend(batch.changed.iter().map(|s| s.message_id.clone()));
                snapshots.extend(batch.changed);
                // A message that failed half way (read, not archived) is
                // undone too.
                snapshots.extend(batch.failed);
                for AccountMutationResultData {
                    account_id, error, ..
                } in batch.accounts
                {
                    if let Some(error) = error {
                        account_errors.entry(account_id).or_insert(error);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "mode done archive failed");
                batch_error = Some(error.to_string());
            }
        }
    }

    // A thread is done only when every message it archives changed; the
    // rest keep their place in the mode, so running done again retries.
    for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
        if plan.archive.iter().all(|id| changed.contains(id)) {
            continue;
        }
        let why = plan
            .account_id
            .as_ref()
            .and_then(|account| account_errors.get(account))
            .or(batch_error.as_ref())
            .map_or("not every message could be archived", String::as_str);
        plan.error = Some(format!("{why}; run done again to retry"));
    }

    let mut desk = DeskUndo::default();
    if let Err(error) = put_away(state, &mut plans, &mut desk).await {
        tracing::warn!(%error, "mode done could not record its marks or to-dos");
        for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
            plan.error = Some(format!("couldn't mark it done ({error}); run done again"));
        }
    }

    let (mutation_id, undo_unavailable) = if snapshots.is_empty() && desk.is_empty() {
        (None, false)
    } else {
        let now = Utc::now().timestamp();
        let entry = UndoEntry {
            mutation_id: mutation_id.clone(),
            kind: UndoableMutationKind::ModeDone,
            snapshots,
            desk: (!desk.is_empty()).then_some(desk),
            applied_at: now,
            expires_at: now + UNDO_WINDOW_SECS,
        };
        match state.store.write_undo_entry(&entry).await {
            Ok(()) => (Some(mutation_id), false),
            Err(error) => {
                tracing::warn!(%error, "failed to write the mode done undo entry");
                (None, true)
            }
        }
    };

    Ok(ResponseData::ModeDone {
        items: plans.iter().map(Plan::outcome).collect(),
        dry_run: false,
        mutation_id,
        undo_unavailable,
    })
}

/// Write the marks and tick the rows off, noting in `desk` how each was
/// before.
async fn put_away(
    state: &AppState,
    plans: &mut [Plan],
    desk: &mut DeskUndo,
) -> Result<(), HandlerError> {
    let marks: Vec<ModeDoneMark> = plans
        .iter()
        .filter(|plan| plan.error.is_none())
        .filter_map(|plan| plan.mark.clone())
        .collect();
    let keys: Vec<(AccountId, ThreadId, String)> = marks
        .iter()
        .map(|mark| {
            (
                mark.account_id.clone(),
                mark.thread_id.clone(),
                mark.mode.clone(),
            )
        })
        .collect();
    desk.mode_done = state.store.mode_done_priors(&keys).await?;
    state.store.mark_mode_done(&marks).await?;

    let now = Utc::now();
    let mut ticked_any = false;
    for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
        if plan.todos.is_empty() {
            continue;
        }
        let ids: Vec<String> = plan.todos.iter().map(|todo| todo.id.clone()).collect();
        let changed = state
            .store
            .set_todos_state(&ids, &[TodoState::Open], TodoState::Done, now)
            .await?;
        // A row ticked off elsewhere since the plan isn't this run's to undo.
        plan.todos.retain(|todo| changed.contains(&todo.id));
        for todo in &plan.todos {
            desk.todos_ticked.push(TodoTickPrior {
                id: todo.id.clone(),
                user_edited: todo.user_edited,
            });
            if let Some(prior) = tick_promise(state, todo, now).await? {
                desk.commitments.push(prior);
            }
        }
        ticked_any |= !plan.todos.is_empty();
    }
    if ticked_any {
        super::mode_guide::retire(state, ModeKindData::Todo.id()).await?;
    }
    Ok(())
}

/// A promise row's commitment is resolved with it, so the desk and `mxr
/// commitments` agree; returns how it was for undo.
async fn tick_promise(
    state: &AppState,
    todo: &TodoRecord,
    now: DateTime<Utc>,
) -> Result<Option<CommitmentPrior>, HandlerError> {
    let Some(commitment_id) = &todo.commitment_id else {
        return Ok(None);
    };
    let Some(record) = state.store.get_contact_commitment(commitment_id).await? else {
        return Ok(None);
    };
    if record.status != CommitmentStatus::Open {
        return Ok(None);
    }
    state
        .store
        .set_contact_commitment_status(
            &todo.account_id,
            commitment_id,
            CommitmentStatus::Resolved,
            Some(now),
        )
        .await?;
    Ok(Some(CommitmentPrior {
        account_id: record.account_id,
        id: record.id,
        status: record.status,
        resolved_at: record.resolved_at,
    }))
}
