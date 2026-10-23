//! Labelled scenarios: mail that carries the answer a classifier should
//! give (blueprint 32, "The demo dataset is rebuilt from labelled
//! scenarios"). `mxr classify eval` measures today's code against them.
//!
//! A scenario is a small typed template that produces mail and declares,
//! per email, what the classifier should say. Three kinds exist:
//!
//! * The curated demo fixtures (`*_demo.rs`) keep generating exactly the
//!   mail they always did, so the demo looks the same. `curated` states
//!   the intended outcome their prose describes, as data.
//! * Near-miss twins (`twins`) are pairs of emails that differ in the one
//!   fact that should change the answer. They are not in the 50,000-email
//!   demo: they would shift its message numbering and the counts its tests
//!   and the web e2e suite read. [`scenario_seed`] returns the curated mail
//!   and the twins together, for the eval's tests.
//! * The bulk of the generated demo (`bulk`) is made of scenario families:
//!   kinds of mail with several phrasings and an expectation per message
//!   position. Their labels are never held in memory: [`full_label_for`]
//!   recomputes one from the message's index. Some phrasings are held out
//!   (`held_out`) and the eval reports them on their own.
//!
//! Expectations state what the classifier SHOULD do, per blueprint 32, not
//! what today's code does, so a gap shows up as a mismatch. A label that
//! rests on a judgement call carries its reasoning in a [`Review`].
//!
//! The questions are the ones a classifier can be graded on today: who sent
//! it, which mode is its home, whether it made a record, and what it asks of
//! you. The last is a list of expected [`Item`]s (kind of ask, counterparty,
//! amount, due words) plus whether a reply is owed, not a yes or no, so the
//! eval can grade each kind of ask and each field. [`LABEL_SCHEMA_VERSION`]
//! changes whenever the questions do, so old expectations are never silently
//! reused.

mod asks;
mod bulk;
mod curated;
#[cfg(test)]
mod date_finder;
mod held_out;
#[cfg(test)]
mod leak_guard;
mod twins;
mod twins_asks;

pub use asks::{locale_of, sender_kind, Amount, AskKind, Direction, Due, Item, SenderKind};

use super::{curated_head_len, curated_seed_messages, demo_thread_id, DemoCuts};
use chrono::{DateTime, TimeZone, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::types::{Address, Envelope, MessageBody};
use std::collections::HashMap;
use std::sync::OnceLock;

pub(crate) use bulk::{render as render_bulk, thread_len as bulk_thread_len};

/// Version of the questions an [`Expect`] and its [`Item`]s answer. Version 2
/// replaces the to-do yes or no with the expected items of the email and
/// whether a reply is owed.
pub const LABEL_SCHEMA_VERSION: u32 = 2;

/// The mailbox owner of the seeded personal demo account.
pub const SELF_EMAIL: &str = "alex@demo.mxr.local";
/// The mailbox owner of the seeded work demo account.
pub const WORK_EMAIL: &str = "alex@work.demo.mxr.local";
const SELF_NAME: &str = "Alex Demo";

/// Who sent it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Who {
    Person,
    List,
    Machine,
    ScreenedOut,
}

impl Who {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::List => "list",
            Self::Machine => "machine",
            Self::ScreenedOut => "screened_out",
        }
    }
}

/// The mode an email lands in. Always one of these; To do and Archive are
/// aspects added on top (`Expect::todo`, `Expect::record`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Home {
    Messages,
    Updates,
    Reading,
    Spam,
    ScreenedOut,
}

impl Home {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Messages => "messages",
            Self::Updates => "updates",
            Self::Reading => "reading",
            Self::Spam => "spam",
            Self::ScreenedOut => "screened_out",
        }
    }
}

/// The right answers for one email, apart from the items it asks (those are
/// [`Labelled::items`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expect {
    pub who: Who,
    pub home: Home,
    /// A record is filed from this email.
    pub record: bool,
    /// A reply is owed: the email asks a question or for an answer, so it is
    /// the owner's turn (Your turn, blueprint 37).
    pub reply: bool,
}

impl Expect {
    /// An email that owes no reply.
    pub const fn new(who: Who, home: Home, record: bool) -> Self {
        Self {
            who,
            home,
            record,
            reply: false,
        }
    }

    /// The same, but a reply is owed.
    pub const fn replying(self) -> Self {
        Self {
            reply: true,
            ..self
        }
    }
}

/// Why some answers of a label are a judgement call, so a mismatch on one
/// of them may be the label's fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Review {
    /// Which questions (`who`, `home`, `todo`, `record`) it covers. `todo`
    /// covers the expected items and whether a reply is owed.
    pub questions: &'static [&'static str],
    pub why: &'static str,
}

/// One labelled email.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Labelled {
    pub provider_id: String,
    /// The sender. With the account and provider id it identifies the email:
    /// the 50-message demo and the generated demo number their mail
    /// differently, so a provider id alone can name another email.
    pub from_email: String,
    pub scenario: &'static str,
    pub description: &'static str,
    pub expect: Expect,
    /// What the email asks of the owner, or what the owner promised in it.
    /// Empty when it asks nothing.
    pub items: Vec<Item>,
    /// The owner's own sent mail: only its promises are graded, because who,
    /// home and record are about mail that arrives.
    pub outbound: bool,
    /// Who the sender is, for slicing.
    pub sender: SenderKind,
    /// `en-GB`, `en-ZA` or `en-US`, for slicing.
    pub locale: &'static str,
    /// The template family: the generator's family for bulk mail, the
    /// scenario for the rest. For slicing.
    pub family: &'static str,
    /// The scenario this email is a near-miss twin of.
    pub twin_of: Option<&'static str>,
    /// The one fact that differs from the twin; empty when not a twin.
    pub differs_by: &'static str,
    pub review: Option<Review>,
    /// The email is worded in a held-out phrasing: the eval reports these
    /// apart, to show whether a judge learned the generator.
    pub held_out: bool,
}

/// Mail and the labels that go with it.
pub struct ScenarioSeed {
    pub messages: Vec<(Envelope, MessageBody)>,
    pub labels: Vec<Labelled>,
}

/// The account the labels belong to: the personal demo account. The work
/// demo account numbers its mail from 1 too and reuses some of the same
/// people, so a provider id and a sender alone can name its mail.
pub fn account_id() -> AccountId {
    AccountId::from_provider_id("fake", SELF_EMAIL)
}

pub(crate) fn self_address() -> Address {
    Address {
        name: Some(SELF_NAME.to_string()),
        email: SELF_EMAIL.to_string(),
    }
}

/// The personal demo account's mail: the curated fixtures, then the twins,
/// numbered from 1 in that order. `now` dates the mail the way the demo
/// does.
pub fn scenario_seed(account_id: &AccountId, now: DateTime<Utc>, cuts: &DemoCuts) -> ScenarioSeed {
    let me = self_address();
    let mut messages = curated_seed_messages(account_id, &me, now, cuts);
    let mut labels = curated::label(&messages);
    let twinned = twins::build(account_id, &me, now, messages.len() + 1);
    messages.extend(twinned.messages);
    labels.extend(twinned.labels);
    ScenarioSeed { messages, labels }
}

struct QuickSet {
    labels: Vec<Labelled>,
    by_provider_id: HashMap<String, usize>,
}

fn quick() -> &'static QuickSet {
    static QUICK: OnceLock<QuickSet> = OnceLock::new();
    QUICK.get_or_init(|| {
        // Provider ids and senders do not depend on the clock or the
        // account's other mail, so any fixed seed labels the same emails.
        let account_id = account_id();
        let at = Utc
            .with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
            .single()
            .unwrap_or_default();
        let mut labels = scenario_seed(&account_id, at, &DemoCuts::default()).labels;
        labels.sort_by(|a, b| natural_key(&a.provider_id).cmp(&natural_key(&b.provider_id)));
        let by_provider_id = labels
            .iter()
            .enumerate()
            .map(|(index, label)| (label.provider_id.clone(), index))
            .collect();
        QuickSet {
            labels,
            by_provider_id,
        }
    })
}

/// `demo-msg-9` before `demo-msg-10`.
fn natural_key(provider_id: &str) -> (usize, &str) {
    (provider_id.len(), provider_id)
}

/// Every labelled email of the quick set, in provider-id order.
pub fn quick_set() -> impl Iterator<Item = &'static Labelled> {
    quick().labels.iter()
}

/// The labelled email with this provider id, if the quick set has one. The
/// caller still checks the sender: see [`Labelled::from_email`].
pub fn expectation_for(provider_id: &str) -> Option<&'static Labelled> {
    let set = quick();
    set.by_provider_id
        .get(provider_id)
        .map(|index| &set.labels[*index])
}

/// One of the two demo accounts the generated demo is split across.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoAccount {
    /// Carries the hand-written demo mail ahead of its generated threads.
    Personal,
    /// Generated threads only.
    Work,
}

/// Which demo account this is, if either.
pub fn demo_account(id: &AccountId) -> Option<DemoAccount> {
    if *id == account_id() {
        Some(DemoAccount::Personal)
    } else if *id == AccountId::from_provider_id("fake", WORK_EMAIL) {
        Some(DemoAccount::Work)
    } else {
        None
    }
}

/// The label for any stored email of a demo account: the curated mail and
/// twins of the quick set, or an email of the generated bulk. The bulk is
/// recomputed from the email's index, so the 50,000 labels are never held in
/// memory.
///
/// Both demo accounts number their mail from 1 and reuse some of the same
/// people, so an id and a sender alone can name another email. A bulk email
/// is claimed only when its thread is the generator's thread for that
/// position, which also rejects a smaller or hand-written demo that numbers
/// its mail differently.
pub fn full_label_for(
    account: &AccountId,
    provider_id: &str,
    thread_id: &ThreadId,
    from_email: &str,
) -> Option<Labelled> {
    let which = demo_account(account)?;
    bulk_label_for(which, account, provider_id, thread_id, from_email).or_else(|| {
        if which != DemoAccount::Personal {
            return None;
        }
        expectation_for(provider_id)
            .filter(|label| label.from_email.eq_ignore_ascii_case(from_email))
            .cloned()
    })
}

fn bulk_label_for(
    which: DemoAccount,
    account: &AccountId,
    provider_id: &str,
    thread_id: &ThreadId,
    from_email: &str,
) -> Option<Labelled> {
    let number: usize = provider_id.strip_prefix("demo-msg-")?.parse().ok()?;
    let head = match which {
        DemoAccount::Personal => curated_head_len(),
        DemoAccount::Work => 0,
    };
    let (thread_num, reply_idx) = bulk::locate(number.checked_sub(1)?.checked_sub(head)?);
    if *thread_id != demo_thread_id(account, thread_num) {
        return None;
    }
    let label = bulk::label(thread_num, reply_idx)?;
    if !label.from_email.eq_ignore_ascii_case(from_email) {
        return None;
    }
    let sender = sender_kind(label.expect.who, from_email);
    Some(Labelled {
        provider_id: provider_id.to_string(),
        from_email: from_email.to_string(),
        scenario: label.turn.scenario,
        description: label.turn.description,
        expect: label.expect,
        items: bulk::items(thread_num, reply_idx),
        outbound: false,
        sender,
        locale: "en-GB",
        family: label.family,
        twin_of: None,
        differs_by: "",
        review: label.turn.review,
        held_out: label.held_out,
    })
}

#[cfg(test)]
mod tests {
    use super::date_finder::written_dates;
    use super::*;

    fn seed() -> ScenarioSeed {
        let account_id = AccountId::from_provider_id("fake", SELF_EMAIL);
        scenario_seed(&account_id, Utc::now(), &DemoCuts::default())
    }

    #[test]
    fn every_label_names_the_email_it_labels() {
        let seed = seed();
        for label in &seed.labels {
            let (envelope, _) = seed
                .messages
                .iter()
                .find(|(envelope, _)| envelope.provider_id == label.provider_id)
                .expect("every label has an email");
            assert!(
                envelope.from.email.eq_ignore_ascii_case(&label.from_email),
                "{} is from {}, labelled as {}",
                label.provider_id,
                envelope.from.email,
                label.from_email
            );
        }
    }

    #[test]
    fn a_label_never_names_the_same_email_twice() {
        let seed = seed();
        let mut ids: Vec<&str> = seed.labels.iter().map(|l| l.provider_id.as_str()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total);
    }

    /// Mail the owner sent is labelled only when it makes a promise, and then
    /// the label says it is outbound, so the eval grades its promises alone.
    #[test]
    fn every_inbound_email_is_labelled_and_sent_mail_only_when_it_promises() {
        let seed = seed();
        let mut promises = 0;
        for (envelope, _) in &seed.messages {
            let label = seed
                .labels
                .iter()
                .find(|label| label.provider_id == envelope.provider_id);
            let sent = envelope.from.email.eq_ignore_ascii_case(SELF_EMAIL);
            match label {
                None => assert!(
                    sent,
                    "{} ({}) from {} is inbound and unlabelled",
                    envelope.provider_id, envelope.subject, envelope.from.email
                ),
                Some(label) => {
                    assert_eq!(label.outbound, sent, "{}", envelope.provider_id);
                    if sent {
                        promises += label.items.len();
                        // Sent mail may promise nothing (you ask someone
                        // for something): that is the near miss of a promise.
                        assert!(
                            label
                                .items
                                .iter()
                                .all(|item| item.direction == Direction::YouPromised),
                            "{} is sent mail labelled with something other than a promise",
                            envelope.provider_id
                        );
                    } else {
                        assert!(
                            label
                                .items
                                .iter()
                                .all(|item| item.direction == Direction::AsksYou),
                            "{} is inbound but labelled with a promise",
                            envelope.provider_id
                        );
                    }
                }
            }
        }
        assert!(promises >= 2, "only {promises} promises are labelled");
    }

    /// Chapter 37's rule: an item asks for something other than a reply, and
    /// a reply is owed only when the email is inbound.
    #[test]
    fn items_are_never_replies_and_only_inbound_mail_owes_one() {
        let seed = seed();
        for label in &seed.labels {
            assert!(
                label.items.iter().all(|item| item.kind != AskKind::Reply),
                "{}: a reply is Your turn, not a to-do",
                label.scenario
            );
            assert!(
                !(label.outbound && label.expect.reply),
                "{}: sent mail owes no reply",
                label.scenario
            );
        }
    }

    /// An item states what it asks with enough to be graded: a counterparty,
    /// and a reference whenever a sender has several items.
    #[test]
    fn every_item_names_its_counterparty() {
        let seed = seed();
        for label in &seed.labels {
            for item in &label.items {
                assert!(
                    item.counterparty
                        .as_deref()
                        .is_some_and(|name| !name.trim().is_empty()),
                    "{} has an item with no counterparty",
                    label.scenario
                );
            }
        }
    }

    #[test]
    fn the_quick_set_is_ordered_and_looked_up_by_provider_id() {
        let ids: Vec<&str> = quick_set().map(|l| l.provider_id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort_by_key(|id| natural_key(id));
        assert_eq!(ids, sorted);
        let first = quick_set().next().expect("a labelled email");
        assert_eq!(expectation_for(&first.provider_id), Some(first));
        assert_eq!(expectation_for("demo-msg-0"), None);
        assert_eq!(expectation_for("not-a-demo-id"), None);
    }

    fn seed_at(now: DateTime<Utc>) -> ScenarioSeed {
        scenario_seed(&account_id(), now, &DemoCuts::default())
    }

    fn at(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, 0, 0)
            .single()
            .expect("a valid date")
    }

    /// Everything about an email a detector can read a date from: its text,
    /// headers, its own date and the calendar event's fields.
    fn dated_content(envelope: &Envelope, body: &MessageBody) -> String {
        let calendar = body.metadata.calendar.as_ref();
        let sent = envelope.date.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        [
            Some(sent.as_str()),
            Some(envelope.subject.as_str()),
            Some(envelope.snippet.as_str()),
            body.text_plain.as_deref(),
            body.text_html.as_deref(),
            body.metadata.raw_headers.as_deref(),
            calendar.and_then(|c| c.starts_at.as_deref()),
            calendar.and_then(|c| c.ends_at.as_deref()),
            calendar.and_then(|c| c.dtstamp.as_deref()),
            calendar.and_then(|c| c.recurrence_id.as_deref()),
            calendar.and_then(|c| c.raw_ics.as_deref()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
    }

    /// Dates the fixtures write out on purpose and that no detector can
    /// expire, matched on the email's subject and the exact date string.
    const KNOWN_FIXED_DATES: [(&str, &str); 3] = [
        // Last year's Airbnb booking (records_demo.rs). A year-less date
        // resolves forward from the email's own date, which is 400 days
        // before the seed, so it always lands in the past.
        ("Booking confirmation - Porto", "14 May"),
        ("Booking confirmation - Porto", "16 May"),
        // An essay title in reading_demo.rs: a name, not a deadline.
        ("New post: Shipping a sync engine", "2026"),
    ];

    /// Dates written in the same email at both clocks. A date derived from
    /// the clock differs between them, so one present in both is absolute:
    /// it will not move with the real date and can expire. Checked per
    /// date, so a fixed event time beside a moving subject is still caught.
    fn absolute_dates(
        base: (&Envelope, &MessageBody),
        moved: (&Envelope, &MessageBody),
    ) -> Vec<String> {
        let (at_moved, subject) = (
            written_dates(&dated_content(moved.0, moved.1)),
            &base.0.subject,
        );
        written_dates(&dated_content(base.0, base.1))
            .into_iter()
            .filter(|date| at_moved.contains(date))
            .filter(|date| {
                !KNOWN_FIXED_DATES.iter().any(|(known_subject, known)| {
                    subject.starts_with(known_subject) && known == date
                })
            })
            .collect()
    }

    /// The post-sync work reads the real clock, so the set cannot be graded
    /// at a faked `now`. This is the check that it never needs to be: build
    /// the set at different clocks (a year away, across a clock change) and
    /// the labels do not move, and no date is written the same way at two
    /// clocks. It is what once let an invite expire in a fixed week.
    #[test]
    fn scenario_dates_follow_the_seed_clock() {
        let now = at(2026, 10, 21, 12);
        let later = now + chrono::Duration::days(400);
        let before_change = at(2026, 10, 24, 12);
        let after_change = at(2026, 10, 26, 12);
        let base = seed_at(now);
        for other in [later, before_change, after_change] {
            assert_eq!(
                seed_at(other).labels,
                base.labels,
                "labels moved at {other}"
            );
        }
        let moved = seed_at(later);
        assert_eq!(moved.messages.len(), base.messages.len());
        let mut dated = 0;
        for (a, b) in base.messages.iter().zip(&moved.messages) {
            assert_eq!(a.0.provider_id, b.0.provider_id);
            assert_eq!(
                absolute_dates((&a.0, &a.1), (&b.0, &b.1)),
                Vec::<String>::new(),
                "{} ({}) writes a date that does not move with the seed clock",
                a.0.provider_id,
                a.0.subject
            );
            dated += usize::from(!written_dates(&dated_content(&a.0, &a.1)).is_empty());
        }
        assert!(
            dated > 20,
            "only {dated} emails carry a date, so the check saw little"
        );
    }

    /// The case that slipped past a per-email comparison: the invite's
    /// subject and body move with the clock but its event start was written
    /// out. The guard must name that date.
    #[test]
    fn a_fixed_event_time_beside_a_moving_subject_is_caught() {
        let now = at(2026, 10, 21, 12);
        let (base, moved) = (seed_at(now), seed_at(now + chrono::Duration::days(400)));
        let invite = |seed: &ScenarioSeed| {
            seed.messages
                .iter()
                .find(|(e, _)| e.subject.starts_with("Invitation: Design review"))
                .cloned()
                .expect("the invite twin")
        };
        let (mut a, mut b) = (invite(&base), invite(&moved));
        assert_ne!(a.0.subject, b.0.subject, "the subject moves with the clock");
        assert_eq!(
            absolute_dates((&a.0, &a.1), (&b.0, &b.1)),
            Vec::<String>::new()
        );
        for (_, body) in [&mut a, &mut b] {
            let calendar = body.metadata.calendar.as_mut().expect("an invite");
            calendar.starts_at = Some("2026-10-29T15:00:00Z".to_string());
        }
        assert_eq!(
            absolute_dates((&a.0, &a.1), (&b.0, &b.1)),
            vec!["2026-10-29".to_string()]
        );
    }

    #[test]
    fn the_date_finder_finds_what_a_detector_would_read() {
        for (text, token) in [
            ("due 2026-10-29", "2026-10-29"),
            ("2026-10-29T15:00:00Z", "2026-10-29"),
            ("© 2027 Acme and since 2027 Acme", "2027"),
            ("renews on 3 November for", "3 November"),
            ("on the 1st of December", "1 December"),
            ("Check-in: 14 November", "14 November"),
            ("from October 14", "October 14"),
        ] {
            assert!(written_dates(text).contains(&token.to_string()), "{text}");
        }
        for plain in [
            "tenant news for October",
            "Your October statement is ready",
            "https://platform.demo.mxr.local/2026/10/shipping-a-sync-engine",
            "Order number: 402-118, tracking 1Z5R89390357567127, +44 20 7946 0102",
            "Total £1,249.00 on card ending 4417",
            "Every reader since 2002 shipped the same window",
            "© 2026 Long Reads Weekly",
        ] {
            assert_eq!(written_dates(plain), Vec::<String>::new(), "{plain}");
        }
    }

    #[test]
    fn a_review_covers_real_questions_and_gives_a_reason() {
        let seed = seed();
        let reviewed: Vec<_> = seed.labels.iter().filter_map(|l| l.review).collect();
        assert!(!reviewed.is_empty());
        for review in reviewed {
            assert!(!review.questions.is_empty() && !review.why.is_empty());
            for question in review.questions {
                assert!(
                    ["who", "home", "todo", "record"].contains(question),
                    "unknown question {question}"
                );
            }
        }
    }

    #[test]
    fn twins_name_a_scenario_that_exists_and_say_what_differs() {
        let seed = seed();
        let ids: Vec<&str> = seed.labels.iter().map(|l| l.scenario).collect();
        let mut twins = 0;
        for label in &seed.labels {
            if let Some(of) = label.twin_of {
                twins += 1;
                assert!(ids.contains(&of), "{} twins unknown {of}", label.scenario);
                assert!(!label.differs_by.is_empty(), "{}", label.scenario);
            } else {
                assert!(label.differs_by.is_empty(), "{}", label.scenario);
            }
        }
        assert!(twins >= 20, "only {twins} near-miss twins");
    }

    /// A pair of copies would measure nothing: the twin's text differs
    /// from the scenario it twins.
    #[test]
    fn a_twin_is_a_different_email_from_the_scenario_it_twins() {
        let seed = seed();
        let text_of = |provider_id: &str| {
            let (envelope, body) = seed
                .messages
                .iter()
                .find(|(e, _)| e.provider_id == provider_id)
                .expect("a labelled email exists");
            (
                envelope.subject.clone(),
                envelope.from.email.clone(),
                body.text_plain.clone(),
            )
        };
        for label in seed.labels.iter().filter(|l| l.twin_of.is_some()) {
            let other = seed
                .labels
                .iter()
                .find(|l| Some(l.scenario) == label.twin_of)
                .expect("the twinned scenario exists");
            assert_ne!(
                text_of(&label.provider_id),
                text_of(&other.provider_id),
                "{} and {} are the same email",
                label.scenario,
                other.scenario
            );
        }
    }

    fn work_account() -> AccountId {
        AccountId::from_provider_id("fake", WORK_EMAIL)
    }

    /// The generated demo and the labels agree on every email of both
    /// accounts: each email you did not send is labelled, and the label names
    /// its sender. A demo the size of a few thousand covers every position of
    /// every family; `bulk` proves the families are all reached.
    #[test]
    fn every_inbound_email_of_the_generated_demo_is_labelled() {
        use crate::fixtures::generate_demo_fixtures;
        for account in [account_id(), work_account()] {
            let (envelopes, _, _) = generate_demo_fixtures(&account, 6_000);
            let (mut labelled, mut sent) = (0, 0);
            for envelope in &envelopes {
                let is_sent = envelope.from.email.eq_ignore_ascii_case(SELF_EMAIL)
                    || envelope.from.email.eq_ignore_ascii_case(WORK_EMAIL);
                let label = full_label_for(
                    &account,
                    &envelope.provider_id,
                    &envelope.thread_id,
                    &envelope.from.email,
                );
                // Mail you sent is labelled only for a promise, and says so.
                assert_eq!(
                    label.as_ref().is_some_and(|label| !label.outbound),
                    !is_sent,
                    "{} ({}) from {}",
                    envelope.provider_id,
                    envelope.subject,
                    envelope.from.email
                );
                assert!(label.as_ref().is_none_or(|label| label.outbound == is_sent));
                labelled += usize::from(label.as_ref().is_some_and(|label| !label.outbound));
                sent += usize::from(is_sent);
            }
            assert!(
                labelled > 2_000 && sent > 300,
                "{labelled} labelled, {sent} sent"
            );
        }
    }

    #[test]
    fn a_label_is_given_only_to_the_email_it_means() {
        use crate::fixtures::generate_demo_fixtures;
        let (envelopes, _, _) = generate_demo_fixtures(&account_id(), 600);
        let bulk = envelopes
            .iter()
            .find(|e| e.provider_id == format!("demo-msg-{}", curated_head_len() + 1))
            .expect("the first generated email");
        let label = full_label_for(
            &account_id(),
            &bulk.provider_id,
            &bulk.thread_id,
            &bulk.from.email,
        )
        .expect("labelled");
        assert!(label.scenario.starts_with("bulk-"));
        // Another sender, another thread, the other account, a stranger.
        assert!(full_label_for(
            &account_id(),
            &bulk.provider_id,
            &bulk.thread_id,
            "x@y.example"
        )
        .is_none());
        let other_thread = envelopes.last().expect("a last email").thread_id.clone();
        assert!(full_label_for(
            &account_id(),
            &bulk.provider_id,
            &other_thread,
            &bulk.from.email
        )
        .is_none());
        assert!(full_label_for(
            &work_account(),
            &bulk.provider_id,
            &bulk.thread_id,
            &bulk.from.email
        )
        .is_none());
        assert!(full_label_for(
            &AccountId::new(),
            &bulk.provider_id,
            &bulk.thread_id,
            &bulk.from.email
        )
        .is_none());
        assert!(full_label_for(
            &account_id(),
            "not-a-demo-id",
            &bulk.thread_id,
            &bulk.from.email
        )
        .is_none());
        assert!(full_label_for(
            &account_id(),
            "demo-msg-0",
            &bulk.thread_id,
            &bulk.from.email
        )
        .is_none());
    }

    /// The hand-written mail at the head of the personal account keeps the
    /// labels the quick set gives it; the work account has none of it.
    #[test]
    fn the_curated_head_keeps_its_quick_set_labels() {
        let seed = seed();
        // The twins are numbered after the head and are not in the demo.
        let in_head = |label: &Labelled| {
            label
                .provider_id
                .strip_prefix("demo-msg-")
                .and_then(|n| n.parse::<usize>().ok())
                .is_some_and(|n| n <= curated_head_len())
        };
        let head: Vec<_> = seed.labels.iter().filter(|l| in_head(l)).collect();
        assert!(head.len() > 90);
        let (envelopes, _, _) = crate::fixtures::generate_demo_fixtures(&account_id(), 600);
        for label in head {
            let envelope = envelopes
                .iter()
                .find(|e| e.provider_id == label.provider_id)
                .expect("the head is in the demo");
            let full = full_label_for(
                &account_id(),
                &envelope.provider_id,
                &envelope.thread_id,
                &envelope.from.email,
            )
            .expect("labelled");
            assert_eq!(full.scenario, label.scenario);
            assert_eq!(full.expect, label.expect);
            assert_eq!(full.items, label.items);
            assert!(!full.held_out);
        }
    }

    /// Labels recomputed from an index agree with the mail however the demo
    /// was cut: a bigger demo only adds threads after the smaller one's.
    #[test]
    fn a_labelled_email_has_the_same_label_in_a_bigger_demo() {
        use crate::fixtures::generate_demo_fixtures;
        let label_of = |count: usize| {
            let (envelopes, _, _) = generate_demo_fixtures(&work_account(), count);
            envelopes
                .iter()
                .filter_map(|e| {
                    full_label_for(&work_account(), &e.provider_id, &e.thread_id, &e.from.email)
                        .map(|l| (l.provider_id, l.scenario, l.expect, l.items, l.held_out))
                })
                .collect::<Vec<_>>()
        };
        let small = label_of(1_000);
        let big = label_of(3_000);
        assert!(!small.is_empty());
        assert_eq!(&big[..small.len() - 1], &small[..small.len() - 1]);
    }

    /// The generated demo's labels, read off the mail it serves, are the same
    /// at every clock, and a live ask is dated to the clock it was built at.
    /// This is the full-set sibling of `scenario_dates_follow_the_seed_clock`:
    /// the post-sync work reads the real clock, so the set can only be graded
    /// if no label depends on the seed's.
    ///
    /// This samples a 6,000-message demo through the stream and `full_label_for`,
    /// so it covers the serving path but not every family, phrasing and position:
    /// `bulk_dates_follow_the_seed_clock` and
    /// `every_ask_thread_is_dated_to_its_label_at_every_clock` in `bulk/tests.rs`
    /// are the exhaustive ones.
    #[test]
    fn bulk_labels_do_not_move_with_the_seed_clock() {
        use crate::fixtures::DemoFixtureStream;
        let base = at(2026, 10, 21, 12);
        let clocks = [
            base,
            base + chrono::Duration::days(400),
            at(2026, 10, 24, 12),
            at(2026, 10, 26, 12),
        ];
        let labels_at = |clock: DateTime<Utc>| {
            let stream =
                DemoFixtureStream::with_clock(&work_account(), 6_000, clock, DemoCuts::default());
            stream
                .page(0, stream.len())
                .into_iter()
                .map(|(envelope, _)| {
                    let label = full_label_for(
                        &work_account(),
                        &envelope.provider_id,
                        &envelope.thread_id,
                        &envelope.from.email,
                    )
                    .map(|l| {
                        (
                            l.scenario,
                            l.expect,
                            l.items,
                            l.held_out,
                            l.review.is_some(),
                        )
                    });
                    // A live ask is the mail that arrived in the last hours.
                    let live_ask = label
                        .as_ref()
                        .is_some_and(|(_, _, items, ..)| !items.is_empty());
                    if live_ask {
                        assert!(
                            clock - envelope.date < chrono::Duration::hours(11),
                            "{} is a live ask dated {} at {clock}",
                            envelope.provider_id,
                            envelope.date
                        );
                    }
                    (envelope.provider_id, envelope.thread_id, label)
                })
                .collect::<Vec<_>>()
        };
        let first = labels_at(clocks[0]);
        assert!(first.iter().any(|(_, _, label)| label.is_some()));
        assert!(
            first.iter().any(|(_, _, label)| label
                .as_ref()
                .is_some_and(|(_, _, items, ..)| !items.is_empty())),
            "the sample holds no live ask, so the check saw little"
        );
        for clock in &clocks[1..] {
            assert_eq!(labels_at(*clock), first, "labels moved at {clock}");
        }
    }

    /// A conversation replies to the message before it; a store's, biller's
    /// or airline's notifications, and a list's, do not.
    #[test]
    fn only_conversations_link_follow_ups_to_the_message_before() {
        use crate::fixtures::generate_demo_fixtures;
        let (envelopes, _, _) = generate_demo_fixtures(&work_account(), 4_000);
        let (mut conversation, mut machine) = (0, 0);
        for envelope in &envelopes {
            let notification = envelope.from.email.ends_with(".demo.mxr.local")
                && !envelope.from.email.eq_ignore_ascii_case(WORK_EMAIL);
            if notification {
                machine += 1;
                assert!(
                    envelope.in_reply_to.is_none() && envelope.references.is_empty(),
                    "{} ({}) from {} reply-threads",
                    envelope.provider_id,
                    envelope.subject,
                    envelope.from.email
                );
            } else if envelope.subject.starts_with("Re: ") {
                conversation += 1;
                assert!(envelope.in_reply_to.is_some(), "{}", envelope.provider_id);
            }
        }
        assert!(
            conversation > 500 && machine > 300,
            "{conversation}, {machine}"
        );
    }
}
