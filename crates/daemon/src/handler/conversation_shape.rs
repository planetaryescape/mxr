//! A thread's shape for Messages (blueprint 22, phase 3; messages.md §4).
//!
//! - One-to-one: exactly one other person took part. It is a topic inside
//!   that person's row.
//! - Group: two or more other people took part (wrote in it, or you wrote
//!   to them). It is its own row, keyed by the thread, never by who is on
//!   it, because CC lists change on almost every reply.
//! - Copied: you never wrote in it, and you were only copied or it went to
//!   a crowd. It belongs in Updates, not Messages.
//!
//! Pure, so every rule is tested directly; the lanes, Messages and mode
//! membership all call it, so they never disagree about a thread.

use super::desk_lanes::is_outbound;
use mxr_store::DeskMessage;

/// The thresholds, from `[messages]` in config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShapeConfig {
    /// A thread with more recipients than this on one message, that you
    /// never wrote in, is copied.
    pub large_thread_recipients: usize,
}

impl Default for ShapeConfig {
    fn default() -> Self {
        Self {
            large_thread_recipients: 10,
        }
    }
}

impl ShapeConfig {
    pub(crate) fn from_config(config: &mxr_config::MessagesConfig) -> Self {
        Self {
            large_thread_recipients: config.large_thread_recipients,
        }
    }
}

/// The thresholds as configured now.
pub(crate) fn shape_config(state: &crate::state::AppState) -> ShapeConfig {
    ShapeConfig::from_config(&state.config_snapshot().messages)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shape {
    /// The other person's address, lowercased.
    OneToOne(String),
    /// The other people, lowercased, in the order they joined.
    Group(Vec<String>),
    Copied,
    /// No person took part: automated or list mail.
    NotConversation,
}

impl Shape {
    /// In Messages: a one-to-one or group conversation.
    pub(crate) fn in_messages(&self) -> bool {
        matches!(self, Self::OneToOne(_) | Self::Group(_))
    }

    /// The other people, lowercased.
    pub(crate) fn people(&self) -> &[String] {
        match self {
            Self::OneToOne(person) => std::slice::from_ref(person),
            Self::Group(people) => people,
            Self::Copied | Self::NotConversation => &[],
        }
    }
}

/// An address you wrote to that could be a person: not automated, not a
/// list, not screened out. `contacts` and `screener` are keyed by
/// lowercased address.
pub(crate) fn human_address(
    email: &str,
    contacts: &std::collections::HashMap<String, mxr_store::DeskContact>,
    screener: &std::collections::HashMap<String, mxr_store::ScreenerDisposition>,
) -> bool {
    let key = email.to_ascii_lowercase();
    !super::mail_kind::looks_automated(email)
        && !contacts.get(&key).is_some_and(|c| c.is_list_sender)
        && screener.get(&key) != Some(&mxr_store::ScreenerDisposition::Deny)
}

/// What the shape rule needs to know about addresses and senders.
pub(crate) struct ShapeInputs<'a> {
    pub is_self: &'a dyn Fn(&str) -> bool,
    /// The message's sender is a person (the shared classifier).
    pub person_sender: &'a dyn Fn(&DeskMessage) -> bool,
    /// An address you wrote to could be a person: not automated, not a
    /// list, not screened out.
    pub human_address: &'a dyn Fn(&str) -> bool,
    pub config: ShapeConfig,
}

pub(crate) fn conversation_shape(thread: &[DeskMessage], inputs: &ShapeInputs<'_>) -> Shape {
    let is_self = inputs.is_self;
    let mut people: Vec<String> = Vec::new();
    let mut add = |email: &str| {
        let email = email.to_ascii_lowercase();
        if !people.contains(&email) {
            people.push(email);
        }
    };
    let mut wrote_in = false;
    let mut addressed = false;
    let mut copied = false;
    let mut most_recipients = 0usize;
    for message in thread.iter().filter(|m| !m.trashed) {
        most_recipients = most_recipients.max(message.to.len() + message.cc.len());
        if is_outbound(message, is_self) {
            wrote_in = true;
            for recipient in message.to.iter().chain(&message.cc).chain(&message.bcc) {
                if !is_self(&recipient.email) && (inputs.human_address)(&recipient.email) {
                    add(&recipient.email);
                }
            }
        } else if (inputs.person_sender)(message) && !is_self(&message.from.email) {
            add(&message.from.email);
            let in_to = message.to.iter().any(|a| is_self(&a.email));
            let in_cc = message.cc.iter().any(|a| is_self(&a.email));
            // Copied needs positive evidence: you in CC and not in To. In
            // neither (Bcc, or an alias mxr doesn't know) counts as
            // addressed, so mail to an unknown alias never vanishes.
            if in_cc && !in_to {
                copied = true;
            } else {
                addressed = true;
            }
        }
    }
    if people.is_empty() {
        return Shape::NotConversation;
    }
    if !wrote_in
        && ((copied && !addressed) || most_recipients > inputs.config.large_thread_recipients)
    {
        return Shape::Copied;
    }
    match people.len() {
        1 => Shape::OneToOne(people.remove(0)),
        _ => Shape::Group(people),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use mxr_core::id::{MessageId, ThreadId};
    use mxr_core::types::{Address, MessageFlags, UnsubscribeMethod};

    const ME: &str = "me@example.com";

    fn addr(email: &str) -> Address {
        Address {
            name: None,
            email: email.into(),
        }
    }

    fn message(thread: &ThreadId, from: &str, to: &[&str], cc: &[&str], seq: i64) -> DeskMessage {
        let outbound = from == ME;
        DeskMessage {
            id: MessageId::new(),
            seq,
            thread_id: thread.clone(),
            direction: if outbound { "outbound" } else { "inbound" }.into(),
            date: Utc::now() - Duration::hours(10 - seq),
            flags: MessageFlags::empty(),
            from: addr(from),
            to: to.iter().map(|e| addr(e)).collect(),
            cc: cc.iter().map(|e| addr(e)).collect(),
            bcc: vec![],
            subject: "Contract renewal".into(),
            list_id: None,
            unsubscribe: UnsubscribeMethod::None,
            in_inbox: true,
            trashed: false,
            snoozed: false,
            is_invite: false,
            is_delivery: false,
        }
    }

    fn shape(thread: &[DeskMessage]) -> Shape {
        let is_self = |email: &str| email.eq_ignore_ascii_case(ME);
        let person_sender = |m: &DeskMessage| !m.from.email.contains("noreply");
        let human = |email: &str| !email.contains("noreply");
        conversation_shape(
            thread,
            &ShapeInputs {
                is_self: &is_self,
                person_sender: &person_sender,
                human_address: &human,
                config: ShapeConfig::default(),
            },
        )
    }

    #[test]
    fn one_person_who_wrote_to_you_is_one_to_one() {
        let t = ThreadId::new();
        let thread = [
            message(&t, "samir@launchpad.example", &[ME], &[], 1),
            message(&t, ME, &["samir@launchpad.example"], &[], 2),
        ];
        assert_eq!(
            shape(&thread),
            Shape::OneToOne("samir@launchpad.example".into())
        );
    }

    #[test]
    fn someone_copied_who_never_took_part_keeps_it_one_to_one() {
        let t = ThreadId::new();
        let thread = [message(
            &t,
            "samir@launchpad.example",
            &[ME],
            &["ruth@keystone.example"],
            1,
        )];
        assert_eq!(
            shape(&thread),
            Shape::OneToOne("samir@launchpad.example".into())
        );
    }

    #[test]
    fn two_people_taking_part_make_a_group_and_cc_churn_keeps_it_one() {
        let t = ThreadId::new();
        let thread = [
            message(
                &t,
                "samir@launchpad.example",
                &[ME],
                &["ruth@keystone.example"],
                1,
            ),
            message(
                &t,
                "ruth@keystone.example",
                &["samir@launchpad.example", ME],
                &[],
                2,
            ),
            // Someone added, someone dropped: still the same group thread.
            message(
                &t,
                ME,
                &["ruth@keystone.example"],
                &["jon@papertrail.example"],
                3,
            ),
            message(&t, "jon@papertrail.example", &[ME], &[], 4),
        ];
        let Shape::Group(people) = shape(&thread) else {
            panic!("expected a group");
        };
        assert_eq!(
            people,
            vec![
                "samir@launchpad.example".to_string(),
                "ruth@keystone.example".to_string(),
                "jon@papertrail.example".to_string(),
            ]
        );
    }

    #[test]
    fn only_copied_and_never_wrote_in_it_is_copied() {
        let t = ThreadId::new();
        let thread = [message(
            &t,
            "iris@meridian.example",
            &["ruth@keystone.example"],
            &[ME],
            1,
        )];
        assert_eq!(shape(&thread), Shape::Copied);
    }

    #[test]
    fn writing_in_a_copied_thread_makes_it_a_conversation() {
        let t = ThreadId::new();
        let thread = [
            message(
                &t,
                "iris@meridian.example",
                &["ruth@keystone.example"],
                &[ME],
                1,
            ),
            message(&t, ME, &["iris@meridian.example"], &[], 2),
        ];
        assert_eq!(
            shape(&thread),
            Shape::OneToOne("iris@meridian.example".into())
        );
    }

    #[test]
    fn a_crowd_you_never_wrote_to_is_copied() {
        let t = ThreadId::new();
        let crowd: Vec<String> = (0..12).map(|i| format!("p{i}@team.example")).collect();
        let mut to: Vec<&str> = crowd.iter().map(String::as_str).collect();
        to.push(ME);
        let thread = [message(&t, "lead@team.example", &to, &[], 1)];
        assert_eq!(shape(&thread), Shape::Copied);
    }

    #[test]
    fn neither_to_nor_cc_counts_as_addressed() {
        // Bcc, or an alias mxr doesn't know: never silently copied.
        let t = ThreadId::new();
        let thread = [message(
            &t,
            "noor@tidewater.example",
            &["alias@example.net"],
            &[],
            1,
        )];
        assert_eq!(
            shape(&thread),
            Shape::OneToOne("noor@tidewater.example".into())
        );
    }

    #[test]
    fn automated_mail_is_not_a_conversation() {
        let t = ThreadId::new();
        let thread = [message(&t, "noreply@builds.example", &[ME], &[], 1)];
        assert_eq!(shape(&thread), Shape::NotConversation);
    }
}
