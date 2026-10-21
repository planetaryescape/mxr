//! Labelled scenarios: mail that carries the answer a classifier should
//! give (blueprint 32, "The demo dataset is rebuilt from labelled
//! scenarios"). `mxr classify eval` measures today's code against them.
//!
//! A scenario is a small typed template that produces mail and declares,
//! per email, what the classifier should say. Two kinds exist:
//!
//! * The curated demo fixtures (`*_demo.rs`) keep generating exactly the
//!   mail they always did, so the demo looks the same. [`curated`] states
//!   the intended outcome their prose describes, as data.
//! * Near-miss twins ([`twins`]) are pairs of emails that differ in the one
//!   fact that should change the answer. They are not in the 50,000-email
//!   demo: they would shift its message numbering and the counts its tests
//!   and the web e2e suite read. [`scenario_seed`] returns the curated mail
//!   and the twins together, for the eval's tests.
//!
//! Expectations state what the classifier SHOULD do, per blueprint 32, not
//! what today's code does, so a gap shows up as a mismatch. A label that
//! rests on a judgement call carries its reasoning in a [`Review`].
//!
//! The four questions are the ones a classifier can be graded on today:
//! who sent it, which mode is its home, and whether it made a to-do or a
//! record. [`LABEL_SCHEMA_VERSION`] changes whenever they do, so old
//! expectations are never silently reused.

mod curated;
mod twins;

use super::{curated_seed_messages, DemoCuts};
use chrono::{DateTime, TimeZone, Utc};
use mxr_core::id::AccountId;
use mxr_core::types::{Address, Envelope, MessageBody};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Version of the questions an [`Expect`] answers.
pub const LABEL_SCHEMA_VERSION: u32 = 1;

/// The mailbox owner of the seeded personal demo account.
pub const SELF_EMAIL: &str = "alex@demo.mxr.local";
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

/// The right answers for one email.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expect {
    pub who: Who,
    pub home: Home,
    /// A to-do is made from this email.
    pub todo: bool,
    /// A record is filed from this email.
    pub record: bool,
}

/// Why some answers of a label are a judgement call, so a mismatch on one
/// of them may be the label's fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Review {
    /// Which questions (`who`, `home`, `todo`, `record`) it covers.
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
    /// The scenario this email is a near-miss twin of.
    pub twin_of: Option<&'static str>,
    /// The one fact that differs from the twin; empty when not a twin.
    pub differs_by: &'static str,
    pub review: Option<Review>,
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

#[cfg(test)]
mod tests {
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

    #[test]
    fn every_inbound_email_is_labelled_and_sent_mail_is_not() {
        let seed = seed();
        for (envelope, _) in &seed.messages {
            let labelled = seed
                .labels
                .iter()
                .any(|label| label.provider_id == envelope.provider_id);
            let sent = envelope.from.email.eq_ignore_ascii_case(SELF_EMAIL);
            assert_eq!(
                labelled, !sent,
                "{} ({}) from {}",
                envelope.provider_id, envelope.subject, envelope.from.email
            );
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

    /// The first date written out in `text`, if any: an ISO date, a current-era year, a
    /// day and month ("3 November", "1st of December") or a month and day
    /// ("October 14"). A bare month ("tenant news for October") is not a
    /// date a detector can expire; nor are a copyright year or a year inside
    /// a URL path.
    fn written_date(text: &str) -> Option<String> {
        const MONTHS: [&str; 12] = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        let chars: Vec<char> = text.chars().collect();
        let word = |at: usize, len: usize| -> String {
            chars[at..(at + len).min(chars.len())].iter().collect()
        };
        for at in 0..chars.len() {
            let before = at.checked_sub(1).map(|i| chars[i]);
            if before.is_some_and(|c| c.is_ascii_digit()) {
                continue;
            }
            if chars[at].is_ascii_digit() {
                let digits: String = chars[at..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                let after = chars.get(at + digits.len()).copied();
                if digits.len() == 4
                    && digits
                        .parse::<u32>()
                        .is_ok_and(|year| (2024..=2040).contains(&year))
                    && before != Some('/')
                    && !(before == Some(' ') && at >= 2 && chars[at - 2] == '©')
                {
                    return Some(word(at, 10));
                }
                if digits.len() <= 2 {
                    let mut rest: String = chars[at + digits.len()..].iter().take(14).collect();
                    for suffix in ["st", "nd", "rd", "th"] {
                        if let Some(stripped) = rest.strip_prefix(suffix) {
                            rest = stripped.to_string();
                            break;
                        }
                    }
                    let rest = rest.trim_start_matches(" of").trim_start();
                    if after.is_some() && MONTHS.iter().any(|m| rest.starts_with(m)) {
                        return Some(word(at, 16));
                    }
                }
            }
            if before.is_none_or(|c| !c.is_alphabetic()) {
                for month in MONTHS {
                    if word(at, month.len()) == month {
                        let tail: String = chars[at + month.len()..].iter().take(3).collect();
                        let mut tail = tail.chars();
                        if tail.next() == Some(' ')
                            && tail.next().is_some_and(|c| c.is_ascii_digit())
                        {
                            return Some(word(at, month.len() + 3));
                        }
                    }
                }
            }
        }
        None
    }

    /// Everything about an email a detector can read a date from.
    fn dated_content(envelope: &Envelope, body: &MessageBody) -> String {
        let calendar = body.metadata.calendar.as_ref();
        [
            Some(envelope.subject.as_str()),
            Some(envelope.snippet.as_str()),
            body.text_plain.as_deref(),
            body.text_html.as_deref(),
            calendar.and_then(|c| c.starts_at.as_deref()),
            calendar.and_then(|c| c.ends_at.as_deref()),
            body.metadata.raw_headers.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
    }

    /// The post-sync work reads the real clock, so the set cannot be graded
    /// at a faked `now`. This is the check that it never needs to be: build
    /// the set at different clocks (a year away, across a clock change) and
    /// the labels do not move, and any email whose content is the same at
    /// two clocks writes no date at all. An absolute date would be the same
    /// at both and so would show up here; it is what once let an invite
    /// expire in a fixed week.
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
        let mut undated = 0;
        for ((env_a, body_a), (env_b, body_b)) in base.messages.iter().zip(&moved.messages) {
            assert_eq!(env_a.provider_id, env_b.provider_id);
            let (a, b) = (dated_content(env_a, body_a), dated_content(env_b, body_b));
            if a == b {
                // One fixture writes a day and month: last year's
                // Airbnb booking in records_demo.rs ("Check-in: 14 May"),
                // dated 400 days before the seed. A year-less date resolves
                // forward from the email's own date, which moves with the
                // seed, so it always lands in the past and cannot expire or
                // flip a label. It is demo mail and is left as it is.
                // The other is an essay title in reading_demo.rs ("Shipping a
                // sync engine in 2026"): a name, not a deadline.
                if [
                    "Booking confirmation - Porto",
                    "New post: Shipping a sync engine",
                ]
                .iter()
                .any(|known| env_a.subject.starts_with(known))
                {
                    continue;
                }
                undated += 1;
                assert_eq!(
                    written_date(&a),
                    None,
                    "{} ({}) is the same 400 days on but writes a date",
                    env_a.provider_id,
                    env_a.subject
                );
            }
        }
        assert!(
            undated > 0,
            "the check saw no undated email, so it checked nothing"
        );
    }

    #[test]
    fn the_date_finder_finds_what_a_detector_would_read() {
        for dated in [
            "due 2026-10-29",
            "2026-10-29T15:00:00Z",
            "since 2027 Acme",
            "renews on 3 November for",
            "on the 1st of December",
            "Check-in: 14 November",
            "from October 14",
        ] {
            assert!(written_date(dated).is_some(), "{dated}");
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
            assert_eq!(written_date(plain), None, "{plain}");
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
}
