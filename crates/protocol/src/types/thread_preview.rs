//! A thread's headline facts with no bodies (`GetThreadPreviews`): what a
//! hover card or a script needs to say what a thread is.

use mxr_core::id::{AccountId, ThreadId};
use serde::{Deserialize, Serialize};

/// The most threads one `GetThreadPreviews` request may name.
pub const THREAD_PREVIEWS_MAX: usize = 100;

/// One thread at a glance. Sender, date, snippet come from its latest
/// message; counts and the attachment flag cover the whole thread.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadPreviewData {
    pub thread_id: ThreadId,
    pub subject: String,
    /// The latest message's sender name, when the message has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub sender_email: String,
    /// When the latest message arrived: unix seconds.
    pub latest_at: i64,
    pub message_count: u32,
    pub unread_count: u32,
    /// The start of the latest message's text, clipped.
    pub snippet: String,
    /// Any message in the thread carries an attachment.
    #[serde(default)]
    pub has_attachments: bool,
    /// The account the latest message came to. Absent from older daemons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<AccountId>,
}
