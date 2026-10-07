//! `Request::GetDesk`: what needs you, not what arrived.
//!
//! Composes lanes from what the store already knows: thread direction and
//! inbox state, contacts, screener decisions, reply latencies, open
//! commitments and cadence drift. Local reads only; no LLM on this path.
//! The lane rules live in `desk_lanes.rs`.

use super::desk_lanes::{
    apply_pace, clean_subject, dedupe_by_precedence, is_outbound, sender_kind, sort_lane,
    thread_lanes, thread_starred, waiting_set_aside, AccountInputs, PaceDirection, WaitingAside,
    DESK_WINDOW_DAYS, DUE_AHEAD_DAYS,
};
use super::desk_timers::DeskTimers;
use super::mail_kind::SenderKind;
use super::HandlerResult;
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{AccountAddressLookup, CalendarPartstat};
use mxr_protocol::{DeskElsewhereData, DeskLaneData, DeskLaneKind, DeskRowData, ResponseData};
use mxr_store::{
    CadenceDriftRow, CommitmentDirection, CommitmentStatus, ContactCommitmentRecord,
    DeliveryListFilter, DeskContact, DeskLatestExchange, DeskMessage, ScreenerDisposition,
};
use std::collections::{HashMap, HashSet};

pub(super) async fn get_desk(
    state: &AppState,
    account_id: Option<&AccountId>,
    lane_limit: u32,
) -> HandlerResult {
    get_desk_at(state, account_id, lane_limit, Utc::now()).await
}

/// The desk as it stands at `now` (tests move the clock).
pub(super) async fn get_desk_at(
    state: &AppState,
    account_id: Option<&AccountId>,
    lane_limit: u32,
    now: DateTime<Utc>,
) -> HandlerResult {
    let started = std::time::Instant::now();
    let desk = compose_desk(state, account_id, now).await?;
    let lane = |kind| desk_lane(&desk.rows, kind, lane_limit);
    let data = ResponseData::Desk {
        account_id: account_id.cloned(),
        owed: lane(DeskLaneKind::Owed),
        due: lane(DeskLaneKind::Due),
        waiting: lane(DeskLaneKind::Waiting),
        people_new: lane(DeskLaneKind::PeopleNew),
        elsewhere: desk.elsewhere,
        last_from_people_at: desk.last_from_people_at,
        generated_at: now,
    };
    tracing::debug!(
        accounts = desk.accounts,
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "desk composed"
    );
    Ok(data)
}

/// Every desk row, each conversation in its highest lane, plus the counts.
pub(super) struct ComposedDesk {
    pub rows: Vec<DeskRowData>,
    elsewhere: DeskElsewhereData,
    last_from_people_at: Option<DateTime<Utc>>,
    accounts: usize,
}

/// The desk for one account, or every enabled account, at `now`.
pub(super) async fn compose_desk(
    state: &AppState,
    account_id: Option<&AccountId>,
    now: DateTime<Utc>,
) -> Result<ComposedDesk, super::HandlerError> {
    let accounts: Vec<AccountId> = match account_id {
        Some(id) => vec![id.clone()],
        None => state
            .store
            .list_accounts()
            .await?
            .into_iter()
            .filter(|account| account.enabled)
            .map(|account| account.id)
            .collect(),
    };

    let mut rows = Vec::new();
    let mut elsewhere = DeskElsewhereData::default();
    let mut last_from_people_at: Option<DateTime<Utc>> = None;
    for account in &accounts {
        let desk = account_desk(state, account, now).await?;
        rows.extend(desk.rows);
        elsewhere.reading += desk.elsewhere.reading;
        elsewhere.paper_trail += desk.elsewhere.paper_trail;
        elsewhere.screener += desk.elsewhere.screener;
        if desk.elsewhere.screener > 0 && elsewhere.screener_account.is_none() {
            elsewhere.screener_account = Some(account.clone());
        }
        elsewhere.invites += desk.elsewhere.invites;
        last_from_people_at = last_from_people_at.max(desk.last_from_people_at);
    }
    elsewhere.deliveries = count_active_deliveries(state, &accounts).await?;
    Ok(ComposedDesk {
        rows: dedupe_by_precedence(rows),
        elsewhere,
        last_from_people_at,
        accounts: accounts.len(),
    })
}

/// One lane in its order, trimmed to `limit`; the total counts every row.
pub(super) fn desk_lane(rows: &[DeskRowData], kind: DeskLaneKind, limit: u32) -> DeskLaneData {
    let mut lane_rows: Vec<DeskRowData> = rows
        .iter()
        .filter(|row| row.lane == kind)
        .cloned()
        .collect();
    sort_lane(kind, &mut lane_rows);
    let total = lane_rows.len() as u32;
    lane_rows.truncate(limit as usize);
    DeskLaneData {
        rows: lane_rows,
        total,
    }
}

/// Whether an address is this account's own (any of its addresses).
pub(super) async fn self_matcher(
    state: &AppState,
    account_id: &AccountId,
) -> Result<impl Fn(&str) -> bool, super::HandlerError> {
    let addresses = state.account_addresses.clone();
    let account_email = state
        .store
        .get_account(account_id)
        .await?
        .map(|account| account.email.to_ascii_lowercase());
    let account_id = account_id.clone();
    Ok(move |email: &str| {
        addresses.is_account_address(&account_id, email)
            || account_email
                .as_deref()
                .is_some_and(|own| own.eq_ignore_ascii_case(email))
    })
}

/// Reads the recent history of every sender in `from` you have never
/// written to into their contact row, so the classifier can tell a person
/// from a machine that writes like one. `from` spells addresses as their
/// messages do: the history lookup is case-sensitive.
pub(super) async fn attach_histories<'a>(
    state: &AppState,
    account_id: &AccountId,
    contacts: &mut HashMap<String, DeskContact>,
    from: impl Iterator<Item = &'a str>,
) -> Result<(), super::HandlerError> {
    let mut senders: Vec<String> = from
        .filter(|email| {
            contacts
                .get(&email.to_ascii_lowercase())
                .is_some_and(|contact| contact.total_outbound == 0)
        })
        .map(str::to_string)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if senders.is_empty() {
        return Ok(());
    }
    senders.sort_unstable();
    for (email, history) in state.store.sender_histories(account_id, &senders).await? {
        if let Some(contact) = contacts.get_mut(&email) {
            // Two spellings of one address: keep the longer history.
            if history.subjects.len() >= contact.history.subjects.len() {
                contact.history = history;
            }
        }
    }
    Ok(())
}

/// What the store knows about the senders of some desk messages, for the
/// shared classifier: contacts and screener decisions, keyed by lowercased
/// email.
pub(super) struct Senders {
    pub contacts: HashMap<String, DeskContact>,
    pub screener: HashMap<String, ScreenerDisposition>,
}

impl Senders {
    pub(super) async fn load(
        state: &AppState,
        account_id: &AccountId,
        messages: &[DeskMessage],
    ) -> Result<Self, super::HandlerError> {
        let mut emails: Vec<String> = messages
            .iter()
            .flat_map(|m| {
                std::iter::once(m.from.email.to_ascii_lowercase())
                    .chain(m.to.iter().map(|a| a.email.to_ascii_lowercase()))
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        emails.sort_unstable();
        let mut contacts = state
            .store
            .desk_contacts(account_id, &emails)
            .await?
            .into_iter()
            .map(|contact| (contact.email.to_ascii_lowercase(), contact))
            .collect();
        attach_histories(
            state,
            account_id,
            &mut contacts,
            messages
                .iter()
                .filter(|m| m.direction != "outbound")
                .map(|m| m.from.email.as_str()),
        )
        .await?;
        let screener = state
            .store
            .list_screener_decisions(account_id)
            .await?
            .into_iter()
            .map(|decision| {
                (
                    decision.sender_email.to_ascii_lowercase(),
                    decision.disposition,
                )
            })
            .collect();
        Ok(Self { contacts, screener })
    }

    /// A person other than you wrote it (not an auto-responder, list or
    /// notification): it answers you.
    pub(super) fn answers(&self, message: &DeskMessage, is_self: &dyn Fn(&str) -> bool) -> bool {
        let email = message.from.email.to_ascii_lowercase();
        !is_outbound(message, is_self)
            && sender_kind(
                message,
                self.contacts.get(&email),
                self.screener.get(&email).copied(),
            ) == SenderKind::Person
    }
}

struct AccountDesk {
    rows: Vec<DeskRowData>,
    elsewhere: DeskElsewhereData,
    last_from_people_at: Option<DateTime<Utc>>,
}

async fn account_desk(
    state: &AppState,
    account_id: &AccountId,
    now: DateTime<Utc>,
) -> Result<AccountDesk, super::HandlerError> {
    let store = &state.store;
    let mut messages = store
        .desk_thread_messages(account_id, now - Duration::days(DESK_WINDOW_DAYS))
        .await?;
    let timers = DeskTimers::new(
        store.desk_reply_later(account_id).await?,
        store
            .desk_reminders(account_id, now - Duration::days(DESK_WINDOW_DAYS))
            .await?,
    );
    // A conversation a time you set brought back may have gone quiet
    // before the window: load it whole, once.
    let loaded: HashSet<&ThreadId> = messages.iter().map(|m| &m.thread_id).collect();
    let mut back: Vec<ThreadId> = timers
        .maybe_back(now)
        .filter(|thread| !loaded.contains(thread))
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    back.sort_by_key(ThreadId::as_str);
    messages.extend(store.desk_messages_in_threads(account_id, &back).await?);

    let senders = Senders::load(state, account_id, &messages).await?;
    let is_self = self_matcher(state, account_id).await?;
    let dismissed = super::modes::messages_dismissals(state, account_id).await?;
    let mut lanes = thread_lanes(&AccountInputs {
        account_id,
        messages: &messages,
        contacts: &senders.contacts,
        screener: &senders.screener,
        dismissed: &dismissed,
        timers: &timers,
        is_self: &is_self,
        shape: super::conversation_shape::shape_config(state),
        now,
    });

    let mut rows: Vec<(DeskRowData, Option<PaceDirection>)> = lanes
        .rows
        .drain(..)
        .map(|draft| (draft.row, draft.pace))
        .collect();

    // Promises and watched contacts start from a thread, which may be older
    // than the window: fetch those threads whole, once.
    let due = due_commitments(state, account_id, now).await?;
    // A watched contact already on the desk (owed, due) needs no drift row.
    let already: HashSet<String> = rows
        .iter()
        .map(|(row, _)| row.counterparty_email.to_ascii_lowercase())
        .chain(
            due.iter()
                .map(|(commitment, _)| commitment.email.to_ascii_lowercase()),
        )
        .collect();
    let drifting: Vec<(CadenceDriftRow, DateTime<Utc>)> = store
        .list_cadence_drift(account_id)
        .await?
        .into_iter()
        .filter(|drift| !already.contains(&drift.email.to_ascii_lowercase()))
        .filter_map(|drift| drift.last_contact_at.map(|last| (drift, last)))
        .collect();
    let wanted: Vec<_> = drifting
        .iter()
        .map(|(drift, last)| (drift.email.clone(), *last))
        .collect();
    let exchanges = store.desk_latest_exchanges(account_id, &wanted).await?;

    // Only the threads these rows point at: from the window when there,
    // fetched whole otherwise.
    let wanted_threads: HashSet<ThreadId> = due
        .iter()
        .map(|(commitment, _)| commitment.thread_id.clone())
        .chain(
            exchanges
                .values()
                .map(|exchange| exchange.thread_id.clone()),
        )
        .collect();
    let mut threads: Threads = HashMap::new();
    for thread in messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
        if let Some(first) = thread.first() {
            if wanted_threads.contains(&first.thread_id) {
                threads.insert(first.thread_id.clone(), thread.to_vec());
            }
        }
    }
    let mut missing: Vec<ThreadId> = wanted_threads
        .into_iter()
        .filter(|thread| !threads.contains_key(thread))
        .collect();
    missing.sort_by_key(ThreadId::as_str);
    for message in store.desk_messages_in_threads(account_id, &missing).await? {
        threads
            .entry(message.thread_id.clone())
            .or_default()
            .push(message);
    }
    rows.extend(due_rows(account_id, due, &threads, now));
    let drift = drift_rows(
        account_id,
        drifting,
        exchanges,
        &threads,
        &WaitingAside {
            dismissed: &dismissed,
            timers: &timers,
            answers: &|m: &DeskMessage| senders.answers(m, &is_self),
            now,
        },
    );
    rows.extend(drift);

    // Usual pace for every counterparty in one query.
    let mut paced: Vec<String> = rows
        .iter()
        .filter(|(_, pace)| pace.is_some())
        .map(|(row, _)| row.counterparty_email.to_ascii_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    paced.sort_unstable();
    let mut latencies: HashMap<(String, &'static str), Vec<i64>> = HashMap::new();
    for latency in store.desk_reply_latencies(account_id, &paced).await? {
        let direction = if latency.direction == "i_replied" {
            "mine"
        } else {
            "theirs"
        };
        latencies
            .entry((latency.email, direction))
            .or_default()
            .push(latency.latency_seconds);
    }
    let rows = rows
        .into_iter()
        .map(|(mut row, pace)| {
            if let Some(pace) = pace {
                let key = (
                    row.counterparty_email.to_ascii_lowercase(),
                    match pace {
                        PaceDirection::Mine => "mine",
                        PaceDirection::Theirs => "theirs",
                    },
                );
                apply_pace(&mut row, latencies.get(&key).map(Vec::as_slice));
            }
            row
        })
        .collect();

    lanes.elsewhere.invites = count_pending_invites(state, account_id, now).await?;
    Ok(AccountDesk {
        rows,
        elsewhere: lanes.elsewhere,
        last_from_people_at: lanes.last_from_people_at,
    })
}

/// Every message of each thread the desk touches, keyed by thread.
type Threads = HashMap<ThreadId, Vec<DeskMessage>>;

/// Promises you made that are due within the week, or overdue this month.
async fn due_commitments(
    state: &AppState,
    account_id: &AccountId,
    now: DateTime<Utc>,
) -> Result<Vec<(ContactCommitmentRecord, DateTime<Utc>)>, super::HandlerError> {
    let horizon = now + Duration::days(DUE_AHEAD_DAYS);
    let floor = now - Duration::days(DESK_WINDOW_DAYS);
    Ok(state
        .store
        .list_contact_commitments(account_id, None, Some(CommitmentStatus::Open))
        .await?
        .into_iter()
        .filter_map(|commitment| {
            let by_when = commitment.by_when?;
            (commitment.direction == CommitmentDirection::Yours
                && by_when <= horizon
                && by_when >= floor)
                .then_some((commitment, by_when))
        })
        .collect())
}

/// A promise stands whatever happens to its thread (archive, snooze), so
/// due rows skip the inbox checks; they still cover the whole thread.
fn due_rows(
    account_id: &AccountId,
    due: Vec<(ContactCommitmentRecord, DateTime<Utc>)>,
    threads: &Threads,
    now: DateTime<Utc>,
) -> Vec<(DeskRowData, Option<PaceDirection>)> {
    due.into_iter()
        .map(|(commitment, due)| {
            let thread = threads
                .get(&commitment.thread_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let age_seconds = (now - due).num_seconds();
            let row = DeskRowData {
                lane: DeskLaneKind::Due,
                account_id: account_id.clone(),
                thread_id: commitment.thread_id.clone(),
                message_id: commitment.evidence_msg_id.clone(),
                message_ids: thread_message_ids(thread, &commitment.evidence_msg_id),
                counterparty_email: commitment.email.clone(),
                counterparty_name: None,
                subject: thread
                    .last()
                    .map(|latest| clean_subject(&latest.subject))
                    .unwrap_or_default(),
                reason: format!("\u{201c}{}\u{201d}", commitment.what.trim()),
                since: due,
                age_seconds,
                usual_seconds: None,
                usual_samples: 0,
                overdue: age_seconds > 0,
                unread: false,
                starred: thread_starred(thread),
                commitment_id: Some(commitment.id),
                back_at: None,
            };
            (row, None)
        })
        .collect()
}

fn thread_message_ids(thread: &[DeskMessage], fallback: &MessageId) -> Vec<MessageId> {
    if thread.is_empty() {
        vec![fallback.clone()]
    } else {
        thread.iter().map(|message| message.id.clone()).collect()
    }
}

/// Watched contacts who have gone quiet longer than usual join the waiting
/// lane, anchored on the latest conversation with them. Snooze, trash,
/// done waiting and a pending "bring it back" time set them aside like
/// every Waiting row; archive does not, because the row is about the
/// person and their last thread is usually archived.
fn drift_rows(
    account_id: &AccountId,
    drifting: Vec<(CadenceDriftRow, DateTime<Utc>)>,
    mut exchanges: HashMap<String, DeskLatestExchange>,
    threads: &Threads,
    aside: &WaitingAside<'_>,
) -> Vec<(DeskRowData, Option<PaceDirection>)> {
    let now = aside.now;
    let mut rows = Vec::new();
    for (drift, last_contact) in drifting {
        let Some(exchange) = exchanges.remove(&drift.email.to_ascii_lowercase()) else {
            continue;
        };
        let thread = threads
            .get(&exchange.thread_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if waiting_set_aside(thread, aside) {
            continue;
        }
        let expected_seconds = (drift.expected_days * 86_400.0).round() as i64;
        rows.push((
            DeskRowData {
                lane: DeskLaneKind::Waiting,
                account_id: account_id.clone(),
                thread_id: exchange.thread_id,
                message_id: exchange.message_id.clone(),
                message_ids: thread_message_ids(thread, &exchange.message_id),
                counterparty_email: drift.email,
                counterparty_name: drift.display_name,
                subject: clean_subject(&exchange.subject),
                reason: format!(
                    "usually in touch every {}",
                    days_phrase(drift.expected_days)
                ),
                since: last_contact,
                age_seconds: (now - last_contact).num_seconds(),
                usual_seconds: Some(expected_seconds),
                // The pace is the watch's cadence, not measured replies.
                usual_samples: 0,
                overdue: true,
                unread: false,
                starred: thread_starred(thread),
                commitment_id: None,
                back_at: None,
            },
            None,
        ));
    }
    rows
}

fn days_phrase(days: f64) -> String {
    let days = days.round().max(1.0) as i64;
    match days {
        1 => "day".to_string(),
        7 => "week".to_string(),
        14 => "two weeks".to_string(),
        d if d % 7 == 0 => format!("{} weeks", d / 7),
        d => format!("{d} days"),
    }
}

async fn count_active_deliveries(
    state: &AppState,
    accounts: &[AccountId],
) -> Result<u32, super::HandlerError> {
    let deliveries = state
        .store
        .list_deliveries(DeliveryListFilter::Active)
        .await?;
    Ok(deliveries
        .iter()
        .filter(|delivery| accounts.contains(&delivery.account_id))
        .count() as u32)
}

/// Upcoming invitations that still want your answer, by the same rule the
/// invites page uses to show response buttons.
async fn count_pending_invites(
    state: &AppState,
    account_id: &AccountId,
    now: DateTime<Utc>,
) -> Result<u32, super::HandlerError> {
    let ResponseData::Invites { invites } =
        super::mailbox::list_invites(state, Some(account_id), 200).await?
    else {
        return Ok(0);
    };
    Ok(invites
        .iter()
        .filter(|invite| {
            let metadata = &invite.metadata;
            let method = metadata
                .method
                .as_deref()
                .unwrap_or("")
                .to_ascii_uppercase();
            let cancelled = method == "CANCEL"
                || metadata
                    .status
                    .as_deref()
                    .is_some_and(|status| status.eq_ignore_ascii_case("CANCELLED"));
            let upcoming = metadata
                .starts_at
                .as_deref()
                .and_then(|start| DateTime::parse_from_rfc3339(start).ok())
                .is_none_or(|start| start.with_timezone(&Utc) >= now);
            (method.is_empty() || method == "REQUEST")
                && !cancelled
                && upcoming
                && matches!(
                    metadata.viewer_partstat,
                    None | Some(CalendarPartstat::NeedsAction | CalendarPartstat::Delegated)
                )
        })
        .count() as u32)
}

/// "Done waiting" on threads, or preview which threads that covers.
pub(super) async fn dismiss_threads(
    state: &AppState,
    thread_ids: &[mxr_core::id::ThreadId],
    dry_run: bool,
) -> HandlerResult {
    let threads = state
        .store
        .dismiss_desk_threads(thread_ids, dry_run)
        .await?
        .into_iter()
        .map(|(account_id, thread_id)| mxr_protocol::DeskThreadRefData {
            account_id,
            thread_id,
        })
        .collect();
    Ok(ResponseData::DeskThreadsDismissed { threads, dry_run })
}

pub(super) async fn restore_threads(
    state: &AppState,
    thread_ids: &[mxr_core::id::ThreadId],
) -> HandlerResult {
    // The desk reads done-in-Messages marks as dismissals too, so restoring
    // clears both, or a thread done in Messages would stay hidden.
    let restored = state.store.restore_desk_threads(thread_ids).await?
        + state.store.clear_mode_done(thread_ids, "messages").await?;
    Ok(ResponseData::DeskThreadsRestored { restored })
}
