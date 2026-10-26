//! To do: email shown as the thing you have to do, with a date.
//!
//! Automatic tasks project the persisted deterministic asks facet. Owner
//! requests and promises share the same facts, tags and exact source quote;
//! the scanner does not run another detector or mirror model commitments.
//!
//! * [`timing`] holds the lead-time table (when to act and when the row
//!   shows up) and the relevancy windows (when it stops mattering).
//! * [`action_link`] picks the link and checks the one-click gate: DMARC
//!   pass, the link's registrable domain matching the sender's, and
//!   earlier mail from that domain.
//! * [`complete`] spots a later message that looks like the confirmation.
//!   It only offers "looks done"; nothing closes a to-do on a guess.
//! * [`pass`] runs it over mail: the post-sync scan, the newest-first first
//!   run with its windows and one bounded catch-up, and
//!   the sweep that expires and claims rows.
//!
//! The crate is provider-agnostic and does no network I/O.

#![cfg_attr(
    test,
    expect(clippy::panic, reason = "unit tests panic on an unexpected variant")
)]

pub mod accepted_promises;
pub mod action_link;
pub mod asks;
pub mod complete;
pub mod dates;
pub mod detect;
pub mod linked;
pub mod money;
pub mod pass;
pub mod provenance;
pub mod schema_org;
pub mod text;
pub mod timing;

use serde::{Deserialize, Serialize};

/// Bumped when detection or derived projection changes require replaying old mail.
/// A new version starts a new first run, which never clears a claim,
/// an expiry or a user decision.
pub const RULES_VERSION: i64 = 3;

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
}

impl std::fmt::Display for TodoKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
