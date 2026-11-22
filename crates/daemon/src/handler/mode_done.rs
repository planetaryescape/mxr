use super::mode_rules::{due_detail, handoff_copy, provider_name, Handoff, StillIn};
use super::modes::{mark_name, place_threads, threads_by_account, without_done, Placement};
use super::mutations::{apply_mutation_batch, UNDO_WINDOW_SECS};
use super::places::placed_inbox;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Local, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::MessageFlags;
use mxr_protocol::{
    AccountMutationResultData, ModeDoneOutcomeData, ModeDoneSenderData, ModeKindData,
    MutationCommand, ResponseData, SenderKindData,
};
use mxr_store::{
    CommitmentPrior, CommitmentStatus, DeskDismissal, DeskUndo, ModeDoneMark, TodoRecord,
    TodoState, TodoTickPrior, UndoEntry, UndoEntrySnapshot, UndoableMutationKind,
};
use std::collections::{HashMap, HashSet};

struct Plan {
    thread_id: ThreadId,
    account_id: Option<AccountId>,
    mode: ModeKindData,
    mark: Option<ModeDoneMark>,
    todos: Vec<TodoRecord>,
    still_in: Vec<StillIn>,
    archive: Vec<MessageId>,
    unread: usize,
    left_in_inbox: bool,
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

pub(super) struct DoneRequest<'a> {
    pub thread_ids: &'a [ThreadId],
    pub mode: ModeKindData,
    pub dry_run: bool,
    pub todo_ids: &'a [String],
    pub sender: Option<&'a ModeDoneSenderData>,
}

pub(super) async fn set_mode_done(state: &AppState, request: DoneRequest<'_>) -> HandlerResult {
    let DoneRequest {
        mode,
        dry_run,
        todo_ids,
        sender,
        ..
    } = request;
    if mode == ModeKindData::Archive {
        return Err(HandlerError::InvalidRequest(
            "Archive has no done: records stay filed.".into(),
        ));
    }
    if !todo_ids.is_empty() && mode != ModeKindData::Todo {
        return Err(HandlerError::InvalidRequest(
            "todo_ids only go with To do".into(),
        ));
    }
    let mut thread_ids = request.thread_ids.to_vec();
    if let Some(sender) = sender {
        for thread in sender_threads(state, mode, sender).await? {
            if !thread_ids.contains(&thread) {
                thread_ids.push(thread);
            }
        }
    } else if thread_ids.is_empty() {
        return Err(HandlerError::InvalidRequest("no threads given".into()));
    }
    let plans = plan(state, &thread_ids, mode, todo_ids, None, Utc::now()).await?;
    finish(state, plans, dry_run).await
}

pub(super) async fn let_go_updates(
    state: &AppState,
    thread_ids: &[ThreadId],
    ids: &HashSet<MessageId>,
    dry_run: bool,
) -> HandlerResult {
    let mut prior = HashMap::new();
    for (account, threads) in threads_by_account(state, thread_ids).await? {
        for ((thread, mode), mark) in state
            .store
            .mode_done_for_threads(&account, &threads)
            .await?
        {
            if Some(mode.as_str()) == mark_name(ModeKindData::Updates) {
                prior.insert(thread, mark);
            }
        }
    }
    let exact = Exact { ids, prior };
    let plans = plan(
        state,
        thread_ids,
        ModeKindData::Updates,
        &[],
        Some(&exact),
        Utc::now(),
    )
    .await?;
    finish(state, plans, dry_run).await
}

struct Exact<'a> {
    ids: &'a HashSet<MessageId>,
    prior: HashMap<ThreadId, DeskDismissal>,
}

impl Exact<'_> {
    fn covers(&self, message: &mxr_store::DeskMessage) -> bool {
        self.ids.contains(&message.id)
            || self
                .prior
                .get(&message.thread_id)
                .is_some_and(|mark| mark.saw(message.date, &message.id))
    }
}

async fn finish(state: &AppState, plans: Vec<Plan>, dry_run: bool) -> HandlerResult {
    if dry_run {
        return Ok(ResponseData::ModeDone {
            items: plans.iter().map(Plan::outcome).collect(),
            dry_run: true,
            mutation_id: None,
            undo_unavailable: false,
        });
    }
    Box::pin(run(state, plans)).await
}

async fn sender_threads(
    state: &AppState,
    mode: ModeKindData,
    sender: &ModeDoneSenderData,
) -> Result<Vec<ThreadId>, HandlerError> {
    let kind = match mode {
        ModeKindData::Updates => SenderKindData::PaperTrail,
        ModeKindData::Reading => SenderKindData::Reading,
        _ => {
            return Err(HandlerError::InvalidRequest(
                "done for a sender is for Updates and Reading".into(),
            ))
        }
    };
    let accounts = [sender.account_id.clone()];
    let placed: Vec<_> = placed_inbox(state, &accounts, Some(&sender.sender_email))
        .await?
        .into_iter()
        .filter(|item| item.kind.kind == kind)
        .collect();
    let mut placed = without_done(state, &accounts, &[mode], placed).await?;
    placed.sort_by_key(|item| std::cmp::Reverse(item.message.date));
    let mut threads: Vec<ThreadId> = Vec::new();
    for item in placed {
        if !threads.contains(&item.message.thread_id) {
            threads.push(item.message.thread_id);
        }
    }
    Ok(threads)
}

async fn plan(
    state: &AppState,
    thread_ids: &[ThreadId],
    mode: ModeKindData,
    todo_ids: &[String],
    exact: Option<&Exact<'_>>,
    now: DateTime<Utc>,
) -> Result<Vec<Plan>, HandlerError> {
    let archive_on_last_done = state.config_snapshot().modes.archive_on_last_done;
    let mut placed: HashMap<ThreadId, Placement> = HashMap::new();
    let mut history_topics = HashSet::new();
    for (account, threads) in threads_by_account(state, thread_ids).await? {
        let placements = Box::pin(place_threads(state, &account, &threads, now)).await?;
        if mode == ModeKindData::Messages {
            let history: Vec<_> = placements
                .iter()
                .filter(|placement| !placement.data.in_inbox)
                .map(|placement| placement.data.thread_id.clone())
                .collect();
            if !history.is_empty() {
                history_topics.extend(
                    Box::pin(super::messages::done_topics(state, &account, &history, now)).await?,
                );
            }
        }
        for placement in placements {
            placed.insert(placement.data.thread_id.clone(), placement);
        }
    }
    let mut seen: HashSet<&ThreadId> = HashSet::new();
    Ok(thread_ids
        .iter()
        .map(|thread_id| {
            if !seen.insert(thread_id) {
                return Plan::failed(thread_id, mode, "this thread is already in the request");
            }
            match placed.get(thread_id) {
                Some(placement) => plan_one(
                    placement,
                    mode,
                    todo_ids,
                    exact,
                    archive_on_last_done,
                    now,
                    history_topics.contains(thread_id),
                ),
                None => Plan::failed(thread_id, mode, "conversation not found"),
            }
        })
        .collect())
}

fn plan_one(
    placement: &Placement,
    mode: ModeKindData,
    todo_ids: &[String],
    exact: Option<&Exact<'_>>,
    archive_on_last_done: bool,
    now: DateTime<Utc>,
    history_topic: bool,
) -> Plan {
    let data = &placement.data;
    let provider = provider_name(placement.provider.as_ref());
    let (ticked, still_open): (Vec<TodoRecord>, Vec<TodoRecord>) = placement
        .todos
        .iter()
        .cloned()
        .partition(|todo| todo_ids.is_empty() || todo_ids.contains(&todo.id));
    let refusal = if data.modes.iter().any(|entry| entry.mode == mode)
        || (mode == ModeKindData::Messages && history_topic)
    {
        None
    } else if data.done_in.contains(&mode) {
        Some(format!("already done in {}", mode.name()))
    } else if mode == ModeKindData::Todo {
        Some("no open to-do on this conversation".to_string())
    } else {
        Some(format!("not in {}", mode.name()))
    };
    let refusal = refusal.or_else(|| {
        (mode == ModeKindData::Todo && ticked.is_empty())
            .then(|| "that to-do is not open on this conversation".to_string())
    });
    if let Some(refusal) = refusal {
        let mut plan = Plan::failed(&data.thread_id, mode, refusal);
        plan.account_id = Some(data.account_id.clone());
        plan.provider = provider;
        return plan;
    }
    let mark = mark_name(mode)
        .zip(DeskDismissal::through(
            placement
                .messages
                .iter()
                .filter(|m| exact.is_none_or(|exact| exact.covers(m))),
        ))
        .map(|(name, through)| ModeDoneMark {
            account_id: data.account_id.clone(),
            thread_id: data.thread_id.clone(),
            mode: name.to_string(),
            through,
        });
    let still_in: Vec<StillIn> = data
        .modes
        .iter()
        .map(|entry| entry.mode)
        .filter(|held| held.holds_inbox())
        .filter(|held| {
            *held != mode
                || (mode == ModeKindData::Todo && !still_open.is_empty())
                || exact.is_some_and(|exact| {
                    placement.messages.iter().any(|m| {
                        m.in_inbox
                            && !m.trashed
                            && m.direction != "outbound"
                            && !exact.ids.contains(&m.id)
                    })
                })
        })
        .map(|held| StillIn {
            mode: held,
            detail: (held == ModeKindData::Todo)
                .then(|| {
                    let open = if mode == ModeKindData::Todo {
                        &still_open
                    } else {
                        &placement.todos
                    };
                    open.iter()
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
    let files_record = ticked
        .iter()
        .any(|todo| super::records::todo_files_record(&todo.kind));
    Plan {
        thread_id: data.thread_id.clone(),
        account_id: Some(data.account_id.clone()),
        mode,
        mark,
        todos: if mode == ModeKindData::Todo {
            ticked
        } else {
            Vec::new()
        },
        left_in_inbox: last && !archive_on_last_done && !live_in_inbox.is_empty(),
        filed: mode == ModeKindData::Todo
            && (data
                .modes
                .iter()
                .any(|entry| entry.mode == ModeKindData::Archive)
                || files_record),
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
    let mut changed: HashSet<MessageId> = HashSet::new();
    let mut account_errors: HashMap<AccountId, String> = HashMap::new();
    let mut batch_error: Option<String> = None;
    if !archive_ids.is_empty() {
        let cmd = MutationCommand::ReadAndArchive {
            message_ids: archive_ids,
        };
        match Box::pin(apply_mutation_batch(state, &cmd, &mutation_id, None)).await {
            Ok(batch) => {
                changed.extend(batch.changed.iter().map(|s| s.message_id.clone()));
                snapshots.extend(batch.changed);
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
    let mut put_away_failed = false;
    if let Err(error) = Box::pin(put_away(state, &mut plans, &mut desk)).await {
        put_away_failed = true;
        tracing::warn!(%error, "mode done could not record its marks or to-dos");
        for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
            plan.error = Some(format!("couldn't mark it done ({error}); run done again"));
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

    changed_accounts.extend(
        plans
            .iter()
            .filter(|plan| put_away_failed || plan.error.is_none())
            .filter_map(|plan| plan.account_id.clone()),
    );
    for account in &changed_accounts {
        super::arrivals::modes_changed(state, account);
    }

    for plan in &plans {
        if let Some(error) = &plan.error {
            tracing::warn!(thread_id = %plan.thread_id, mode = %plan.mode.name(), %error,
                "could not mark conversation done");
        }
    }

    Ok(ResponseData::ModeDone {
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
    for plan in plans.iter_mut().filter(|plan| plan.error.is_none()) {
        if plan.todos.is_empty() {
            continue;
        }
        let ids: Vec<String> = plan.todos.iter().map(|todo| todo.id.clone()).collect();
        let changed = state
            .store
            .set_todos_state(&ids, &[TodoState::Open], TodoState::Done, now)
            .await?;
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
        super::records::file_ticked_todos(state, &plan.todos).await;
    }
    Ok(())
}

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
