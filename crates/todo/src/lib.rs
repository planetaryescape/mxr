//! To do: email shown as the thing you have to do, with a date.
//!
//! Rules only, no model: a message becomes a to-do when deterministic
//! detectors find an action with an object (pay a bill, fix a failed
//! payment, renew, verify, sign, RSVP), schema.org says so (`Invoice`,
//! `Order` with payment due, a pending `Reservation`), or it is a promise
//! you made (`contact_commitments`).
//!
//! * [`detect`] reads one message and returns what to do, from whom, how
//!   much, by when, the one link to do it with, and where each field came
//!   from.
//! * [`timing`] holds the lead-time table (when to act and when the row
//!   shows up) and the relevancy windows (when it stops mattering).
//! * [`action_link`] picks the link and checks the one-click gate: DMARC
//!   pass, the link's registrable domain matching the sender's, and
//!   earlier mail from that domain.
//! * [`complete`] spots a later message that looks like the confirmation.
//!   It only offers "looks done"; nothing closes a to-do on a guess.
//! * [`pass`] runs it over mail: the post-sync scan, the newest-first first
//!   run with its windows and one bounded catch-up, the promise mirror and
//!   the sweep that expires and claims rows.
//!
//! The crate is provider-agnostic and does no network I/O.

#![cfg_attr(
    test,
    expect(clippy::panic, reason = "unit tests panic on an unexpected variant")
)]

pub mod action_link;
pub mod complete;
pub mod dates;
pub mod detect;
pub mod money;
pub mod pass;
pub mod provenance;
pub mod schema_org;
pub mod timing;

use serde::{Deserialize, Serialize};

/// Bumped when detection changes enough that old mail should be classified
/// again. A new version starts a new first run, which never clears a claim,
/// an expiry or a user decision.
pub const RULES_VERSION: i64 = 1;

/// The kind of action, which picks the lead time and the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoKind {
    Bill,
    PaymentFailed,
    Renewal,
    /// Passport, visa, driving licence or ID card expiry.
    Document,
    Lease,
    Return,
    Rsvp,
    Verify,
    Sign,
    Promise,
    Other,
}

impl TodoKind {
    pub const ALL: [Self; 11] = [
        Self::Bill,
        Self::PaymentFailed,
        Self::Renewal,
        Self::Document,
        Self::Lease,
        Self::Return,
        Self::Rsvp,
        Self::Verify,
        Self::Sign,
        Self::Promise,
        Self::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bill => "bill",
            Self::PaymentFailed => "payment_failed",
            Self::Renewal => "renewal",
            Self::Document => "document",
            Self::Lease => "lease",
            Self::Return => "return",
            Self::Rsvp => "rsvp",
            Self::Verify => "verify",
            Self::Sign => "sign",
            Self::Promise => "promise",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// The verb a row of this kind starts with when nothing more specific
    /// was found.
    pub fn default_verb(self) -> &'static str {
        match self {
            Self::Bill => "pay",
            Self::PaymentFailed => "fix",
            Self::Renewal | Self::Document => "renew",
            Self::Lease => "give notice",
            Self::Return => "return",
            Self::Rsvp => "rsvp",
            Self::Verify => "verify",
            Self::Sign => "sign",
            Self::Promise => "send",
            Self::Other => "do",
        }
    }

    /// Bills and promises stay "was due" until the user acts (D117); every
    /// other detected kind lets go once its window closes.
    pub fn expires_while_open(self) -> bool {
        !matches!(self, Self::Bill | Self::PaymentFailed | Self::Promise)
    }
}

impl std::fmt::Display for TodoKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
