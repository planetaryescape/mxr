//! A later message that looks like a to-do's confirmation: the receipt for
//! a bill, the "you're verified" for a verify link, the completed envelope
//! for a signature.
//!
//! Matching only offers: the row reads "Looks done: payment received 3
//! Oct" and the user ticks it off. A strong match (same sender domain, the
//! same amount and a receipt word) is marked strong so a later slice can
//! complete it with a day of undo; nothing here closes a row.

use crate::TodoKind;
use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LooksDone {
    /// "payment received" plus the day: what the row shows.
    pub reason: String,
    /// Same domain, same amount, and a receipt word.
    pub strong: bool,
}

/// What matching needs from the open row.
#[derive(Debug, Clone, Copy)]
pub struct OpenRow {
    pub kind: TodoKind,
    pub amount_minor: Option<i64>,
    pub source_date: Option<DateTime<Utc>>,
}

/// What matching needs from the later message, which the caller has
/// already checked comes from the row's sender domain.
#[derive(Debug, Clone, Copy)]
pub struct LaterMessage<'a> {
    pub subject: &'a str,
    pub text: &'a str,
    pub date: DateTime<Utc>,
}

static RECEIPT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(payment (received|confirmation|confirmed|successful|complete)|thank(s| you) for (your )?payment|we('ve| have) received your payment|your receipt|receipt for|payment of [^.\n]{0,40}(received|successful)|has been paid|paid in full)")
        .expect("valid receipt regex")
});
static RENEWED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(renewed|thank(s| you) for renewing|renewal (confirmation|confirmed)|your new (policy|cover|documents)|policy documents)")
        .expect("valid renewed regex")
});
static VERIFIED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)\b(verified|email (address )?confirmed|account (is )?(now )?active|welcome to)",
    )
    .expect("valid verified regex")
});
static SIGNED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(completed|signed)\b|has been (signed|completed)|signed by all|all parties have signed")
        .expect("valid signed regex")
});
static CONFIRMED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(booking|reservation) (is )?confirmed").expect("valid confirmed regex")
});

/// Whether `later` looks like the confirmation that `row` is done.
pub fn looks_done(row: &OpenRow, later: &LaterMessage<'_>) -> Option<LooksDone> {
    if row.source_date.is_some_and(|source| later.date <= source) {
        return None;
    }
    let text = format!("{}\n{}", later.subject, later.text);
    let regex: &Regex = match row.kind {
        TodoKind::Bill | TodoKind::PaymentFailed => &RECEIPT,
        TodoKind::Renewal | TodoKind::Document => &RENEWED,
        TodoKind::Verify => {
            // A welcome weeks later is a newsletter, not the confirmation.
            if row
                .source_date
                .is_some_and(|source| later.date - source > Duration::days(1))
            {
                return None;
            }
            &VERIFIED
        }
        TodoKind::Sign => &SIGNED,
        TodoKind::Other => &CONFIRMED,
        TodoKind::Rsvp | TodoKind::Promise | TodoKind::Lease | TodoKind::Return => return None,
    };
    let matched = regex
        .find(later.subject)
        .or_else(|| regex.find(&text))?
        .as_str()
        .to_lowercase();
    let amount_matches = row.amount_minor.is_some_and(|minor| {
        crate::money::find_amounts(&text)
            .iter()
            .any(|(_, amount)| amount.minor == minor)
    });
    Some(LooksDone {
        reason: format!("{matched} {}", later.date.format("%-d %b")),
        strong: amount_matches && matches!(row.kind, TodoKind::Bill | TodoKind::PaymentFailed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, d, 9, 0, 0)
            .single()
            .expect("valid time")
    }

    fn bill() -> OpenRow {
        OpenRow {
            kind: TodoKind::Bill,
            amount_minor: Some(14200),
            source_date: Some(at(1)),
        }
    }

    #[test]
    fn a_receipt_with_the_same_amount_is_a_strong_match() {
        let later = LaterMessage {
            subject: "Payment received",
            text: "Thank you. We received £142.00 for your council tax.",
            date: at(3),
        };
        let done = looks_done(&bill(), &later).expect("looks done");
        assert_eq!(done.reason, "payment received 3 Oct");
        assert!(done.strong);
    }

    #[test]
    fn a_receipt_for_another_amount_is_only_an_offer() {
        let later = LaterMessage {
            subject: "Thank you for your payment",
            text: "We received £20.00.",
            date: at(3),
        };
        let done = looks_done(&bill(), &later).expect("looks done");
        assert!(!done.strong);
    }

    #[test]
    fn earlier_or_unrelated_mail_does_not_match() {
        let earlier = LaterMessage {
            subject: "Payment received",
            text: "",
            date: at(1),
        };
        assert_eq!(looks_done(&bill(), &earlier), None);
        let newsletter = LaterMessage {
            subject: "Your October newsletter",
            text: "News from the council.",
            date: at(5),
        };
        assert_eq!(looks_done(&bill(), &newsletter), None);
    }

    #[test]
    fn verify_confirmation_counts_only_within_a_day() {
        let row = OpenRow {
            kind: TodoKind::Verify,
            amount_minor: None,
            source_date: Some(at(1)),
        };
        let soon = LaterMessage {
            subject: "Welcome to Octopus",
            text: "",
            date: at(1) + Duration::hours(2),
        };
        assert!(looks_done(&row, &soon).is_some());
        let late = LaterMessage {
            subject: "Welcome to Octopus",
            text: "",
            date: at(9),
        };
        assert_eq!(looks_done(&row, &late), None);
    }

    #[test]
    fn completed_envelope_matches_a_sign_request() {
        let row = OpenRow {
            kind: TodoKind::Sign,
            amount_minor: None,
            source_date: Some(at(1)),
        };
        let later = LaterMessage {
            subject: "Completed: Engagement letter",
            text: "",
            date: at(2),
        };
        assert!(looks_done(&row, &later).is_some());
    }
}
