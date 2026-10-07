//! Archive: mail as records you ask questions of.
//!
//! A record is a receipt, order, booking, invoice, statement, ticket,
//! contract, warranty or account, built from one or more emails. Rules and
//! schema.org only in this phase, no model:
//!
//! * [`schema_org`] reads the markup senders put in the email (`Order`,
//!   `ParcelDelivery`, `Invoice`, `FlightReservation`, `LodgingReservation`,
//!   `EventReservation` and the other reservations). Markup is the sender's
//!   own statement, so its money and dates are checked.
//! * [`rules`] reads the subject and body: the kind from the subject, the
//!   reference, total and dates from labelled lines. A rule's money and
//!   dates are unchecked until the user confirms them.
//! * [`detect`] combines the two into what to file, with where each field
//!   came from.
//! * [`group`] builds the composite records: trips from bookings whose dates
//!   overlap, series from recurring bills of one issuer.
//! * [`answer`] ranks records for the answer box, by fields, with no model.
//! * [`export`] writes the CSV and its preview.
//! * [`pass`] runs it over mail: the post-sync scan, the newest-first first
//!   run, filing by hand, from a ticked-off to-do and from a delivered
//!   parcel.
//!
//! The crate is provider-agnostic and does no network I/O.

pub mod answer;
pub mod coming_up;
pub mod detect;
pub mod export;
pub mod fields;
pub mod group;
pub mod pass;
pub mod rules;
pub mod schema_org;

use serde::{Deserialize, Serialize};

/// Bumped when detection changes enough that old mail should be read
/// again. A new version starts a new first run, which never undoes a
/// correction or a dismissal.
pub const RULES_VERSION: i64 = 1;

/// What a record is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Receipt,
    Order,
    Booking,
    Invoice,
    Statement,
    Ticket,
    Contract,
    Warranty,
    Account,
}

impl RecordKind {
    pub const ALL: [Self; 9] = [
        Self::Receipt,
        Self::Order,
        Self::Booking,
        Self::Invoice,
        Self::Statement,
        Self::Ticket,
        Self::Contract,
        Self::Warranty,
        Self::Account,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Receipt => "receipt",
            Self::Order => "order",
            Self::Booking => "booking",
            Self::Invoice => "invoice",
            Self::Statement => "statement",
            Self::Ticket => "ticket",
            Self::Contract => "contract",
            Self::Warranty => "warranty",
            Self::Account => "account",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        let value = value.strip_suffix('s').unwrap_or(&value);
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// "Receipt", for labels.
    pub fn label(self) -> &'static str {
        match self {
            Self::Receipt => "Receipt",
            Self::Order => "Order",
            Self::Booking => "Booking",
            Self::Invoice => "Invoice",
            Self::Statement => "Statement",
            Self::Ticket => "Ticket",
            Self::Contract => "Contract",
            Self::Warranty => "Warranty",
            Self::Account => "Account",
        }
    }

    /// What the reference is called on this kind's card: "Order",
    /// "Booking ref".
    pub fn reference_label(self) -> &'static str {
        match self {
            Self::Order => "Order",
            Self::Booking => "Booking ref",
            Self::Ticket => "Ticket ref",
            Self::Invoice => "Invoice",
            Self::Statement | Self::Account => "Account",
            Self::Receipt => "Receipt",
            Self::Contract | Self::Warranty => "Reference",
        }
    }

    /// Kinds that go into a tax export by default: invoices, receipts and
    /// statements, plus orders, which are receipts with a delivery.
    pub fn is_financial(self) -> bool {
        matches!(
            self,
            Self::Receipt | Self::Order | Self::Invoice | Self::Statement
        )
    }
}

impl std::fmt::Display for RecordKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What one email said about its record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Confirmation,
    Shipped,
    Delivered,
    Return,
    Refund,
    Invoice,
    Statement,
    Booking,
    Change,
    Cancellation,
    Receipt,
    Other,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmation => "confirmation",
            Self::Shipped => "shipped",
            Self::Delivered => "delivered",
            Self::Return => "return",
            Self::Refund => "refund",
            Self::Invoice => "invoice",
            Self::Statement => "statement",
            Self::Booking => "booking",
            Self::Change => "change",
            Self::Cancellation => "cancellation",
            Self::Receipt => "receipt",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "confirmation" => Self::Confirmation,
            "shipped" => Self::Shipped,
            "delivered" => Self::Delivered,
            "return" => Self::Return,
            "refund" => Self::Refund,
            "invoice" => Self::Invoice,
            "statement" => Self::Statement,
            "booking" => Self::Booking,
            "change" => Self::Change,
            "cancellation" => Self::Cancellation,
            "receipt" => Self::Receipt,
            _ => Self::Other,
        }
    }

    /// The word in an order's stage line: "ordered · shipped · delivered".
    pub fn word(self) -> &'static str {
        match self {
            Self::Confirmation => "ordered",
            Self::Shipped => "shipped",
            Self::Delivered => "delivered",
            Self::Return => "returned",
            Self::Refund => "refunded",
            Self::Invoice => "invoiced",
            Self::Statement => "statement",
            Self::Booking => "booked",
            Self::Change => "changed",
            Self::Cancellation => "cancelled",
            Self::Receipt => "paid",
            Self::Other => "email",
        }
    }

    /// Order of stages in a stage line.
    pub fn order(self) -> u8 {
        match self {
            Self::Confirmation | Self::Booking | Self::Invoice | Self::Statement => 0,
            Self::Receipt => 1,
            Self::Change => 2,
            Self::Shipped => 3,
            Self::Delivered => 4,
            Self::Return => 5,
            Self::Refund => 6,
            Self::Cancellation => 7,
            Self::Other => 8,
        }
    }
}

/// Where a field's value came from. Ranks decide which candidate wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// schema.org markup the sender put in the email.
    Schema,
    /// A pattern in the subject or body.
    Rule,
    /// The delivery tracker the parcel's emails built.
    Delivery,
    /// The to-do you ticked off.
    Todo,
    /// You.
    User,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Rule => "rule",
            Self::Delivery => "delivery",
            Self::Todo => "todo",
            Self::User => "user",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "schema" => Self::Schema,
            "delivery" => Self::Delivery,
            "todo" => Self::Todo,
            "user" => Self::User,
            _ => Self::Rule,
        }
    }

    pub fn rank(self) -> i64 {
        match self {
            Self::User => 4,
            Self::Schema => 3,
            Self::Delivery | Self::Todo => 2,
            Self::Rule => 1,
        }
    }

    /// "schema.org markup", for provenance lines.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Schema => "schema.org markup",
            Self::Rule => "a pattern in the email",
            Self::Delivery => "the delivery tracker",
            Self::Todo => "the to-do you ticked off",
            Self::User => "you",
        }
    }
}

/// A normalised issuer key: lowercase, trimmed. The store keeps the same
/// shape in `issuer_key`.
pub fn issuer_key(issuer: &str) -> String {
    issuer.trim().to_lowercase()
}

/// A reference without spaces and with consistent case, for dedup keys.
pub fn reference_key(reference: &str) -> String {
    reference
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase()
}
