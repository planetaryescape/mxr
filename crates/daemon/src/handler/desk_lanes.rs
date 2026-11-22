use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::{MessageDirection, UnsubscribeMethod};
use mxr_protocol::{DeskElsewhereData, DeskLaneKind, DeskRowData};
use mxr_store::{DeskContact, DeskDismissal, DeskMessage, ScreenerDisposition};
use std::collections::{HashMap, HashSet};

pub(super) const DESK_WINDOW_DAYS: i64 = 30;
pub(super) const RECENT_DAYS: i64 = 7;
pub(super) const SCREENER_NEW_SENDER_DAYS: i64 = 14;
pub(super) const WAITING_MIN_HOURS: i64 = 12;
pub(super) const DUE_AHEAD_DAYS: i64 = 7;
pub(super) const USUAL_MIN_SAMPLES: usize = 2;
pub(super) const TURN_DECAY_FLOOR_DAYS: i64 = 7;
const OVERDUE_FLOOR_SECONDS: i64 = 60 * 60;
const DEFAULT_PACE_SECONDS: i64 = 24 * 60 * 60;

use super::conversation_shape::{conversation_shape, Shape, ShapeConfig, ShapeInputs};
use super::desk_timers::{DeskTimers, Timer};
use super::mail_kind::{self, KindSignals};
use super::mail_kind::{looks_automated, SenderKind};

pub(super) fn is_outbound(message: &DeskMessage, is_self: &dyn Fn(&str) -> bool) -> bool {
    message.direction == "outbound"
        || (message.direction != "inbound"
            && MessageDirection::for_message(is_self(&message.from.email), message.flags)
                == MessageDirection::Outbound)
}

pub(super) fn needs_attention(row: &DeskRowData) -> bool {
    matches!(row.lane, DeskLaneKind::Owed | DeskLaneKind::PeopleNew)
        || (row.lane == DeskLaneKind::Waiting && row.back_at.is_some())
}

pub(super) fn current_messages(thread: &[DeskMessage], now: DateTime<Utc>) -> &[DeskMessage] {
    let cutoff = now + Duration::days(1);
    &thread[..thread.partition_point(|m| m.date <= cutoff)]
}

pub(super) fn last_stored(
    current: &[DeskMessage],
    keep: impl Fn(&DeskMessage) -> bool,
) -> Option<&DeskMessage> {
    current
        .iter()
        .filter(|message| keep(message))
        .max_by_key(|message| message.seq)
}

pub(super) struct WaitingAside<'a> {
    pub dismissed: &'a HashMap<ThreadId, DeskDismissal>,
    pub timers: &'a DeskTimers,
    pub answers: &'a dyn Fn(&DeskMessage) -> bool,
    pub now: DateTime<Utc>,
}

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

fn waiting_archived(thread: &[DeskMessage], is_self: &dyn Fn(&str) -> bool) -> bool {
    let has_inbound = thread.iter().any(|m| !is_outbound(m, is_self));
    let in_inbox = thread.iter().any(|m| m.in_inbox && !m.trashed);
    has_inbound && !in_inbox
}

pub(super) fn thread_starred(thread: &[DeskMessage]) -> bool {
    thread
        .iter()
        .any(|m| m.flags.contains(mxr_core::MessageFlags::STARRED))
}

pub(super) struct AccountInputs<'a> {
    pub pending_threads: &'a HashSet<ThreadId>,
    pub assessments: &'a HashMap<MessageId, mxr_core::assessment::MailAssessment>,
    pub asks: &'a HashMap<MessageId, mxr_store::AskSummary>,
    pub account_id: &'a AccountId,
    pub messages: &'a [DeskMessage],
    pub contacts: &'a HashMap<String, DeskContact>,
    pub written_to: &'a HashSet<String>,
    pub screener: &'a HashMap<String, ScreenerDisposition>,
    pub dismissed: &'a HashMap<ThreadId, DeskDismissal>,
    pub timers: &'a DeskTimers,
    pub is_self: &'a dyn Fn(&str) -> bool,
    pub moves: &'a HashMap<MessageId, SenderKind>,
    pub treatments: &'a HashMap<MessageId, (String, String)>,
    pub shape: ShapeConfig,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Default)]
struct WindowFacts {
    written_to: HashSet<String>,
    inbound: HashMap<String, u32>,
}

impl WindowFacts {
    fn wrote_to(&self, inputs: &AccountInputs<'_>, email: &str) -> bool {
        inputs.contact(email).is_some_and(|c| c.total_outbound > 0)
            || inputs.written_to.contains(&email.to_ascii_lowercase())
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

    fn signals<'a>(&'a self, message: &'a DeskMessage) -> KindSignals<'a> {
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
        signals.written_to |= self.written_to.contains(&email.to_ascii_lowercase());
        signals.assessment = self.assessments.get(&message.id);
        signals
    }

    pub(super) fn sender_kind(&self, message: &DeskMessage) -> SenderKind {
        if let Some(home) = self.assessments.get(&message.id) {
            return mail_kind::stored_classification(home).kind;
        }
        mail_kind::classify(&self.signals(message)).kind
    }

    pub(super) fn answers(&self, message: &DeskMessage) -> bool {
        !self.is_outbound(message) && mail_kind::human_response(&self.signals(message))
    }

    pub(super) fn shape(&self, thread: &[DeskMessage]) -> Shape {
        let assessed: Vec<_> = thread
            .iter()
            .filter_map(|m| self.assessments.get(&m.id))
            .collect();
        if !assessed.is_empty() && !assessed.iter().any(|a| a.home == "messages") {
            return if assessed.iter().any(|a| a.deciding.rule == "copied") {
                Shape::Copied
            } else {
                Shape::NotConversation
            };
        }
        let person_sender = |m: &DeskMessage| self.sender_kind(m) == SenderKind::Person;
        let human_address = |email: &str| self.human_address(email);
        let kept = |m: &DeskMessage| mail_kind::kept_in_messages(&self.signals(m));
        let geometry = if assessed.iter().any(|a| a.home == "messages") {
            super::conversation_shape::conversation_geometry
        } else {
            conversation_shape
        };
        geometry(
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

    pub(super) fn human_address(&self, email: &str) -> bool {
        super::conversation_shape::human_address(email, self.screener)
    }

    fn only_copied(&self, message: &DeskMessage) -> bool {
        !message.to.iter().any(|a| (self.is_self)(&a.email))
            && message.cc.iter().any(|a| (self.is_self)(&a.email))
    }
}

pub(super) fn desk_signals<'a>(
    message: &'a DeskMessage,
    contact: Option<&DeskContact>,
    decision: Option<ScreenerDisposition>,
    moved: Option<SenderKind>,
    is_self: &dyn Fn(&str) -> bool,
) -> KindSignals<'a> {
    KindSignals {
        assessment: None,
        evidence: &message.evidence,
        reply_request: mail_kind::needs_reply(message, is_self),
        recipients: message.to.len() + message.cc.len(),
        email: &message.from.email,
        subject: &message.subject,
        has_list_id: message
            .list_id
            .as_deref()
            .and_then(mxr_core::mail_evidence::normalize_list_id)
            .is_some()
            || message.evidence.list_id.is_some(),
        has_unsubscribe: message.evidence.unsubscribe
            || !matches!(message.unsubscribe, UnsubscribeMethod::None),
        is_delivery: message.is_delivery,
        is_invite: message.is_invite,
        sender: mail_kind::SenderFacts::of(contact),
        decision,
        written_to: contact.is_some_and(|c| c.total_outbound > 0),
        addressed: addressed_to_you(&message.to, is_self),
        moved,
        treatment: None,
    }
}

pub(super) fn addressed_to_you(
    to: &[mxr_core::types::Address],
    is_self: &dyn Fn(&str) -> bool,
) -> bool {
    to.iter().any(|a| is_self(&a.email))
}

#[derive(Debug, Clone)]
pub(super) struct DraftRow {
    pub row: DeskRowData,
    pub pace: Option<PaceDirection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PaceDirection {
    Mine,
    Theirs,
}

#[derive(Debug, Default)]
pub(super) struct ThreadLanes {
    pub rows: Vec<DraftRow>,
    pub elsewhere: DeskElsewhereData,
    pub last_from_people_at: Option<DateTime<Utc>>,
    pub turn_people: HashMap<ThreadId, Vec<String>>,
    pub screener_senders: HashSet<String>,
}

pub(super) fn thread_lanes(inputs: &AccountInputs<'_>) -> ThreadLanes {
    let now = inputs.now;
    let recent = now - Duration::days(RECENT_DAYS);
    let screener_since = now - Duration::days(SCREENER_NEW_SENDER_DAYS);
    let mut lanes = ThreadLanes::default();
    let mut screener_senders: HashSet<String> = HashSet::new();
    let facts = WindowFacts::from_inputs(inputs);

    for thread in inputs.messages.chunk_by(|a, b| a.thread_id == b.thread_id) {
        if thread.is_empty() || inputs.pending_threads.contains(&thread[0].thread_id) {
            continue;
        }

        for message in thread {
            if inputs.is_outbound(message)
                || !message.in_inbox
                || message.trashed
                || message.date < recent
            {
                continue;
            }
            match inputs.sender_kind(message) {
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

        if thread.iter().any(|m| m.snoozed)
            || inputs.timers.reply_later(&thread[0].thread_id, inputs.now) == Some(Timer::Pending)
        {
            continue;
        }
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
            if needs_attention(&row.row) {
                lanes.turn_people.insert(
                    row.row.thread_id.clone(),
                    inputs.shape(thread).people().to_vec(),
                );
            }
            lanes.rows.push(row);
        }
    }
    lanes.elsewhere.screener = screener_senders.len() as u32;
    lanes.screener_senders = screener_senders;
    lanes
}

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
    if let Some(sent) = latest_outbound {
        let answers = |m: &DeskMessage| inputs.answers(m);
        if inputs
            .timers
            .waiting(conversation.all, &answers, now)
            .is_some()
            && !inputs.answers(latest)
        {
            return waiting_row(inputs, conversation, sent, latest_inbound.is_some());
        }
    }

    let inbound = latest_inbound?;
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
    let owes_reply = inputs.asks.get(&inbound.id).is_some_and(|answer| {
        answer
            .answers
            .contains(&mxr_core::asks::AskAnswer::AsksYouToReply)
    });
    if !owes_reply
        && inputs.asks.get(&latest.id).is_some_and(|answer| {
            answer
                .answers
                .contains(&mxr_core::asks::AskAnswer::TheyPromisedYou)
        })
    {
        return waiting_row(inputs, conversation, latest, latest_inbound.is_some());
    }
    if inbound.trashed || !in_inbox || inputs.sender_kind(inbound) != SenderKind::Person {
        return None;
    }
    if inputs.shape(conversation.all) == Shape::Copied {
        return None;
    }
    if !owes_reply {
        return None;
    }
    let email = inbound.from.email.as_str();
    let key = email.to_ascii_lowercase();
    let contact = inputs.contact(email);
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
        if inbound.date < now - turn_decay(contact) {
            return None;
        }
        (DeskLaneKind::Owed, reason, Some(PaceDirection::Mine))
    } else {
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
    let recipient = if inputs.is_outbound(sent) {
        mxr_core::first_non_owned_recipient(&sent.to, &sent.cc, inputs.is_self)?
    } else {
        &sent.from
    };
    let email = recipient.email.as_str();
    let contact = inputs.contact(email);
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
    if !inputs.asks.get(&sent.id).is_some_and(|answer| {
        answer.answers.iter().any(|answer| {
            matches!(
                answer,
                mxr_core::asks::AskAnswer::YouAskedThem
                    | mxr_core::asks::AskAnswer::TheyPromisedYou
            )
        })
    }) {
        return None;
    }
    if looks_automated(email)
        || contact.is_some_and(|c| c.is_list_sender)
        || inputs.decision(email) == Some(ScreenerDisposition::Deny)
    {
        return None;
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

pub(super) fn pace_seconds(row: &DeskRowData) -> i64 {
    row.usual_seconds.unwrap_or(DEFAULT_PACE_SECONDS).max(60)
}

pub(super) fn pace_ratio(row: &DeskRowData) -> f64 {
    row.age_seconds as f64 / pace_seconds(row) as f64
}

pub(super) const LANE_PRECEDENCE: [DeskLaneKind; 4] = [
    DeskLaneKind::Owed,
    DeskLaneKind::Due,
    DeskLaneKind::Waiting,
    DeskLaneKind::PeopleNew,
];

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

    #[test]
    fn unknown_direction_mail_filed_as_sent_is_outbound_from_an_unowned_alias() {
        let thread = ThreadId::new();
        let mut sent = message(&thread, "alias@send-as.example", "carol@example.com", 2);
        sent.direction = "unknown".into();
        sent.flags = MessageFlags::SENT;
        let is_self = |email: &str| email == ME;
        assert!(is_outbound(&sent, &is_self));

        sent.flags = MessageFlags::empty();
        assert!(!is_outbound(&sent, &is_self));
    }

    fn message(thread: &ThreadId, from: &str, to: &str, hours_ago: i64) -> DeskMessage {
        let outbound = from == ME;
        DeskMessage {
            evidence: mxr_core::mail_evidence::MailEvidence {
                text: Some("Could you reply?".into()),
                ..Default::default()
            },
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

    #[test]
    fn directed_requests_need_recipient_evidence_not_history_or_recipient_count() {
        let mut mail = message(&ThreadId::new(), "sam@example.com", ME, 1);
        let own = |address: &str| address == ME;
        mail.evidence.text = None;
        mail.subject = "Could you reply?".into();
        assert!(
            !mail_kind::needs_reply(&mail, &own),
            "missing body is incomplete evidence, not reply debt"
        );
        for text in [
            "FYI: weekly report",
            "Thanks, received",
            "Please do not reply",
            "Please find attached the weekly report",
            "Please pay invoice INV-1234",
            "Alice, could you decide?",
            "Alice:\nCould you confirm the reference?",
            "> Could you reply?",
            "Thanks\n-----Original Message-----\nCould you reply?",
            "Thanks\n-- \nCould you reply?",
            "Could you not reply?",
        ] {
            mail.evidence.text = Some(text.into());
            assert!(!mail_kind::needs_reply(&mail, &own), "{text}");
        }
        mail.to.extend((0..8).map(|i| Address {
            name: None,
            email: format!("p{i}@example.com"),
        }));
        mail.evidence.text = Some("Could you decide?".into());
        assert!(
            !mail_kind::needs_reply(&mail, &own),
            "ambiguous team request"
        );
        mail.evidence.text = Some("Me, could you confirm the invoice reference?".into());
        assert!(mail_kind::needs_reply(&mail, &own), "directed team request");
        mail.to[0].name = Some("Alex Example".into());
        mail.evidence.text = Some("Alex, could you confirm the invoice reference?".into());
        assert!(
            mail_kind::needs_reply(&mail, &own),
            "owned recipient's first name"
        );
        mail.to.retain(|a| a.email != ME);
        mail.cc.push(Address {
            name: None,
            email: ME.into(),
        });
        assert!(!mail_kind::needs_reply(&mail, &own), "only copied");
    }

    #[test]
    fn reader_authored_scope_not_old_outlook_or_forwarded_requests_decides_attention() {
        let old = "From: Alex <alex@example.org>\nSent: Monday\nTo: Me <me@example.com>\nSubject: Old request\n\nCould you confirm the details?";
        let mut mail = message(&ThreadId::new(), "sam@example.org", ME, 1);
        let own = |email: &str| email == ME;
        for text in [
            old.to_string(),
            format!("FYI, this was handled.\n\n{old}"),
            format!("FYI, this was handled.\n\n---------- Forwarded message ---------\n{old}"),
        ] {
            mail.evidence.text = Some(text);
            assert!(!mail_kind::needs_reply(&mail, &own));
        }
        mail.evidence.text = Some(format!("Could you confirm the new date?\n\n{old}"));
        assert!(mail_kind::needs_reply(&mail, &own));
        let prior = "Could you confirm the launch date by Monday and send the signed copy?\nPlease also share the final reference with everyone before the Friday review meeting.";
        mail.evidence.previous_text = Some(prior.into());
        mail.evidence.text = Some(format!("FYI, this was handled.\n\n{prior}"));
        assert!(
            !mail_kind::needs_reply(&mail, &own),
            "unmarked history is compared with the bounded local cache"
        );
        mail.evidence.text = Some(format!("Could you confirm the new date?\n\n{prior}"));
        assert!(mail_kind::needs_reply(&mail, &own));
    }

    fn lanes_with_screener(
        messages: &[DeskMessage],
        contacts: &[DeskContact],
        screener: HashMap<String, ScreenerDisposition>,
    ) -> ThreadLanes {
        let account = AccountId::new();
        let contacts: HashMap<String, DeskContact> = contacts
            .iter()
            .map(|c| (c.email.to_ascii_lowercase(), c.clone()))
            .collect();
        let dismissed = HashMap::new();
        let timers = DeskTimers::default();
        let is_self = |email: &str| email.eq_ignore_ascii_case(ME);
        let moves = HashMap::new();
        let asks: HashMap<_, _> = messages
            .iter()
            .map(|message| {
                let reply_capable = mail_kind::human_response(&desk_signals(
                    message,
                    contacts.get(&message.from.email.to_ascii_lowercase()),
                    screener
                        .get(&message.from.email.to_ascii_lowercase())
                        .copied(),
                    None,
                    &is_self,
                ));
                let answer = mxr_todo::asks::assess(
                    &mxr_todo::asks::AskInput {
                        message_id: &message.id,
                        account_id: &account,
                        subject: &message.subject,
                        body: message.evidence.text.as_deref(),
                        from: &message.from,
                        to: &message.to,
                        cc: &message.cc,
                        bcc: &message.bcc,
                        list_mail: message.list_id.is_some(),
                        bulk: message.evidence.bulk,
                        owner_names: &[],
                        owner_addresses: &[ME.into()],
                        is_owner: &is_self,
                        sent: message.date,
                        predecessor: None,
                        owner_replied_afterwards: false,
                        reply_capable,
                    },
                    &Utc,
                    "lane-fixture".into(),
                );
                (message.id.clone(), mxr_store::AskSummary::from(answer))
            })
            .collect();
        thread_lanes(&AccountInputs {
            pending_threads: &HashSet::new(),
            assessments: &HashMap::new(),
            asks: &asks,
            account_id: &account,
            messages,
            contacts: &contacts,
            written_to: &HashSet::new(),
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
    fn allowing_a_sender_controls_visibility_not_human_response_or_reply_debt() {
        let never_replied = ThreadId::new();
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
        assert_eq!(lane_of(&lanes, &never_replied), None);
        assert_eq!(lane_of(&lanes, &replied), Some(DeskLaneKind::Owed));
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
    fn a_future_dated_message_cannot_hide_an_owed_reply() {
        let thread = ThreadId::new();
        let mut messages = vec![
            message(&thread, ME, "maya@example.com", 30),
            message(&thread, "maya@example.com", ME, 5),
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
}
