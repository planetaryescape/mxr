//! Daemon-resolved selections for bulk mutations.
//!
//! A selection names the messages an action covers (for example, every match
//! of a search) without listing their ids. The daemon resolves it, keeps the
//! id list, and answers with counts and a sample. No message id list crosses
//! IPC in either direction, so a selection of any size fits in one frame.
use mxr_core::{AccountId, MessageId, ThreadId};
use serde::{Deserialize, Serialize};

/// What a bulk action covers. The daemon resolves it lexically, newest first,
/// with the same visibility and trash/spam rules as `Request::Search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectionData {
    Search {
        query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account_id: Option<AccountId>,
    },
}

/// How many messages of a selection one account holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SelectionAccountData {
    pub account_id: AccountId,
    pub account_name: String,
    pub count: u32,
}

/// One of the newest messages in a selection, for the confirmation table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SelectionSampleData {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub from: String,
    pub subject: String,
    pub date: chrono::DateTime<chrono::Utc>,
}

/// What a selection covers right now, and the token that starts a job over
/// exactly that set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SelectionPreviewData {
    /// Exact: messages the user can see, still in the store, after the same
    /// filters `Request::Search` applies.
    pub count: u32,
    pub threads: u32,
    pub accounts: Vec<SelectionAccountData>,
    /// The newest messages, newest first.
    pub sample: Vec<SelectionSampleData>,
    pub sample_size: u32,
    /// Always false: a selection is never cut to a cap. Present so one parser
    /// reads both this shape and the capped one.
    pub capped: bool,
    /// Single use. `StartSelectionJob` acts on exactly the messages counted
    /// here: mail that arrives later is left alone.
    pub preview_token: String,
    /// Unix milliseconds; the token stops working at this time.
    pub expires_at: i64,
}

/// A message a job could not change, with why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct JobFailureData {
    pub message_id: MessageId,
    pub error: String,
}
