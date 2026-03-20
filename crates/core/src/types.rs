use crate::id::*;
use bitflags::bitflags;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// -- Address ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

// -- Account ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub email: String,
    pub sync_backend: Option<BackendRef>,
    pub send_backend: Option<BackendRef>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendRef {
    pub provider_kind: ProviderKind,
    pub config_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    Gmail,
    Imap,
    Smtp,
    Fake,
}

// -- Label --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub id: LabelId,
    pub account_id: AccountId,
    pub name: String,
    pub kind: LabelKind,
    pub color: Option<String>,
    pub provider_id: String,
    pub unread_count: u32,
    pub total_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelKind {
    System,
    Folder,
    User,
}

// -- MessageFlags -------------------------------------------------------------

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct MessageFlags: u32 {
        const READ     = 0b0000_0001;
        const STARRED  = 0b0000_0010;
        const DRAFT    = 0b0000_0100;
        const SENT     = 0b0000_1000;
        const TRASH    = 0b0001_0000;
        const SPAM     = 0b0010_0000;
        const ARCHIVED = 0b0100_0000;
        const ANSWERED = 0b1000_0000;
    }
}

// -- Envelope -----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: MessageId,
    pub account_id: AccountId,
    pub provider_id: String,
    pub thread_id: ThreadId,
    pub message_id_header: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub flags: MessageFlags,
    pub snippet: String,
    pub has_attachments: bool,
    pub size_bytes: u64,
    pub unsubscribe: UnsubscribeMethod,
    /// Provider-specific label IDs (e.g. "INBOX", "SENT", "Label_123").
    /// Transient: used during sync to populate the message_labels junction table.
    #[serde(default)]
    pub label_provider_ids: Vec<String>,
}

// -- UnsubscribeMethod --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UnsubscribeMethod {
    OneClick {
        url: String,
    },
    HttpLink {
        url: String,
    },
    Mailto {
        address: String,
        subject: Option<String>,
    },
    BodyLink {
        url: String,
    },
    None,
}

// -- ReplyHeaders ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplyHeaders {
    pub in_reply_to: String,
    #[serde(default)]
    pub references: Vec<String>,
}

// -- MessageMetadata ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MessageMetadata {
    pub list_id: Option<String>,
    #[serde(default)]
    pub auth_results: Vec<String>,
    #[serde(default)]
    pub content_language: Vec<String>,
    pub text_plain_format: Option<TextPlainFormat>,
    pub calendar: Option<CalendarMetadata>,
    pub raw_headers: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TextPlainFormat {
    Fixed,
    Flowed { delsp: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarMetadata {
    pub method: Option<String>,
    pub summary: Option<String>,
}

// -- MessageBody --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBody {
    pub message_id: MessageId,
    pub text_plain: Option<String>,
    pub text_html: Option<String>,
    pub attachments: Vec<AttachmentMeta>,
    pub fetched_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: MessageMetadata,
}

// -- AttachmentMeta -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMeta {
    pub id: AttachmentId,
    pub message_id: MessageId,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub local_path: Option<PathBuf>,
    pub provider_id: String,
}

// -- Thread -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub subject: String,
    pub participants: Vec<Address>,
    pub message_count: u32,
    pub unread_count: u32,
    pub latest_date: DateTime<Utc>,
    pub snippet: String,
}

// -- Draft --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: DraftId,
    pub account_id: AccountId,
    pub reply_headers: Option<ReplyHeaders>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub body_markdown: String,
    pub attachments: Vec<PathBuf>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// -- SavedSearch --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSearch {
    pub id: SavedSearchId,
    pub account_id: Option<AccountId>,
    pub name: String,
    pub query: String,
    pub sort: SortOrder,
    pub icon: Option<String>,
    pub position: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    DateDesc,
    DateAsc,
    Relevance,
}

// -- Snoozed ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snoozed {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub snoozed_at: DateTime<Utc>,
    pub wake_at: DateTime<Utc>,
    pub original_labels: Vec<LabelId>,
}

// -- Sync types ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapMailboxCursor {
    pub mailbox: String,
    pub uid_validity: u32,
    pub uid_next: u32,
    #[serde(default)]
    pub highest_modseq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapCapabilityState {
    pub move_ext: bool,
    pub uidplus: bool,
    pub idle: bool,
    pub condstore: bool,
    pub qresync: bool,
    pub namespace: bool,
    pub list_status: bool,
    pub utf8_accept: bool,
    pub imap4rev2: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncCursor {
    Gmail { history_id: u64 },
    GmailBackfill { history_id: u64, page_token: String },
    Imap {
        uid_validity: u32,
        uid_next: u32,
        #[serde(default)]
        mailboxes: Vec<ImapMailboxCursor>,
        #[serde(default)]
        capabilities: Option<ImapCapabilityState>,
    },
    Initial,
}

// -- SyncedMessage ------------------------------------------------------------

/// A message with both envelope and body, returned by sync.
/// Bodies are always fetched eagerly during sync — no lazy hydration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedMessage {
    pub envelope: Envelope,
    pub body: MessageBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBatch {
    pub upserted: Vec<SyncedMessage>,
    pub deleted_provider_ids: Vec<String>,
    pub label_changes: Vec<LabelChange>,
    pub next_cursor: SyncCursor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelChange {
    pub provider_message_id: String,
    pub added_labels: Vec<String>,
    pub removed_labels: Vec<String>,
}

// -- Export -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Markdown,
    Json,
    Mbox,
    LlmContext,
}

// -- ProviderMeta -------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMeta {
    pub message_id: MessageId,
    pub provider: ProviderKind,
    pub remote_id: String,
    pub thread_remote_id: Option<String>,
    pub sync_token: Option<String>,
    pub raw_labels: Option<String>,
    pub mailbox_id: Option<String>,
    pub uid_validity: Option<u32>,
    pub raw_json: Option<String>,
}

// -- SyncCapabilities ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCapabilities {
    pub labels: bool,
    pub server_search: bool,
    pub delta_sync: bool,
    pub push: bool,
    pub batch_operations: bool,
    pub native_thread_ids: bool,
}

// -- SendReceipt --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendReceipt {
    pub provider_message_id: Option<String>,
    pub sent_at: DateTime<Utc>,
}
