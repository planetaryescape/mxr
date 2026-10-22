//! The outcomes the curated demo fixtures describe in prose, as data.
//!
//! Each spec claims the demo emails sent from `from` whose subject
//! matches, and states what the classifier should say about them. The mail
//! itself is untouched. `label` skips mail the account sent (it is not
//! classified); a test in `mod.rs` fails if any inbound email is claimed by
//! no spec.

use super::{
    locale_of, sender_kind, AskKind, Due, Expect, Home, Item, Labelled, Review, SenderKind, Who,
    SELF_EMAIL,
};
use mxr_core::types::{Envelope, MessageBody};

enum Subject {
    Any,
    Exact(&'static str),
    Prefix(&'static str),
}

impl Subject {
    fn matches(&self, subject: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(wanted) => subject == *wanted,
            Self::Prefix(prefix) => subject.starts_with(prefix),
        }
    }
}

struct Spec {
    id: &'static str,
    description: &'static str,
    from: &'static str,
    subject: Subject,
    expect: Expect,
    /// What the email asks of you (or, for sent mail, what you promised).
    items: Vec<Item>,
    /// The email is mail you sent, labelled for its promise alone.
    outbound: bool,
    locale: Option<&'static str>,
    review: Option<Review>,
}

const fn expect(who: Who, home: Home, record: bool) -> Expect {
    Expect::new(who, home, record)
}

/// A machine's notice: Updates, nothing to do, nothing to keep.
const NOTICE: Expect = expect(Who::Machine, Home::Updates, false);
/// A machine's receipt, order, booking or bill: Updates, kept as a record.
const FILED: Expect = expect(Who::Machine, Home::Updates, true);
/// A machine's notice that asks you to act. Its items say what it asks.
const ACTION: Expect = expect(Who::Machine, Home::Updates, false);
/// A newsletter: Reading.
const READING: Expect = expect(Who::List, Home::Reading, false);
/// A person's conversation with you.
const CONVERSATION: Expect = expect(Who::Person, Home::Messages, false);
/// A person's message that asks a question or for an answer: your turn.
const QUESTION: Expect = CONVERSATION.replying();
/// A person's thread you were only copied on: Updates (the copied rule).
const COPIED: Expect = expect(Who::Person, Home::Updates, false);

fn spec(
    id: &'static str,
    description: &'static str,
    from: &'static str,
    subject: Subject,
    expect: Expect,
) -> Spec {
    Spec {
        id,
        description,
        from,
        subject,
        expect,
        items: Vec::new(),
        outbound: false,
        locale: None,
        review: None,
    }
}

/// The email asks this of you.
fn asking(spec: Spec, items: Vec<Item>) -> Spec {
    Spec { items, ..spec }
}

/// The email is in a locale its domain does not say.
fn in_locale(spec: Spec, locale: &'static str) -> Spec {
    Spec {
        locale: Some(locale),
        ..spec
    }
}

/// The email is mail you sent, labelled for the promise it makes.
fn outbound(spec: Spec, items: Vec<Item>) -> Spec {
    Spec {
        items,
        outbound: true,
        ..spec
    }
}

fn judged(spec: Spec, questions: &'static [&'static str], why: &'static str) -> Spec {
    Spec {
        review: Some(Review { questions, why }),
        ..spec
    }
}

fn specs() -> Vec<Spec> {
    use Subject::{Any, Exact, Prefix};
    vec![
        // --- Deliveries (fixtures.rs: delivery_demo_messages) ---
        spec(
            "delivery-order-confirmation",
            "An Amazon order confirmation: a notification from a machine that files the order",
            "shipment-tracking@amazon.com",
            Prefix("Your Amazon.com order #"),
            FILED,
        ),
        spec(
            "delivery-order-shipped",
            "The same order shipping: an update that joins the order's record",
            "shipment-tracking@amazon.com",
            Prefix("Shipped:"),
            FILED,
        ),
        spec(
            "delivery-order-delivered",
            "The same order delivered: an update that joins the order's record",
            "shipment-tracking@amazon.com",
            Prefix("Delivered:"),
            FILED,
        ),
        spec(
            "delivery-in-transit",
            "A UPS package on its way, no order number: a notification, nothing to keep",
            "mcinfo@ups.com",
            Prefix("UPS Update:"),
            NOTICE,
        ),
        spec(
            "delivery-out-for-delivery",
            "A USPS package out for delivery: a notification, nothing to keep",
            "auto-reply@usps.com",
            Prefix("Your USPS package"),
            NOTICE,
        ),
        // --- To do (todo_demo.rs) ---
        judged(
            spec(
                "todo-council-statement",
                "A council tax statement that says there is nothing to do yet",
                "council.tax@camden.gov.uk",
                Exact("Your council tax statement"),
                expect(Who::Machine, Home::Updates, true),
            ),
            &["record"],
            "Whether a statement with no amount or attachment is filed as a record is a policy call; labelled as a billing document worth keeping.",
        ),
        asking(
            spec(
                "todo-bill-with-due-date",
                "Council tax bill due in five days, with an invoice markup and a pay link: a to-do and a record",
                "council.tax@camden.gov.uk",
                Exact("Your council tax bill"),
                expect(Who::Machine, Home::Updates, true),
            ),
            vec![Item::asks(AskKind::Pay, "Camden Council")
                .amount(14_200, "GBP")
                .due(Due::Stated)
                .reference("4471 0092 18")],
        ),
        asking(
            spec(
                "todo-failed-payment",
                "Spotify could not charge the card: a to-do at once",
                "no-reply@spotify.com",
                Exact("We can't process your payment"),
                ACTION,
            ),
            vec![Item::asks(AskKind::FixPayment, "Spotify").amount(1_199, "GBP")],
        ),
        judged(
            spec(
                "todo-event-already-over",
                "A colleague's leaving drinks that ended yesterday, RSVP date also past",
                "sam@work.com",
                Exact("Sam's leaving drinks"),
                CONVERSATION,
            ),
            &["todo", "home"],
            "todo_demo.rs says the event is over so it is never shown; labelled no to-do. Home is Messages because a colleague wrote to you, though it is a broadcast.",
        ),
        judged(
            spec(
                "todo-statement-ready",
                "A bank statement is ready to view in the app: an update",
                "statements@monzo.com",
                Exact("Your October statement is ready"),
                NOTICE,
            ),
            &["record"],
            "No document is attached, only a notice that a statement exists; labelled no record. Same call as todo-council-statement the other way round.",
        ),
        spec(
            "todo-collected-automatically",
            "A scheduled Klarna payment that is collected automatically: nothing to do",
            "noreply@klarna.com",
            Exact("Your scheduled payment"),
            NOTICE,
        ),
        judged(
            asking(
                spec(
                    "todo-overdue-bill-reminder",
                    "A water bill reminder past its due date, with a pay link",
                    "billing@thameswater.co.uk",
                    Exact("Reminder: your water bill"),
                    ACTION,
                ),
                vec![Item::asks(AskKind::Pay, "Thames Water")
                    .amount(4_820, "GBP")
                    .due(Due::Stated)],
            ),
            &["record"],
            "A reminder about a bill is not the bill; labelled no record. The to-do waits in the catch-up rather than in Now, but it exists.",
        ),
        asking(
            spec(
                "todo-document-to-sign",
                "A DocuSign request to sign a tenancy renewal",
                "dse@docusign.net",
                Exact("Please DocuSign: Tenancy renewal"),
                ACTION,
            ),
            // "Foxtons via DocuSign": the requester is Foxtons.
            vec![Item::asks(AskKind::Sign, "Foxtons")],
        ),
        // The promise in the owner's sent mail (todo_demo.rs).
        outbound(
            spec(
                "todo-promise-to-send-a-form",
                "You tell Priya you will send the signed engagement form",
                SELF_EMAIL,
                Exact("Re: Engagement form"),
                NOTICE,
            ),
            vec![Item::promised(AskKind::Send, "Priya Shah")],
        ),
        // --- Modes (modes_demo.rs) ---
        asking(
            spec(
                "modes-landlord-reply",
                "The landlord answers a question you asked, sends the lease to sign by a date, and asks if you are free Thursday",
                "sam@okafor-lettings.example",
                Exact("Re: Lease renewal"),
                QUESTION,
            ),
            // The Thursday question is Your turn, not a second item.
            vec![Item::asks(AskKind::Sign, "Sam Okafor").due(Due::Stated)],
        ),
        judged(
            spec(
                "modes-first-time-sender",
                "Someone writes for the first time and asks for twenty minutes",
                "noor@tidewater.example",
                Exact("Introduction from Tidewater"),
                QUESTION,
            ),
            &["todo"],
            "\"Would you have twenty minutes next week?\" asks whether you are free, which is an answer to give, so it is Your turn and no item; if it is read as a request to meet it would be a `book` item.",
        ),
        judged(
            spec(
                "modes-landlord-newsletter",
                "The landlord's lettings newsletter, with List-Unsubscribe, from someone you have written to",
                "sam@okafor-lettings.example",
                Exact("Okafor Lettings: tenant news for October"),
                READING,
            ),
            &["who", "home"],
            "modes_demo.rs says this reaches Messages by D119's never-bury rule, but the amended D119 (decision log 874-890) says per-message List-Unsubscribe evidence survives correspondence history. Labelled with the amendment.",
        ),
        judged(
            spec(
                "modes-copied-only",
                "The landlord copies you on a note to the boiler engineer",
                "sam@okafor-lettings.example",
                Exact("Boiler service at the flat"),
                COPIED,
            ),
            &["home"],
            "The rules disagree and Now asks 'Not sure' (D119 U1); the home is still Updates under the copied rule. The not-sure mark is not one of the four questions.",
        ),
        // --- Messages (messages_demo.rs) ---
        judged(
            spec(
                "messages-long-letter",
                "A partner's long reply to your renewal draft, quoting you, with a request to reply",
                "samir@launchpad.example",
                Exact("Re: Contract renewal"),
                QUESTION,
            ),
            &["todo"],
            "The closing request is to review the redline and reply with the next step: the action is a reply, so it is Your turn and no item. \"Legal wants a signed copy before the 15th\" is the partner's schedule, not a request of you.",
        ),
        spec(
            "messages-group-thread-reply",
            "A partner's reply in a group thread of three",
            "samir@launchpad.example",
            Exact("Re: Pricing copy"),
            CONVERSATION,
        ),
        judged(
            asking(
                spec(
                    "messages-group-thread-ask",
                    "A colleague's reply in the same group thread, asking you to make a change",
                    "ruth@keystone.example",
                    Exact("Re: Pricing copy"),
                    CONVERSATION,
                ),
                vec![Item::asks(AskKind::Other, "Ruth Vega")],
            ),
            &["todo"],
            "\"Alex, can you make the change?\" names you and asks for an action other than a reply, so blueprint 37 makes it an item. The earlier label said no to-do because it has no date.",
        ),
        spec(
            "messages-one-line-question",
            "A one-line question from someone you wrote to",
            "jon@papertrail.example",
            Exact("Re: Pricing copy for the docs"),
            QUESTION,
        ),
        judged(
            spec(
                "messages-answered-note",
                "A colleague's note you already answered with 'Thanks, on it.'",
                "iris@meridian.example",
                Exact("Incident note"),
                CONVERSATION,
            ),
            &["todo"],
            "\"Ready for a read when you have a minute\" is an invitation with no deadline, and you answered it within two hours, so it is labelled as asking nothing now.",
        ),
        spec(
            "messages-copied-only",
            "A colleague writes to someone else and copies you, naming you only as a courtesy",
            "iris@meridian.example",
            Exact("Offsite dates"),
            COPIED,
        ),
        // --- Records (records_demo.rs) ---
        spec(
            "records-order-confirmation",
            "A Dell order confirmation with schema.org Order markup and an invoice PDF",
            "orders@dell.co.uk",
            Exact("Your Dell order confirmation"),
            FILED,
        ),
        spec(
            "records-order-shipped",
            "The same Dell order shipping, naming the order number: joins the record",
            "orders@dell.co.uk",
            Prefix("Your order 402-118"),
            FILED,
        ),
        spec(
            "records-carrier-delivered",
            "The carrier's delivery email, naming the same order number: joins the record",
            "notifications@dpd.co.uk",
            Exact("Delivered: your Dell parcel"),
            FILED,
        ),
        spec(
            "records-flight-booking",
            "A flight booking with reservation markup",
            "booking@flytap.com",
            Exact("Your booking is confirmed"),
            FILED,
        ),
        spec(
            "records-hotel-reservation",
            "A hotel reservation with reservation markup",
            "reservations@lisboaplaza.pt",
            Prefix("Your reservation at"),
            FILED,
        ),
        spec(
            "records-event-tickets",
            "Tickets with event reservation markup",
            "tickets@oceanario.pt",
            Prefix("Your tickets for"),
            FILED,
        ),
        spec(
            "records-utility-bill",
            "A monthly energy bill with the amount in the text, paid by direct debit: nothing to do",
            "hello@octopus.energy",
            Exact("Your bill is ready"),
            FILED,
        ),
        spec(
            "records-receipt",
            "An iCloud+ receipt filed by rule",
            "no_reply@email.apple.com",
            Prefix("Your receipt from Apple"),
            FILED,
        ),
        spec(
            "records-old-booking",
            "Last year's Airbnb booking, so the ledger has a year to step back to",
            "automated@airbnb.com",
            Prefix("Booking confirmation"),
            FILED,
        ),
        spec(
            "records-order-with-warranty",
            "A headphone order whose warranty runs out in three weeks",
            "orders@bose.co.uk",
            Exact("Your Bose order confirmation"),
            FILED,
        ),
        // --- Reading (reading_demo.rs): every issue of a newsletter shares a label ---
        spec(
            "reading-weekly-essay",
            "An essay newsletter with List-Unsubscribe",
            "essays@longreads.demo.mxr.local",
            Any,
            READING,
        ),
        spec(
            "reading-link-digest",
            "A weekly digest of links with List-Unsubscribe",
            "links@links.demo.mxr.local",
            Any,
            READING,
        ),
        spec(
            "reading-teaser",
            "A newsletter teaser pointing at one article",
            "weekly@platform.demo.mxr.local",
            Any,
            READING,
        ),
        spec(
            "reading-unopened-digest",
            "A growth newsletter nobody opens",
            "digest@growth.demo.mxr.local",
            Any,
            READING,
        ),
        // --- Updates (updates_demo.rs) ---
        spec(
            "updates-weekly-summary",
            "A fitness app's weekly summary",
            "no-reply@strava.com",
            Any,
            NOTICE,
        ),
        spec(
            "updates-ci-run",
            "A CI run succeeding or failing",
            "notifications@github.com",
            Any,
            NOTICE,
        ),
        spec(
            "updates-deploy",
            "A routine deployment notice",
            "notifications@vercel.com",
            Any,
            NOTICE,
        ),
        judged(
            spec(
                "updates-new-sign-in",
                "A new Google sign-in alert that says no action is needed if it was you",
                "no-reply@accounts.google.com",
                Prefix("Security alert:"),
                NOTICE,
            ),
            &["todo"],
            "updates_demo.rs suggests a to-do ('t' makes it one). The mail itself says no action is needed if it was you, so it is labelled no to-do; Now's severity (not asked here) is where alerts rise.",
        ),
        judged(
            in_locale(
                asking(
                    spec(
                        "updates-payout-failed",
                        "A failed payout: the bank declined it and the details need updating",
                        "notifications@stripe.com",
                        Prefix("Payout of"),
                        ACTION,
                    ),
                    vec![Item::asks(AskKind::FixPayment, "Stripe").amount(421_000, "ZAR")],
                ),
                "en-ZA",
            ),
            &["todo"],
            "updates_demo.rs says suggested to-do; BK wants detected to-dos created automatically (memory: auto to-do creation) and a failed payment is a to-do under D119, so labelled a to-do.",
        ),
        spec(
            "updates-verification-code",
            "A verification code that expires in ten minutes",
            "verify@acme-login.example",
            Any,
            NOTICE,
        ),
        spec(
            "updates-status-incident",
            "A status page incident opening or resolving",
            "notifications@linearstatus.example",
            Any,
            NOTICE,
        ),
        spec(
            "updates-analytics-report",
            "A weekly analytics report",
            "notifications@plausible.io",
            Any,
            NOTICE,
        ),
        spec(
            "updates-project-activity",
            "Issues moved to Done in a project tracker",
            "notifications@linear.app",
            Any,
            NOTICE,
        ),
        // --- Subscriptions (subscriptions_demo.rs) ---
        spec(
            "subscriptions-spotify-receipt",
            "A monthly Spotify receipt (and a one-off gift card receipt)",
            "no-reply@spotify.com",
            Prefix("Your Spotify"),
            FILED,
        ),
        spec(
            "subscriptions-netflix-receipt",
            "A monthly Netflix receipt",
            "info@account.netflix.com",
            Any,
            FILED,
        ),
        spec(
            "subscriptions-headspace-receipt",
            "A monthly Headspace receipt",
            "no-reply@headspace.com",
            Any,
            FILED,
        ),
        spec(
            "subscriptions-password-manager-receipt",
            "A yearly 1Password receipt in dollars",
            "billing@1password.com",
            Any,
            FILED,
        ),
        spec(
            "subscriptions-disney-receipt",
            "A monthly Disney+ receipt",
            "disneyplus@mail.disneyplus.com",
            Exact("Your Disney+ receipt"),
            FILED,
        ),
        spec(
            "subscriptions-disney-cancellation",
            "A Disney+ cancellation email: the subscription ended",
            "disneyplus@mail.disneyplus.com",
            Prefix("Your Disney+ subscription"),
            NOTICE,
        ),
    ]
}

/// The labels for the curated mail. Mail the account sent is context and is
/// labelled only when a spec claims it for the promise it makes.
pub(super) fn label(messages: &[(Envelope, MessageBody)]) -> Vec<Labelled> {
    let specs = specs();
    messages
        .iter()
        .filter_map(|(envelope, _)| {
            let spec = specs.iter().find(|spec| {
                spec.from.eq_ignore_ascii_case(&envelope.from.email)
                    && spec.subject.matches(&envelope.subject)
                    && (spec.outbound || !spec.from.eq_ignore_ascii_case(SELF_EMAIL))
            })?;
            Some(Labelled {
                provider_id: envelope.provider_id.clone(),
                from_email: envelope.from.email.clone(),
                scenario: spec.id,
                description: spec.description,
                expect: spec.expect,
                items: spec.items.clone(),
                outbound: spec.outbound,
                sender: if spec.outbound {
                    // The owner writes as a person.
                    SenderKind::Person
                } else {
                    sender_kind(spec.expect.who, &envelope.from.email)
                },
                locale: spec
                    .locale
                    .unwrap_or_else(|| locale_of(&envelope.from.email)),
                family: spec.id,
                twin_of: None,
                differs_by: "",
                review: spec.review,
                held_out: false,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{curated_seed_messages, DemoCuts};
    use mxr_core::id::AccountId;

    fn curated() -> Vec<(Envelope, MessageBody)> {
        curated_seed_messages(
            &AccountId::from_provider_id("fake", SELF_EMAIL),
            &super::super::self_address(),
            chrono::Utc::now(),
            &DemoCuts::default(),
        )
    }

    #[test]
    fn no_two_specs_claim_the_same_email() {
        let specs = specs();
        for (envelope, _) in &curated() {
            let claims = specs
                .iter()
                .filter(|spec| {
                    spec.from.eq_ignore_ascii_case(&envelope.from.email)
                        && spec.subject.matches(&envelope.subject)
                })
                .count();
            assert!(claims <= 1, "{} claimed {claims} times", envelope.subject);
        }
    }

    #[test]
    fn every_spec_labels_at_least_one_email() {
        let labels = label(&curated());
        for spec in specs() {
            assert!(
                labels.iter().any(|l| l.scenario == spec.id),
                "{} matches no demo email",
                spec.id
            );
        }
    }
}
