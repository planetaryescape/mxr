use mxr_core::id::*;
use serde::{Deserialize, Serialize};

/// Which part of the desk a row belongs to. A thread appears in at most one
/// lane; when it qualifies for several, the earlier lane in this order wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DeskLaneKind {
    /// Someone you are in conversation with wrote last and you have not replied.
    Owed,
    /// A promise you made that is due soon or overdue.
    Due,
    /// You wrote last and are waiting on the other person.
    Waiting,
    /// Recent mail from a person you have not written to before.
    PeopleNew,
}

/// One row on the desk: a thread, who it is with, and why it is here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeskRowData {
    pub lane: DeskLaneKind,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    /// The message to open: the latest inbound message for owed and
    /// people-new rows, your latest message for waiting rows, the promise's
    /// evidence message for due rows.
    pub message_id: MessageId,
    /// Every message of the thread in this account, so a verb on the row
    /// (archive, snooze) covers the whole conversation.
    pub message_ids: Vec<MessageId>,
    pub counterparty_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty_name: Option<String>,
    pub subject: String,
    /// A short human reason, e.g. "replied to your message".
    pub reason: String,
    /// When the clock for this row started: their message for owed and
    /// people-new, your message for waiting, the due date for due.
    pub since: chrono::DateTime<chrono::Utc>,
    /// Seconds from `since` to `generated_at`. Negative for a promise that
    /// is not due yet.
    pub age_seconds: i64,
    /// The usual time for the relevant reply with this person: yours to them
    /// on owed rows, theirs to you on waiting rows. Median of past replies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usual_seconds: Option<i64>,
    /// How many past replies `usual_seconds` is based on.
    #[serde(default)]
    pub usual_samples: u32,
    /// True when the row is past the person's usual pace, or past its due date.
    #[serde(default)]
    pub overdue: bool,
    #[serde(default)]
    pub unread: bool,
    /// Any message in the conversation is starred.
    #[serde(default)]
    pub starred: bool,
    /// Due rows: the promise's id, for resolving it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commitment_id: Option<String>,
}

/// One lane: the rows returned (capped by the request's `lane_limit`) and
/// how many there are in total.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeskLaneData {
    pub rows: Vec<DeskRowData>,
    pub total: u32,
}

/// Counts for everything that is not on the desk, for the one-line summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeskElsewhereData {
    /// Inbox mail from newsletters and lists in the recent window, read or
    /// not: never an unread count.
    pub reading: u32,
    /// Inbox mail from automated senders (receipts, notifications) in the
    /// recent window.
    pub paper_trail: u32,
    /// Deliveries that have not arrived and were not dismissed.
    pub deliveries: u32,
    /// Upcoming invites you have not answered.
    pub invites: u32,
    /// Senders with recent mail and no screener decision.
    pub screener: u32,
    /// The first account (in account order) with senders waiting on a
    /// screener decision, so a link from an all-accounts desk opens a queue
    /// that has them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screener_account: Option<AccountId>,
}

/// A conversation, named with its account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeskThreadRefData {
    pub account_id: AccountId,
    pub thread_id: ThreadId,
}
