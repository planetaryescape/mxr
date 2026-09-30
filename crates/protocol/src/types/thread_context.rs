//! Reader context for one conversation: facts the store already knows
//! (`GetThreadContext`) and the optional model-written gist and ask
//! (`GetThreadGist`). The two are separate requests so the facts never wait
//! on a model.

use super::CommitmentData;
use mxr_core::id::*;
use serde::{Deserialize, Serialize};

/// Returned by `Request::GetThreadContext`. Computed from the local store
/// with no model involved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadContextData {
    pub thread_id: ThreadId,
    pub account_id: AccountId,
    /// The person this conversation is mainly with: the latest sender who
    /// isn't you, or the first recipient of your latest message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterparty: Option<ThreadCounterpartyData>,
    /// Set when the newest message is from someone else, it isn't bulk
    /// mail, and you haven't written in the thread since.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owed_reply: Option<OwedReplyHereData>,
    /// Open promises recorded against this thread, both directions.
    #[serde(default)]
    pub promises: Vec<ThreadPromiseData>,
}

/// An open promise in the thread and who made it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadPromiseData {
    /// Who owes it, as the thread knows them: "you" for your own promises,
    /// else the owner's display name from the thread's messages, falling back
    /// to their address. In a group thread each promise keeps its own owner.
    pub owner: String,
    pub commitment: CommitmentData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadCounterpartyData {
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Messages they sent you, across every thread.
    pub messages_from_them: u32,
    /// Messages you sent them (to, cc or bcc), across every thread.
    pub messages_from_you: u32,
    /// Median time you take to answer them, from past reply pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub your_reply_p50_seconds: Option<u32>,
    pub your_reply_samples: u32,
    /// Median time they take to answer you.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub their_reply_p50_seconds: Option<u32>,
    pub their_reply_samples: u32,
    /// Latest message between you in any other thread. `None` means this is
    /// your first conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_contact_elsewhere_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Mailing-list or bulk sender; relationship stats mean little here.
    pub bulk_sender: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OwedReplyHereData {
    pub message_id: MessageId,
    pub since: chrono::DateTime<chrono::Utc>,
}

/// Returned by `Request::GetThreadGist`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadGistData {
    pub thread_id: ThreadId,
    pub status: ThreadGistStatusData,
    /// One plain-text sentence, at most 240 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gist: Option<String>,
    /// What the conversation asks of you. `None` with `status: ready` means
    /// the model found no ask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<ThreadAskData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<AiProvenanceData>,
    /// Why there is no gist, for `disabled`, `blocked` and `failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub from_cache: bool,
    /// The conversation's newest message when the gist was written. A client
    /// that has seen a newer message in the conversation ignores the gist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub newest_message_id: Option<MessageId>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ThreadGistStatusData {
    Ready,
    /// No model is configured.
    Disabled,
    /// Privacy settings keep this thread from the configured model.
    Blocked,
    /// The model failed or returned something unusable.
    Failed,
}

/// Most conversations one `Request::GetThreadGists` may name: a screen of
/// rows plus lookahead is a few dozen.
pub const THREAD_GISTS_MAX_BATCH: usize = 100;

/// Returned by `Request::GetThreadGists`: the gists a list can show now,
/// straight from the cache, and what was queued to be written.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadGistBatchData {
    /// Whether gists can be written at all. Anything but `available` means
    /// lists show their rows as they are, with nothing to wait for.
    pub model: GistModelData,
    /// Cached gists that match each conversation's newest message, in
    /// request order. Only `ready` gists are listed.
    pub gists: Vec<ThreadGistData>,
    /// Conversations waiting in the queue after this request. Each one's
    /// gist arrives as a `ThreadGistReady` event.
    #[serde(default)]
    pub queued: Vec<ThreadId>,
    /// Conversations a writer is on right now; their gists arrive as events
    /// too.
    #[serde(default)]
    pub in_flight: Vec<ThreadId>,
    /// Conversations that have no gist and won't get one from this request.
    #[serde(default)]
    pub skipped: Vec<ThreadGistSkipData>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GistModelData {
    Available,
    /// No model is configured.
    Disabled,
    /// Privacy settings keep conversations from the configured model.
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadGistSkipData {
    pub thread_id: ThreadId,
    pub reason: ThreadGistSkipReasonData,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ThreadGistSkipReasonData {
    /// Newsletters, lists and automated mail never go to the model for a
    /// list gist; the row's own snippet says enough.
    NotPeople,
    /// No such conversation, or it is in the trash.
    NotFound,
    /// The model failed on it a moment ago; it is tried again later.
    RecentlyFailed,
    /// Only asked for cached gists (`generate: false`).
    NotGenerated,
    /// The queue was full: older requests are ahead of it. Asking again
    /// once the rows are still on screen queues it.
    QueueFull,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadAskData {
    /// The ask in a short plain-text clause, at most 160 characters.
    pub summary: String,
    /// The sentence that makes the ask, verified by the daemon to appear in
    /// that message's text (whitespace-insensitive), so clients can
    /// highlight it. Dropped when it can't be found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote: Option<VerifiedQuoteData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VerifiedQuoteData {
    pub message_id: MessageId,
    pub text: String,
}

/// Where model-written text came from, so every client can say so.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AiProvenanceData {
    pub model: String,
    pub locality: AiLocalityData,
    /// What the prompt carried, in order.
    pub sources: Vec<AiSourceData>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AiLocalityData {
    /// The endpoint is on this machine.
    Local,
    Cloud,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AiSourceData {
    /// The messages in this conversation.
    ThisThread,
    /// Stats about your history with the counterparty (message counts,
    /// reply times). Only sent to a cloud model with
    /// `llm.allow_cloud_relationship_data`.
    RelationshipHistory,
    /// The message you are sending (promise detection).
    YourMessage,
}
