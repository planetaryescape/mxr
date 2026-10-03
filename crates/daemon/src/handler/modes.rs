//! Mode membership and the rail (blueprint 22, phase 2).
//!
//! Which modes hold a thread is computed, never stored (D097):
//!
//! - Messages: the desk's thread lanes (You owe, New from people, Waiting
//!   on), so Messages, Now's People and the desk share one owed rule.
//! - To do: the thread's open to-do rows.
//! - Updates and Reading: inbox mail the sender classifier puts in Paper
//!   trail or Reading, read through `places::placed_inbox`, so each mode
//!   and its early view never disagree.
//! - Archive: automated mail that is a record (`mode_rules`). Archive
//!   never keeps a thread in the provider's inbox.
//!
//! minus each mode's done mark (`mode_done`, the desk's watermark).

use super::desk::{self_matcher, Senders};
use super::desk_lanes::{
    clean_subject, is_outbound, thread_lanes, AccountInputs, DESK_WINDOW_DAYS,
    SCREENER_NEW_SENDER_DAYS,
};
use super::desk_timers::DeskTimers;
use super::mail_kind::{self, KindSignals};
use super::mode_rules::{
    mark_covers, membership, merge_marks, messages_membership, quiet_membership, record_evidence,
    screener_question,
};
use super::places::{placed_inbox, scoped_accounts, Placed};
use super::todo_view::{now_order, to_data};
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Local, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::UnsubscribeMethod;
use mxr_protocol::{
    mode_guide, rail_copy, DeskRowData, ModeKindData, RailData, RailEntryData, RailLinkData,
    RailStatusData, ResponseData, SenderKindData, ThreadModesData,
};
use mxr_store::{DeskDismissal, DeskMessage, ScreenerDisposition, TodoRecord};
use std::collections::{HashMap, HashSet};

/// The most threads one membership request may name.
pub(crate) const MEMBERSHIP_MAX_THREADS: usize = 100;

/// The mode names `mode_done` stores.
pub(super) const fn mark_name(mode: ModeKindData) -> Option<&'static str> {
    match mode {
        ModeKindData::Messages => Some("messages"),
        ModeKindData::Updates => Some("updates"),
        ModeKindData::Reading => Some("reading"),
        ModeKindData::Todo | ModeKindData::Archive => None,
    }
}

/// Built in its researched shape (To do) or an early version on an
/// existing view (the rest, for now).
const fn is_early(mode: ModeKindData) -> bool {
    !matches!(mode, ModeKindData::Todo)
}

/// The threads the desk's Done or done-in-Messages put away, as one map
/// the lane rules read.
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

/// One thread placed in its modes, with what done needs to act on it.
pub(super) struct Placement {
    pub data: ThreadModesData,
    /// In Messages as quiet: person mail in the inbox no lane holds.
    pub quiet: bool,
    /// Every message of the thread, in storage order within date order.
    pub messages: Vec<DeskMessage>,
    /// Open to-do rows on the thread.
    pub todos: Vec<TodoRecord>,
    pub provider: Option<mxr_core::types::ProviderKind>,
}

/// The kind signals for one desk message, as the desk and places build
/// them.
pub(super) fn signals<'a>(message: &'a DeskMessage, senders: &Senders) -> KindSignals<'a> {
    let key = message.from.email.to_ascii_lowercase();
    KindSignals {
        email: &message.from.email,
        has_list_id: message.list_id.is_some(),
        has_unsubscribe: !matches!(message.unsubscribe, UnsubscribeMethod::None),
        is_delivery: message.is_delivery,
        is_invite: message.is_invite,
        list_sender: senders
            .contacts
            .get(&key)
            .is_some_and(|contact| contact.is_list_sender),
        decision: senders.screener.get(&key).copied(),
    }
}

/// Place `thread_ids` of one account in their modes at `now`. Threads
/// with no stored message are skipped.
pub(super) async fn place_threads(
    state: &AppState,
    account_id: &AccountId,
    thread_ids: &[ThreadId],
    now: DateTime<Utc>,
) -> Result<Vec<Placement>, HandlerError> {
    let store = &state.store;
    let messages = store
        .desk_messages_in_threads(account_id, thread_ids)
        .await?;
    if messages.is_empty() {
        return Ok(Vec::new());
    }
    let senders = Senders::load(state, account_id, &messages).await?;
    let is_self = self_matcher(state, account_id).await?;
    let dismissed = messages_dismissals(state, account_id).await?;
    let window_start = now - Duration::days(DESK_WINDOW_DAYS);
    let timers = DeskTimers::new(
        store.desk_reply_later(account_id).await?,
        store.desk_reminders(account_id, window_start).await?,
    );

    // The desk reads threads active in its window, plus any a time you set
    // brought back; the same here, so Messages agrees with the desk.
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
        account_id,
        messages: &lane_messages,
        contacts: &senders.contacts,
        screener: &senders.screener,
        dismissed: &dismissed,
        timers: &timers,
        is_self: &is_self,
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
    // Who you have ever written to, among the senders a new-sender
    // question could be about: the lanes above only saw these threads.
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
            marks: &marks,
            senders: &senders,
            written_to: &written_to,
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
    /// Messages' own put-away mark: the desk's Done or done in Messages.
    put_away: Option<&'a DeskDismissal>,
    todos: &'a [TodoRecord],
    marks: &'a HashMap<(ThreadId, String), DeskDismissal>,
    senders: &'a Senders,
    /// Senders you have ever written to, anywhere in the account.
    written_to: &'a HashSet<String>,
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

    // Messages: the lane rules already left out a thread put away.
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
        );
        entry.todo_ids = inputs.todos.iter().map(|todo| todo.id.clone()).collect();
        modes.push(entry);
    }

    // Updates and Reading read inbox mail, as their early views do.
    let inbound = |m: &&DeskMessage| !is_outbound(m, inputs.is_self) && !m.trashed;
    let mut by_kind: HashMap<SenderKindData, Vec<&DeskMessage>> = HashMap::new();
    for message in thread.iter().filter(inbound) {
        if !message.in_inbox || message.snoozed || message.is_delivery || message.is_invite {
            continue;
        }
        let kind = mail_kind::classify(&signals(message, inputs.senders))
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
        if mark(mode).is_some_and(|mark| mark_covers(mark, held.iter().map(|m| (m.date, &m.id)))) {
            done_in.push(mode);
            continue;
        }
        let Some(latest) = held.iter().max_by_key(|m| (m.date, m.seq)) else {
            continue;
        };
        let described = mail_kind::describe(&signals(latest, inputs.senders));
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
        ));
    }

    // Archive: a record stays filed whatever happens in the inbox.
    if let Some((message, evidence)) = thread.iter().filter(inbound).find_map(|message| {
        let automated = mail_kind::classify(&signals(message, inputs.senders)).kind
            == mail_kind::SenderKind::Automated;
        automated
            .then(|| record_evidence(&message.from.email, &message.subject))
            .flatten()
            .map(|evidence| (message, evidence))
    }) {
        modes.push(membership(
            ModeKindData::Archive,
            format!("Here because: looks like a record, {evidence} (rule)."),
            format!("Also in Archive: {}", clean_subject(&message.subject)),
            is_early(ModeKindData::Archive),
        ));
    }

    modes.sort_by_key(|entry| entry.mode);
    done_in.sort();
    let held_by = modes
        .iter()
        .map(|entry| entry.mode)
        .filter(|mode| mode.holds_inbox())
        .collect();
    let latest = thread.iter().max_by_key(|m| (m.date, m.seq));
    ThreadModesData {
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

/// Person mail still in the inbox that no lane holds: nobody's turn, but
/// nobody let it go either. Messages keeps it as Quiet until it is done
/// there, so inbox mail never vanishes from every mode and a to-do ticked
/// off on it never archives it by default.
fn quiet<'a>(inputs: &PlaceInputs<'a>) -> Option<&'a DeskMessage> {
    let thread = inputs.thread;
    if inputs.put_away.is_some_and(|mark| mark.covers(thread)) {
        return None;
    }
    thread
        .iter()
        .filter(|m| m.in_inbox && !m.trashed && !is_outbound(m, inputs.is_self))
        .filter(|m| {
            mail_kind::classify(&signals(m, inputs.senders)).kind == mail_kind::SenderKind::Person
        })
        .max_by_key(|m| (m.date, m.seq))
}

/// A first-time person wrote the thread's latest inbound message: no
/// decision about them, first seen in the last two weeks, and you have
/// never written to them (D104, D117).
fn new_sender(inputs: &PlaceInputs<'_>) -> Option<mxr_protocol::ScreenerQuestionData> {
    let thread = inputs.thread;
    let latest = thread
        .iter()
        .filter(|m| !is_outbound(m, inputs.is_self) && !m.trashed)
        .max_by_key(|m| (m.date, m.seq))?;
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
    // Only a person is asked about. A machine's mail goes to its mode
    // silently, and its row's why line says which rule put it there.
    let kind = mail_kind::classify(&signals(latest, inputs.senders)).kind;
    if kind != mail_kind::SenderKind::Person {
        return None;
    }
    screener_question(inputs.account_id, &key, kind.to_data())
}

/// Group threads by the account that owns them, in request order.
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
        for placement in place_threads(state, &account, &threads, now).await? {
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

/// What Updates and Reading hold across the inbox, after their done
/// marks: the early views' mail, for Now's card and pick and the rail.
pub(super) struct InboxModes {
    pub updates: Vec<Placed>,
    pub reading: Vec<Placed>,
}

pub(super) async fn inbox_modes(
    state: &AppState,
    accounts: &[AccountId],
) -> Result<InboxModes, HandlerError> {
    let placed = without_done(
        state,
        accounts,
        &[ModeKindData::Updates, ModeKindData::Reading],
        placed_inbox(state, accounts, None).await?,
    )
    .await?;
    let mut out = InboxModes {
        updates: Vec::new(),
        reading: Vec::new(),
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

/// Placed inbox mail minus what `modes` (Updates, Reading or both) marked
/// done: a thread stays out of a mode until a message of that mode arrives
/// after its mark. Each thread's messages of one mode are checked
/// together, as membership does. Order is not kept.
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

/// What an early mode's view is built on, for the clients' "early version"
/// note.
const fn early_note(mode: ModeKindData) -> Option<&'static str> {
    match mode {
        ModeKindData::Messages => {
            Some("Early version: the desk's You owe, New from people and Waiting on.")
        }
        ModeKindData::Updates => {
            Some("Early version: Paper trail, automated mail in your inbox by sender.")
        }
        ModeKindData::Reading => {
            Some("Early version: newsletters and lists in your inbox by sender.")
        }
        ModeKindData::Archive => Some("Early version: search, with receipts and records marked."),
        ModeKindData::Todo => None,
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

/// How many threads Messages keeps as quiet: person mail in the inbox no
/// lane holds and nobody marked done there.
pub(super) async fn quiet_count(
    state: &AppState,
    accounts: &[AccountId],
    now: DateTime<Utc>,
) -> Result<u32, HandlerError> {
    let mut quiet = 0u32;
    for account in accounts {
        let candidates = state.store.place_candidates(account).await?;
        let mut senders: Vec<String> = candidates
            .iter()
            .map(|m| m.from_email.to_ascii_lowercase())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        senders.sort_unstable();
        let kinds = super::places::AccountKinds::load(state, account, &senders).await?;
        let mut seen: HashSet<&ThreadId> = HashSet::new();
        let mut threads: Vec<ThreadId> = Vec::new();
        for message in &candidates {
            if message.snoozed || kinds.is_outbound(message) {
                continue;
            }
            if mail_kind::classify(&kinds.signals(message)).kind == mail_kind::SenderKind::Person
                && seen.insert(&message.thread_id)
            {
                threads.push(message.thread_id.clone());
            }
        }
        for chunk in threads.chunks(MEMBERSHIP_MAX_THREADS) {
            let placed = place_threads(state, account, chunk, now).await?;
            quiet += u32::try_from(placed.iter().filter(|p| p.quiet).count()).unwrap_or(u32::MAX);
        }
    }
    Ok(quiet)
}

pub(super) async fn get_rail(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    let now = Utc::now();
    let accounts = scoped_accounts(state, account_id).await?;
    let snapshot = super::now::snapshot(state, account_id, &accounts, now, &Local).await?;
    let count = |n: usize| Some(super::now::count(n));
    // Badges count work only: Now's people and due soon. To do earns its
    // own badge only once `mxr modes eval` shows under one false to-do a
    // week (D117).
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
            quiet: Some(quiet_count(state, &accounts, now).await?),
            ..mode_entry(ModeKindData::Messages, Some(snapshot.people_total()))
        },
        mode_entry(ModeKindData::Todo, count(snapshot.due_now.len())),
        mode_entry(ModeKindData::Updates, count(snapshot.inbox.updates.len())),
        mode_entry(ModeKindData::Reading, count(snapshot.inbox.reading.len())),
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
