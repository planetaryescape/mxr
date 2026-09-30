//! `Request::DeferThreads`: reply later, or wait, until a time you chose.
//!
//! Who wrote last decides, by the desk's own rule:
//!
//! - They did: reply later until the time. Their latest message is flagged
//!   with the time, and so are the conversation's other flagged messages,
//!   so the whole conversation leaves the desk and the reply queue and
//!   comes back to both then.
//! - You did: bring it back if nobody replies. A reminder goes on your
//!   latest message (`auto_reminders`, which already cancels when someone
//!   answers), other pending reminders on the conversation are cancelled
//!   and its reply-later flags cleared, so this one time is what brings it
//!   back. When it fires, your message joins the reply queue.
//!
//! One plan serves the preview and the run. The run saves one undo entry
//! holding every flag and reminder it replaced, as it was.

use super::desk::{self_matcher, Senders};
use super::desk_lanes::is_outbound;
use super::mutations::UNDO_WINDOW_SECS;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::{DeferKindData, DeferredThreadData, ResponseData};
use mxr_store::{DeskMessage, DeskUndo, UndoEntry, UndoableMutationKind};
use std::collections::{HashMap, HashSet};

/// What deferring one conversation will do.
struct Plan {
    thread_id: ThreadId,
    account_id: Option<AccountId>,
    kind: Option<DeferKindData>,
    /// The message the time goes on.
    target: Option<MessageId>,
    /// The conversation's reply-later flags: the others moved to the same
    /// time (reply later), or all cleared (waiting).
    flags: Vec<MessageId>,
    /// The conversation's other pending reminders: cancelled (waiting).
    reminders: Vec<MessageId>,
    error: Option<String>,
}

impl Plan {
    fn failed(thread_id: &ThreadId, account_id: Option<AccountId>, error: &str) -> Self {
        Self {
            thread_id: thread_id.clone(),
            account_id,
            kind: None,
            target: None,
            flags: Vec::new(),
            reminders: Vec::new(),
            error: Some(error.to_string()),
        }
    }

    fn item(&self) -> DeferredThreadData {
        DeferredThreadData {
            thread_id: self.thread_id.clone(),
            account_id: self.account_id.clone(),
            kind: self.kind.filter(|_| self.error.is_none()),
            message_id: self.target.clone().filter(|_| self.error.is_none()),
            error: self.error.clone(),
        }
    }
}

pub(super) async fn defer_threads(
    state: &AppState,
    thread_ids: &[ThreadId],
    until: DateTime<Utc>,
    dry_run: bool,
) -> HandlerResult {
    defer_threads_at(state, thread_ids, until, dry_run, Utc::now()).await
}

/// `defer_threads` with the clock given (tests move it).
pub(super) async fn defer_threads_at(
    state: &AppState,
    thread_ids: &[ThreadId],
    until: DateTime<Utc>,
    dry_run: bool,
    now: DateTime<Utc>,
) -> HandlerResult {
    if thread_ids.is_empty() {
        return Err("no conversations given".into());
    }
    if until <= now {
        return Err("that time has already passed; pick a later one".into());
    }
    let mut plans = plan(state, thread_ids).await?;
    if dry_run {
        return Ok(ResponseData::ThreadsDeferred {
            items: plans.iter().map(Plan::item).collect(),
            until,
            dry_run: true,
            mutation_id: None,
            undo_unavailable: false,
        });
    }

    // Every change is noted before it is made, so a failure part way still
    // leaves an undo for what already changed.
    let mut undo = DeskUndo::default();
    for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
        if let Err(error) = apply(state, plan, until, now, &mut undo).await {
            tracing::warn!(%error, thread_id = %plan.thread_id, "defer failed part way");
            plan.error = Some(format!("{error}; try again"));
        }
    }

    let (mutation_id, undo_unavailable) = if undo.is_empty() {
        (None, false)
    } else {
        let mutation_id = uuid::Uuid::now_v7().to_string();
        let applied_at = now.timestamp();
        let entry = UndoEntry {
            mutation_id: mutation_id.clone(),
            kind: UndoableMutationKind::Deferral,
            snapshots: Vec::new(),
            desk: Some(undo),
            applied_at,
            expires_at: applied_at + UNDO_WINDOW_SECS,
        };
        match state.store.write_undo_entry(&entry).await {
            Ok(()) => (Some(mutation_id), false),
            Err(error) => {
                tracing::warn!(%error, "failed to write the deferral undo entry");
                (None, true)
            }
        }
    };
    Ok(ResponseData::ThreadsDeferred {
        items: plans.iter().map(Plan::item).collect(),
        until,
        dry_run: false,
        mutation_id,
        undo_unavailable,
    })
}

async fn plan(state: &AppState, thread_ids: &[ThreadId]) -> Result<Vec<Plan>, HandlerError> {
    let owners: HashMap<ThreadId, AccountId> = state
        .store
        .get_threads_batch(thread_ids)
        .await?
        .into_iter()
        .map(|thread| (thread.id, thread.account_id))
        .collect();
    let mut by_account: HashMap<AccountId, Vec<ThreadId>> = HashMap::new();
    for thread_id in thread_ids {
        if let Some(account) = owners.get(thread_id) {
            by_account
                .entry(account.clone())
                .or_default()
                .push(thread_id.clone());
        }
    }

    let mut threads: HashMap<ThreadId, Vec<DeskMessage>> = HashMap::new();
    let mut matchers = HashMap::new();
    let mut senders = HashMap::new();
    for (account, ids) in by_account {
        let messages = state.store.desk_messages_in_threads(&account, &ids).await?;
        senders.insert(
            account.clone(),
            Senders::load(state, &account, &messages).await?,
        );
        for message in messages {
            threads
                .entry(message.thread_id.clone())
                .or_default()
                .push(message);
        }
        matchers.insert(account.clone(), self_matcher(state, &account).await?);
    }
    let all_messages: Vec<MessageId> = threads.values().flatten().map(|m| m.id.clone()).collect();
    let flagged: HashSet<MessageId> = state
        .store
        .reply_later_states(&all_messages)
        .await?
        .into_keys()
        .collect();
    let pending: HashSet<MessageId> = state
        .store
        .auto_reminder_states(&all_messages)
        .await?
        .into_iter()
        .filter(|(_, reminder)| reminder.is_pending())
        .map(|(id, _)| id)
        .collect();

    let mut seen = HashSet::new();
    let plans = thread_ids
        .iter()
        .map(|thread_id| {
            let account = owners.get(thread_id);
            if !seen.insert(thread_id) {
                return Plan::failed(
                    thread_id,
                    account.cloned(),
                    "this conversation is already in the request",
                );
            }
            let (Some(account), Some(thread)) = (account, threads.get(thread_id)) else {
                return Plan::failed(thread_id, None, "conversation not found");
            };
            let is_self = &matchers[account];
            let answers = |m: &DeskMessage| senders[account].answers(m, is_self);
            plan_thread(account, thread, is_self, &answers, &flagged, &pending)
        })
        .collect();
    Ok(plans)
}

fn plan_thread(
    account_id: &AccountId,
    thread: &[DeskMessage],
    is_self: &dyn Fn(&str) -> bool,
    answers: &dyn Fn(&DeskMessage) -> bool,
    flagged: &HashSet<MessageId>,
    pending: &HashSet<MessageId>,
) -> Plan {
    let thread_id = &thread[0].thread_id;
    // Who wrote last, by storage order (a skewed Date header can't reorder
    // it), among you and people: an auto-responder or notification after
    // your message doesn't make it theirs. With neither (a newsletter
    // thread), their latest message: reply later still works on it.
    let live = || thread.iter().filter(|message| !message.trashed);
    let Some(latest) = live()
        .filter(|message| is_outbound(message, is_self) || answers(message))
        .max_by_key(|message| message.seq)
        .or_else(|| live().max_by_key(|message| message.seq))
    else {
        return Plan::failed(
            thread_id,
            Some(account_id.clone()),
            "nothing in this conversation outside the trash",
        );
    };
    let kind = if is_outbound(latest, is_self) {
        DeferKindData::Waiting
    } else {
        DeferKindData::ReplyLater
    };
    let in_thread = |set: &HashSet<MessageId>, skip_latest: bool| -> Vec<MessageId> {
        thread
            .iter()
            .filter(|message| !(skip_latest && message.id == latest.id))
            .filter(|message| set.contains(&message.id))
            .map(|message| message.id.clone())
            .collect()
    };
    let (flags, reminders) = match kind {
        DeferKindData::ReplyLater => (in_thread(flagged, true), Vec::new()),
        // Your own message too: a reminder that fired put it in the queue,
        // and the new time is what brings it back now.
        DeferKindData::Waiting => (in_thread(flagged, false), in_thread(pending, true)),
    };
    Plan {
        thread_id: thread_id.clone(),
        account_id: Some(account_id.clone()),
        kind: Some(kind),
        target: Some(latest.id.clone()),
        flags,
        reminders,
        error: None,
    }
}

async fn apply(
    state: &AppState,
    plan: &Plan,
    until: DateTime<Utc>,
    now: DateTime<Utc>,
    undo: &mut DeskUndo,
) -> Result<(), HandlerError> {
    let (Some(kind), Some(target), Some(account_id)) =
        (plan.kind, plan.target.as_ref(), plan.account_id.as_ref())
    else {
        return Ok(());
    };
    let store = &state.store;
    match kind {
        DeferKindData::ReplyLater => {
            let ids: Vec<MessageId> = std::iter::once(target.clone())
                .chain(plan.flags.iter().cloned())
                .collect();
            // Read again here rather than from the plan, so the undo holds
            // what this write replaces.
            let mut priors = store.reply_later_states(&ids).await?;
            for id in &ids {
                let prior = priors.remove(id);
                let newly_flagged = prior.is_none();
                undo.reply_later_priors.push((id.clone(), prior));
                store.defer_reply_later(id, now, until).await?;
                // The search marker only changes for a message not flagged yet.
                if newly_flagged {
                    super::reply_later::refresh_reply_later_search_marker(state, id, true).await?;
                }
            }
        }
        DeferKindData::Waiting => {
            let ids: Vec<MessageId> = std::iter::once(target.clone())
                .chain(plan.reminders.iter().cloned())
                .collect();
            let mut reminders = store.auto_reminder_states(&ids).await?;
            undo.reminder_priors
                .push((target.clone(), reminders.remove(target)));
            store
                .set_auto_reminder(target, account_id, until, now)
                .await?;
            for id in &plan.reminders {
                undo.reminder_priors
                    .push((id.clone(), reminders.remove(id)));
                store.cancel_auto_reminder(id, now).await?;
            }
            let mut flags = store.reply_later_states(&plan.flags).await?;
            for id in &plan.flags {
                undo.reply_later_priors.push((id.clone(), flags.remove(id)));
                super::reply_later::set_reply_later_at(state, id, false, now).await?;
            }
        }
    }
    Ok(())
}

/// Undo's half for flags and reminders: each back exactly as it was.
/// Reminders go first: one that fired since (it had not before) put its
/// message in the reply queue, and that flag goes too. Flags recorded in
/// `undo` are then put back over it as they were.
pub(super) async fn restore_timers(state: &AppState, undo: &DeskUndo) -> Result<(), HandlerError> {
    let now = Utc::now();
    let reminder_ids: Vec<MessageId> = undo
        .reminder_priors
        .iter()
        .map(|(id, _)| id.clone())
        .collect();
    let current = state.store.auto_reminder_states(&reminder_ids).await?;
    for (id, prior) in &undo.reminder_priors {
        let fired_since = current
            .get(id)
            .is_some_and(|now| now.triggered_at.is_some())
            && prior.as_ref().is_none_or(|was| was.triggered_at.is_none());
        state
            .store
            .restore_auto_reminder(id, prior.as_ref())
            .await?;
        if fired_since {
            super::reply_later::set_reply_later_at(state, id, false, now).await?;
        }
    }
    for (id, prior) in &undo.reply_later_priors {
        state.store.restore_reply_later(id, *prior, now).await?;
        super::reply_later::refresh_reply_later_search_marker(state, id, prior.is_some()).await?;
    }
    Ok(())
}

/// Cancel due "bring it back if nobody replies" reminders that a person
/// answered: someone other than you, not an auto-responder or a
/// notification (the shared classifier), stored after the message the
/// reminder is on (storage order, not the Date header).
pub(crate) async fn settle_answered_reminders(
    state: &AppState,
    now: DateTime<Utc>,
) -> Result<(), HandlerError> {
    let due = state.store.get_due_auto_reminders(now).await?;
    if due.is_empty() {
        return Ok(());
    }
    let sent: Vec<MessageId> = due.iter().map(|r| r.sent_message_id.clone()).collect();
    let mut by_account: HashMap<AccountId, Vec<(MessageId, ThreadId)>> = HashMap::new();
    for envelope in state.store.list_envelopes_by_ids(&sent).await? {
        by_account
            .entry(envelope.account_id)
            .or_default()
            .push((envelope.id, envelope.thread_id));
    }
    for (account, reminders) in by_account {
        let mut thread_ids: Vec<ThreadId> =
            reminders.iter().map(|(_, thread)| thread.clone()).collect();
        thread_ids.sort_by_key(ThreadId::as_str);
        thread_ids.dedup();
        let messages = state
            .store
            .desk_messages_in_threads(&account, &thread_ids)
            .await?;
        let senders = Senders::load(state, &account, &messages).await?;
        let is_self = self_matcher(state, &account).await?;
        for (sent_id, thread_id) in reminders {
            let thread = messages.iter().filter(|m| m.thread_id == thread_id);
            let Some(sent_seq) = thread.clone().find(|m| m.id == sent_id).map(|m| m.seq) else {
                continue;
            };
            if thread
                .into_iter()
                .any(|m| m.seq > sent_seq && senders.answers(m, &is_self))
            {
                state
                    .store
                    .cancel_pending_auto_reminder(&sent_id, now)
                    .await?;
            }
        }
    }
    Ok(())
}
