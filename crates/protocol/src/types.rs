use mxr_core::id::*;
use mxr_core::types::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub id: u64,
    pub payload: IpcPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum IpcPayload {
    Request(Request),
    Response(Response),
    Event(DaemonEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Request {
    ListEnvelopes {
        label_id: Option<LabelId>,
        account_id: Option<AccountId>,
        limit: u32,
        offset: u32,
    },
    GetEnvelope {
        message_id: MessageId,
    },
    GetBody {
        message_id: MessageId,
    },
    GetThread {
        thread_id: ThreadId,
    },
    ListLabels {
        account_id: Option<AccountId>,
    },
    Search {
        query: String,
        limit: u32,
    },
    SyncNow {
        account_id: Option<AccountId>,
    },
    GetSyncStatus {
        account_id: AccountId,
    },
    SetFlags {
        message_id: MessageId,
        flags: MessageFlags,
    },
    Count {
        query: String,
    },
    GetHeaders {
        message_id: MessageId,
    },
    ListSavedSearches,
    CreateSavedSearch {
        name: String,
        query: String,
    },
    DeleteSavedSearch {
        name: String,
    },
    RunSavedSearch {
        name: String,
        limit: u32,
    },
    // Mutations (Phase 2)
    Mutation(MutationCommand),
    Unsubscribe {
        message_id: MessageId,
    },
    Snooze {
        message_id: MessageId,
        wake_at: chrono::DateTime<chrono::Utc>,
    },
    Unsnooze {
        message_id: MessageId,
    },
    ListSnoozed,
    // Compose (Phase 2)
    PrepareReply {
        message_id: MessageId,
        reply_all: bool,
    },
    PrepareForward {
        message_id: MessageId,
    },
    SendDraft {
        draft: Draft,
    },
    /// Save draft to the mail server (e.g. Gmail Drafts folder).
    SaveDraftToServer {
        draft: Draft,
    },
    ListDrafts,

    GetStatus,
    Ping,
    Shutdown,
}

/// Mutation commands for modifying messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mutation")]
pub enum MutationCommand {
    Archive {
        message_ids: Vec<MessageId>,
    },
    Trash {
        message_ids: Vec<MessageId>,
    },
    Spam {
        message_ids: Vec<MessageId>,
    },
    Star {
        message_ids: Vec<MessageId>,
        starred: bool,
    },
    SetRead {
        message_ids: Vec<MessageId>,
        read: bool,
    },
    ModifyLabels {
        message_ids: Vec<MessageId>,
        add: Vec<String>,
        remove: Vec<String>,
    },
    Move {
        message_ids: Vec<MessageId>,
        target_label: String,
    },
}

/// Reply context returned by PrepareReply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyContext {
    pub in_reply_to: String,
    pub reply_to: String,
    pub cc: String,
    pub subject: String,
    pub from: String,
    pub thread_context: String,
}

/// Forward context returned by PrepareForward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardContext {
    pub subject: String,
    pub from: String,
    pub forwarded_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
#[allow(clippy::large_enum_variant)]
pub enum Response {
    Ok { data: ResponseData },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
#[allow(clippy::large_enum_variant)]
pub enum ResponseData {
    Envelopes {
        messages: Vec<SyncedMessage>,
    },
    Envelope {
        envelope: Envelope,
    },
    Body {
        body: MessageBody,
    },
    Thread {
        thread: Thread,
        messages: Vec<Envelope>,
    },
    Labels {
        labels: Vec<Label>,
    },
    SearchResults {
        results: Vec<SearchResultItem>,
    },
    SyncStatus {
        last_sync: Option<String>,
        status: String,
    },
    Count {
        count: u32,
    },
    Headers {
        headers: Vec<(String, String)>,
    },
    SavedSearches {
        searches: Vec<mxr_core::types::SavedSearch>,
    },
    SavedSearchData {
        search: mxr_core::types::SavedSearch,
    },
    Status {
        uptime_secs: u64,
        accounts: Vec<String>,
        total_messages: u32,
    },
    ReplyContext {
        context: ReplyContext,
    },
    ForwardContext {
        context: ForwardContext,
    },
    Drafts {
        drafts: Vec<Draft>,
    },
    SnoozedMessages {
        snoozed: Vec<Snoozed>,
    },
    Pong,
    Ack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum DaemonEvent {
    SyncCompleted {
        account_id: AccountId,
        messages_synced: u32,
    },
    SyncError {
        account_id: AccountId,
        error: String,
    },
    NewMessages {
        envelopes: Vec<Envelope>,
    },
    MessageUnsnoozed {
        message_id: MessageId,
    },
    LabelCountsUpdated {
        counts: Vec<LabelCount>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelCount {
    pub label_id: LabelId,
    pub unread_count: u32,
    pub total_count: u32,
}
