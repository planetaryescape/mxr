use super::desk::self_matcher;
use super::desk_lanes::{current_messages, is_outbound, last_stored};
use super::mutations::{apply_mutation_batch, UNDO_WINDOW_SECS};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::Utc;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::MessageFlags;
use mxr_protocol::{
    AccountMutationResultData, DeskDoneItemData, DeskDoneOutcomeData, DeskLaneKind, ModeKindData,
    MutationCommand, ResponseData,
};
use mxr_store::{
    CommitmentStatus, ContactCommitmentRecord, DeskDismissal, DeskDismissalMark, DeskMessage,
    DeskUndo, ReminderState, ReplyLaterState, UndoEntry, UndoEntrySnapshot, UndoableMutationKind,
};
use std::collections::{HashMap, HashSet};

struct Plan {
    thread_id: ThreadId,
    account_id: Option<AccountId>,
    lane: DeskLaneKind,
    archive: Vec<MessageId>,
    unread: Vec<MessageId>,
    mark: Option<DeskDismissalMark>,
    commitment: Option<ContactCommitmentRecord>,
    reply_later: Vec<(MessageId, ReplyLaterState)>,
    reminders: Vec<(MessageId, ReminderState)>,
    thread_messages: Vec<MessageId>,
    error: Option<String>,
}

impl Plan {
    fn failed(item: &DeskDoneItemData, lane: DeskLaneKind, error: impl Into<String>) -> Self {
        Self {
            thread_id: item.thread_id.clone(),
            account_id: None,
            lane,
            archive: Vec::new(),
            unread: Vec::new(),
            mark: None,
            commitment: None,
            reply_later: Vec::new(),
            reminders: Vec::new(),
            thread_messages: Vec::new(),
            error: Some(error.into()),
        }
    }

    fn archives(&self) -> bool {
        lane_archives(self.lane) && !self.archive.is_empty()
    }

    fn messages(&self) -> Vec<MessageId> {
        let mut seen = HashSet::new();
        self.archive
            .iter()
            .chain(&self.unread)
            .filter(|id| seen.insert((*id).clone()))
            .cloned()
            .collect()
    }

    fn outcome(&self) -> DeskDoneOutcomeData {
        DeskDoneOutcomeData {
            thread_id: self.thread_id.clone(),
            account_id: self.account_id.clone(),
            lane: self.lane,
            archived: self.archive.len() as u32,
            marked_read: self.unread.len() as u32,
            dismissed: self.mark.is_some(),
            reply_later_cleared: self.reply_later.len() as u32,
            reminders_cancelled: self.reminders.len() as u32,
            resolved_commitment_id: self.commitment.as_ref().map(|c| c.id.clone()),
            error: self.error.clone(),
        }
    }
}

pub(super) async fn resolve_desk_items(
    state: &AppState,
    items: &[DeskDoneItemData],
    dry_run: bool,
) -> HandlerResult {
    if items.is_empty() {
        return Err("no desk items given".into());
    }
    let plans = plan(state, items).await?;
    if dry_run {
        return Ok(ResponseData::DeskItemsResolved {
            items: plans.iter().map(Plan::outcome).collect(),
            dry_run: true,
            mutation_id: None,
            undo_unavailable: false,
        });
    }
    run(state, plans).await
}

async fn plan(state: &AppState, items: &[DeskDoneItemData]) -> Result<Vec<Plan>, HandlerError> {
    let thread_ids: Vec<ThreadId> = items.iter().map(|item| item.thread_id.clone()).collect();
    let owners: HashMap<ThreadId, AccountId> = state
        .store
        .get_threads_batch(&thread_ids)
        .await?
        .into_iter()
        .map(|thread| (thread.id, thread.account_id))
        .collect();

    let mut by_account: HashMap<AccountId, Vec<ThreadId>> = HashMap::new();
    for thread_id in &thread_ids {
        if let Some(account) = owners.get(thread_id) {
            by_account
                .entry(account.clone())
                .or_default()
                .push(thread_id.clone());
        }
    }
    let by_account_ids: Vec<(AccountId, Vec<ThreadId>)> = by_account.into_iter().collect();
    let mut threads: HashMap<ThreadId, Vec<DeskMessage>> = HashMap::new();
    for (account, ids) in &by_account_ids {
        for message in state.store.desk_messages_in_threads(account, ids).await? {
            threads
                .entry(message.thread_id.clone())
                .or_default()
                .push(message);
        }
    }

    let all_messages: Vec<MessageId> = threads
        .values()
        .flatten()
        .map(|message| message.id.clone())
        .collect();
    let flags = Timers {
        reply_later: state.store.reply_later_states(&all_messages).await?,
        reminders: state
            .store
            .auto_reminder_states(&all_messages)
            .await?
            .into_iter()
            .filter(|(_, reminder)| reminder.is_pending())
            .collect(),
    };

    let archive_on_last_done = state.config_snapshot().modes.archive_on_last_done;
    let mut held_elsewhere: HashSet<ThreadId> = HashSet::new();
    for (account, ids) in &by_account_ids {
        for placement in super::modes::place_threads(state, account, ids, Utc::now()).await? {
            let held = placement
                .data
                .modes
                .iter()
                .any(|entry| entry.mode != ModeKindData::Messages && entry.mode.holds_inbox());
            if held || !archive_on_last_done {
                held_elsewhere.insert(placement.data.thread_id);
            }
        }
    }

    let mut plans = Vec::with_capacity(items.len());
    let mut seen: HashSet<&ThreadId> = HashSet::new();
    for item in items {
        let found = owners
            .get(&item.thread_id)
            .zip(threads.get(&item.thread_id));
        let plan = if !seen.insert(&item.thread_id) {
            Err("this conversation is already in the request".to_string())
        } else {
            match found {
                Some((account_id, thread)) => {
                    plan_item(state, item, account_id, thread, &flags).await?
                }
                None => Err("conversation not found".to_string()),
            }
        };
        let plan = plan.map(|mut plan| {
            if held_elsewhere.contains(&plan.thread_id) {
                plan.archive.clear();
            }
            plan
        });
        plans.push(plan.unwrap_or_else(|error| {
            let lane = item.lane.unwrap_or_else(|| {
                if item.commitment_id.is_some() {
                    DeskLaneKind::Due
                } else {
                    DeskLaneKind::Owed
                }
            });
            Plan::failed(item, lane, error)
        }));
    }
    Ok(plans)
}

struct Timers {
    reply_later: HashMap<MessageId, ReplyLaterState>,
    reminders: HashMap<MessageId, ReminderState>,
}

async fn plan_item(
    state: &AppState,
    item: &DeskDoneItemData,
    account_id: &AccountId,
    thread: &[DeskMessage],
    timers: &Timers,
) -> Result<Result<Plan, String>, HandlerError> {
    let lane = match (item.lane, &item.commitment_id) {
        (Some(DeskLaneKind::Due) | None, Some(_)) => DeskLaneKind::Due,
        (Some(DeskLaneKind::Due), None) => {
            return Ok(Err("a promise under Due needs its commitment id".into()));
        }
        (Some(_), Some(_)) => {
            return Ok(Err("a commitment id only goes with the Due lane".into()));
        }
        (Some(lane), None) => lane,
        (None, None) => {
            let is_self = self_matcher(state, account_id).await?;
            inferred_lane(thread, &is_self)
        }
    };

    let commitment = match &item.commitment_id {
        None => None,
        Some(id) => match state.store.get_contact_commitment(id).await? {
            None => return Ok(Err(format!("promise not found: {id}"))),
            Some(record)
                if record.thread_id != item.thread_id || &record.account_id != account_id =>
            {
                return Ok(Err("that promise belongs to another conversation".into()));
            }
            Some(record) => (record.status == CommitmentStatus::Open).then_some(record),
        },
    };

    let live = || thread.iter().filter(|message| !message.trashed);
    let archive = if lane_archives(lane) {
        live()
            .filter(|message| message.in_inbox)
            .map(|message| message.id.clone())
            .collect()
    } else {
        Vec::new()
    };
    let unread = live()
        .filter(|message| !message.flags.contains(MessageFlags::READ))
        .map(|message| message.id.clone())
        .collect();
    let mark = DeskDismissal::through(thread).map(|through| DeskDismissalMark {
        account_id: account_id.clone(),
        thread_id: item.thread_id.clone(),
        through,
    });
    let reply_later = thread
        .iter()
        .filter_map(|message| {
            timers
                .reply_later
                .get(&message.id)
                .map(|flag| (message.id.clone(), *flag))
        })
        .collect();
    let reminders = thread
        .iter()
        .filter_map(|message| {
            timers
                .reminders
                .get(&message.id)
                .map(|reminder| (message.id.clone(), reminder.clone()))
        })
        .collect();
    Ok(Ok(Plan {
        thread_id: item.thread_id.clone(),
        account_id: Some(account_id.clone()),
        lane,
        archive,
        unread,
        mark,
        commitment,
        reply_later,
        reminders,
        thread_messages: thread.iter().map(|message| message.id.clone()).collect(),
        error: None,
    }))
}

fn lane_archives(lane: DeskLaneKind) -> bool {
    matches!(lane, DeskLaneKind::Owed | DeskLaneKind::PeopleNew)
}

fn inferred_lane(thread: &[DeskMessage], is_self: &dyn Fn(&str) -> bool) -> DeskLaneKind {
    match last_stored(current_messages(thread, Utc::now()), |_| true) {
        Some(message) if is_outbound(message, is_self) => DeskLaneKind::Waiting,
        _ => DeskLaneKind::Owed,
    }
}

async fn run(state: &AppState, mut plans: Vec<Plan>) -> HandlerResult {
    let mutation_id = uuid::Uuid::now_v7().to_string();
    let (archiving, reading): (Vec<&Plan>, Vec<&Plan>) = plans
        .iter()
        .filter(|plan| plan.error.is_none())
        .partition(|plan| plan.archives());
    let archive_ids: Vec<MessageId> = archiving.into_iter().flat_map(Plan::messages).collect();
    let read_ids: Vec<MessageId> = reading.into_iter().flat_map(Plan::messages).collect();

    let mut snapshots: Vec<UndoEntrySnapshot> = Vec::new();
    let mut failed: Vec<UndoEntrySnapshot> = Vec::new();
    let mut account_errors: HashMap<AccountId, String> = HashMap::new();
    let mut batch_error: Option<String> = None;
    for cmd in [
        (!archive_ids.is_empty()).then_some(MutationCommand::ReadAndArchive {
            message_ids: archive_ids,
        }),
        (!read_ids.is_empty()).then_some(MutationCommand::SetRead {
            message_ids: read_ids,
            read: true,
        }),
    ]
    .into_iter()
    .flatten()
    {
        let batch = match apply_mutation_batch(state, &cmd, &mutation_id, None).await {
            Ok(batch) => batch,
            Err(error) => {
                tracing::warn!(%error, "desk done batch failed");
                batch_error.get_or_insert_with(|| error.to_string());
                continue;
            }
        };
        snapshots.extend(batch.changed);
        failed.extend(batch.failed);
        for AccountMutationResultData {
            account_id, error, ..
        } in batch.accounts
        {
            if let Some(error) = error {
                account_errors.entry(account_id).or_insert(error);
            }
        }
    }

    let changed: HashSet<&MessageId> = snapshots.iter().map(|s| &s.message_id).collect();
    for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
        if plan.messages().iter().all(|id| changed.contains(id)) {
            continue;
        }
        let why = plan
            .account_id
            .as_ref()
            .and_then(|account| account_errors.get(account))
            .or(batch_error.as_ref())
            .map_or("not every message could be updated", String::as_str);
        plan.error = Some(format!("{why}; run Done again to retry"));
    }

    snapshots.extend(failed);

    let mut desk = DeskUndo::default();
    let mut put_away_failed = false;
    if let Err(error) = put_away(state, &mut plans, &mut desk).await {
        put_away_failed = true;
        tracing::warn!(%error, "desk done could not record dismissals or promises");
        for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
            plan.error = Some(format!("couldn't put it away ({error}); run Done again"));
        }
    }

    let mut changed_accounts: HashSet<AccountId> = snapshots
        .iter()
        .map(|snapshot| snapshot.account_id.clone())
        .collect();

    let (mutation_id, undo_unavailable) = if snapshots.is_empty() && desk.is_empty() {
        (None, false)
    } else {
        let now = Utc::now().timestamp();
        let entry = UndoEntry {
            mutation_id: mutation_id.clone(),
            kind: UndoableMutationKind::DeskDone,
            snapshots,
            desk: (!desk.is_empty()).then_some(desk),
            applied_at: now,
            expires_at: now + UNDO_WINDOW_SECS,
        };
        match state.store.write_undo_entry(&entry).await {
            Ok(()) => (Some(mutation_id), false),
            Err(error) => {
                tracing::warn!(%error, "failed to write the desk done undo entry");
                (None, true)
            }
        }
    };

    changed_accounts.extend(
        plans
            .iter()
            .filter(|plan| put_away_failed || plan.error.is_none())
            .filter_map(|plan| plan.account_id.clone()),
    );
    for account in &changed_accounts {
        super::arrivals::modes_changed(state, account);
    }

    Ok(ResponseData::DeskItemsResolved {
        items: plans.iter().map(Plan::outcome).collect(),
        dry_run: false,
        mutation_id,
        undo_unavailable,
    })
}

async fn put_away(
    state: &AppState,
    plans: &mut [Plan],
    desk: &mut DeskUndo,
) -> Result<(), HandlerError> {
    let marks: Vec<DeskDismissalMark> = plans
        .iter()
        .filter(|plan| plan.error.is_none())
        .filter_map(|plan| plan.mark.clone())
        .collect();
    let pairs: Vec<(AccountId, ThreadId)> = marks
        .iter()
        .map(|mark| (mark.account_id.clone(), mark.thread_id.clone()))
        .collect();
    desk.dismissals = state.store.desk_dismissal_priors(&pairs).await?;
    state.store.mark_desk_dismissals(&marks).await?;
    for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
        let Some(commitment) = plan.commitment.as_ref() else {
            continue;
        };
        if let Some(resolution) = super::commitments::resolve_promise(
            state,
            &commitment.account_id,
            &commitment.id,
            Some(CommitmentStatus::Open),
        )
        .await?
        {
            desk.promise_resolutions.push(resolution);
        } else {
            plan.error = Some("the promise was removed before it could be resolved".into());
            plan.commitment = None;
        }
    }
    let now = Utc::now();
    for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
        let taken = state.store.take_timers(&plan.thread_messages, now).await?;
        for (message_id, flag) in &taken.reply_later {
            desk.reply_later_priors
                .push((message_id.clone(), Some(*flag)));
        }
        for (message_id, reminder) in &taken.reminders {
            desk.reminder_priors
                .push((message_id.clone(), Some(reminder.clone())));
        }
        for (message_id, _) in &taken.reply_later {
            super::reply_later::refresh_reply_later_search_marker(state, message_id, false).await?;
        }
        plan.reply_later = taken.reply_later;
        plan.reminders = taken.reminders;
    }
    Ok(())
}

pub(super) async fn restore_desk_state(
    state: &AppState,
    desk: &DeskUndo,
    applied_at: i64,
) -> Result<(), HandlerError> {
    state
        .store
        .put_back_desk_dismissals(&desk.dismissals)
        .await?;
    state.store.put_back_mode_done(&desk.mode_done).await?;
    state
        .store
        .reopen_ticked_todos(&desk.todos_ticked, Utc::now())
        .await?;
    let reopened: Vec<String> = desk
        .todos_ticked
        .iter()
        .map(|prior| prior.id.clone())
        .collect();
    let reopened = state.store.get_todos(&reopened).await?;
    super::records::unfile_todos(state, &reopened).await;
    for resolution in &desk.promise_resolutions {
        let restored = state.store.undo_promise_resolution(resolution).await?;
        super::records::unfile_todos(state, &restored).await;
    }
    for commitment in &desk.commitments {
        state
            .store
            .set_contact_commitment_status(
                &commitment.account_id,
                &commitment.id,
                commitment.status,
                commitment.resolved_at,
            )
            .await?;
    }
    for (message_id, set_at) in &desk.reply_later {
        super::reply_later::set_reply_later_at(state, message_id, true, *set_at).await?;
    }
    super::deferral::restore_timers(state, desk, applied_at).await
}

