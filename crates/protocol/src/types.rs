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
    Ping,
    Shutdown,
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
        envelopes: Vec<Envelope>,
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
