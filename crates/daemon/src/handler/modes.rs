use super::desk::{self_matcher, Senders};
use super::desk_lanes::{
    clean_subject, is_outbound, thread_lanes, AccountInputs, DESK_WINDOW_DAYS,
    SCREENER_NEW_SENDER_DAYS,
};
use super::desk_timers::DeskTimers;
use super::mail_kind::{self, KindSignals};
use super::mode_rules::{
    mark_covers, membership, merge_marks, messages_membership, quiet_membership, screener_question,
};
use super::places::{scoped_cached_accounts, Placed};
use super::todo_view::{now_order, to_data};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Local, Utc};
use futures::{StreamExt, TryStreamExt};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_protocol::{
    mode_guide, rail_copy, DeskRowData, ModeKindData, RailData, RailEntryData, RailLinkData,
    RailStatusData, ResponseData, SenderKindData, ThreadModesData,
};
use mxr_store::{ArchiveRecord, DeskDismissal, DeskMessage, ScreenerDisposition, TodoRecord};
use std::collections::{HashMap, HashSet};

pub(crate) const MEMBERSHIP_MAX_THREADS: usize = 100;

pub(super) const fn mark_name(mode: ModeKindData) -> Option<&'static str> {
    match mode {
        ModeKindData::Messages => Some("messages"),
        ModeKindData::Updates => Some("updates"),
        ModeKindData::Reading => Some("reading"),
        ModeKindData::Todo | ModeKindData::Archive => None,
    }
}

pub(super) const fn is_early(mode: ModeKindData) -> bool {
    !matches!(
        mode,
        ModeKindData::Todo
            | ModeKindData::Messages
            | ModeKindData::Updates
            | ModeKindData::Reading
            | ModeKindData::Archive
    )
}

pub(super) async fn messages_dismissals(
    state: &AppState,
    account_id: &AccountId,
) -> Result<HashMap<ThreadId, DeskDismissal>, HandlerError> {
    let mark = mark_name(ModeKindData::Messages).unwrap_or("messages");
    Ok(merge_marks(
        state.store.desk_dismissals(account_id).await?,
        state.store.mode_done_marks(account_id, mark).await?,
    ))
}

pub(super) struct Placement {
    pub data: ThreadModesData,
    pub quiet: bool,
    pub messages: Vec<DeskMessage>,
    pub todos: Vec<TodoRecord>,
    pub provider: Option<mxr_core::types::ProviderKind>,
}

pub(super) fn signals<'a>(
    message: &'a DeskMessage,
    senders: &'a Senders,
    is_self: &dyn Fn(&str) -> bool,
) -> KindSignals<'a> {
    senders.signals(message, is_self)
}

pub(super) async fn place_threads(
    state: &AppState,
    account_id: &AccountId,
    thread_ids: &[ThreadId],
    now: DateTime<Utc>,
) -> Result<Vec<Placement>, HandlerError> {
    place_threads_with_evidence(state, account_id, thread_ids, now, true, None).await
}

pub(super) async fn place_threads_windowed(
    state: &AppState,
    account_id: &AccountId,
    thread_ids: &[ThreadId],
    now: DateTime<Utc>,
    evidence_limit: usize,
) -> Result<Vec<Placement>, HandlerError> {
    place_threads_with_evidence(
        state,
        account_id,
        thread_ids,
        now,
        true,
        Some(evidence_limit),
    )
    .await
}

async fn place_threads_with_evidence(
    state: &AppState,
    account_id: &AccountId,
    thread_ids: &[ThreadId],
    now: DateTime<Utc>,
    include_evidence: bool,
    evidence_limit: Option<usize>,
) -> Result<Vec<Placement>, HandlerError> {
    let store = &state.store;
    let mut messages = store
        .desk_messages_in_threads(account_id, thread_ids)
        .await?;
    if messages.is_empty() {
        return Ok(Vec::new());
    }
    let message_ids: Vec<_> = messages.iter().map(|m| m.id.clone()).collect();
    let externally_filed = store
        .externally_filed_messages(Some(account_id), &message_ids)
        .await?;
    let mut senders = Senders::load(state, account_id, &messages).await?;
    let mut detailed_asks = HashMap::new();
    if include_evidence {
        detailed_asks = store.asks_for_messages(account_id, &message_ids).await?;
        detailed_asks.retain(|id, answer| {
            senders
                .asks
                .get(id)
                .is_some_and(|view| view.input_fingerprint == answer.input_fingerprint)
        });
        senders.asks.retain(|id, _| detailed_asks.contains_key(id));
        senders
            .assessments
            .retain(|id, _| senders.asks.contains_key(id));
    }
    let invites: Vec<_> = messages
        .iter()
        .filter(|m| m.is_invite)
        .map(|m| m.id.clone())
        .collect();
    let released =
        super::home_projection::released_specialists(state, account_id, &invites, now).await?;
    for message in &mut messages {
        if released.contains(&message.id) {
            message.in_inbox = false;
        }
    }
    let is_self = self_matcher(state, account_id).await?;
    let dismissed = messages_dismissals(state, account_id).await?;
    let window_start = now - Duration::days(DESK_WINDOW_DAYS);
    let timers = DeskTimers::new(
        store.desk_reply_later(account_id).await?,
        store.desk_reminders(account_id, window_start).await?,
    );

    let back: HashSet<&ThreadId> = timers.maybe_back(now).collect();
    let future_cutoff = now + Duration::days(1);
    let lane_messages: Vec<DeskMessage> = messages
        .chunk_by(|a, b| a.thread_id == b.thread_id)
        .filter(|thread| {
            back.contains(&thread[0].thread_id)
                || thread
                    .iter()
                    .any(|m| m.date >= window_start && m.date <= future_cutoff)
        })
        .flatten()
        .cloned()
        .collect();
    let lanes = thread_lanes(&AccountInputs {
        pending_threads: &senders.pending_threads(&messages, &is_self),
        assessments: &senders.assessments,
        asks: &senders.asks,
        account_id,
        messages: &lane_messages,
        contacts: &senders.contacts,
        written_to: &senders.written_to,
        screener: &senders.screener,
        dismissed: &dismissed,
        timers: &timers,
        is_self: &is_self,
        moves: &senders.moves,
        treatments: &senders.treatments,
        shape: super::conversation_shape::shape_config(state),
        now,
    });
    let rows: HashMap<ThreadId, DeskRowData> = lanes
        .rows
        .into_iter()
        .map(|draft| (draft.row.thread_id.clone(), draft.row))
        .collect();

    let mut todos: HashMap<ThreadId, Vec<TodoRecord>> = HashMap::new();
    for record in store.open_todos_for_threads(account_id, thread_ids).await? {
        if let Some(thread) = record.thread_id.clone() {
            todos.entry(thread).or_default().push(record);
        }
    }
    let marks = store.mode_done_for_threads(account_id, thread_ids).await?;
    let mut records: HashMap<ThreadId, ArchiveRecord> = HashMap::new();
    for (thread, record) in store.archive_records_for_threads(thread_ids).await? {
        records.entry(thread).or_insert(record);
    }
    let mut maybe_new: Vec<String> = messages
        .iter()
        .filter(|m| {
            !is_outbound(m, &is_self) && m.date >= now - Duration::days(SCREENER_NEW_SENDER_DAYS)
        })
        .map(|m| m.from.email.to_ascii_lowercase())
        .filter(|email| {
            senders
                .screener
                .get(email)
                .is_none_or(|decision| *decision == ScreenerDisposition::Unknown)
        })
        .collect();
    maybe_new.sort_unstable();
    maybe_new.dedup();
    let written_to = store.addresses_written_to(account_id, &maybe_new).await?;
    let provider = store
        .get_account(account_id)
        .await?
        .and_then(|account| account.sync_backend)
        .map(|backend| backend.provider_kind);

    let mut placements = Vec::new();
    for thread in messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
        let thread_id = thread[0].thread_id.clone();
        let mut thread_todos = todos.remove(&thread_id).unwrap_or_default();
        thread_todos.sort_by(|a, b| now_order(a, b, now));
        let data = place_one(&PlaceInputs {
            account_id,
            thread,
            row: rows.get(&thread_id),
            put_away: dismissed.get(&thread_id),
            todos: &thread_todos,
            record: records.get(&thread_id),
            marks: &marks,
            senders: &senders,
            detailed_asks: &detailed_asks,
            evidence_limit,
            written_to: &written_to,
            externally_filed: &externally_filed,
            is_self: &is_self,
            now,
        });
        let quiet = !rows.contains_key(&thread_id)
            && data
                .modes
                .iter()
                .any(|entry| entry.mode == ModeKindData::Messages);
        placements.push(Placement {
            quiet,
            data,
            messages: thread.to_vec(),
            todos: thread_todos,
            provider: provider.clone(),
        });
    }
    Ok(placements)
}

struct PlaceInputs<'a> {
    account_id: &'a AccountId,
    thread: &'a [DeskMessage],
    row: Option<&'a DeskRowData>,
    put_away: Option<&'a DeskDismissal>,
    todos: &'a [TodoRecord],
    record: Option<&'a ArchiveRecord>,
    marks: &'a HashMap<(ThreadId, String), DeskDismissal>,
    senders: &'a Senders,
    detailed_asks: &'a HashMap<mxr_core::MessageId, mxr_core::asks::AskAssessment>,
    evidence_limit: Option<usize>,
    written_to: &'a HashSet<String>,
    externally_filed: &'a HashMap<mxr_core::MessageId, DateTime<Utc>>,
    is_self: &'a dyn Fn(&str) -> bool,
    now: DateTime<Utc>,
}

fn place_one(inputs: &PlaceInputs<'_>) -> ThreadModesData {
    let thread = inputs.thread;
    let thread_id = &thread[0].thread_id;
    let mark = |mode: ModeKindData| {
        mark_name(mode).and_then(|name| inputs.marks.get(&(thread_id.clone(), name.to_string())))
    };
    let mut modes = Vec::new();
    let mut done_in = Vec::new();

    if let Some(row) = inputs.row {
        modes.push(messages_membership(row, is_early(ModeKindData::Messages)));
    } else if let Some(message) = quiet(inputs) {
        modes.push(quiet_membership(
            &message.from,
            message.date,
            inputs.now,
            is_early(ModeKindData::Messages),
        ));
    }
    if mark(ModeKindData::Messages).is_some_and(|mark| mark.covers(thread)) {
        done_in.push(ModeKindData::Messages);
    }

    if let Some(first) = inputs.todos.first() {
        let shown = to_data(first, inputs.now);
        let mut entry = membership(
            ModeKindData::Todo,
            shown.why.clone(),
            format!("Also in To do: {}, {}", shown.title, shown.when_label),
            is_early(ModeKindData::Todo),
            "task",
        );
        entry.todo_ids = inputs.todos.iter().map(|todo| todo.id.clone()).collect();
        modes.push(entry);
    }

    let inbound = |m: &&DeskMessage| !is_outbound(m, inputs.is_self) && !m.trashed;
    let mut by_kind: HashMap<SenderKindData, Vec<&DeskMessage>> = HashMap::new();
    for message in thread.iter().filter(inbound) {
        if !(message.in_inbox || inputs.externally_filed.contains_key(&message.id))
            || message.snoozed
            || !inputs.senders.assessments.contains_key(&message.id)
        {
            continue;
        }
        let kind = mail_kind::classify(&signals(message, inputs.senders, inputs.is_self))
            .kind
            .to_data();
        by_kind.entry(kind).or_default().push(message);
    }
    for (mode, kind) in [
        (ModeKindData::Updates, SenderKindData::PaperTrail),
        (ModeKindData::Reading, SenderKindData::Reading),
    ] {
        let Some(held) = by_kind.get(&kind) else {
            continue;
        };
        if modes
            .iter()
            .any(|entry: &mxr_protocol::ModeMembershipData| entry.mode == mode)
            || done_in.contains(&mode)
        {
            continue;
        }
        if mark(mode).is_some_and(|mark| mark_covers(mark, held.iter().map(|m| (m.date, &m.id)))) {
            done_in.push(mode);
            continue;
        }
        let Some(latest) = held.iter().max_by_key(|m| (m.date, m.seq)) else {
            continue;
        };
        let described = mail_kind::describe(&signals(latest, inputs.senders, inputs.is_self));
        let source = if described.corrected { "you" } else { "rule" };
        let sender = latest
            .from
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&latest.from.email);
        modes.push(membership(
            mode,
            format!("Here because: {} ({source}).", described.reason),
            format!("Also in {}: from {sender}", mode.name()),
            is_early(mode),
            mail_kind::rule_tag(described.rule),
        ));
    }

    if let Some(record) = inputs.record {
        let what = [record.issuer.as_deref(), record.title.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
        modes.push(membership(
            ModeKindData::Archive,
            format!(
                "Here because: {} ({}).",
                record.reason,
                if record.checked {
                    "checked"
                } else {
                    "unchecked"
                }
            ),
            format!(
                "Also in Archive: {}",
                if what.is_empty() {
                    record.kind.clone()
                } else {
                    what
                }
            ),
            is_early(ModeKindData::Archive),
            "record",
        ));
    }

    for entry in &mut modes {
        if let Some(assessment) = thread
            .iter()
            .rev()
            .filter_map(|m| inputs.senders.assessments.get(&m.id))
            .find(|a| a.home == entry.mode.id())
        {
            let source = if assessment.not_sure.is_some() {
                "Not sure"
            } else if assessment.deciding.source == "you" {
                "you"
            } else {
                "rule"
            };
            entry.reason = format!("Here because: {} ({source}).", assessment.deciding.reason);
        }
    }
    modes.sort_by_key(|entry| entry.mode);
    done_in.sort();
    let held_by = modes
        .iter()
        .map(|entry| entry.mode)
        .filter(|mode| mode.holds_inbox())
        .collect();
    let latest = thread.iter().max_by_key(|m| (m.date, m.seq));
    let evidence: &[DeskMessage] = match inputs.evidence_limit {
        Some(limit) => &thread[thread.len().saturating_sub(limit)..],
        None => thread,
    };
    let asks: Vec<_> = evidence
        .iter()
        .filter_map(|m| {
            inputs
                .detailed_asks
                .get(&m.id)
                .map(mxr_core::asks::AskAssessment::summary)
        })
        .take(32)
        .collect();
    let detailed_ask_count = thread
        .iter()
        .filter(|m| inputs.detailed_asks.contains_key(&m.id))
        .count();
    ThreadModesData {
        asks_more: detailed_ask_count > asks.len(),
        asks,
        asks_pending: evidence
            .iter()
            .filter(|m| !inputs.senders.asks.contains_key(&m.id))
            .map(|m| m.id.clone())
            .collect(),
        assessments: evidence
            .iter()
            .filter_map(|m| inputs.senders.assessments.get(&m.id).cloned())
            .collect(),
        account_id: inputs.account_id.clone(),
        thread_id: thread_id.clone(),
        subject: latest
            .map(|message| clean_subject(&message.subject))
            .unwrap_or_default(),
        modes,
        done_in,
        held_by,
        in_inbox: thread.iter().any(|m| m.in_inbox && !m.trashed),
        new_sender: new_sender(inputs),
    }
}

fn quiet<'a>(inputs: &PlaceInputs<'a>) -> Option<&'a DeskMessage> {
    let thread = inputs.thread;
    if inputs.put_away.is_some_and(|mark| mark.covers(thread)) {
        return None;
    }
    thread
        .iter()
        .filter(|m| m.in_inbox && !m.trashed && !m.snoozed && !is_outbound(m, inputs.is_self))
        .filter(|m| inputs.senders.assessments.contains_key(&m.id))
        .filter(|m| {
            mail_kind::classify(&signals(m, inputs.senders, inputs.is_self)).kind
                == mail_kind::SenderKind::Person
        })
        .max_by_key(|m| (m.date, m.seq))
}

fn new_sender(inputs: &PlaceInputs<'_>) -> Option<mxr_protocol::ScreenerQuestionData> {
    let thread = inputs.thread;
    let latest = thread
        .iter()
        .filter(|m| !is_outbound(m, inputs.is_self) && !m.trashed)
        .max_by_key(|m| (m.date, m.seq))?;
    let assessment = inputs.senders.assessments.get(&latest.id)?;
    let key = latest.from.email.to_ascii_lowercase();
    if inputs
        .senders
        .screener
        .get(&key)
        .is_some_and(|decision| *decision != ScreenerDisposition::Unknown)
    {
        return None;
    }
    let contact = inputs.senders.contacts.get(&key);
    let since = inputs.now - Duration::days(SCREENER_NEW_SENDER_DAYS);
    let wrote_to = contact.is_some_and(|c| c.total_outbound > 0)
        || inputs.written_to.contains(&key)
        || thread.iter().any(|m| is_outbound(m, inputs.is_self));
    if wrote_to || contact.is_some_and(|c| c.first_seen_at < since) || latest.date < since {
        return None;
    }
    let person = assessment.who == "person";
    if !person {
        return None;
    }
    screener_question(inputs.account_id, &key, SenderKindData::People)
}

pub(super) async fn threads_by_account(
    state: &AppState,
    thread_ids: &[ThreadId],
) -> Result<Vec<(AccountId, Vec<ThreadId>)>, HandlerError> {
    let owners: HashMap<ThreadId, AccountId> = state
        .store
        .get_threads_batch(thread_ids)
        .await?
        .into_iter()
        .map(|thread| (thread.id, thread.account_id))
        .collect();
    let mut grouped: Vec<(AccountId, Vec<ThreadId>)> = Vec::new();
    for thread_id in thread_ids {
        let Some(account) = owners.get(thread_id) else {
            continue;
        };
        match grouped.iter_mut().find(|(owner, _)| owner == account) {
            Some((_, threads)) => threads.push(thread_id.clone()),
            None => grouped.push((account.clone(), vec![thread_id.clone()])),
        }
    }
    Ok(grouped)
}

pub(super) async fn get_membership(
    state: &AppState,
    message_id: Option<&MessageId>,
    thread_id: Option<&ThreadId>,
    thread_ids: &[ThreadId],
    limit: Option<u32>,
) -> HandlerResult {
    let mut wanted: Vec<ThreadId> = thread_ids.to_vec();
    wanted.extend(thread_id.cloned());
    if let Some(message_id) = message_id {
        let envelope = state
            .store
            .get_envelope(message_id)
            .await?
            .ok_or_else(|| HandlerError::from(format!("Message not found: {message_id}")))?;
        wanted.push(envelope.thread_id);
    }
    let mut seen = HashSet::new();
    wanted.retain(|thread| seen.insert(thread.clone()));
    if wanted.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "name a thread_id, a message_id or thread_ids".into(),
        ));
    }
    if wanted.len() > MEMBERSHIP_MAX_THREADS {
        return Err(HandlerError::InvalidRequest(format!(
            "at most {MEMBERSHIP_MAX_THREADS} threads at once"
        )));
    }
    let now = Utc::now();
    let mut placed: HashMap<ThreadId, ThreadModesData> = HashMap::new();
    for (account, threads) in threads_by_account(state, &wanted).await? {
        let placements = match limit {
            Some(limit) => {
                place_threads_windowed(state, &account, &threads, now, limit as usize).await?
            }
            None => place_threads(state, &account, &threads, now).await?,
        };
        for placement in placements {
            placed.insert(placement.data.thread_id.clone(), placement.data);
        }
    }
    Ok(ResponseData::ModeMembership {
        threads: wanted
            .iter()
            .filter_map(|thread| placed.remove(thread))
            .collect(),
    })
}

pub(super) struct InboxModes {
    pub updates: Vec<Placed>,
    pub reading: Vec<Placed>,
    pub people_threads: HashMap<AccountId, Vec<ThreadId>>,
}

pub(super) async fn inbox_modes(
    state: &AppState,
    accounts: &[AccountId],
) -> Result<InboxModes, HandlerError> {
    let inbox = super::places::scan_inbox(state, accounts, None).await?;
    let placed = without_done(
        state,
        accounts,
        &[ModeKindData::Updates, ModeKindData::Reading],
        inbox.mail,
    )
    .await?;
    let mut out = InboxModes {
        updates: Vec::new(),
        reading: Vec::new(),
        people_threads: inbox.people_threads,
    };
    for item in placed {
        match item.kind.kind {
            SenderKindData::PaperTrail => out.updates.push(item),
            SenderKindData::Reading => out.reading.push(item),
            _ => {}
        }
    }
    let newest_first = |a: &Placed, b: &Placed| {
        b.message
            .date
            .cmp(&a.message.date)
            .then_with(|| b.message.seq.cmp(&a.message.seq))
    };
    out.updates.sort_by(newest_first);
    out.reading.sort_by(newest_first);
    Ok(out)
}

pub(super) async fn without_done(
    state: &AppState,
    accounts: &[AccountId],
    modes: &[ModeKindData],
    placed: Vec<Placed>,
) -> Result<Vec<Placed>, HandlerError> {
    let mut marks: HashMap<(AccountId, &'static str), HashMap<ThreadId, DeskDismissal>> =
        HashMap::new();
    for account in accounts {
        for &mode in modes {
            let name = mark_name(mode).unwrap_or_default();
            marks.insert(
                (account.clone(), name),
                state.store.mode_done_marks(account, name).await?,
            );
        }
    }
    let mut threads: HashMap<(AccountId, ThreadId, SenderKindData), Vec<Placed>> = HashMap::new();
    for item in placed {
        let key = (
            item.message.account_id.clone(),
            item.message.thread_id.clone(),
            item.kind.kind,
        );
        threads.entry(key).or_default().push(item);
    }
    let mut kept = Vec::new();
    for ((account, thread, kind), items) in threads {
        let mode = match kind {
            SenderKindData::PaperTrail => ModeKindData::Updates,
            SenderKindData::Reading => ModeKindData::Reading,
            _ => {
                kept.extend(items);
                continue;
            }
        };
        let name = mark_name(mode).unwrap_or_default();
        let covered = marks
            .get(&(account, name))
            .and_then(|marks| marks.get(&thread))
            .is_some_and(|mark| {
                mark_covers(
                    mark,
                    items
                        .iter()
                        .map(|item| (item.message.date, &item.message.id)),
                )
            });
        if !covered {
            kept.extend(items);
        }
    }
    Ok(kept)
}

const fn early_note(mode: ModeKindData) -> Option<&'static str> {
    match mode {
        ModeKindData::Todo
        | ModeKindData::Messages
        | ModeKindData::Updates
        | ModeKindData::Reading
        | ModeKindData::Archive => None,
    }
}

fn mode_entry(mode: ModeKindData, count: Option<u32>) -> RailEntryData {
    RailEntryData {
        id: mode.id().to_string(),
        name: mode.name().to_string(),
        key: mode.key().to_string(),
        group: "modes".to_string(),
        count,
        badge: None,
        status: if is_early(mode) {
            RailStatusData::Early
        } else {
            RailStatusData::Built
        },
        early_note: early_note(mode).map(str::to_string),
        header: mode_guide(mode.id()).map(|guide| guide.header.to_string()),
        quiet: None,
    }
}

pub(super) async fn quiet_count(
    state: &AppState,
    people: HashMap<AccountId, Vec<ThreadId>>,
    now: DateTime<Utc>,
) -> Result<u32, HandlerError> {
    let counts: Vec<_> = futures::stream::iter(people)
        .map(|(account, threads)| quiet_account(state, account, threads, now))
        .buffered(3)
        .try_collect()
        .await?;
    Ok(counts.into_iter().sum())
}

async fn quiet_account(
    state: &AppState,
    account: AccountId,
    threads: Vec<ThreadId>,
    now: DateTime<Utc>,
) -> Result<u32, HandlerError> {
    let mut quiet = 0u32;
    for chunk in threads.chunks(MEMBERSHIP_MAX_THREADS) {
        let placed = place_threads_with_evidence(state, &account, chunk, now, false, None).await?;
        quiet += u32::try_from(placed.iter().filter(|p| p.quiet).count()).unwrap_or(u32::MAX);
    }
    Ok(quiet)
}

pub(super) async fn get_rail(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    let now = Utc::now();
    let accounts = scoped_cached_accounts(state, account_id).await?;
    let (snapshot, quiet) =
        super::now::rail_snapshot(state, account_id, &accounts, now, &Local).await?;
    let count = |n: usize| Some(super::now::count(n));
    let work = Some(snapshot.badge());
    let mut entries = vec![RailEntryData {
        id: "now".to_string(),
        name: "Now".to_string(),
        key: "g h".to_string(),
        group: "home".to_string(),
        count: work,
        badge: work,
        status: RailStatusData::Built,
        early_note: None,
        header: mode_guide("now").map(|guide| guide.header.to_string()),
        quiet: None,
    }];
    entries.extend([
        RailEntryData {
            quiet: Some(quiet),
            ..mode_entry(ModeKindData::Messages, Some(snapshot.your_turn_total()))
        },
        mode_entry(ModeKindData::Todo, count(snapshot.due_now.len())),
        mode_entry(
            ModeKindData::Updates,
            Some(snapshot.digest.message_count + snapshot.digest.since.message_count),
        ),
        mode_entry(ModeKindData::Reading, None),
        mode_entry(ModeKindData::Archive, None),
    ]);
    entries.push(RailEntryData {
        id: "inbox".to_string(),
        name: "Inbox".to_string(),
        key: "g i".to_string(),
        group: "lens".to_string(),
        count: None,
        badge: None,
        status: RailStatusData::Built,
        early_note: None,
        header: Some(rail_copy::INBOX_HEADER.to_string()),
        quiet: None,
    });
    let link = |id: &str, name: &str, key: Option<&str>, note: Option<&str>| RailLinkData {
        id: id.to_string(),
        name: name.to_string(),
        key: key.map(str::to_string),
        note: note.map(str::to_string),
    };
    let more = vec![
        link(
            "screener",
            "Screener",
            Some("g S"),
            Some("Every sender you've decided on. A new sender is asked about on their row."),
        ),
        link("snoozed", "Snoozed", None, None),
        link("invites", "Invites", Some("g v"), None),
        link("deliveries", "Deliveries", None, None),
        link("subscriptions", "Subscriptions", None, None),
        link("drafts", "Drafts", Some("g d"), None),
        link("sent", "Sent", Some("g t"), None),
        link("starred", "Starred", Some("g s"), None),
        link("all_mail", "All mail", Some("g a"), None),
    ];
    Ok(ResponseData::Rail {
        rail: RailData {
            generated_at: now,
            entries,
            more,
        },
    })
}
