//! Lane rules for the desk: pure functions from store rows to desk rows.
//!
//! Kept free of I/O so every rule (who counts as a person, which lane a
//! thread lands in, how rows are ordered, what the reason says) is tested
//! directly. `desk.rs` fetches the inputs and assembles the response.

use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::UnsubscribeMethod;
use mxr_protocol::{DeskElsewhereData, DeskLaneKind, DeskRowData};
use mxr_store::{DeskContact, DeskDismissal, DeskMessage, ScreenerDisposition};
use std::collections::{HashMap, HashSet};

/// Threads with activity this recent are considered for the desk.
pub(super) const DESK_WINDOW_DAYS: i64 = 30;
/// "New from people" and the everything-else counts look this far back.
pub(super) const RECENT_DAYS: i64 = 7;
/// Senders first seen this recently, with no screener decision, count as
/// waiting on a screener decision.
pub(super) const SCREENER_NEW_SENDER_DAYS: i64 = 14;
/// A thread only shows as waiting on someone after this long; a message
/// sent a minute ago is not yet waiting on anyone.
pub(super) const WAITING_MIN_HOURS: i64 = 12;
/// Promises due within this many days show under Due.
pub(super) const DUE_AHEAD_DAYS: i64 = 7;
/// A person's usual pace needs at least this many past replies.
pub(super) const USUAL_MIN_SAMPLES: usize = 2;
/// Your turn lasts at least this long before it can go quiet.
pub(super) const TURN_DECAY_FLOOR_DAYS: i64 = 7;
/// Rows younger than this never show as overdue, whatever the usual pace.
const OVERDUE_FLOOR_SECONDS: i64 = 60 * 60;
/// Pace assumed for ordering when a person has no reply history.
const DEFAULT_PACE_SECONDS: i64 = 24 * 60 * 60;

use super::conversation_shape::{conversation_shape, Shape, ShapeConfig, ShapeInputs};
use super::desk_timers::{DeskTimers, Timer};
use super::mail_kind::{self, KindSignals};
use super::mail_kind::{looks_automated, SenderKind};

pub(super) fn is_outbound(message: &DeskMessage, is_self: &dyn Fn(&str) -> bool) -> bool {
    message.direction == "outbound"
        || (message.direction != "inbound" && is_self(&message.from.email))
}

/// A thread's messages up to a day ahead of `now`, in date order: who wrote
/// last is judged on these, so a bad future Date header cannot pose as the
/// newest message. Messages are in date order, so the rest are a suffix.
pub(super) fn current_messages(thread: &[DeskMessage], now: DateTime<Utc>) -> &[DeskMessage] {
    let cutoff = now + Duration::days(1);
    &thread[..thread.partition_point(|m| m.date <= cutoff)]
}

/// The message of `current` (see `current_messages`) stored last among
/// those `keep` accepts. Who wrote last goes by the order mail was stored,
/// not its Date header, so a reply whose clock is behind still counts as
/// the newest. The desk, Done and deferral all judge it here.
pub(super) fn last_stored(
    current: &[DeskMessage],
    keep: impl Fn(&DeskMessage) -> bool,
) -> Option<&DeskMessage> {
    current
        .iter()
        .filter(|message| keep(message))
        .max_by_key(|message| message.seq)
}

/// What sets a Waiting row aside besides the thread itself.
pub(super) struct WaitingAside<'a> {
    pub dismissed: &'a HashMap<ThreadId, DeskDismissal>,
    pub timers: &'a DeskTimers,
    /// A message that answers you: a person other than you wrote it.
    pub answers: &'a dyn Fn(&DeskMessage) -> bool,
    pub now: DateTime<Utc>,
}

/// Every Waiting row, from a thread or from a watched contact, stays off
/// the desk while its conversation is snoozed, trashed, marked done
/// waiting with nothing new since, or waiting on a time you set.
pub(super) fn waiting_set_aside(thread: &[DeskMessage], aside: &WaitingAside<'_>) -> bool {
    let Some(latest) = thread.last() else {
        return true;
    };
    thread.iter().any(|m| m.snoozed)
        || latest.trashed
        || aside
            .dismissed
            .get(&latest.thread_id)
            .is_some_and(|d| d.covers(thread))
        || aside.timers.waiting(thread, aside.answers, aside.now) == Some(Timer::Pending)
}

/// A thread you wrote last also leaves Waiting when you archive it, if it
/// has mail from them to archive. (A watched contact's row is about the
/// person, whose old conversation is usually archived, so it skips this.)
fn waiting_archived(thread: &[DeskMessage], is_self: &dyn Fn(&str) -> bool) -> bool {
    let has_inbound = thread.iter().any(|m| !is_outbound(m, is_self));
    let in_inbox = thread.iter().any(|m| m.in_inbox && !m.trashed);
    has_inbound && !in_inbox
}

/// Any message of the conversation is starred.
pub(super) fn thread_starred(thread: &[DeskMessage]) -> bool {
    thread
        .iter()
        .any(|m| m.flags.contains(mxr_core::MessageFlags::STARRED))
}

/// Everything the lane rules need about one account.
pub(super) struct AccountInputs<'a> {
    pub account_id: &'a AccountId,
    /// Messages of recently active threads, ordered by thread then date.
    pub messages: &'a [DeskMessage],
    /// Keyed by lowercased email.
    pub contacts: &'a HashMap<String, DeskContact>,
    /// Keyed by lowercased email.
    pub screener: &'a HashMap<String, ScreenerDisposition>,
    /// Threads marked done (or "done waiting"), through the messages
    /// stored at the time. Covers every thread lane; promises stand.
    pub dismissed: &'a HashMap<ThreadId, DeskDismissal>,
    /// Times you set: reply later, and "bring it back if nobody replies".
    pub timers: &'a DeskTimers,
    pub is_self: &'a dyn Fn(&str) -> bool,
    /// Emails the user moved (`X`), with the kind each move gave.
    pub moves: &'a HashMap<MessageId, SenderKind>,
    pub treatments: &'a HashMap<MessageId, (String, String)>,
    /// The thread shape thresholds: a copied thread is nobody's turn.
    pub shape: ShapeConfig,
    pub now: DateTime<Utc>,
}

/// What the window itself shows about each address, for when the contacts
/// table has not been refreshed yet (a fresh sync, a new account).
#[derive(Debug, Default)]
struct WindowFacts {
    /// Addresses you sent mail to inside the window.
    written_to: HashSet<String>,
    /// Inbound messages per sender inside the window.
    inbound: HashMap<String, u32>,
}

impl WindowFacts {
    /// You have written to them: the window shows it, or the contacts
    /// table counts mail from you. Being in conversation and never being
    /// screened both rest on this (D104).
    fn wrote_to(&self, inputs: &AccountInputs<'_>, email: &str) -> bool {
        inputs.contact(email).is_some_and(|c| c.total_outbound > 0)
            || self.written_to.contains(&email.to_ascii_lowercase())
    }

    fn from_inputs(inputs: &AccountInputs<'_>) -> Self {
        let mut facts = Self::default();
        for message in inputs.messages {
            if inputs.is_outbound(message) {
                for recipient in message.to.iter().chain(&message.cc).chain(&message.bcc) {
                    facts
                        .written_to
                        .insert(recipient.email.to_ascii_lowercase());
                }
            } else {
                *facts
                    .inbound
                    .entry(message.from.email.to_ascii_lowercase())
                    .or_default() += 1;
            }
        }
        facts
    }
}

impl AccountInputs<'_> {
    fn contact(&self, email: &str) -> Option<&DeskContact> {
        self.contacts.get(&email.to_ascii_lowercase())
    }

    fn decision(&self, email: &str) -> Option<ScreenerDisposition> {
        self.screener.get(&email.to_ascii_lowercase()).copied()
    }

    fn is_outbound(&self, message: &DeskMessage) -> bool {
        is_outbound(message, self.is_self)
    }

    /// The shared classifier (`mail_kind`), so the desk and the places
    /// never disagree about a message.
    pub(super) fn sender_kind(&self, message: &DeskMessage) -> SenderKind {
        let email = &message.from.email;
        let mut signals = desk_signals(
            message,
            self.contact(email),
            self.decision(email),
            self.moves.get(&message.id).copied(),
            self.is_self,
        );
        signals.treatment = self
            .treatments
            .get(&message.id)
            .and_then(|(mode, name)| Some((mail_kind::kind_for_stored_mode(mode)?, name.as_str())));
        mail_kind::classify(&signals).kind
    }

    /// A person other than you wrote it: an answer. An auto-responder or a
    /// notification in the thread is not.
    pub(super) fn answers(&self, message: &DeskMessage) -> bool {
        !self.is_outbound(message) && self.sender_kind(message) == SenderKind::Person
    }

    /// The thread's shape, judged by the shared rule (`conversation_shape`).
    pub(super) fn shape(&self, thread: &[DeskMessage]) -> Shape {
        let person_sender = |m: &DeskMessage| self.sender_kind(m) == SenderKind::Person;
        let human_address = |email: &str| self.human_address(email);
        let kept = |m: &DeskMessage| {
            let email = &m.from.email;
            let mut signals = desk_signals(
                m,
                self.contact(email),
                self.decision(email),
                self.moves.get(&m.id).copied(),
                self.is_self,
            );
            signals.treatment = self.treatments.get(&m.id).and_then(|(mode, name)| {
                Some((mail_kind::kind_for_stored_mode(mode)?, name.as_str()))
            });
            mail_kind::kept_in_messages(&signals)
        };
        conversation_shape(
            thread,
            &ShapeInputs {
                is_self: self.is_self,
                person_sender: &person_sender,
                human_address: &human_address,
                kept_in_messages: &kept,
                config: self.shape,
            },
        )
    }

    /// An address you wrote to that could be a person.
    pub(super) fn human_address(&self, email: &str) -> bool {
        super::conversation_shape::human_address(email, self.contacts, self.screener)
    }

    /// Only copied, not addressed: the reason says so.
    fn only_copied(&self, message: &DeskMessage) -> bool {
        !message.to.iter().any(|a| (self.is_self)(&a.email))
            && message.cc.iter().any(|a| (self.is_self)(&a.email))
    }
}

/// The kind signals for one desk message. "Written to" comes from the
/// contacts table, the same everywhere a message is classified.
pub(super) fn desk_signals<'a>(
    message: &'a DeskMessage,
    contact: Option<&DeskContact>,
    decision: Option<ScreenerDisposition>,
    moved: Option<SenderKind>,
    is_self: &dyn Fn(&str) -> bool,
) -> KindSignals<'a> {
    KindSignals {
        email: &message.from.email,
        subject: &message.subject,
        has_list_id: message.list_id.is_some(),
        has_unsubscribe: !matches!(message.unsubscribe, UnsubscribeMethod::None),
        is_delivery: message.is_delivery,
        is_invite: message.is_invite,
        list_sender: contact.is_some_and(|c| c.is_list_sender),
        sender: mail_kind::SenderFacts::of(contact),
        decision,
        written_to: contact.is_some_and(|c| c.total_outbound > 0),
        addressed: addressed_to_you(&message.to, &message.cc, is_self),
        moved,
        treatment: None,
    }
}

/// You are in To, or in neither To nor Cc (Bcc, or an alias mxr doesn't
/// know): addressed. Only Cc without To is "only copied", the same
/// positive evidence the shape rule asks for.
pub(super) fn addressed_to_you(
    to: &[mxr_core::types::Address],
    cc: &[mxr_core::types::Address],
    is_self: &dyn Fn(&str) -> bool,
) -> bool {
    to.iter().any(|a| is_self(&a.email)) || !cc.iter().any(|a| is_self(&a.email))
}

/// A desk row before the usual pace is known.
#[derive(Debug, Clone)]
pub(super) struct DraftRow {
    pub row: DeskRowData,
    /// Which reply direction's history gives this row its usual pace.
    pub pace: Option<PaceDirection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PaceDirection {
    /// How fast you usually reply to them (owed).
    Mine,
    /// How fast they usually reply to you (waiting).
    Theirs,
}

/// The thread-driven lanes (owed, waiting, people new) and the counts for
/// everything else, for one account.
#[derive(Debug, Default)]
pub(super) struct ThreadLanes {
    pub rows: Vec<DraftRow>,
    pub elsewhere: DeskElsewhereData,
    pub last_from_people_at: Option<DateTime<Utc>>,
}

pub(super) fn thread_lanes(inputs: &AccountInputs<'_>) -> ThreadLanes {
    let now = inputs.now;
    let recent = now - Duration::days(RECENT_DAYS);
    let screener_since = now - Duration::days(SCREENER_NEW_SENDER_DAYS);
    let mut lanes = ThreadLanes::default();
    let mut screener_senders: HashSet<String> = HashSet::new();
    let facts = WindowFacts::from_inputs(inputs);

    for thread in inputs.messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
        if thread.is_empty() {
            continue;
        }

        // Everything-else counts and the quiet line look at recent inbox
        // mail, message by message.
        for message in thread {
            if inputs.is_outbound(message)
                || !message.in_inbox
                || message.trashed
                || message.date < recent
            {
                continue;
            }
            match inputs.sender_kind(message) {
                // Issues this week, read or not: the count is never an
                // unread count.
                SenderKind::List => {
                    lanes.elsewhere.reading += 1;
                }
                SenderKind::Automated if !message.is_delivery && !message.is_invite => {
                    lanes.elsewhere.paper_trail += 1;
                }
                SenderKind::Person => {
                    lanes.last_from_people_at = lanes.last_from_people_at.max(Some(message.date));
                }
                _ => {}
            }
        }
        for message in thread {
            if inputs.is_outbound(message) || !message.in_inbox || message.date < screener_since {
                continue;
            }
            let email = message.from.email.to_ascii_lowercase();
            let first_seen_recently = inputs
                .contact(&email)
                .is_none_or(|c| c.first_seen_at >= screener_since);
            if inputs
                .decision(&email)
                .is_none_or(|d| d == ScreenerDisposition::Unknown)
                && first_seen_recently
                && !facts.wrote_to(inputs, &email)
            {
                screener_senders.insert(email);
            }
        }

        // A snoozed conversation is out of sight until it wakes, and one
        // set to reply later until its time comes.
        if thread.iter().any(|m| m.snoozed)
            || inputs.timers.reply_later(&thread[0].thread_id, inputs.now) == Some(Timer::Pending)
        {
            continue;
        }
        // The lane is judged on current mail; verbs still cover the whole
        // thread.
        let current = current_messages(thread, inputs.now);
        let Some(latest) = last_stored(current, |_| true) else {
            continue;
        };
        if let Some(row) = thread_row(
            inputs,
            &facts,
            Conversation {
                all: thread,
                current,
            },
            latest,
        ) {
            lanes.rows.push(row);
        }
    }
    lanes.elsewhere.screener = screener_senders.len() as u32;
    lanes
}

/// A thread's messages: all of them (what a verb acts on) and those not
/// dated in the future (what decides the lane).
#[derive(Clone, Copy)]
struct Conversation<'a> {
    all: &'a [DeskMessage],
    current: &'a [DeskMessage],
}

fn thread_row(
    inputs: &AccountInputs<'_>,
    facts: &WindowFacts,
    conversation: Conversation<'_>,
    latest: &DeskMessage,
) -> Option<DraftRow> {
    // Done (or done waiting) with nothing stored since: put away, from
    // every thread lane.
    if inputs
        .dismissed
        .get(&latest.thread_id)
        .is_some_and(|dismissal| dismissal.covers(conversation.all))
    {
        return None;
    }
    let thread = conversation.current;
    let now = inputs.now;
    let latest_inbound = last_stored(thread, |m| !inputs.is_outbound(m));
    let latest_outbound = last_stored(thread, |m| inputs.is_outbound(m));
    let in_inbox = thread.iter().any(|m| m.in_inbox && !m.trashed);

    if inputs.is_outbound(latest) {
        return waiting_row(inputs, conversation, latest, latest_inbound.is_some());
    }
    // An auto-responder or notification after your message answers nothing:
    // a time you set on the wait still hides it or brings it back.
    if let Some(sent) = latest_outbound.filter(|_| !inputs.answers(latest)) {
        let answers = |m: &DeskMessage| inputs.answers(m);
        if inputs
            .timers
            .waiting(conversation.all, &answers, now)
            .is_some()
        {
            return waiting_row(inputs, conversation, sent, latest_inbound.is_some());
        }
    }

    let inbound = latest_inbound?;
    // You said you'd reply by now: owed, wherever the mail is and whoever
    // sent it, since you asked for it back. The newest message from a
    // person anchors it; any inbound one when no person wrote (a reply
    // later on a newsletter still comes back).
    if let Some(Timer::Back(back_at)) = inputs.timers.reply_later(&latest.thread_id, now) {
        let anchor = last_stored(thread, |m| inputs.answers(m)).unwrap_or(inbound);
        if !anchor.trashed {
            return Some(back_row(
                inputs,
                DeskLaneKind::Owed,
                conversation.all,
                anchor,
                &anchor.from,
                "back from reply later",
                back_at,
            ));
        }
    }
    if inbound.trashed || !in_inbox || inputs.sender_kind(inbound) != SenderKind::Person {
        return None;
    }
    // A thread you were only copied on, or one sent to a crowd, that you
    // never wrote in is nobody's turn: it belongs in Updates.
    if inputs.shape(conversation.all) == Shape::Copied {
        return None;
    }
    let email = inbound.from.email.as_str();
    let key = email.to_ascii_lowercase();
    let contact = inputs.contact(email);
    // An actual exchange: you wrote in this thread or to them before.
    // Allowing a sender makes them a person (the classifier), not someone
    // you owe; their first mail is new from people until you write (D104).
    let in_conversation = latest_outbound.is_some() || facts.wrote_to(inputs, email);

    let (lane, reason, pace) = if in_conversation {
        let unanswered = latest_outbound.map_or(0, |sent| {
            thread
                .iter()
                .filter(|m| !inputs.is_outbound(m) && m.seq > sent.seq)
                .count()
        });
        let reason = if unanswered > 1 {
            format!("{unanswered} messages since you last wrote")
        } else if latest_outbound.is_some() {
            "replied to your message".to_string()
        } else if inputs.only_copied(inbound) {
            "copied you".to_string()
        } else {
            "wrote to you".to_string()
        };
        // Your turn decays: past max(3 times their cadence, 7 days), capped
        // at the window, a turn nobody took goes quiet (blueprint 22's
        // relevancy table; Kooti et al., WWW 2015).
        if inbound.date < now - turn_decay(contact) {
            return None;
        }
        (DeskLaneKind::Owed, reason, Some(PaceDirection::Mine))
    } else {
        // A sender you allowed stays as long as the window would keep them
        // in You owe; Allow took them out of there, not off the desk (D104).
        let days = if inputs.decision(email) == Some(ScreenerDisposition::Allow) {
            DESK_WINDOW_DAYS
        } else {
            RECENT_DAYS
        };
        if inbound.date < now - Duration::days(days) {
            return None;
        }
        let seen = contact
            .map_or(0, |c| c.total_inbound)
            .max(facts.inbound.get(&key).copied().unwrap_or(0));
        let first_time = seen <= 1;
        let reason = if first_time {
            "first message from them"
        } else if inputs.only_copied(inbound) {
            "copied you"
        } else {
            "wrote to you"
        };
        (DeskLaneKind::PeopleNew, reason.to_string(), None)
    };

    Some(DraftRow {
        row: base_row(
            inputs,
            lane,
            conversation.all,
            inbound,
            email,
            inbound
                .from
                .name
                .clone()
                .or_else(|| contact.and_then(|c| c.display_name.clone())),
            reason,
            inbound.date,
            !inbound.flags.contains(mxr_core::MessageFlags::READ),
        ),
        pace,
    })
}

/// How long your turn with someone lasts before it goes quiet:
/// max(3 times their cadence, 7 days), never past the window.
pub(super) fn turn_decay(contact: Option<&DeskContact>) -> Duration {
    let floor = Duration::days(TURN_DECAY_FLOOR_DAYS);
    let cadence = contact
        .and_then(|c| c.cadence_seconds)
        .map_or(floor, |seconds| {
            Duration::seconds(seconds.saturating_mul(3))
        });
    cadence.max(floor).min(Duration::days(DESK_WINDOW_DAYS))
}

fn waiting_row(
    inputs: &AccountInputs<'_>,
    conversation: Conversation<'_>,
    sent: &DeskMessage,
    has_inbound: bool,
) -> Option<DraftRow> {
    let thread = conversation.current;
    let now = inputs.now;
    // Done waiting counts every stored message, future-dated or not.
    let answers = |m: &DeskMessage| inputs.answers(m);
    let aside = WaitingAside {
        dismissed: inputs.dismissed,
        timers: inputs.timers,
        answers: &answers,
        now,
    };
    if waiting_set_aside(conversation.all, &aside) {
        return None;
    }
    let timer = inputs.timers.waiting(conversation.all, &answers, now);
    let recipient = sent.to.iter().find(|a| !(inputs.is_self)(&a.email))?;
    let email = recipient.email.as_str();
    let contact = inputs.contact(email);
    if looks_automated(email)
        || contact.is_some_and(|c| c.is_list_sender)
        || inputs.decision(email) == Some(ScreenerDisposition::Deny)
    {
        return None;
    }
    // Nobody replied by the time you set: back, however old or archived.
    if let Some(Timer::Back(back_at)) = timer {
        return Some(back_row(
            inputs,
            DeskLaneKind::Waiting,
            conversation.all,
            sent,
            recipient,
            "no reply by the time you set",
            back_at,
        ));
    }
    if sent.date > now - Duration::hours(WAITING_MIN_HOURS)
        || sent.date < now - Duration::days(DESK_WINDOW_DAYS)
        || waiting_archived(thread, inputs.is_self)
    {
        return None;
    }
    let followed_up = last_stored(thread, |m| m.seq < sent.seq)
        .is_some_and(|previous| inputs.is_outbound(previous));
    let reason = if followed_up {
        "you followed up, no reply yet"
    } else if has_inbound {
        "no reply to your last message"
    } else {
        "no reply yet"
    };
    Some(DraftRow {
        row: base_row(
            inputs,
            DeskLaneKind::Waiting,
            conversation.all,
            sent,
            email,
            recipient
                .name
                .clone()
                .or_else(|| contact.and_then(|c| c.display_name.clone())),
            reason.to_string(),
            sent.date,
            false,
        ),
        pace: Some(PaceDirection::Theirs),
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "one constructor for every lane keeps the row shape in one place"
)]
fn base_row(
    inputs: &AccountInputs<'_>,
    lane: DeskLaneKind,
    thread: &[DeskMessage],
    open: &DeskMessage,
    email: &str,
    name: Option<String>,
    reason: String,
    since: DateTime<Utc>,
    unread: bool,
) -> DeskRowData {
    DeskRowData {
        lane,
        account_id: inputs.account_id.clone(),
        thread_id: open.thread_id.clone(),
        message_id: open.id.clone(),
        message_ids: thread.iter().map(|m| m.id.clone()).collect(),
        counterparty_email: email.to_string(),
        counterparty_name: name.filter(|n| !n.trim().is_empty()),
        subject: clean_subject(&open.subject),
        reason,
        since,
        age_seconds: (inputs.now - since).num_seconds(),
        usual_seconds: None,
        usual_samples: 0,
        overdue: false,
        unread,
        starred: thread_starred(thread),
        commitment_id: None,
        back_at: None,
    }
}

/// A row a time you set brought back. It is due by definition, so it is
/// overdue and takes no pace (the usual pace would second-guess it).
fn back_row(
    inputs: &AccountInputs<'_>,
    lane: DeskLaneKind,
    thread: &[DeskMessage],
    open: &DeskMessage,
    counterparty: &mxr_core::types::Address,
    reason: &str,
    back_at: DateTime<Utc>,
) -> DraftRow {
    let name = counterparty.name.clone().or_else(|| {
        inputs
            .contact(&counterparty.email)
            .and_then(|c| c.display_name.clone())
    });
    let mut row = base_row(
        inputs,
        lane,
        thread,
        open,
        &counterparty.email,
        name,
        reason.to_string(),
        open.date,
        lane == DeskLaneKind::Owed && !open.flags.contains(mxr_core::MessageFlags::READ),
    );
    row.overdue = true;
    row.back_at = Some(back_at);
    DraftRow { row, pace: None }
}

/// Attach a person's usual pace and decide whether the row is past it.
pub(super) fn apply_pace(row: &mut DeskRowData, latencies: Option<&[i64]>) {
    let Some(latencies) = latencies.filter(|l| l.len() >= USUAL_MIN_SAMPLES) else {
        return;
    };
    let mut sorted = latencies.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    row.usual_seconds = Some(median);
    row.usual_samples = sorted.len() as u32;
    row.overdue = row.age_seconds > median && row.age_seconds >= OVERDUE_FLOOR_SECONDS;
}

/// Lane order, most pressing first. Owed and waiting rows rank by how far
/// past the person's usual pace they are; due rows by due date; new mail
/// from people newest first.
pub(super) fn sort_lane(lane: DeskLaneKind, rows: &mut [DeskRowData]) {
    match lane {
        DeskLaneKind::Owed | DeskLaneKind::Waiting => rows.sort_by(|a, b| {
            pace_ratio(b)
                .total_cmp(&pace_ratio(a))
                .then(a.since.cmp(&b.since))
                .then_with(|| a.thread_id.as_str().cmp(&b.thread_id.as_str()))
        }),
        DeskLaneKind::Due => rows.sort_by(|a, b| {
            a.since
                .cmp(&b.since)
                .then_with(|| a.thread_id.as_str().cmp(&b.thread_id.as_str()))
        }),
        DeskLaneKind::PeopleNew => rows.sort_by(|a, b| {
            b.since
                .cmp(&a.since)
                .then_with(|| a.thread_id.as_str().cmp(&b.thread_id.as_str()))
        }),
    }
}

/// The pace an owed or waiting row is measured against: the person's usual
/// pace, or a day when there is no history.
pub(super) fn pace_seconds(row: &DeskRowData) -> i64 {
    row.usual_seconds.unwrap_or(DEFAULT_PACE_SECONDS).max(60)
}

/// How many paces old a row is: the lane's sort key.
pub(super) fn pace_ratio(row: &DeskRowData) -> f64 {
    row.age_seconds as f64 / pace_seconds(row) as f64
}

/// Lane precedence when one conversation qualifies for several.
pub(super) const LANE_PRECEDENCE: [DeskLaneKind; 4] = [
    DeskLaneKind::Owed,
    DeskLaneKind::Due,
    DeskLaneKind::Waiting,
    DeskLaneKind::PeopleNew,
];

/// Keep each conversation in its highest-precedence lane only.
pub(super) fn dedupe_by_precedence(rows: Vec<DeskRowData>) -> Vec<DeskRowData> {
    let rank = |lane: DeskLaneKind| {
        LANE_PRECEDENCE
            .iter()
            .position(|l| *l == lane)
            .unwrap_or(LANE_PRECEDENCE.len())
    };
    let mut best: HashMap<(AccountId, ThreadId), usize> = HashMap::new();
    for row in &rows {
        let key = (row.account_id.clone(), row.thread_id.clone());
        let entry = best.entry(key).or_insert(usize::MAX);
        *entry = (*entry).min(rank(row.lane));
    }
    let mut seen: HashSet<(AccountId, ThreadId)> = HashSet::new();
    rows.into_iter()
        .filter(|row| {
            let key = (row.account_id.clone(), row.thread_id.clone());
            best.get(&key) == Some(&rank(row.lane)) && seen.insert(key)
        })
        .collect()
}

/// "Re: Re: Fwd: Launch plan" reads as "Launch plan" on the desk.
pub(super) fn clean_subject(subject: &str) -> String {
    mxr_compose::strip_reply_forward_prefixes(subject)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::MessageId;
    use mxr_core::types::{Address, MessageFlags};

    const ME: &str = "me@example.com";

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-26T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn message(thread: &ThreadId, from: &str, to: &str, hours_ago: i64) -> DeskMessage {
        let outbound = from == ME;
        DeskMessage {
            id: MessageId::new(),
            seq: now().timestamp() - hours_ago * 3600,
            thread_id: thread.clone(),
            direction: if outbound { "outbound" } else { "inbound" }.into(),
            date: now() - Duration::hours(hours_ago),
            flags: if outbound {
                MessageFlags::READ | MessageFlags::SENT
            } else {
                MessageFlags::empty()
            },
            from: Address {
                name: None,
                email: from.into(),
            },
            to: vec![Address {
                name: None,
                email: to.into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Re: Launch plan".into(),
            list_id: None,
            unsubscribe: UnsubscribeMethod::None,
            in_inbox: !outbound,
            trashed: false,
            snoozed: false,
            is_invite: false,
            is_delivery: false,
        }
    }

    fn lanes(messages: &[DeskMessage], contacts: &[DeskContact]) -> ThreadLanes {
        lanes_with_screener(messages, contacts, HashMap::new())
    }

    fn lanes_with_screener(
        messages: &[DeskMessage],
        contacts: &[DeskContact],
        screener: HashMap<String, ScreenerDisposition>,
    ) -> ThreadLanes {
        let account = AccountId::new();
        let contacts = contacts
            .iter()
            .map(|c| (c.email.to_ascii_lowercase(), c.clone()))
            .collect();
        let dismissed = HashMap::new();
        let timers = DeskTimers::default();
        let is_self = |email: &str| email.eq_ignore_ascii_case(ME);
        let moves = HashMap::new();
        thread_lanes(&AccountInputs {
            account_id: &account,
            messages,
            contacts: &contacts,
            screener: &screener,
            dismissed: &dismissed,
            timers: &timers,
            is_self: &is_self,
            moves: &moves,
            treatments: &HashMap::new(),
            shape: ShapeConfig::default(),
            now: now(),
        })
    }

    fn lane_of(lanes: &ThreadLanes, thread: &ThreadId) -> Option<DeskLaneKind> {
        lanes
            .rows
            .iter()
            .find(|r| &r.row.thread_id == thread)
            .map(|r| r.row.lane)
    }

    #[test]
    fn reply_in_a_conversation_is_owed_and_a_stranger_is_new() {
        let conversation = ThreadId::new();
        let stranger = ThreadId::new();
        let messages = vec![
            message(&conversation, ME, "maya@example.com", 30),
            message(&conversation, "maya@example.com", ME, 5),
            message(&stranger, "theo@example.com", ME, 2),
        ];
        let lanes = lanes(&messages, &[]);
        assert_eq!(lane_of(&lanes, &conversation), Some(DeskLaneKind::Owed));
        assert_eq!(lane_of(&lanes, &stranger), Some(DeskLaneKind::PeopleNew));
        let owed = lanes
            .rows
            .iter()
            .find(|r| r.row.thread_id == conversation)
            .unwrap();
        assert_eq!(owed.row.reason, "replied to your message");
        assert_eq!(owed.row.subject, "Launch plan");
        assert_eq!(owed.row.message_id, messages[1].id);
        assert_eq!(owed.row.message_ids.len(), 2);
        assert_eq!(owed.pace, Some(PaceDirection::Mine));
    }

    #[test]
    fn someone_you_have_written_to_before_is_owed_even_in_a_new_thread() {
        let thread = ThreadId::new();
        let messages = vec![message(&thread, "priya@example.com", ME, 3)];
        let contact = DeskContact {
            email: "priya@example.com".into(),
            display_name: Some("Priya Raman".into()),
            first_seen_at: now() - Duration::days(200),
            total_inbound: 12,
            total_outbound: 4,
            is_list_sender: false,
            cadence_seconds: None,
            history: mxr_store::SenderHistory::default(),
        };
        let lanes = lanes(&messages, &[contact]);
        let row = &lanes.rows[0].row;
        assert_eq!(row.lane, DeskLaneKind::Owed);
        assert_eq!(row.reason, "wrote to you");
        assert_eq!(row.counterparty_name.as_deref(), Some("Priya Raman"));
    }

    #[test]
    fn allowing_a_sender_makes_a_person_not_a_conversation() {
        // Allowed and looks automated: Allow makes them a person, but you
        // have never written to them, so their mail is new, not owed.
        let never_replied = ThreadId::new();
        // Allowed and you replied in the thread: an exchange, so owed.
        let replied = ThreadId::new();
        let messages = vec![
            message(&never_replied, "notifications@studio.example", ME, 3),
            message(&replied, "ops@studio.example", ME, 30),
            message(&replied, ME, "ops@studio.example", 20),
            message(&replied, "ops@studio.example", ME, 2),
        ];
        let screener = HashMap::from([
            (
                "notifications@studio.example".to_string(),
                ScreenerDisposition::Allow,
            ),
            ("ops@studio.example".to_string(), ScreenerDisposition::Allow),
        ]);
        assert!(looks_automated("notifications@studio.example"));
        let lanes = lanes_with_screener(&messages, &[], screener);
        let new = lanes
            .rows
            .iter()
            .find(|r| r.row.thread_id == never_replied)
            .expect("an allowed sender is a person");
        assert_eq!(new.row.lane, DeskLaneKind::PeopleNew);
        assert_eq!(new.row.reason, "first message from them");
        assert_eq!(lane_of(&lanes, &replied), Some(DeskLaneKind::Owed));
    }

    #[test]
    fn an_allowed_sender_waiting_two_weeks_is_still_new_from_people() {
        let allowed = ThreadId::new();
        let stranger = ThreadId::new();
        let messages = vec![
            message(&allowed, "ops@studio.example", ME, 15 * 24),
            message(&stranger, "theo@example.com", ME, 15 * 24),
        ];
        let screener =
            HashMap::from([("ops@studio.example".to_string(), ScreenerDisposition::Allow)]);
        let lanes = lanes_with_screener(&messages, &[], screener);
        assert_eq!(lane_of(&lanes, &allowed), Some(DeskLaneKind::PeopleNew));
        assert_eq!(
            lane_of(&lanes, &stranger),
            None,
            "only a week for strangers"
        );
    }

    #[test]
    fn a_bcc_counts_as_writing_to_them() {
        let earlier = ThreadId::new();
        let theirs = ThreadId::new();
        let mut sent = message(&earlier, ME, "team@example.com", 50);
        sent.bcc = vec![Address {
            name: None,
            email: "Priya@Example.com".into(),
        }];
        let messages = vec![
            sent,
            message(&theirs, "priya@example.com", ME, 5),
            message(&ThreadId::new(), "theo@example.com", ME, 3),
        ];
        let lanes = lanes(&messages, &[]);
        assert_eq!(lane_of(&lanes, &theirs), Some(DeskLaneKind::Owed));
        assert_eq!(lanes.elsewhere.screener, 1, "only Theo");
    }

    #[test]
    fn the_screener_count_leaves_out_anyone_you_have_written_to() {
        let messages = vec![
            message(&ThreadId::new(), ME, "maya@example.com", 50),
            message(&ThreadId::new(), "maya@example.com", ME, 5),
            message(&ThreadId::new(), "theo@example.com", ME, 3),
            message(&ThreadId::new(), "priya@example.com", ME, 2),
        ];
        // Priya: the contacts table knows you wrote to her before.
        let priya = DeskContact {
            email: "priya@example.com".into(),
            display_name: None,
            first_seen_at: now() - Duration::days(3),
            total_inbound: 1,
            total_outbound: 1,
            is_list_sender: false,
            cadence_seconds: None,
            history: mxr_store::SenderHistory::default(),
        };
        assert_eq!(
            lanes(&messages, &[priya]).elsewhere.screener,
            1,
            "only Theo"
        );
    }

    #[test]
    fn you_wrote_last_a_day_ago_is_waiting_but_not_a_minute_ago() {
        let old = ThreadId::new();
        let fresh = ThreadId::new();
        let messages = vec![
            message(&old, "jon@example.com", ME, 80),
            message(&old, ME, "jon@example.com", 50),
            message(&fresh, ME, "nora@example.com", 0),
        ];
        let lanes = lanes(&messages, &[]);
        let waiting = lanes.rows.iter().find(|r| r.row.thread_id == old).unwrap();
        assert_eq!(waiting.row.lane, DeskLaneKind::Waiting);
        assert_eq!(waiting.row.counterparty_email, "jon@example.com");
        assert_eq!(waiting.row.reason, "no reply to your last message");
        assert_eq!(waiting.pace, Some(PaceDirection::Theirs));
        assert_eq!(lane_of(&lanes, &fresh), None);
    }

    #[test]
    fn archived_snoozed_and_bulk_mail_stay_off_the_desk() {
        let archived = ThreadId::new();
        let snoozed = ThreadId::new();
        let newsletter = ThreadId::new();
        let robot = ThreadId::new();
        let mut archived_msg = message(&archived, "maya@example.com", ME, 2);
        archived_msg.in_inbox = false;
        let mut snoozed_msg = message(&snoozed, "maya@example.com", ME, 2);
        snoozed_msg.snoozed = true;
        let mut list_msg = message(&newsletter, "weekly@lists.example.com", ME, 2);
        list_msg.list_id = Some("<weekly.lists.example.com>".into());
        let robot_msg = message(&robot, "no-reply@shop.example.com", ME, 2);
        let lanes = lanes(&[archived_msg, snoozed_msg, list_msg, robot_msg], &[]);
        assert!(lanes.rows.is_empty(), "{:?}", lanes.rows);
        assert_eq!(lanes.elsewhere.reading, 1);
        assert_eq!(lanes.elsewhere.paper_trail, 1);
    }

    #[test]
    fn reading_counts_this_weeks_issues_read_or_not() {
        let mut read = message(&ThreadId::new(), "weekly@lists.example.com", ME, 2);
        read.list_id = Some("<weekly.lists.example.com>".into());
        read.flags = MessageFlags::READ;
        let mut unread = read.clone();
        unread.thread_id = ThreadId::new();
        unread.flags = MessageFlags::empty();
        let lanes = lanes(&[read, unread], &[]);
        assert_eq!(lanes.elsewhere.reading, 2);
    }

    #[test]
    fn the_window_stands_in_for_contacts_that_are_not_refreshed_yet() {
        let earlier = ThreadId::new();
        let fresh = ThreadId::new();
        let twice = ThreadId::new();
        let twice_again = ThreadId::new();
        let messages = vec![
            // You wrote Maya in one thread; her new thread is owed.
            message(&earlier, ME, "maya@example.com", 100),
            message(&fresh, "maya@example.com", ME, 4),
            // Theo wrote twice: new from people, but not a first message.
            message(&twice, "theo@example.com", ME, 30),
            message(&twice_again, "theo@example.com", ME, 3),
        ];
        let lanes = lanes(&messages, &[]);
        assert_eq!(lane_of(&lanes, &fresh), Some(DeskLaneKind::Owed));
        let theo = lanes
            .rows
            .iter()
            .find(|r| r.row.thread_id == twice_again)
            .unwrap();
        assert_eq!(theo.row.lane, DeskLaneKind::PeopleNew);
        assert_eq!(theo.row.reason, "wrote to you");
    }

    #[test]
    fn a_future_dated_message_cannot_hide_an_owed_reply() {
        let thread = ThreadId::new();
        let mut messages = vec![
            message(&thread, ME, "maya@example.com", 30),
            message(&thread, "maya@example.com", ME, 5),
            // Your outbound with a bogus Date a year ahead.
            message(&thread, ME, "maya@example.com", 0),
        ];
        messages[2].date = now() + Duration::days(365);
        let lanes = lanes(&messages, &[]);
        let row = &lanes.rows[0].row;
        assert_eq!(row.lane, DeskLaneKind::Owed);
        assert_eq!(row.message_id, messages[1].id);
        assert_eq!(
            row.message_ids.len(),
            3,
            "verbs still cover the whole thread"
        );
    }

    #[test]
    fn several_unanswered_messages_say_how_many() {
        let thread = ThreadId::new();
        let messages = vec![
            message(&thread, ME, "maya@example.com", 40),
            message(&thread, "maya@example.com", ME, 20),
            message(&thread, "maya@example.com", ME, 10),
        ];
        let lanes = lanes(&messages, &[]);
        assert_eq!(lanes.rows[0].row.reason, "2 messages since you last wrote");
    }

    #[test]
    fn pace_marks_rows_past_the_usual_reply_time() {
        let thread = ThreadId::new();
        let messages = vec![
            message(&thread, ME, "maya@example.com", 60),
            message(&thread, "maya@example.com", ME, 48),
        ];
        let mut row = lanes(&messages, &[]).rows.remove(0).row;
        apply_pace(&mut row, Some(&[4 * 3600, 3 * 3600, 5 * 3600]));
        assert_eq!(row.usual_seconds, Some(4 * 3600));
        assert_eq!(row.usual_samples, 3);
        assert!(row.overdue);

        // One past reply is not a pattern.
        let mut single = row.clone();
        single.usual_seconds = None;
        single.overdue = false;
        apply_pace(&mut single, Some(&[60]));
        assert_eq!(single.usual_seconds, None);
        assert!(!single.overdue);
    }

    #[test]
    fn owed_rows_rank_by_distance_past_usual_pace() {
        let make = |age_hours: i64, usual_hours: Option<i64>| {
            let thread = ThreadId::new();
            let messages = vec![
                message(&thread, ME, "a@example.com", age_hours + 10),
                message(&thread, "a@example.com", ME, age_hours),
            ];
            let mut row = lanes(&messages, &[]).rows.remove(0).row;
            if let Some(usual) = usual_hours {
                apply_pace(&mut row, Some(&[usual * 3600, usual * 3600]));
            }
            row
        };
        // 10h against a 2h habit outranks 30h against the 24h default.
        let quick_person = make(10, Some(2));
        let unknown_pace = make(30, None);
        let slow_person = make(20, Some(48));
        let mut rows = vec![
            slow_person.clone(),
            unknown_pace.clone(),
            quick_person.clone(),
        ];
        sort_lane(DeskLaneKind::Owed, &mut rows);
        let order: Vec<_> = rows.iter().map(|r| r.thread_id.clone()).collect();
        assert_eq!(
            order,
            vec![
                quick_person.thread_id,
                unknown_pace.thread_id,
                slow_person.thread_id
            ]
        );
    }

    #[test]
    fn a_thread_keeps_only_its_highest_lane() {
        let thread = ThreadId::new();
        let messages = vec![
            message(&thread, ME, "maya@example.com", 40),
            message(&thread, "maya@example.com", ME, 20),
        ];
        let owed = lanes(&messages, &[]).rows.remove(0).row;
        let mut due = owed.clone();
        due.lane = DeskLaneKind::Due;
        let mut other = owed.clone();
        other.thread_id = ThreadId::new();
        other.lane = DeskLaneKind::PeopleNew;
        let kept = dedupe_by_precedence(vec![due, other.clone(), owed.clone()]);
        assert_eq!(kept.len(), 2);
        assert!(kept
            .iter()
            .any(|r| r.thread_id == owed.thread_id && r.lane == DeskLaneKind::Owed));
        assert!(kept.iter().any(|r| r.thread_id == other.thread_id));
    }

    #[test]
    fn subjects_lose_reply_prefixes() {
        assert_eq!(clean_subject("Re: RE: Fwd: Launch plan"), "Launch plan");
        assert_eq!(clean_subject("Regarding the plan"), "Regarding the plan");
        assert_eq!(clean_subject("Re:"), "");
    }

    #[test]
    fn automated_senders_are_recognised_by_local_part() {
        assert!(looks_automated("no-reply@github.com"));
        assert!(looks_automated("Notifications@Example.com"));
        assert!(looks_automated("uptime@alerts.example.com"));
        assert!(!looks_automated("maya@orbit.example"));
        assert!(!looks_automated("editor@news.com"));
    }
}
