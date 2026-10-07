//! `Request::ResolveDeskItems`: the desk's Done. "Nothing for me to do
//! here, put it away", per lane:
//!
//! - You owe, New from people: archive and mark read, and keep the
//!   conversation off the desk (any lane, a watched contact's row too)
//!   until a message is stored in it after this.
//! - Waiting on: mark read and done waiting (the same dismissal).
//! - Due: resolve the promise, mark its conversation read and dismiss it
//!   like the others. Nothing is archived. Other open promises on the
//!   conversation stay under Due: due rows never read dismissals.
//!
//! Every lane also takes the conversation out of the reply-later queue,
//! timed or not, and cancels a pending "bring it back if no reply"
//! reminder, so nothing Done put away comes back on a timer.
//!
//! One plan serves the preview and the run, so a dry run lists exactly
//! what the run changes. Messages go through the shared mutation path; the
//! run saves one undo entry holding the messages' prior labels and read
//! state plus the dismissals and promises as they were.

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
    CommitmentPrior, CommitmentStatus, ContactCommitmentRecord, DeskDismissal, DeskDismissalMark,
    DeskMessage, DeskUndo, ReminderState, ReplyLaterState, UndoEntry, UndoEntrySnapshot,
    UndoableMutationKind,
};
use std::collections::{HashMap, HashSet};

/// What Done will do to one item.
struct Plan {
    thread_id: ThreadId,
    account_id: Option<AccountId>,
    lane: DeskLaneKind,
    /// In the inbox: archived (and marked read). Owed and people-new only.
    archive: Vec<MessageId>,
    /// Unread: marked read.
    unread: Vec<MessageId>,
    /// The dismissal, through the messages this plan read.
    mark: Option<DeskDismissalMark>,
    /// The open promise to resolve (Due).
    commitment: Option<ContactCommitmentRecord>,
    /// Flagged for reply later, as stored: cleared.
    reply_later: Vec<(MessageId, ReplyLaterState)>,
    /// Pending "bring it back if no reply" reminders, as stored: cancelled.
    reminders: Vec<(MessageId, ReminderState)>,
    /// Every message of the conversation: the run takes the timers off all
    /// of them as they are then, not as the plan read them, so a reminder
    /// that fires between plan and run can't outlive Done.
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

    /// Archived and read, rather than only read: a lane that archives,
    /// when no other mode keeps the thread in the inbox.
    fn archives(&self) -> bool {
        lane_archives(self.lane) && !self.archive.is_empty()
    }

    /// Every message the run changes, each once.
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

    // Each account's threads in one read, so every plan's messages and its
    // dismissal watermark come from the same snapshot.
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

    // The last-mode rule SetModeDone follows: the provider archive happens
    // only when no other mode (To do, Updates, Reading) still holds the
    // thread and `modes.archive_on_last_done` is on.
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
        // One plan per conversation, so each message is changed and
        // snapshotted for undo once.
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

/// What would bring a conversation back on its own, per message.
struct Timers {
    reply_later: HashMap<MessageId, ReplyLaterState>,
    /// Pending only.
    reminders: HashMap<MessageId, ReminderState>,
}

/// One item's plan, or why it cannot be done (the outer error is the
/// store's).
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
        // Only here is who wrote last needed, so only here is it looked up.
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
            // Already kept: the rest of Done still applies.
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
    // The watermark `DeskDismissal::covers` reads: anything stored after
    // this plan brings the conversation back. Due too, or keeping the
    // promise would uncover the conversation's Waiting on row.
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

/// You owe and New from people are archived; the other lanes never are.
fn lane_archives(lane: DeskLaneKind) -> bool {
    matches!(lane, DeskLaneKind::Owed | DeskLaneKind::PeopleNew)
}

/// The desk's own rule when the caller names no lane: waiting when you
/// wrote last, otherwise owed.
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

    // Both commands share the id: their dedup keys never collide, and one
    // undo entry covers them.
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
        // A batch that fails outright must not lose the undo of one that
        // already ran: note the error and carry on to the undo entry.
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

    // An item is done only when all of its messages changed; the rest keep
    // their row, so running Done again retries them.
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

    // A message that failed half way (read, not archived) is undone too.
    snapshots.extend(failed);

    // The messages changed already: whatever happens here, their undo is
    // saved below.
    let mut desk = DeskUndo::default();
    if let Err(error) = put_away(state, &mut plans, &mut desk).await {
        tracing::warn!(%error, "desk done could not record dismissals or promises");
        for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
            plan.error = Some(format!("couldn't put it away ({error}); run Done again"));
        }
    }

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

    Ok(ResponseData::DeskItemsResolved {
        items: plans.iter().map(Plan::outcome).collect(),
        dry_run: false,
        mutation_id,
        undo_unavailable,
    })
}

/// Dismiss the done conversations and resolve their promises, noting in
/// `desk` how each was before.
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
        if state
            .store
            .resolve_account_commitment(&commitment.account_id, &commitment.id)
            .await?
        {
            desk.commitments.push(CommitmentPrior {
                account_id: commitment.account_id.clone(),
                id: commitment.id.clone(),
                status: commitment.status,
                resolved_at: commitment.resolved_at,
            });
        } else {
            plan.error = Some("the promise was removed before it could be resolved".into());
            plan.commitment = None;
        }
    }
    let now = Utc::now();
    for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
        // One transaction reads and clears, so the undo holds exactly what
        // this took, including a flag a reminder set since the plan.
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

/// Undo's half for what Done (the desk's, or a mode's) changed beyond
/// messages.
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

/// `run` over hand-built plans (each conversation's messages, archived or
/// marked read by its lane), for tests that need a batch to fail.
#[cfg(test)]
pub(super) async fn run_messages_for_test(
    state: &AppState,
    items: Vec<(ThreadId, AccountId, DeskLaneKind, Vec<MessageId>)>,
) -> HandlerResult {
    let plans = items
        .into_iter()
        .map(|(thread_id, account_id, lane, messages)| Plan {
            thread_id,
            account_id: Some(account_id),
            lane,
            archive: if lane_archives(lane) {
                messages.clone()
            } else {
                Vec::new()
            },
            unread: messages,
            mark: None,
            commitment: None,
            reply_later: Vec::new(),
            reminders: Vec::new(),
            thread_messages: Vec::new(),
            error: None,
        })
        .collect();
    run(state, plans).await
}

/// Done with `between` run after the plan and before the run, for tests of
/// what happens in that gap (a reminder firing).
#[cfg(test)]
pub(super) async fn resolve_with_gap_for_test(
    state: &AppState,
    items: &[DeskDoneItemData],
    between: impl std::future::Future<Output = ()>,
) -> HandlerResult {
    let plans = plan(state, items).await?;
    between.await;
    run(state, plans).await
}
