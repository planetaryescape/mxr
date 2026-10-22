//! The bulk demo as labelled scenario families (blueprint 32, "The demo
//! dataset is rebuilt from labelled scenarios").
//!
//! The generated demo used to pick a sender pool by `thread_num % 13` and
//! say nothing about what each email should be. Now each of those 13 slots
//! belongs to a [`Family`]: a kind of mail (a colleague's thread, a thread
//! you were only copied on, a receipt, a newsletter, spam...). A family
//! states, per message position in its thread, who writes and what the
//! classifier should answer ([`Turn`]), and carries several phrasings of the
//! same mail ([`Variant`]). Some phrasings are held out
//! (`scenarios/held_out.rs`).
//!
//! Everything here is a pure function of the thread number and the message
//! position, never of the clock or of how many messages the demo holds:
//!
//! * [`family_for`], [`thread_len`] and [`variant_for`] pick the mail;
//! * [`locate`] turns a message's place in the bulk into its thread and
//!   position without generating any mail;
//! * [`label`] answers for a position, so the 50,000 labels are never held
//!   in memory and a page, a `fetch_message` and a regeneration all agree.
//!
//! Dates written in the text come from the email's own date, so the mail
//! and its labels do not move with the seed clock (see
//! `scenario_dates_follow_the_seed_clock` for the curated set and
//! `bulk_dates_follow_the_seed_clock` for this one).

mod commerce;
mod lists;
mod notices;
mod people;
pub(crate) mod pools;
mod render;
mod spam;

pub(crate) use render::render;

use super::held_out;
use super::{Expect, Home, Review, Who};
use pools::{Person, DEMO_PEOPLE};
use std::sync::OnceLock;

/// The slots a thread number maps onto. Kept at 13 so the mix of mail, the
/// thread lengths and the numbering stay what they were.
const SLOTS: usize = 13;
/// Threads after which both the slot and the length pattern repeat:
/// `lcm(13, 4)`, because lengths depend on `thread_num % 4` too.
const PERIOD: usize = 52;
/// Minutes between the messages of a thread.
pub(crate) const MINUTES_BETWEEN_MESSAGES: i64 = 47;

/// What the classifier should say, built from the four questions.
pub(crate) const fn expect(who: Who, home: Home, todo: bool, record: bool) -> Expect {
    Expect {
        who,
        home,
        todo,
        record,
    }
}

/// A machine's notice: Updates, nothing to do, nothing to keep.
pub(crate) const NOTICE: Expect = expect(Who::Machine, Home::Updates, false, false);
/// A machine's receipt, order, booking or bill: Updates, kept as a record.
pub(crate) const FILED: Expect = expect(Who::Machine, Home::Updates, false, true);
/// A newsletter: Reading.
pub(crate) const READING: Expect = expect(Who::List, Home::Reading, false, false);
/// A person's conversation with you.
pub(crate) const CONVERSATION: Expect = expect(Who::Person, Home::Messages, false, false);
/// A person's thread you were only copied on: Updates (the copied rule).
pub(crate) const COPIED: Expect = expect(Who::Person, Home::Updates, false, false);

/// Who writes a message of a thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Speaker {
    /// The thread's sender: inbound mail, labelled.
    Them,
    /// You, replying. Sent mail is context and is never classified.
    You,
}

/// One message position in a family's thread.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Turn {
    /// The label's scenario id, unique across families.
    pub scenario: &'static str,
    pub description: &'static str,
    pub from: Speaker,
    /// The answers when the mail arrives. A to-do whose deadline had already
    /// passed when the mail was found is never shown, so `todo` is true only
    /// while the deadline is open (see [`Dating::Asks`]).
    pub expect: Expect,
    pub review: Option<Review>,
}

impl Turn {
    /// A message from the thread's sender.
    pub(crate) const fn them(
        scenario: &'static str,
        description: &'static str,
        expect: Expect,
    ) -> Self {
        Self {
            scenario,
            description,
            from: Speaker::Them,
            expect,
            review: None,
        }
    }

    /// Your reply: context, never labelled.
    pub(crate) const fn you(scenario: &'static str, description: &'static str) -> Self {
        Self {
            scenario,
            description,
            from: Speaker::You,
            expect: CONVERSATION,
            review: None,
        }
    }

    /// The answers in `questions` are a judgement call, for `why`.
    pub(crate) const fn reviewed(
        self,
        questions: &'static [&'static str],
        why: &'static str,
    ) -> Self {
        Self {
            review: Some(Review { questions, why }),
            ..self
        }
    }
}

/// One phrasing of a family's mail.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Variant {
    /// Names the phrasing in the data and in test failures; the generator
    /// does not read it.
    #[allow(dead_code)]
    pub id: &'static str,
    /// Per [`Family::subjects`]: none (the shared subject pool), one for the
    /// whole thread, or one per turn.
    pub subjects: &'static [&'static str],
    /// The body of each inbound turn, in order. Your replies come from
    /// [`Family::sent_replies`].
    pub bodies: &'static [&'static str],
}

/// Where a thread's subjects come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Subjects {
    /// The pool shared by people threads; replies are "Re: ...".
    Pool,
    /// One subject per variant; replies are "Re: ...".
    Thread,
    /// One subject per turn, as a machine sends them.
    PerTurn,
}

/// Which senders a family writes from.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Senders {
    /// A person from the demo's people. `copied`: you are only copied, and
    /// two of them write to each other.
    People { copied: bool },
    /// A machine or list, picked by thread from a pool.
    Pool(&'static [Person]),
    /// A machine whose phrasing belongs to one sender, so the variant picks
    /// the sender: variant `i` is written by `pool[i % pool.len()]`.
    ByVariant(&'static [Person]),
}

/// How the mail is dated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dating {
    /// Spread over the year behind the seed clock, as the demo always was.
    Spread,
    /// Mail that asks for something by a date. A to-do is shown only while
    /// its deadline is open, so one thread in [`ASKS_RECENT_EVERY`] arrives
    /// in the last hours (the ask is live: a to-do) and the rest arrive at
    /// least two months back (the deadline passed before the mail was found:
    /// never a to-do, as `todo-event-already-over` says).
    Asks,
}

/// One in this many ask threads (by `thread_num / SLOTS`) arrives recent.
/// The first threads, which the web e2e suite reads, are all old.
pub(crate) const ASKS_RECENT_EVERY: usize = 30;

/// Which attachments a family's mail carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attachments {
    None,
    /// Every message.
    Always,
    /// The first message only.
    FirstTurn,
    /// Every message of one thread in this many.
    EveryNth(usize),
}

/// How the mail offers to unsubscribe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unsubscribe {
    None,
    /// One-click on even threads, a mailto on odd ones.
    Lists,
    /// One-click to the promotions host.
    Promo,
}

/// A kind of mail.
#[derive(Debug)]
pub(crate) struct Family {
    pub id: &'static str,
    pub senders: Senders,
    pub subjects: Subjects,
    /// A second person is copied on every message.
    pub observer_cc: bool,
    pub dating: Dating,
    /// Days from an email's date to a date it asks for.
    pub due_days: i64,
    /// Prefix of the reference numbers in the text ("ORD-48213").
    pub ref_prefix: &'static str,
    /// The look of the mail by thread number: attachments, links and image.
    /// The demo's old category numbers, which `demo_attachments` and
    /// `demo_html_body` read.
    pub style: fn(usize) -> usize,
    pub attachments: Attachments,
    /// An extra brief attached to one people thread in 41.
    pub brief_every_41: bool,
    pub unsubscribe: Unsubscribe,
    /// The provider label of the thread, by thread number.
    pub provider_label: fn(usize) -> Option<&'static str>,
    /// The provider's Waiting label on a thread's last inbound message.
    pub waiting: bool,
    /// Filed under Spam by the provider.
    pub spam_folder: bool,
    /// Every this-many-th thread is archived.
    pub archived_every: Option<usize>,
    pub turns: &'static [Turn],
    pub sent_replies: &'static [&'static str],
    pub variants: &'static [Variant],
}

impl Family {
    /// Visible variants plus the held-out ones.
    pub(crate) fn variant_count(&self) -> usize {
        self.variants.len() + held_out::variants(self.id).len()
    }

    /// The variant at `index` and whether it is held out.
    pub(crate) fn variant(&self, index: usize) -> (&'static Variant, bool) {
        match self.variants.get(index) {
            Some(variant) => (variant, false),
            None => (
                &held_out::variants(self.id)[index - self.variants.len()],
                true,
            ),
        }
    }

    /// The inbound turns before position `turn`: which body to use.
    pub(crate) fn inbound_before(&self, turn: usize) -> usize {
        self.turns[..turn]
            .iter()
            .filter(|t| t.from == Speaker::Them)
            .count()
    }
}

/// Every family, for the tests.
#[cfg(test)]
pub(crate) fn families() -> [&'static Family; 13] {
    [
        &people::PEOPLE_THREAD,
        &people::DATED_REQUEST,
        &people::HIRING_THREAD,
        &people::COPIED_THREAD,
        &commerce::ORDER_RECEIPT,
        &commerce::BILL_DUE,
        &notices::TRAVEL_BOOKING,
        &notices::SERVICE_ALERT,
        &notices::SECURITY_NOTICE,
        &lists::NEWSLETTER,
        &lists::PROMOTION,
        &spam::SPAM_PRIZE,
        &spam::PHISHING,
    ]
}

/// The family of a thread: its slot, and for the two slots that hold two
/// kinds, the round it falls in.
pub(crate) fn family_for(thread_num: usize) -> &'static Family {
    let round = thread_num / SLOTS;
    match thread_num % SLOTS {
        0 | 1 | 4 => &people::PEOPLE_THREAD,
        2 => &lists::NEWSLETTER,
        3 => &people::DATED_REQUEST,
        // Two receipts for every bill.
        5 if round % 3 == 2 => &commerce::BILL_DUE,
        5 => &commerce::ORDER_RECEIPT,
        6 => &notices::TRAVEL_BOOKING,
        7 => &notices::SERVICE_ALERT,
        8 => &people::HIRING_THREAD,
        9 => &people::COPIED_THREAD,
        10 => &spam::SPAM_PRIZE,
        11 => &lists::PROMOTION,
        // The demo's first suspicious inbox mail has always been thread 12.
        _ if round.is_multiple_of(2) => &spam::PHISHING,
        _ => &notices::SECURITY_NOTICE,
    }
}

/// Messages in a thread. The same as the demo always had, so the numbering
/// of every message stays put.
pub(crate) fn thread_len(thread_num: usize) -> usize {
    match thread_num % SLOTS {
        1 | 4 => 5,
        2 | 7 | 10 | 11 | 12 => 1,
        8 => 3,
        _ => 2 + (thread_num % 4),
    }
}

/// Which phrasing a thread uses, and whether it is held out. A hash rather
/// than the thread number, because a family's threads are evenly spaced and
/// a plain modulo would only ever reach some of its variants.
pub(crate) fn variant_for(family: &Family, thread_num: usize) -> (usize, bool) {
    let mixed = (thread_num as u64)
        .wrapping_add(1)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let index = ((mixed >> 33) % family.variant_count() as u64) as usize;
    (index, index >= family.variants.len())
}

/// Whether an ask thread arrives in the last hours, so its ask is live.
pub(crate) fn asks_recent(thread_num: usize) -> bool {
    (thread_num / SLOTS) % ASKS_RECENT_EVERY == ASKS_RECENT_EVERY - 1
}

struct Layout {
    /// Messages in one period of [`PERIOD`] threads.
    per_period: usize,
    /// `starts[i]`: the bulk index of thread `i` of a period.
    starts: [usize; PERIOD + 1],
}

fn layout() -> &'static Layout {
    static LAYOUT: OnceLock<Layout> = OnceLock::new();
    LAYOUT.get_or_init(|| {
        let mut starts = [0; PERIOD + 1];
        for thread in 0..PERIOD {
            starts[thread + 1] = starts[thread] + thread_len(thread);
        }
        Layout {
            per_period: starts[PERIOD],
            starts,
        }
    })
}

/// The thread and position of the `index`th message of the bulk (counted
/// from 0, after any hand-written mail at the head of the account).
pub(crate) fn locate(index: usize) -> (usize, usize) {
    let layout = layout();
    let period = index / layout.per_period;
    let within = index % layout.per_period;
    let thread = layout.starts.partition_point(|start| *start <= within) - 1;
    (period * PERIOD + thread, within - layout.starts[thread])
}

/// The first message of a thread, as an index into the bulk.
#[cfg(test)]
pub(crate) fn first_index(thread_num: usize) -> usize {
    let layout = layout();
    (thread_num / PERIOD) * layout.per_period + layout.starts[thread_num % PERIOD]
}

/// The person writing the thread: the same one throughout, so a thread reads
/// as a conversation with someone.
pub(crate) fn counterpart(thread_num: usize) -> Person {
    DEMO_PEOPLE[thread_num % DEMO_PEOPLE.len()]
}

/// A second person: copied on a thread, or the other party of a copied one.
pub(crate) fn observer(thread_num: usize) -> Person {
    DEMO_PEOPLE[(thread_num + 3) % DEMO_PEOPLE.len()]
}

/// Who writes position `reply_idx` of a thread, or `None` when you do.
pub(crate) fn sender_of(family: &Family, thread_num: usize, reply_idx: usize) -> Option<Person> {
    if family.turns[reply_idx].from == Speaker::You {
        return None;
    }
    Some(match family.senders {
        Senders::People { copied: false } => counterpart(thread_num),
        // Two colleagues talk; you are copied. They take turns.
        Senders::People { copied: true } if reply_idx.is_multiple_of(2) => {
            DEMO_PEOPLE[(thread_num + 5) % DEMO_PEOPLE.len()]
        }
        Senders::People { copied: true } => counterpart(thread_num),
        Senders::Pool(pool) => pool[thread_num % pool.len()],
        Senders::ByVariant(pool) => pool[variant_for(family, thread_num).0 % pool.len()],
    })
}

/// What a bulk position should be classified as.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BulkLabel {
    pub turn: &'static Turn,
    pub held_out: bool,
    pub expect: Expect,
    pub from_email: &'static str,
}

/// The answer for a position, or `None` when you wrote it.
pub(crate) fn label(thread_num: usize, reply_idx: usize) -> Option<BulkLabel> {
    let family = family_for(thread_num);
    if reply_idx >= family.turns.len() {
        return None;
    }
    let turn = &family.turns[reply_idx];
    let sender = sender_of(family, thread_num, reply_idx)?;
    let (_, held_out) = variant_for(family, thread_num);
    let mut expect = turn.expect;
    if family.dating == Dating::Asks && !asks_recent(thread_num) {
        // The deadline passed before the mail was found: no to-do is shown.
        expect.todo = false;
    }
    Some(BulkLabel {
        turn,
        held_out,
        expect,
        from_email: sender.1,
    })
}

#[cfg(test)]
mod tests;
