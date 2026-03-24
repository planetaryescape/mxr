# Phase 0 — Implementation Plan

> **Current Layout Note**
> This phase plan was written for the old multi-crate workspace. Current code ships as one publishable package, `mxr`. Names like `mxr-core`, `mxr-store`, `mxr-protocol`, and `mxr-tui` now refer to modules mounted from `crates/*/src` under the root package.

## Goal

Prove the architecture. Daemon runs, TUI connects, fake data flows end-to-end through SQLite and Tantivy.

## Key Decisions

- Unified `mxr` binary with clap subcommands (daemon crate produces the binary, tui is a library crate)
- Rust edition 2021
- Runtime sqlx queries (not compile-time checked `query!` macro)
- Two-pool SQLite (single writer + 4-conn reader pool, WAL mode, `foreign_keys=ON`)
- Length-delimited JSON over Unix socket for IPC (`tokio_util::codec::LengthDelimitedCodec` + `serde_json`)
- Tantivy 0.22+ with in-memory index for tests

---

## Step 1: mxr-core

### Crate/module affected

`crates/core/`

### Files to create/modify

```
crates/core/Cargo.toml
crates/core/src/lib.rs
crates/core/src/id.rs
crates/core/src/types.rs
crates/core/src/provider.rs
crates/core/src/error.rs
```

### External crate dependencies

```toml
[dependencies]
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
bitflags = "2"
thiserror = "2"
async-trait = "0.1"
```

### Key code patterns and struct signatures

#### `src/id.rs` — Typed ID macro and all ID types

```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            pub fn as_str(&self) -> String {
                self.0.to_string()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

typed_id!(AccountId);
typed_id!(MessageId);
typed_id!(ThreadId);
typed_id!(LabelId);
typed_id!(DraftId);
typed_id!(AttachmentId);
typed_id!(SavedSearchId);
typed_id!(RuleId);
```

#### `src/types.rs` — All domain types

```rust
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
}

// -- UnsubscribeMethod --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UnsubscribeMethod {
    OneClick { url: String },
    HttpLink { url: String },
    Mailto { address: String, subject: Option<String> },
    BodyLink { url: String },
    None,
}

// -- MessageBody --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBody {
    pub message_id: MessageId,
    pub text_plain: Option<String>,
    pub text_html: Option<String>,
    pub attachments: Vec<AttachmentMeta>,
    pub fetched_at: DateTime<Utc>,
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
    pub in_reply_to: Option<MessageId>,
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
pub enum SyncCursor {
    Gmail { history_id: u64 },
    Imap { uid_validity: u32, uid_next: u32 },
    Initial,
}

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
```

#### `src/provider.rs` — Provider traits

```rust
use crate::error::MxrError;
use crate::id::AccountId;
use crate::types::*;
use async_trait::async_trait;

pub type Result<T> = std::result::Result<T, MxrError>;

#[async_trait]
pub trait MailSyncProvider: Send + Sync {
    fn name(&self) -> &str;
    fn account_id(&self) -> &AccountId;
    fn capabilities(&self) -> SyncCapabilities;

    async fn authenticate(&mut self) -> Result<()>;
    async fn refresh_auth(&mut self) -> Result<()>;

    async fn sync_labels(&self) -> Result<Vec<Label>>;
    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch>;

    async fn fetch_body(&self, provider_message_id: &str) -> Result<MessageBody>;
    async fn fetch_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<Vec<u8>>;

    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<()>;

    async fn trash(&self, provider_message_id: &str) -> Result<()>;
    async fn set_read(&self, provider_message_id: &str, read: bool) -> Result<()>;
    async fn set_starred(&self, provider_message_id: &str, starred: bool) -> Result<()>;

    async fn search_remote(&self, _query: &str) -> Result<Vec<String>> {
        Err(MxrError::Provider("Server-side search not supported".into()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCapabilities {
    pub labels: bool,
    pub server_search: bool,
    pub delta_sync: bool,
    pub push: bool,
    pub batch_operations: bool,
}

#[async_trait]
pub trait MailSendProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn send(&self, draft: &Draft, from: &Address) -> Result<SendReceipt>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendReceipt {
    pub provider_message_id: Option<String>,
    pub sent_at: DateTime<Utc>,
}
```

#### `src/error.rs` — Error types

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MxrError {
    #[error("Store error: {0}")]
    Store(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Sync error: {0}")]
    Sync(String),

    #[error("Search error: {0}")]
    Search(String),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

#### `src/lib.rs` — Module re-exports

```rust
pub mod error;
pub mod id;
pub mod provider;
pub mod types;

pub use error::MxrError;
pub use id::*;
pub use provider::*;
pub use types::*;
```

### What to test and how

- **Typed ID roundtrip**: Create each ID type via `::new()`, serialize to JSON, deserialize back, assert equality.
- **MessageFlags bitwise**: Assert `READ | STARRED` contains both, `bits()` returns expected u32, round-trip through serde.
- **Serde roundtrip for all types**: Construct each struct with representative data, `serde_json::to_string` then `serde_json::from_str`, assert equality. This catches missing `Serialize`/`Deserialize` derives and field mismatches.
- **UnsubscribeMethod variants**: Serde roundtrip each variant (OneClick, HttpLink, Mailto, BodyLink, None).
- **SyncCursor variants**: Serde roundtrip Gmail, Imap, Initial variants.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_id_roundtrip() {
        let id = MessageId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: MessageId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn message_flags_bitwise() {
        let flags = MessageFlags::READ | MessageFlags::STARRED;
        assert!(flags.contains(MessageFlags::READ));
        assert!(flags.contains(MessageFlags::STARRED));
        assert!(!flags.contains(MessageFlags::DRAFT));
        assert_eq!(flags.bits(), 0b0000_0011);
    }

    #[test]
    fn envelope_serde_roundtrip() {
        let env = Envelope { /* ... populate all fields ... */ };
        let json = serde_json::to_string(&env).unwrap();
        let parsed: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(env.id, parsed.id);
        assert_eq!(env.subject, parsed.subject);
    }
}
```

---

## Step 2: mxr-store

### Crate/module affected

`crates/store/`

### Files to create/modify

```
crates/store/Cargo.toml
crates/store/src/lib.rs
crates/store/src/pool.rs
crates/store/src/account.rs
crates/store/src/message.rs
crates/store/src/label.rs
crates/store/src/body.rs
crates/store/src/thread.rs
crates/store/src/draft.rs
crates/store/src/search.rs
crates/store/src/snooze.rs
crates/store/src/sync_log.rs
crates/store/src/event_log.rs
migrations/001_initial.sql
```

### External crate dependencies

```toml
[dependencies]
mxr-core = { path = "../core" }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
```

### Key code patterns and struct signatures

#### `src/pool.rs` — Two-pool architecture

```rust
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteJournalMode, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

pub struct Store {
    writer: SqlitePool,
    reader: SqlitePool,
}

impl Store {
    pub async fn new(db_path: &Path) -> Result<Self, sqlx::Error> {
        let db_url = format!("sqlite:{}", db_path.display());

        let write_opts = SqliteConnectOptions::from_str(&db_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .pragma("foreign_keys", "ON");

        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(write_opts)
            .await?;

        let read_opts = SqliteConnectOptions::from_str(&db_url)?
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .pragma("foreign_keys", "ON")
            .read_only(true);

        let reader = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(read_opts)
            .await?;

        let store = Self { writer, reader };
        store.run_migrations().await?;
        Ok(store)
    }

    /// In-memory store for tests. Single pool serves both reads and writes.
    pub async fn in_memory() -> Result<Self, sqlx::Error> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?
            .journal_mode(SqliteJournalMode::Wal)
            .pragma("foreign_keys", "ON");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        let store = Self {
            writer: pool.clone(),
            reader: pool,
        };
        store.run_migrations().await?;
        Ok(store)
    }

    async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        // Execute the migration SQL against the writer pool
        let sql = include_str!("../../migrations/001_initial.sql");
        sqlx::raw_sql(sql).execute(&self.writer).await?;
        Ok(())
    }

    pub fn writer(&self) -> &SqlitePool {
        &self.writer
    }

    pub fn reader(&self) -> &SqlitePool {
        &self.reader
    }
}
```

#### `migrations/001_initial.sql`

The full schema from `02-data-model.md` — all tables, indexes, FTS5 virtual table, and triggers. Copy the complete SQL block from the blueprint verbatim (accounts, labels, messages, message_labels, bodies, attachments, provider_meta, drafts, snoozed, saved_searches, rules, messages_fts + triggers, sync_log).

Additionally, include the `event_log` table from A006 (daemon observability):

```sql
CREATE TABLE IF NOT EXISTS event_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   INTEGER NOT NULL,
    level       TEXT NOT NULL CHECK (level IN ('error', 'warn', 'info')),
    category    TEXT NOT NULL,
    account_id  TEXT,
    message_id  TEXT,
    rule_id     TEXT,
    summary     TEXT NOT NULL,
    details     TEXT,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX idx_event_log_time ON event_log(timestamp DESC);
CREATE INDEX idx_event_log_category ON event_log(category, timestamp DESC);
CREATE INDEX idx_event_log_level ON event_log(level, timestamp DESC);
```

#### `src/account.rs` — Account CRUD

```rust
use mxr_core::{Account, AccountId};
use sqlx::SqlitePool;

impl super::Store {
    pub async fn insert_account(&self, account: &Account) -> Result<(), sqlx::Error> {
        let id = account.id.as_str();
        let sync_provider = account.sync_backend.as_ref().map(|b| {
            serde_json::to_string(&b.provider_kind).unwrap()
        });
        let send_provider = account.send_backend.as_ref().map(|b| {
            serde_json::to_string(&b.provider_kind).unwrap()
        });
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            "INSERT INTO accounts (id, name, email, sync_provider, send_provider, enabled, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&account.name)
        .bind(&account.email)
        .bind(&sync_provider)
        .bind(&send_provider)
        .bind(account.enabled)
        .bind(now)
        .bind(now)
        .execute(&self.writer)
        .await?;

        Ok(())
    }

    pub async fn get_account(&self, id: &AccountId) -> Result<Option<Account>, sqlx::Error> {
        // SELECT from accounts, map Row to Account
        // Use self.reader for reads
        todo!()
    }

    pub async fn list_accounts(&self) -> Result<Vec<Account>, sqlx::Error> {
        todo!()
    }
}
```

#### `src/message.rs` — Message/Envelope CRUD

```rust
impl super::Store {
    pub async fn upsert_envelope(&self, envelope: &Envelope) -> Result<(), sqlx::Error> {
        // INSERT OR REPLACE into messages table
        // Serialize Vec<Address> fields as JSON TEXT via serde_json::to_string
        // Serialize UnsubscribeMethod as JSON TEXT
        // Store flags as envelope.flags.bits() (u32 integer)
        // Store date as envelope.date.timestamp() (i64)
        // Write through self.writer
        todo!()
    }

    pub async fn get_envelope(&self, id: &MessageId) -> Result<Option<Envelope>, sqlx::Error> {
        // Read through self.reader
        // Deserialize JSON TEXT fields back to Vec<Address>, UnsubscribeMethod
        // Reconstruct MessageFlags from bits
        todo!()
    }

    pub async fn list_envelopes_by_label(
        &self,
        label_id: &LabelId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        // JOIN messages with message_labels, ORDER BY date DESC
        // Read through self.reader
        todo!()
    }

    pub async fn list_envelopes_by_account(
        &self,
        account_id: &AccountId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        todo!()
    }

    pub async fn delete_messages_by_provider_ids(
        &self,
        account_id: &AccountId,
        provider_ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        // DELETE FROM messages WHERE account_id = ? AND provider_id IN (...)
        // Write through self.writer
        // Return number of rows affected
        todo!()
    }

    pub async fn set_message_labels(
        &self,
        message_id: &MessageId,
        label_ids: &[LabelId],
    ) -> Result<(), sqlx::Error> {
        // DELETE existing labels for message, INSERT new ones
        // Write through self.writer
        todo!()
    }

    pub async fn update_flags(
        &self,
        message_id: &MessageId,
        flags: MessageFlags,
    ) -> Result<(), sqlx::Error> {
        // UPDATE messages SET flags = ? WHERE id = ?
        todo!()
    }
}
```

#### `src/body.rs` — Body cache

```rust
impl super::Store {
    pub async fn get_body(&self, message_id: &MessageId) -> Result<Option<MessageBody>, sqlx::Error> {
        // SELECT from bodies + attachments WHERE message_id = ?
        // Read through self.reader
        todo!()
    }

    pub async fn insert_body(&self, body: &MessageBody) -> Result<(), sqlx::Error> {
        // INSERT into bodies table
        // INSERT each attachment into attachments table
        // Write through self.writer
        todo!()
    }
}
```

#### `src/label.rs` — Label CRUD

```rust
impl super::Store {
    pub async fn upsert_label(&self, label: &Label) -> Result<(), sqlx::Error> {
        todo!()
    }

    pub async fn list_labels_by_account(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<Label>, sqlx::Error> {
        todo!()
    }

    pub async fn update_label_counts(
        &self,
        label_id: &LabelId,
        unread_count: u32,
        total_count: u32,
    ) -> Result<(), sqlx::Error> {
        todo!()
    }

    pub async fn find_label_by_provider_id(
        &self,
        account_id: &AccountId,
        provider_id: &str,
    ) -> Result<Option<Label>, sqlx::Error> {
        todo!()
    }
}
```

#### `src/thread.rs` — Thread aggregation

```rust
impl super::Store {
    pub async fn get_thread(&self, thread_id: &ThreadId) -> Result<Option<Thread>, sqlx::Error> {
        // Aggregate from messages: COUNT, MAX(date), etc.
        // Read through self.reader
        todo!()
    }

    pub async fn get_thread_envelopes(
        &self,
        thread_id: &ThreadId,
    ) -> Result<Vec<Envelope>, sqlx::Error> {
        // SELECT from messages WHERE thread_id = ? ORDER BY date ASC
        todo!()
    }
}
```

#### `src/draft.rs` — Draft CRUD

```rust
impl super::Store {
    pub async fn insert_draft(&self, draft: &Draft) -> Result<(), sqlx::Error> {
        todo!()
    }

    pub async fn get_draft(&self, id: &DraftId) -> Result<Option<Draft>, sqlx::Error> {
        todo!()
    }

    pub async fn list_drafts(&self, account_id: &AccountId) -> Result<Vec<Draft>, sqlx::Error> {
        todo!()
    }

    pub async fn delete_draft(&self, id: &DraftId) -> Result<(), sqlx::Error> {
        todo!()
    }
}
```

#### `src/search.rs` — Saved search CRUD

```rust
impl super::Store {
    pub async fn insert_saved_search(&self, search: &SavedSearch) -> Result<(), sqlx::Error> {
        todo!()
    }

    pub async fn list_saved_searches(&self) -> Result<Vec<SavedSearch>, sqlx::Error> {
        todo!()
    }

    pub async fn delete_saved_search(&self, id: &SavedSearchId) -> Result<(), sqlx::Error> {
        todo!()
    }
}
```

#### `src/snooze.rs` — Snooze queries

```rust
impl super::Store {
    pub async fn insert_snooze(&self, snoozed: &Snoozed) -> Result<(), sqlx::Error> {
        todo!()
    }

    pub async fn get_due_snoozes(&self, now: DateTime<Utc>) -> Result<Vec<Snoozed>, sqlx::Error> {
        // SELECT * FROM snoozed WHERE wake_at <= ?
        todo!()
    }

    pub async fn remove_snooze(&self, message_id: &MessageId) -> Result<(), sqlx::Error> {
        todo!()
    }
}
```

#### `src/sync_log.rs` — Sync diagnostics

```rust
use chrono::{DateTime, Utc};
use mxr_core::AccountId;

pub struct SyncLogEntry {
    pub id: i64,
    pub account_id: AccountId,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: SyncStatus,
    pub messages_synced: u32,
    pub error_message: Option<String>,
}

pub enum SyncStatus {
    Running,
    Success,
    Error,
}

impl super::Store {
    pub async fn insert_sync_log(
        &self,
        account_id: &AccountId,
        status: &SyncStatus,
    ) -> Result<i64, sqlx::Error> {
        // INSERT INTO sync_log, return id
        todo!()
    }

    pub async fn complete_sync_log(
        &self,
        log_id: i64,
        status: &SyncStatus,
        messages_synced: u32,
        error_message: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        todo!()
    }

    pub async fn get_last_sync(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<SyncLogEntry>, sqlx::Error> {
        todo!()
    }
}
```

#### `src/event_log.rs` — Event log CRUD (A006)

```rust
use mxr_core::AccountId;

pub struct EventLogEntry {
    pub id: i64,
    pub timestamp: i64,
    pub level: String,     // "error" | "warn" | "info"
    pub category: String,  // "sync" | "rule" | "send" | "auth" | ...
    pub account_id: Option<AccountId>,
    pub message_id: Option<String>,
    pub rule_id: Option<String>,
    pub summary: String,
    pub details: Option<String>,
}

impl super::Store {
    pub async fn insert_event(
        &self,
        level: &str,
        category: &str,
        summary: &str,
        account_id: Option<&AccountId>,
        details: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let aid = account_id.map(|a| a.as_str());
        sqlx::query(
            "INSERT INTO event_log (timestamp, level, category, account_id, summary, details)
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(now)
        .bind(level)
        .bind(category)
        .bind(&aid)
        .bind(summary)
        .bind(details)
        .execute(&self.writer)
        .await?;
        Ok(())
    }

    pub async fn list_events(
        &self,
        limit: u32,
        level: Option<&str>,
        category: Option<&str>,
    ) -> Result<Vec<EventLogEntry>, sqlx::Error> {
        // SELECT from event_log with optional level/category filters
        // ORDER BY timestamp DESC LIMIT ?
        // Read through self.reader
        todo!()
    }
}
```

#### `src/lib.rs`

```rust
mod pool;
mod account;
mod message;
mod label;
mod body;
mod thread;
mod draft;
mod search;
mod snooze;
mod sync_log;
mod event_log;

pub use pool::Store;
pub use sync_log::{SyncLogEntry, SyncStatus};
pub use event_log::EventLogEntry;
```

### What to test and how

All tests use `Store::in_memory()`. Each test creates a fresh in-memory database.

- **Account roundtrip**: Insert account, query by ID, assert fields match.
- **Envelope upsert + query**: Insert envelope, query by ID, assert all fields including JSON-serialized `Vec<Address>` and `MessageFlags` roundtrip correctly.
- **Label CRUD**: Insert labels, list by account, verify counts update.
- **Body cache**: Insert body with attachments, query by message_id, verify text and attachment meta.
- **Message-label junction**: Insert envelope + labels, set message labels, query envelopes by label.
- **Thread aggregation**: Insert 3 messages with same thread_id, call `get_thread`, verify message_count=3 and latest_date correct.
- **Draft CRUD**: Insert, list, delete.
- **Snooze lifecycle**: Insert snooze, query due snoozes with a future `now`, verify returned. Remove snooze, verify gone.
- **Sync log**: Insert running entry, complete it, query last sync.
- **Event log**: Insert events with different levels and categories, query with filters, verify ordering by timestamp DESC.

```rust
#[tokio::test]
async fn envelope_roundtrip() {
    let store = Store::in_memory().await.unwrap();
    // Insert account first (FK constraint)
    store.insert_account(&test_account()).await.unwrap();
    let env = test_envelope();
    store.upsert_envelope(&env).await.unwrap();
    let fetched = store.get_envelope(&env.id).await.unwrap().unwrap();
    assert_eq!(fetched.id, env.id);
    assert_eq!(fetched.subject, env.subject);
    assert_eq!(fetched.from.email, env.from.email);
    assert_eq!(fetched.flags, env.flags);
}
```

---

## Step 3: mxr-search

### Crate/module affected

`crates/search/`

### Files to create/modify

```
crates/search/Cargo.toml
crates/search/src/lib.rs
crates/search/src/schema.rs
crates/search/src/index.rs
```

### External crate dependencies

```toml
[dependencies]
mxr-core = { path = "../core" }
tantivy = "0.22"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
```

### Key code patterns and struct signatures

#### `src/schema.rs` — Index schema definition

```rust
use tantivy::schema::*;

pub struct MxrSchema {
    pub schema: Schema,
    pub message_id: Field,
    pub account_id: Field,
    pub thread_id: Field,
    pub subject: Field,
    pub from_name: Field,
    pub from_email: Field,
    pub to_email: Field,
    pub snippet: Field,
    pub body_text: Field,
    pub labels: Field,
    pub date: Field,
    pub flags: Field,
    pub has_attachments: Field,
}

impl MxrSchema {
    pub fn build() -> Self {
        let mut builder = Schema::builder();

        let message_id = builder.add_text_field("message_id", STRING | STORED);
        let account_id = builder.add_text_field("account_id", STRING | STORED);
        let thread_id = builder.add_text_field("thread_id", STRING | STORED);

        let subject = builder.add_text_field("subject", TEXT);
        let from_name = builder.add_text_field("from_name", TEXT);
        let from_email = builder.add_text_field("from_email", STRING);
        let to_email = builder.add_text_field("to_email", STRING);
        let snippet = builder.add_text_field("snippet", TEXT);
        let body_text = builder.add_text_field("body_text", TEXT);

        let labels = builder.add_text_field("labels", STRING);
        let date = builder.add_date_field("date", INDEXED | STORED);
        let flags = builder.add_u64_field("flags", INDEXED);
        let has_attachments = builder.add_bool_field("has_attachments", INDEXED);

        let schema = builder.build();

        Self {
            schema,
            message_id,
            account_id,
            thread_id,
            subject,
            from_name,
            from_email,
            to_email,
            snippet,
            body_text,
            labels,
            date,
            flags,
            has_attachments,
        }
    }
}
```

#### `src/index.rs` — SearchIndex operations

```rust
use crate::schema::MxrSchema;
use mxr_core::{Envelope, MessageBody, MessageId, MxrError};
use std::path::Path;
use tantivy::{
    collector::TopDocs,
    query::QueryParser,
    schema::Value,
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument,
};

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    schema: MxrSchema,
}

/// Search result returned to callers.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub message_id: String,
    pub account_id: String,
    pub thread_id: String,
    pub score: f32,
}

impl SearchIndex {
    /// Open or create a persistent index on disk.
    pub fn open(index_path: &Path) -> Result<Self, MxrError> {
        let schema_def = MxrSchema::build();
        let index = Index::open_or_create(
            tantivy::directory::MmapDirectory::open(index_path)
                .map_err(|e| MxrError::Search(e.to_string()))?,
            schema_def.schema.clone(),
        )
        .map_err(|e| MxrError::Search(e.to_string()))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| MxrError::Search(e.to_string()))?;

        let writer = index
            .writer(50_000_000) // 50MB heap
            .map_err(|e| MxrError::Search(e.to_string()))?;

        Ok(Self {
            index,
            reader,
            writer,
            schema: schema_def,
        })
    }

    /// Create an in-memory index for tests.
    pub fn in_memory() -> Result<Self, MxrError> {
        let schema_def = MxrSchema::build();
        let index = Index::create_in_ram(schema_def.schema.clone());

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|e: tantivy::TantivyError| MxrError::Search(e.to_string()))?;

        let writer = index
            .writer(15_000_000) // 15MB heap for tests
            .map_err(|e| MxrError::Search(e.to_string()))?;

        Ok(Self {
            index,
            reader,
            writer,
            schema: schema_def,
        })
    }

    /// Index an envelope (headers/metadata). Does not include body text.
    pub fn index_envelope(&mut self, envelope: &Envelope) -> Result<(), MxrError> {
        let s = &self.schema;
        let mut doc = TantivyDocument::new();
        doc.add_text(s.message_id, &envelope.id.as_str());
        doc.add_text(s.account_id, &envelope.account_id.as_str());
        doc.add_text(s.thread_id, &envelope.thread_id.as_str());
        doc.add_text(s.subject, &envelope.subject);
        doc.add_text(s.from_name, envelope.from.name.as_deref().unwrap_or(""));
        doc.add_text(s.from_email, &envelope.from.email);
        for addr in &envelope.to {
            doc.add_text(s.to_email, &addr.email);
        }
        doc.add_text(s.snippet, &envelope.snippet);
        doc.add_u64(s.flags, envelope.flags.bits() as u64);
        doc.add_bool(s.has_attachments, envelope.has_attachments);

        // Convert chrono DateTime to tantivy DateTime
        let dt = tantivy::DateTime::from_timestamp_secs(envelope.date.timestamp());
        doc.add_date(s.date, dt);

        self.writer
            .add_document(doc)
            .map_err(|e| MxrError::Search(e.to_string()))?;
        Ok(())
    }

    /// Index (or re-index) body text for an existing message.
    /// Deletes old doc and re-adds with body text included.
    pub fn index_body(
        &mut self,
        envelope: &Envelope,
        body: &MessageBody,
    ) -> Result<(), MxrError> {
        // Delete existing document by message_id
        let term = tantivy::Term::from_field_text(
            self.schema.message_id,
            &envelope.id.as_str(),
        );
        self.writer.delete_term(term);

        // Re-add with body
        let s = &self.schema;
        let mut doc = TantivyDocument::new();
        doc.add_text(s.message_id, &envelope.id.as_str());
        doc.add_text(s.account_id, &envelope.account_id.as_str());
        doc.add_text(s.thread_id, &envelope.thread_id.as_str());
        doc.add_text(s.subject, &envelope.subject);
        doc.add_text(s.from_name, envelope.from.name.as_deref().unwrap_or(""));
        doc.add_text(s.from_email, &envelope.from.email);
        for addr in &envelope.to {
            doc.add_text(s.to_email, &addr.email);
        }
        doc.add_text(s.snippet, &envelope.snippet);

        // Add body text
        let body_text = body.text_plain.as_deref().unwrap_or("");
        doc.add_text(s.body_text, body_text);

        doc.add_u64(s.flags, envelope.flags.bits() as u64);
        doc.add_bool(s.has_attachments, envelope.has_attachments);
        let dt = tantivy::DateTime::from_timestamp_secs(envelope.date.timestamp());
        doc.add_date(s.date, dt);

        self.writer
            .add_document(doc)
            .map_err(|e| MxrError::Search(e.to_string()))?;
        Ok(())
    }

    /// Remove a document by message_id.
    pub fn remove_document(&mut self, message_id: &MessageId) {
        let term = tantivy::Term::from_field_text(
            self.schema.message_id,
            &message_id.as_str(),
        );
        self.writer.delete_term(term);
    }

    /// Commit pending writes. Must be called after batch indexing.
    pub fn commit(&mut self) -> Result<(), MxrError> {
        self.writer
            .commit()
            .map_err(|e| MxrError::Search(e.to_string()))?;
        self.reader
            .reload()
            .map_err(|e| MxrError::Search(e.to_string()))?;
        Ok(())
    }

    /// Search with a query string. Uses default QueryParser with field boosts.
    /// Fields: subject (3.0), from_name (2.0), snippet (1.0), body_text (0.5).
    pub fn search(
        &self,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, MxrError> {
        let s = &self.schema;

        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![s.subject, s.from_name, s.snippet, s.body_text],
        );
        query_parser.set_field_boost(s.subject, 3.0);
        query_parser.set_field_boost(s.from_name, 2.0);
        query_parser.set_field_boost(s.snippet, 1.0);
        query_parser.set_field_boost(s.body_text, 0.5);

        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| MxrError::Search(e.to_string()))?;

        let searcher = self.reader.searcher();
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| MxrError::Search(e.to_string()))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher
                .doc(doc_address)
                .map_err(|e| MxrError::Search(e.to_string()))?;

            let message_id = doc
                .get_first(s.message_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let account_id = doc
                .get_first(s.account_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let thread_id = doc
                .get_first(s.thread_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            results.push(SearchResult {
                message_id,
                account_id,
                thread_id,
                score,
            });
        }

        Ok(results)
    }
}
```

#### `src/lib.rs`

```rust
mod schema;
mod index;

pub use index::{SearchIndex, SearchResult};
pub use schema::MxrSchema;
```

### What to test and how

- **Index 10 envelopes, search by keyword**: Create in-memory index, index 10 envelopes with distinct subjects, commit, search for a subject keyword, verify correct message_id returned.
- **Field boost ranking**: Index two envelopes — one with keyword in subject, one in snippet. Search keyword, verify subject-match ranks higher.
- **Body indexing**: Index envelope, then index body text. Search for a body-only keyword, verify it's found.
- **Remove document**: Index, remove, commit, search. Verify not found.

```rust
#[test]
fn search_by_subject_keyword() {
    let mut idx = SearchIndex::in_memory().unwrap();
    // index 10 envelopes with unique subjects
    for env in &test_envelopes() {
        idx.index_envelope(env).unwrap();
    }
    idx.commit().unwrap();

    let results = idx.search("deployment", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].message_id, expected_id);
}
```

---

## Step 4: mxr-protocol

### Crate/module affected

`crates/protocol/`

### Files to create/modify

```
crates/protocol/Cargo.toml
crates/protocol/src/lib.rs
crates/protocol/src/types.rs
crates/protocol/src/codec.rs
```

### External crate dependencies

```toml
[dependencies]
mxr-core = { path = "../core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["io-util"] }
tokio-util = { version = "0.7", features = ["codec"] }
bytes = "1"
```

### Key code patterns and struct signatures

#### `src/types.rs` — IPC message types

```rust
use mxr_core::*;
use serde::{Deserialize, Serialize};

/// Top-level IPC message. Every message has an ID for request/response matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub id: u64,
    pub payload: IpcPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcPayload {
    Request(Request),
    Response(Response),
    Event(DaemonEvent),
}

/// Requests from client (TUI/CLI) to daemon.
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

/// Responses from daemon to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Response {
    Ok { data: ResponseData },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ResponseData {
    Envelopes { envelopes: Vec<Envelope> },
    Envelope { envelope: Envelope },
    Body { body: MessageBody },
    Thread { thread: Thread, messages: Vec<Envelope> },
    Labels { labels: Vec<Label> },
    SearchResults { results: Vec<SearchResultItem> },
    SyncStatus { last_sync: Option<String>, status: String },
    Pong,
    Ack,
}

/// Minimal search result for IPC transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub score: f32,
}

/// Push events from daemon to connected clients.
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
```

#### `src/codec.rs` — Length-delimited JSON codec

```rust
use crate::types::IpcMessage;
use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder, LengthDelimitedCodec};

/// Codec that wraps LengthDelimitedCodec to frame JSON-serialized IpcMessages.
/// Wire format: [4-byte big-endian length][JSON payload]
pub struct IpcCodec {
    inner: LengthDelimitedCodec,
}

impl IpcCodec {
    pub fn new() -> Self {
        Self {
            inner: LengthDelimitedCodec::builder()
                .length_field_length(4)
                .max_frame_length(16 * 1024 * 1024) // 16MB max message
                .new_codec(),
        }
    }
}

impl Decoder for IpcCodec {
    type Item = IpcMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.inner.decode(src)? {
            Some(frame) => {
                let msg: IpcMessage = serde_json::from_slice(&frame)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }
}

impl Encoder<IpcMessage> for IpcCodec {
    type Error = std::io::Error;

    fn encode(&mut self, item: IpcMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let json = serde_json::to_vec(&item)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.inner.encode(json.into(), dst)
    }
}
```

#### `src/lib.rs`

```rust
mod types;
mod codec;

pub use types::*;
pub use codec::IpcCodec;
```

### What to test and how

- **Serde roundtrip for all IPC types**: Construct each `Request` variant, wrap in `IpcMessage`, serialize/deserialize, assert equality.
- **Codec encode/decode**: Create an `IpcCodec`, encode an `IpcMessage` into a `BytesMut`, decode it back, assert equality. Test with multiple messages in sequence to verify framing.
- **Error response roundtrip**: Verify `Response::Error { message }` serializes/deserializes correctly.
- **DaemonEvent variants**: Roundtrip each event variant.

```rust
#[test]
fn codec_roundtrip() {
    let mut codec = IpcCodec::new();
    let msg = IpcMessage {
        id: 1,
        payload: IpcPayload::Request(Request::Ping),
    };

    let mut buf = BytesMut::new();
    codec.encode(msg.clone(), &mut buf).unwrap();

    let decoded = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(decoded.id, 1);
    // assert payload matches
}
```

---

## Step 5: mxr-provider-fake

### Crate/module affected

`crates/providers/fake/`

### Files to create/modify

```
crates/providers/fake/Cargo.toml
crates/providers/fake/src/lib.rs
crates/providers/fake/src/fixtures.rs
```

### External crate dependencies

```toml
[dependencies]
mxr-core = { path = "../../core" }
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v7"] }
```

### Key code patterns and struct signatures

#### `src/lib.rs` — FakeProvider

```rust
use async_trait::async_trait;
use mxr_core::*;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct FakeProvider {
    account_id: AccountId,
    messages: Vec<Envelope>,
    bodies: HashMap<String, MessageBody>,   // keyed by provider_id
    labels: Vec<Label>,
    sent: Mutex<Vec<Draft>>,
    mutations: Mutex<Vec<Mutation>>,
}

#[derive(Debug, Clone)]
pub enum Mutation {
    LabelsModified {
        provider_id: String,
        added: Vec<String>,
        removed: Vec<String>,
    },
    Trashed {
        provider_id: String,
    },
    ReadSet {
        provider_id: String,
        read: bool,
    },
    StarredSet {
        provider_id: String,
        starred: bool,
    },
}

impl FakeProvider {
    pub fn new(account_id: AccountId) -> Self {
        let (messages, bodies, labels) = crate::fixtures::generate_fixtures(&account_id);
        Self {
            account_id,
            messages,
            bodies,
            labels,
            sent: Mutex::new(Vec::new()),
            mutations: Mutex::new(Vec::new()),
        }
    }

    /// Inspect sent drafts (for test assertions).
    pub fn sent_drafts(&self) -> Vec<Draft> {
        self.sent.lock().unwrap().clone()
    }

    /// Inspect mutations (for test assertions).
    pub fn mutations(&self) -> Vec<Mutation> {
        self.mutations.lock().unwrap().clone()
    }
}

#[async_trait]
impl MailSyncProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities {
            labels: true,
            server_search: false,
            delta_sync: false,
            push: false,
            batch_operations: false,
        }
    }

    async fn authenticate(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn refresh_auth(&mut self) -> Result<(), MxrError> {
        Ok(())
    }

    async fn sync_labels(&self) -> Result<Vec<Label>, MxrError> {
        Ok(self.labels.clone())
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch, MxrError> {
        // On Initial cursor: return all messages.
        // On subsequent calls: return empty batch (no deltas).
        match cursor {
            SyncCursor::Initial => Ok(SyncBatch {
                upserted: self.messages.clone(),
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: SyncCursor::Gmail { history_id: 1 },
            }),
            _ => Ok(SyncBatch {
                upserted: vec![],
                deleted_provider_ids: vec![],
                label_changes: vec![],
                next_cursor: cursor.clone(),
            }),
        }
    }

    async fn fetch_body(&self, provider_message_id: &str) -> Result<MessageBody, MxrError> {
        self.bodies
            .get(provider_message_id)
            .cloned()
            .ok_or_else(|| MxrError::NotFound(format!("Body not found: {}", provider_message_id)))
    }

    async fn fetch_attachment(
        &self,
        _provider_message_id: &str,
        _provider_attachment_id: &str,
    ) -> Result<Vec<u8>, MxrError> {
        Ok(b"fake attachment content".to_vec())
    }

    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<(), MxrError> {
        self.mutations.lock().unwrap().push(Mutation::LabelsModified {
            provider_id: provider_message_id.to_string(),
            added: add.to_vec(),
            removed: remove.to_vec(),
        });
        Ok(())
    }

    async fn trash(&self, provider_message_id: &str) -> Result<(), MxrError> {
        self.mutations.lock().unwrap().push(Mutation::Trashed {
            provider_id: provider_message_id.to_string(),
        });
        Ok(())
    }

    async fn set_read(&self, provider_message_id: &str, read: bool) -> Result<(), MxrError> {
        self.mutations.lock().unwrap().push(Mutation::ReadSet {
            provider_id: provider_message_id.to_string(),
            read,
        });
        Ok(())
    }

    async fn set_starred(&self, provider_message_id: &str, starred: bool) -> Result<(), MxrError> {
        self.mutations.lock().unwrap().push(Mutation::StarredSet {
            provider_id: provider_message_id.to_string(),
            starred,
        });
        Ok(())
    }
}

#[async_trait]
impl MailSendProvider for FakeProvider {
    fn name(&self) -> &str {
        "fake"
    }

    async fn send(&self, draft: &Draft, _from: &Address) -> Result<SendReceipt, MxrError> {
        self.sent.lock().unwrap().push(draft.clone());
        Ok(SendReceipt {
            provider_message_id: Some(format!("fake-sent-{}", uuid::Uuid::now_v7())),
            sent_at: chrono::Utc::now(),
        })
    }
}
```

#### `src/fixtures.rs` — Fixture data generation

Generate 55 messages across 12 threads and 8 labels (5 system + 3 user). The function signature:

```rust
use chrono::{Duration, Utc};
use mxr_core::*;
use std::collections::HashMap;

/// Returns (envelopes, bodies_by_provider_id, labels).
pub fn generate_fixtures(
    account_id: &AccountId,
) -> (Vec<Envelope>, HashMap<String, MessageBody>, Vec<Label>) {
    let mut envelopes = Vec::new();
    let mut bodies = HashMap::new();
    let now = Utc::now();

    // -- Labels ---------------------------------------------------------------
    let inbox_label = Label {
        id: LabelId::new(),
        account_id: account_id.clone(),
        name: "Inbox".to_string(),
        kind: LabelKind::System,
        color: None,
        provider_id: "INBOX".to_string(),
        unread_count: 0,
        total_count: 0,
    };
    let sent_label = Label { /* ... kind: System, provider_id: "SENT" */ };
    let trash_label = Label { /* ... kind: System, provider_id: "TRASH" */ };
    let spam_label = Label { /* ... kind: System, provider_id: "SPAM" */ };
    let starred_label = Label { /* ... kind: System, provider_id: "STARRED" */ };
    let work_label = Label { /* ... kind: User, provider_id: "work" */ };
    let personal_label = Label { /* ... kind: User, provider_id: "personal" */ };
    let newsletters_label = Label { /* ... kind: User, provider_id: "newsletters" */ };

    let labels = vec![
        inbox_label, sent_label, trash_label, spam_label,
        starred_label, work_label, personal_label, newsletters_label,
    ];

    // -- Threads and messages -------------------------------------------------
    // Thread 1: Deployment discussion (4 messages, work)
    // Thread 2: Q1 Report (3 messages, work)
    // Thread 3: Rust newsletter (1 message, newsletters, with UnsubscribeMethod::OneClick)
    // Thread 4: Invoice thread (2 messages, work, has_attachments)
    // Thread 5: Meeting notes (3 messages, work)
    // Thread 6: Personal travel plans (2 messages, personal)
    // Thread 7: PR review #487 (5 messages, work, starred)
    // Thread 8: Weekly digest (1 message, newsletters, UnsubscribeMethod::HttpLink)
    // Thread 9: Conference invite (2 messages, personal)
    // Thread 10: CI/CD pipeline alert (3 messages, work)
    // Thread 11: Newsletter from HN (1 message, newsletters, UnsubscribeMethod::Mailto)
    // Thread 12: Misc single messages to fill to 55 total

    // Each message gets:
    // - Unique MessageId and provider_id (format: "fake-msg-{n}")
    // - Dates spread over 30 days (now - Duration::days(30) .. now)
    // - Mix of READ/unread, 5 STARRED, 3 with has_attachments=true
    // - Realistic from addresses and subjects
    // - A corresponding MessageBody in the bodies map

    // Example message construction:
    let thread1_id = ThreadId::new();
    let msg1 = Envelope {
        id: MessageId::new(),
        account_id: account_id.clone(),
        provider_id: "fake-msg-1".to_string(),
        thread_id: thread1_id.clone(),
        message_id_header: Some("<deploy-1@work.com>".to_string()),
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: Some("Alice Chen".to_string()),
            email: "alice@work.com".to_string(),
        },
        to: vec![Address {
            name: Some("Team".to_string()),
            email: "team@work.com".to_string(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Deployment plan for v2.3".to_string(),
        date: now - Duration::days(2),
        flags: MessageFlags::READ | MessageFlags::STARRED,
        snippet: "Here's the rollback strategy for v2.3 deployment...".to_string(),
        has_attachments: false,
        size_bytes: 4200,
        unsubscribe: UnsubscribeMethod::None,
    };
    envelopes.push(msg1);

    bodies.insert(
        "fake-msg-1".to_string(),
        MessageBody {
            message_id: envelopes.last().unwrap().id.clone(),
            text_plain: Some("Here's the rollback strategy for v2.3 deployment.\n\n1. Canary to 5%\n2. Monitor for 30min\n3. Full rollout\n\nRollback trigger: >1% error rate.".to_string()),
            text_html: None,
            attachments: vec![],
            fetched_at: now,
        },
    );

    // ... Continue for all 55 messages across 12 threads ...
    // (Full generation logic with realistic data)

    (envelopes, bodies, labels)
}
```

**Fixture data requirements (implement all of these)**:

| Thread | Subject pattern | Messages | Labels | Flags | Attachments | Unsubscribe |
|--------|----------------|----------|--------|-------|-------------|-------------|
| 1 | Deployment plan v2.3 | 4 | work | 2 read, 1 starred | No | None |
| 2 | Q1 Report review | 3 | work | All read | 1 (report.pdf) | None |
| 3 | This Week in Rust #580 | 1 | newsletters | Unread | No | OneClick |
| 4 | Invoice #2847 | 2 | work | 1 read | 1 (invoice.pdf) | None |
| 5 | Team standup notes | 3 | work | All read | No | None |
| 6 | Summer trip planning | 2 | personal | 1 read | No | None |
| 7 | PR review: fix auth | 5 | work | 3 read, starred | No | None |
| 8 | HN Weekly Digest | 1 | newsletters | Unread | No | HttpLink |
| 9 | RustConf 2026 invite | 2 | personal | Unread | 1 (ticket.pdf) | None |
| 10 | CI pipeline failures | 3 | work | 1 read | No | None |
| 11 | Changelog newsletter | 1 | newsletters | Unread | No | Mailto |
| 12 | (fill remaining ~28) | 28 | mixed | mixed | No | mixed |

### What to test and how

- **Fixture count**: Assert `generate_fixtures` returns exactly 55 envelopes and 8 labels.
- **Sync initial**: Call `sync_messages(SyncCursor::Initial)`, verify batch contains all 55 messages.
- **Sync delta**: Call `sync_messages` with non-Initial cursor, verify empty batch.
- **Fetch body**: Call `fetch_body` with valid provider_id, verify body returned.
- **Mutations recorded**: Call `trash`, `set_read`, `modify_labels`, verify `mutations()` contains expected entries.
- **Send recorded**: Call `send`, verify `sent_drafts()` contains the draft.

---

## Step 6: mxr-sync

### Crate/module affected

`crates/sync/`

### Files to create/modify

```
crates/sync/Cargo.toml
crates/sync/src/lib.rs
crates/sync/src/engine.rs
```

### External crate dependencies

```toml
[dependencies]
mxr-core = { path = "../core" }
mxr-store = { path = "../store" }
mxr-search = { path = "../search" }
tokio = { version = "1", features = ["full"] }
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
```

### Key code patterns and struct signatures

#### `src/engine.rs` — SyncEngine

```rust
use mxr_core::*;
use mxr_search::SearchIndex;
use mxr_store::Store;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SyncEngine {
    store: Arc<Store>,
    search: Arc<Mutex<SearchIndex>>,
}

impl SyncEngine {
    pub fn new(store: Arc<Store>, search: Arc<Mutex<SearchIndex>>) -> Self {
        Self { store, search }
    }

    /// Run a full sync cycle for one account.
    /// 1. Read cursor from store (or default to Initial)
    /// 2. Call provider.sync_labels() -> upsert labels
    /// 3. Call provider.sync_messages(cursor) -> get SyncBatch
    /// 4. Apply batch: upsert envelopes, delete removed, apply label changes
    /// 5. Index new/updated envelopes in Tantivy
    /// 6. Update cursor in store
    /// 7. Log sync result
    pub async fn sync_account(
        &self,
        provider: &dyn MailSyncProvider,
    ) -> Result<u32, MxrError> {
        // 1. Get current cursor
        let account_id = provider.account_id();
        let cursor = self.store.get_sync_cursor(account_id).await
            .unwrap_or(SyncCursor::Initial);

        // 2. Sync labels
        let labels = provider.sync_labels().await?;
        for label in &labels {
            self.store.upsert_label(label).await
                .map_err(|e| MxrError::Store(e.to_string()))?;
        }

        // 3. Sync messages
        let batch = provider.sync_messages(&cursor).await?;
        let synced_count = batch.upserted.len() as u32;

        // 4. Apply upserts
        {
            let mut search = self.search.lock().await;
            for envelope in &batch.upserted {
                self.store.upsert_envelope(envelope).await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                search.index_envelope(envelope)?;
            }

            // Apply deletes
            if !batch.deleted_provider_ids.is_empty() {
                self.store
                    .delete_messages_by_provider_ids(account_id, &batch.deleted_provider_ids)
                    .await
                    .map_err(|e| MxrError::Store(e.to_string()))?;
                // Note: Tantivy delete by provider_id requires lookup;
                // for Phase 0 this is acceptable as FakeProvider never deletes
            }

            search.commit()?;
        }

        // 5. Update cursor
        self.store.set_sync_cursor(account_id, &batch.next_cursor).await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        Ok(synced_count)
    }

    /// Fetch body on demand. Check cache first, fetch from provider if miss.
    pub async fn fetch_body(
        &self,
        provider: &dyn MailSyncProvider,
        message_id: &MessageId,
    ) -> Result<MessageBody, MxrError> {
        // Check cache
        if let Some(body) = self.store.get_body(message_id).await
            .map_err(|e| MxrError::Store(e.to_string()))? {
            return Ok(body);
        }

        // Cache miss: fetch from provider
        let envelope = self.store.get_envelope(message_id).await
            .map_err(|e| MxrError::Store(e.to_string()))?
            .ok_or_else(|| MxrError::NotFound(format!("Message {}", message_id)))?;

        let body = provider.fetch_body(&envelope.provider_id).await?;

        // Cache in store
        self.store.insert_body(&body).await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        // Update search index with body text
        {
            let mut search = self.search.lock().await;
            search.index_body(&envelope, &body)?;
            search.commit()?;
        }

        Ok(body)
    }

    /// Check for due snoozes and restore them.
    pub async fn check_snoozes(&self) -> Result<Vec<MessageId>, MxrError> {
        let now = chrono::Utc::now();
        let due = self.store.get_due_snoozes(now).await
            .map_err(|e| MxrError::Store(e.to_string()))?;

        let mut woken = Vec::new();
        for snoozed in &due {
            // Restore labels (in Phase 0, just remove the snooze record)
            self.store.remove_snooze(&snoozed.message_id).await
                .map_err(|e| MxrError::Store(e.to_string()))?;
            woken.push(snoozed.message_id.clone());
        }

        Ok(woken)
    }
}
```

#### `src/lib.rs`

```rust
mod engine;
pub use engine::SyncEngine;
```

### What to test and how

- **Sync with FakeProvider**: Create in-memory Store + SearchIndex + FakeProvider. Call `sync_account`. Verify store has 55 envelopes, search index returns results.
- **Body caching**: After sync, call `fetch_body` for a message. Verify body returned. Call again, verify cached (no second provider call — check via FakeProvider mutation log which won't record body fetches, but verify store has the body).
- **Snooze wake**: Insert a snooze with `wake_at` in the past. Call `check_snoozes`. Verify snooze record removed.

```rust
#[tokio::test]
async fn sync_populates_store_and_search() {
    let store = Arc::new(Store::in_memory().await.unwrap());
    let search = Arc::new(Mutex::new(SearchIndex::in_memory().unwrap()));
    let engine = SyncEngine::new(store.clone(), search.clone());

    let account_id = AccountId::new();
    // Insert account into store first
    store.insert_account(&test_account(account_id.clone())).await.unwrap();

    let provider = FakeProvider::new(account_id);
    let count = engine.sync_account(&provider).await.unwrap();
    assert_eq!(count, 55);

    // Verify store
    let envelopes = store.list_envelopes_by_account(&provider.account_id(), 100, 0).await.unwrap();
    assert_eq!(envelopes.len(), 55);

    // Verify search
    let results = search.lock().await.search("deployment", 10).unwrap();
    assert!(!results.is_empty());
}
```

---

## Step 7: mxr-daemon (produces the `mxr` binary)

### Crate/module affected

`crates/daemon/`

### Files to create/modify

```
crates/daemon/Cargo.toml
crates/daemon/src/main.rs
crates/daemon/src/state.rs
crates/daemon/src/server.rs
crates/daemon/src/handler.rs
crates/daemon/src/loops.rs
```

### External crate dependencies

```toml
[dependencies]
mxr-core = { path = "../core" }
mxr-store = { path = "../store" }
mxr-search = { path = "../search" }
mxr-sync = { path = "../sync" }
mxr-protocol = { path = "../protocol" }
mxr-provider-fake = { path = "../providers/fake" }
mxr-tui = { path = "../tui" }

tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["codec"] }
futures = "0.3"
clap = { version = "4", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "registry"] }
dirs = "6"
```

### Key code patterns and struct signatures

#### `src/main.rs` — Unified binary with clap subcommands

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mxr", about = "Terminal email client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the daemon explicitly
    Daemon {
        /// Run in foreground (for debugging / systemd)
        #[arg(long)]
        foreground: bool,
    },
    /// Trigger sync
    Sync {
        #[arg(long)]
        account: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing based on mode (A006: daemon observability)
    let is_foreground = matches!(cli.command, Some(Commands::Daemon { foreground: true }));
    init_tracing(is_foreground)?;

    match cli.command {
        Some(Commands::Daemon { foreground }) => {
            // Start daemon (foreground flag already used for tracing init above)
            crate::server::run_daemon().await?;
        }
        Some(Commands::Sync { account }) => {
            // Connect to daemon, send SyncNow request
            todo!("CLI sync command");
        }
        None => {
            // Default: start TUI (auto-start daemon if needed)
            crate::server::ensure_daemon_running().await?;
            mxr_tui::run().await?;
        }
    }

    Ok(())
}

/// Initialize tracing-subscriber (A006: daemon observability).
///
/// - Always logs to file: `$XDG_DATA_HOME/mxr/logs/mxr.log`
/// - In foreground mode (`--foreground`): also logs to stdout
/// - Log level controlled by `RUST_LOG` env or defaults to `mxr=info`
fn init_tracing(foreground: bool) -> anyhow::Result<()> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::fmt;
    use tracing_subscriber::EnvFilter;

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "mxr=info".into());

    let log_dir = AppState::data_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("mxr.log"))?;

    let file_layer = fmt::layer()
        .with_writer(file)
        .with_ansi(false);

    if foreground {
        let stdout_layer = fmt::layer()
            .with_writer(std::io::stdout);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .with(stdout_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .init();
    }

    Ok(())
}
```

#### `src/state.rs` — Shared daemon state

```rust
use mxr_core::*;
use mxr_protocol::*;
use mxr_search::SearchIndex;
use mxr_store::Store;
use mxr_sync::SyncEngine;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

pub struct AppState {
    pub store: Arc<Store>,
    pub search: Arc<Mutex<SearchIndex>>,
    pub sync_engine: Arc<SyncEngine>,
    pub provider: Arc<dyn MailSyncProvider>,
    /// Broadcast channel for push events to connected clients.
    pub event_tx: broadcast::Sender<IpcMessage>,
}

impl AppState {
    pub async fn new() -> anyhow::Result<Self> {
        let data_dir = Self::data_dir();
        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("mxr.db");
        let index_path = data_dir.join("search_index");
        std::fs::create_dir_all(&index_path)?;

        let store = Arc::new(Store::new(&db_path).await?);
        let search = Arc::new(Mutex::new(SearchIndex::open(&index_path)?));
        let sync_engine = Arc::new(SyncEngine::new(store.clone(), search.clone()));

        // Phase 0: use FakeProvider
        let account_id = AccountId::new();
        let account = Account {
            id: account_id.clone(),
            name: "Fake Account".to_string(),
            email: "user@example.com".to_string(),
            sync_backend: Some(BackendRef {
                provider_kind: ProviderKind::Fake,
                config_key: "fake".to_string(),
            }),
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await?;

        let provider: Arc<dyn MailSyncProvider> =
            Arc::new(mxr_provider_fake::FakeProvider::new(account_id));

        let (event_tx, _) = broadcast::channel(256);

        Ok(Self {
            store,
            search,
            sync_engine,
            provider,
            event_tx,
        })
    }

    fn data_dir() -> std::path::PathBuf {
        if cfg!(target_os = "macos") {
            dirs::home_dir()
                .unwrap()
                .join("Library/Application Support/mxr")
        } else {
            dirs::data_dir().unwrap().join("mxr")
        }
    }

    pub fn socket_path() -> std::path::PathBuf {
        if cfg!(target_os = "macos") {
            dirs::home_dir()
                .unwrap()
                .join("Library/Application Support/mxr/mxr.sock")
        } else {
            dirs::runtime_dir()
                .or_else(|| Some(std::path::PathBuf::from("/tmp")))
                .unwrap()
                .join("mxr/mxr.sock")
        }
    }
}
```

#### `src/server.rs` — Unix socket server

```rust
use crate::handler::handle_request;
use crate::loops;
use crate::state::AppState;
use mxr_protocol::*;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio_util::codec::Framed;
use futures::{SinkExt, StreamExt};

pub async fn run_daemon() -> anyhow::Result<()> {
    let state = Arc::new(AppState::new().await?);

    // Remove stale socket
    let sock_path = AppState::socket_path();
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(&sock_path);

    let listener = UnixListener::bind(&sock_path)?;
    tracing::info!("Daemon listening on {}", sock_path.display());

    // Initial sync with FakeProvider
    {
        let count = state.sync_engine.sync_account(state.provider.as_ref()).await?;
        tracing::info!("Initial sync complete: {} messages", count);
    }

    // Spawn background loops
    let sync_state = state.clone();
    tokio::spawn(async move {
        loops::sync_loop(sync_state).await;
    });

    let snooze_state = state.clone();
    tokio::spawn(async move {
        loops::snooze_loop(snooze_state).await;
    });

    // Accept connections
    loop {
        let (stream, _addr) = listener.accept().await?;
        let state = state.clone();
        let mut event_rx = state.event_tx.subscribe();

        tokio::spawn(async move {
            let mut framed = Framed::new(stream, IpcCodec::new());

            loop {
                tokio::select! {
                    // Handle incoming requests
                    msg = framed.next() => {
                        match msg {
                            Some(Ok(ipc_msg)) => {
                                let response = handle_request(&state, &ipc_msg).await;
                                if framed.send(response).await.is_err() {
                                    break;
                                }
                            }
                            Some(Err(e)) => {
                                tracing::error!("IPC decode error: {}", e);
                                break;
                            }
                            None => break, // Client disconnected
                        }
                    }
                    // Forward push events
                    event = event_rx.recv() => {
                        if let Ok(event_msg) = event {
                            if framed.send(event_msg).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }

            tracing::debug!("Client disconnected");
        });
    }
}

/// Check if daemon is running. If not, start it as a background process.
pub async fn ensure_daemon_running() -> anyhow::Result<()> {
    let sock_path = AppState::socket_path();

    // Try to connect
    if tokio::net::UnixStream::connect(&sock_path).await.is_ok() {
        return Ok(()); // Already running
    }

    // Start daemon as background process
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("daemon")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    // Wait for daemon to be ready (retry with backoff)
    for i in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(100 * (i + 1))).await;
        if tokio::net::UnixStream::connect(&sock_path).await.is_ok() {
            return Ok(());
        }
    }

    anyhow::bail!("Failed to start daemon")
}
```

#### `src/handler.rs` — Request dispatch

```rust
use crate::state::AppState;
use mxr_protocol::*;
use std::sync::Arc;

pub async fn handle_request(state: &Arc<AppState>, msg: &IpcMessage) -> IpcMessage {
    let response_data = match &msg.payload {
        IpcPayload::Request(req) => dispatch(state, req).await,
        _ => Response::Error {
            message: "Expected a Request".to_string(),
        },
    };

    IpcMessage {
        id: msg.id,
        payload: IpcPayload::Response(response_data),
    }
}

async fn dispatch(state: &Arc<AppState>, req: &Request) -> Response {
    match req {
        Request::ListEnvelopes {
            label_id,
            account_id,
            limit,
            offset,
        } => {
            match state.store.list_envelopes_by_account(
                state.provider.account_id(),
                *limit,
                *offset,
            ).await {
                Ok(envelopes) => Response::Ok {
                    data: ResponseData::Envelopes { envelopes },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetEnvelope { message_id } => {
            match state.store.get_envelope(message_id).await {
                Ok(Some(envelope)) => Response::Ok {
                    data: ResponseData::Envelope { envelope },
                },
                Ok(None) => Response::Error {
                    message: "Not found".to_string(),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetBody { message_id } => {
            match state.sync_engine.fetch_body(
                state.provider.as_ref(),
                message_id,
            ).await {
                Ok(body) => Response::Ok {
                    data: ResponseData::Body { body },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetThread { thread_id } => {
            // Aggregate thread from store
            match state.store.get_thread(thread_id).await {
                Ok(Some(thread)) => {
                    let messages = state.store.get_thread_envelopes(thread_id).await
                        .unwrap_or_default();
                    Response::Ok {
                        data: ResponseData::Thread { thread, messages },
                    }
                }
                Ok(None) => Response::Error {
                    message: "Thread not found".to_string(),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::ListLabels { account_id } => {
            let aid = account_id.as_ref()
                .unwrap_or(state.provider.account_id());
            match state.store.list_labels_by_account(aid).await {
                Ok(labels) => Response::Ok {
                    data: ResponseData::Labels { labels },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Search { query, limit } => {
            let search = state.search.lock().await;
            match search.search(query, *limit as usize) {
                Ok(results) => {
                    let items: Vec<SearchResultItem> = results
                        .into_iter()
                        .map(|r| SearchResultItem {
                            message_id: r.message_id.parse().unwrap_or_default(),
                            account_id: r.account_id.parse().unwrap_or_default(),
                            thread_id: r.thread_id.parse().unwrap_or_default(),
                            score: r.score,
                        })
                        .collect();
                    Response::Ok {
                        data: ResponseData::SearchResults { results: items },
                    }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::SyncNow { .. } => {
            match state.sync_engine.sync_account(state.provider.as_ref()).await {
                Ok(_) => Response::Ok {
                    data: ResponseData::Ack,
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Ping => Response::Ok {
            data: ResponseData::Pong,
        },

        Request::Shutdown => {
            // Graceful shutdown: in Phase 0, just exit
            std::process::exit(0);
        }

        _ => Response::Error {
            message: "Not implemented yet".to_string(),
        },
    }
}
```

#### `src/loops.rs` — Background loops

```rust
use crate::state::AppState;
use mxr_protocol::*;
use std::sync::Arc;
use tokio::time::{interval, Duration};

pub async fn sync_loop(state: Arc<AppState>) {
    let mut ticker = interval(Duration::from_secs(60));
    loop {
        ticker.tick().await;
        match state.sync_engine.sync_account(state.provider.as_ref()).await {
            Ok(count) => {
                if count > 0 {
                    tracing::info!("Sync completed: {} messages", count);
                    let event = IpcMessage {
                        id: 0, // Events use id=0
                        payload: IpcPayload::Event(DaemonEvent::SyncCompleted {
                            account_id: state.provider.account_id().clone(),
                            messages_synced: count,
                        }),
                    };
                    let _ = state.event_tx.send(event);
                }
            }
            Err(e) => {
                tracing::error!("Sync error: {}", e);
                let event = IpcMessage {
                    id: 0,
                    payload: IpcPayload::Event(DaemonEvent::SyncError {
                        account_id: state.provider.account_id().clone(),
                        error: e.to_string(),
                    }),
                };
                let _ = state.event_tx.send(event);
            }
        }
    }
}

pub async fn snooze_loop(state: Arc<AppState>) {
    let mut ticker = interval(Duration::from_secs(60));
    loop {
        ticker.tick().await;
        match state.sync_engine.check_snoozes().await {
            Ok(woken) => {
                for message_id in woken {
                    let event = IpcMessage {
                        id: 0,
                        payload: IpcPayload::Event(DaemonEvent::MessageUnsnoozed {
                            message_id,
                        }),
                    };
                    let _ = state.event_tx.send(event);
                }
            }
            Err(e) => {
                tracing::error!("Snooze check error: {}", e);
            }
        }
    }
}
```

### What to test and how

- **Handler dispatch**: Unit test `dispatch()` with mock `AppState` using in-memory Store/Search. Send `Ping`, verify `Pong`. Send `ListEnvelopes`, verify envelopes returned after sync.
- **Server integration**: Start daemon in a test, connect a `UnixStream`, send a `Ping` request via `IpcCodec`, verify `Pong` response. (Use a temp socket path.)
- **Sync loop**: Verify sync_loop calls sync_account on tick (test with shorter interval).

Note: Full integration tests live in `tests/` at workspace root, not inside the daemon crate.

---

## Step 8: mxr-tui (library crate)

### Crate/module affected

`crates/tui/`

### Files to create/modify

```
crates/tui/Cargo.toml
crates/tui/src/lib.rs
crates/tui/src/app.rs
crates/tui/src/client.rs
crates/tui/src/input.rs
crates/tui/src/action.rs
crates/tui/src/ui/mod.rs
crates/tui/src/ui/sidebar.rs
crates/tui/src/ui/mail_list.rs
crates/tui/src/ui/status_bar.rs
```

### External crate dependencies

```toml
[dependencies]
mxr-core = { path = "../core" }
mxr-protocol = { path = "../protocol" }
ratatui = "0.29"
crossterm = { version = "0.28", features = ["event-stream"] }
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["codec"] }
futures = "0.3"
chrono = { version = "0.4", features = ["serde"] }
```

### Key code patterns and struct signatures

#### `src/action.rs` — Phase 0 action enum

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    MoveDown,
    MoveUp,
    JumpTop,
    JumpBottom,
    PageDown,
    PageUp,
    ViewportTop,     // H — top of visible area
    ViewportMiddle,  // M — middle of visible area
    ViewportBottom,  // L — bottom of visible area
    CenterCurrent,   // zz — center current item
    SwitchPane,
    OpenSelected,    // Enter / o
    Back,            // Escape — back / close / cancel
    QuitView,        // q — quit current view
}
```

#### `src/input.rs` — Key state machine

```rust
use crate::action::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

const MULTI_KEY_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub enum KeyState {
    Normal,
    WaitingForSecond { first: char, deadline: Instant },
}

pub struct InputHandler {
    state: KeyState,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            state: KeyState::Normal,
        }
    }

    /// Returns true if the input handler is waiting for a second key.
    pub fn is_pending(&self) -> bool {
        matches!(self.state, KeyState::WaitingForSecond { .. })
    }

    /// Check if the pending multi-key sequence has timed out.
    /// Call this before processing a new key event.
    pub fn check_timeout(&mut self) -> Option<Action> {
        if let KeyState::WaitingForSecond { deadline, .. } = &self.state {
            if Instant::now() > *deadline {
                self.state = KeyState::Normal;
                // 'g' alone does nothing in Phase 0
                return None;
            }
        }
        None
    }

    /// Process a key event and return an optional action.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        // Check timeout first
        self.check_timeout();

        match (&self.state, key.code, key.modifiers) {
            // -- Multi-key: gg ------------------------------------------------
            (KeyState::Normal, KeyCode::Char('g'), KeyModifiers::NONE) => {
                self.state = KeyState::WaitingForSecond {
                    first: 'g',
                    deadline: Instant::now() + MULTI_KEY_TIMEOUT,
                };
                None
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('g'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::JumpTop)
            }

            // -- Multi-key: zz ------------------------------------------------
            (KeyState::Normal, KeyCode::Char('z'), KeyModifiers::NONE) => {
                self.state = KeyState::WaitingForSecond {
                    first: 'z',
                    deadline: Instant::now() + MULTI_KEY_TIMEOUT,
                };
                None
            }
            (
                KeyState::WaitingForSecond { first: 'z', .. },
                KeyCode::Char('z'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::CenterCurrent)
            }

            (KeyState::WaitingForSecond { .. }, _, _) => {
                // Different key or timeout: cancel pending, handle new key
                self.state = KeyState::Normal;
                self.handle_key(key) // Recurse once to handle the new key
            }

            // -- Single keys --------------------------------------------------
            (KeyState::Normal, KeyCode::Char('j') | KeyCode::Down, _) => {
                Some(Action::MoveDown)
            }
            (KeyState::Normal, KeyCode::Char('k') | KeyCode::Up, _) => {
                Some(Action::MoveUp)
            }
            (KeyState::Normal, KeyCode::Char('G'), KeyModifiers::SHIFT) => {
                Some(Action::JumpBottom)
            }
            (KeyState::Normal, KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                Some(Action::PageDown)
            }
            (KeyState::Normal, KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                Some(Action::PageUp)
            }
            // -- Viewport positioning (vim H/M/L) ----------------------------
            (KeyState::Normal, KeyCode::Char('H'), KeyModifiers::SHIFT) => {
                Some(Action::ViewportTop)
            }
            (KeyState::Normal, KeyCode::Char('M'), KeyModifiers::SHIFT) => {
                Some(Action::ViewportMiddle)
            }
            (KeyState::Normal, KeyCode::Char('L'), KeyModifiers::SHIFT) => {
                Some(Action::ViewportBottom)
            }
            // -- Open / navigate / quit (A005) --------------------------------
            (KeyState::Normal, KeyCode::Tab, _) => Some(Action::SwitchPane),
            (KeyState::Normal, KeyCode::Enter, _)
            | (KeyState::Normal, KeyCode::Char('o'), KeyModifiers::NONE) => {
                Some(Action::OpenSelected)
            }
            (KeyState::Normal, KeyCode::Esc, _) => Some(Action::Back),
            (KeyState::Normal, KeyCode::Char('q'), _) => Some(Action::QuitView),

            _ => None,
        }
    }
}
```

#### `src/client.rs` — IPC client

```rust
use mxr_protocol::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::net::UnixStream;
use tokio_util::codec::Framed;
use futures::{SinkExt, StreamExt};

pub struct Client {
    framed: Framed<UnixStream, IpcCodec>,
    next_id: AtomicU64,
}

impl Client {
    pub async fn connect(socket_path: &Path) -> std::io::Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        Ok(Self {
            framed: Framed::new(stream, IpcCodec::new()),
            next_id: AtomicU64::new(1),
        })
    }

    /// Send a request and wait for the matching response.
    async fn request(&mut self, req: Request) -> Result<Response, MxrError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let msg = IpcMessage {
            id,
            payload: IpcPayload::Request(req),
        };
        self.framed
            .send(msg)
            .await
            .map_err(|e| MxrError::Ipc(e.to_string()))?;

        // Read response (may receive events in between; buffer them)
        loop {
            match self.framed.next().await {
                Some(Ok(resp_msg)) => {
                    if resp_msg.id == id {
                        match resp_msg.payload {
                            IpcPayload::Response(resp) => return Ok(resp),
                            _ => continue,
                        }
                    }
                    // TODO: buffer events for processing
                }
                Some(Err(e)) => return Err(MxrError::Ipc(e.to_string())),
                None => return Err(MxrError::Ipc("Connection closed".into())),
            }
        }
    }

    /// Convenience: list envelopes.
    pub async fn list_envelopes(
        &mut self,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Envelope>, MxrError> {
        let resp = self
            .request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit,
                offset,
            })
            .await?;

        match resp {
            Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            } => Ok(envelopes),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    /// Convenience: get body.
    pub async fn get_body(&mut self, message_id: MessageId) -> Result<MessageBody, MxrError> {
        let resp = self.request(Request::GetBody { message_id }).await?;
        match resp {
            Response::Ok {
                data: ResponseData::Body { body },
            } => Ok(body),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    /// Convenience: list labels.
    pub async fn list_labels(&mut self) -> Result<Vec<Label>, MxrError> {
        let resp = self
            .request(Request::ListLabels { account_id: None })
            .await?;
        match resp {
            Response::Ok {
                data: ResponseData::Labels { labels },
            } => Ok(labels),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    /// Convenience: search.
    pub async fn search(
        &mut self,
        query: String,
        limit: u32,
    ) -> Result<Vec<SearchResultItem>, MxrError> {
        let resp = self.request(Request::Search { query, limit }).await?;
        match resp {
            Response::Ok {
                data: ResponseData::SearchResults { results },
            } => Ok(results),
            Response::Error { message } => Err(MxrError::Ipc(message)),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    /// Convenience: ping.
    pub async fn ping(&mut self) -> Result<(), MxrError> {
        let resp = self.request(Request::Ping).await?;
        match resp {
            Response::Ok {
                data: ResponseData::Pong,
            } => Ok(()),
            _ => Err(MxrError::Ipc("Unexpected response".into())),
        }
    }

    /// Get the inner framed stream for event loop integration.
    pub fn into_framed(self) -> Framed<UnixStream, IpcCodec> {
        self.framed
    }
}
```

#### `src/app.rs` — App state and event loop

```rust
use crate::action::Action;
use crate::client::Client;
use crate::input::InputHandler;
use crate::ui;
use mxr_core::*;
use mxr_protocol::*;
use crossterm::event::{Event, EventStream};
use futures::StreamExt;
use ratatui::prelude::*;
use ratatui::DefaultTerminal;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Sidebar,
    MailList,
}

pub struct App {
    pub envelopes: Vec<Envelope>,
    pub labels: Vec<Label>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub active_pane: ActivePane,
    pub should_quit: bool,
    input: InputHandler,
}

impl App {
    pub fn new() -> Self {
        Self {
            envelopes: Vec::new(),
            labels: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            active_pane: ActivePane::MailList,
            should_quit: false,
            input: InputHandler::new(),
        }
    }

    /// Load initial data from daemon.
    pub async fn load(&mut self, client: &mut Client) -> Result<(), MxrError> {
        self.envelopes = client.list_envelopes(100, 0).await?;
        self.labels = client.list_labels().await?;
        Ok(())
    }

    /// Apply an action to app state.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::MoveDown => {
                if self.selected_index + 1 < self.envelopes.len() {
                    self.selected_index += 1;
                }
            }
            Action::MoveUp => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            Action::JumpTop => {
                self.selected_index = 0;
                self.scroll_offset = 0;
            }
            Action::JumpBottom => {
                if !self.envelopes.is_empty() {
                    self.selected_index = self.envelopes.len() - 1;
                }
            }
            Action::PageDown => {
                let page = 20; // half-page
                self.selected_index =
                    (self.selected_index + page).min(self.envelopes.len().saturating_sub(1));
            }
            Action::PageUp => {
                let page = 20;
                self.selected_index = self.selected_index.saturating_sub(page);
            }
            Action::ViewportTop => {
                // Jump to top of visible area
                self.selected_index = self.scroll_offset;
            }
            Action::ViewportMiddle => {
                // Jump to middle of visible area
                let visible_height = 20; // approximation, real value comes from frame area
                self.selected_index = (self.scroll_offset + visible_height / 2)
                    .min(self.envelopes.len().saturating_sub(1));
            }
            Action::ViewportBottom => {
                // Jump to bottom of visible area
                let visible_height = 20;
                self.selected_index = (self.scroll_offset + visible_height)
                    .min(self.envelopes.len().saturating_sub(1));
            }
            Action::CenterCurrent => {
                // Center current item in viewport (adjust scroll_offset)
                let visible_height = 20;
                self.scroll_offset = self.selected_index.saturating_sub(visible_height / 2);
            }
            Action::SwitchPane => {
                self.active_pane = match self.active_pane {
                    ActivePane::Sidebar => ActivePane::MailList,
                    ActivePane::MailList => ActivePane::Sidebar,
                };
            }
            Action::OpenSelected => {
                // Phase 0: no-op (message view not yet implemented)
            }
            Action::Back => {
                // Phase 0: no-op (no view stack to navigate back through)
            }
            Action::QuitView => {
                self.should_quit = true;
            }
        }
    }

    /// Main render function. Delegates to ui submodules.
    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();

        // Two-pane layout: sidebar (20%) + mail list (80%)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(80),
            ])
            .split(area);

        // Reserve bottom row for status bar
        let sidebar_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(chunks[0]);

        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(chunks[1]);

        ui::sidebar::draw(frame, sidebar_chunks[0], &self.labels, &self.active_pane);
        ui::mail_list::draw(
            frame,
            main_chunks[0],
            &self.envelopes,
            self.selected_index,
            self.scroll_offset,
            &self.active_pane,
        );

        // Status bar spans full width
        let status_area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        ui::status_bar::draw(frame, status_area[1], &self.envelopes);
    }
}
```

#### `src/ui/sidebar.rs` — Sidebar widget

```rust
use crate::app::ActivePane;
use mxr_core::Label;
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, labels: &[Label], active_pane: &ActivePane) {
    let is_focused = *active_pane == ActivePane::Sidebar;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let items: Vec<ListItem> = labels
        .iter()
        .map(|label| {
            let count_str = if label.unread_count > 0 {
                format!(" ({})", label.unread_count)
            } else {
                String::new()
            };
            ListItem::new(format!("{}{}", label.name, count_str))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Labels ")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    frame.render_widget(list, area);
}
```

#### `src/ui/mail_list.rs` — Mail list widget

```rust
use crate::app::ActivePane;
use mxr_core::{Envelope, MessageFlags};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    envelopes: &[Envelope],
    selected_index: usize,
    scroll_offset: usize,
    active_pane: &ActivePane,
) {
    let is_focused = *active_pane == ActivePane::MailList;
    let border_style = if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let visible_height = area.height.saturating_sub(2) as usize; // minus border
    let items: Vec<ListItem> = envelopes
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(i, env)| {
            let star = if env.flags.contains(MessageFlags::STARRED) {
                "★"
            } else {
                " "
            };
            let is_unread = !env.flags.contains(MessageFlags::READ);
            let from = env.from.name.as_deref().unwrap_or(&env.from.email);
            let from_truncated: String = from.chars().take(15).collect();
            let date = env.date.format("%b %d").to_string();

            let line = format!(
                " {} {:<15} {:<40} {}",
                star,
                from_truncated,
                truncate(&env.subject, 40),
                date,
            );

            let style = if i == selected_index {
                Style::default().bg(Color::DarkGray).bold()
            } else if is_unread {
                Style::default().bold()
            } else {
                Style::default()
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(" Messages ")
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    frame.render_widget(list, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        format!("{:<width$}", s, width = max)
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
```

#### `src/ui/status_bar.rs` — Status bar

```rust
use mxr_core::{Envelope, MessageFlags};
use ratatui::prelude::*;
use ratatui::widgets::*;

pub fn draw(frame: &mut Frame, area: Rect, envelopes: &[Envelope]) {
    let unread_count = envelopes
        .iter()
        .filter(|e| !e.flags.contains(MessageFlags::READ))
        .count();

    let status = format!(
        " [INBOX] {} unread | {} total",
        unread_count,
        envelopes.len(),
    );

    let bar = Paragraph::new(status)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    frame.render_widget(bar, area);
}
```

#### `src/ui/mod.rs`

```rust
pub mod sidebar;
pub mod mail_list;
pub mod status_bar;
```

#### `src/lib.rs` — Public entry point

```rust
mod app;
mod client;
mod input;
mod action;
mod ui;

use app::App;
use client::Client;
use crossterm::event::{Event, EventStream};
use futures::StreamExt;

pub async fn run() -> anyhow::Result<()> {
    let socket_path = crate::daemon_socket_path();
    let mut client = Client::connect(&socket_path).await?;

    let mut app = App::new();
    app.load(&mut client).await?;

    let mut terminal = ratatui::init();
    let mut events = EventStream::new();

    loop {
        terminal.draw(|frame| app.draw(frame))?;

        // Wait for input with a timeout (for multi-key sequences)
        let timeout = if app.input_pending() {
            std::time::Duration::from_millis(500)
        } else {
            std::time::Duration::from_secs(60)
        };

        tokio::select! {
            event = events.next() => {
                if let Some(Ok(Event::Key(key))) = event {
                    if let Some(action) = app.handle_key(key) {
                        app.apply(action);
                    }
                }
            }
            _ = tokio::time::sleep(timeout) => {
                // Timeout: check for pending multi-key timeout
                app.tick();
            }
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
    Ok(())
}

fn daemon_socket_path() -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .unwrap()
            .join("Library/Application Support/mxr/mxr.sock")
    } else {
        dirs::runtime_dir()
            .or_else(|| Some(std::path::PathBuf::from("/tmp")))
            .unwrap()
            .join("mxr/mxr.sock")
    }
}
```

### What to test and how

- **InputHandler key sequences**: Unit test `handle_key` with simulated KeyEvents. Verify `j` -> `MoveDown`, `k` -> `MoveUp`, `g` then `g` within 500ms -> `JumpTop`, `G` -> `JumpBottom`, `Ctrl-d` -> `PageDown`, `o` -> `OpenSelected`, `Enter` -> `OpenSelected`, `H` -> `ViewportTop`, `M` -> `ViewportMiddle`, `L` -> `ViewportBottom`, `z` then `z` -> `CenterCurrent`, `Escape` -> `Back`, `q` -> `QuitView`.
- **App state transitions**: Create `App`, set envelopes, call `apply(MoveDown)`, verify `selected_index` incremented. Call `apply(JumpTop)`, verify `selected_index == 0`. Verify bounds checking (can't go below 0 or past end).
- **Client round-trip**: Integration test: start daemon in background, connect Client, call `ping()`, call `list_envelopes()`, verify data returned. (Requires running daemon with temp socket.)

```rust
#[test]
fn input_gg_jump_top() {
    let mut handler = InputHandler::new();

    let g1 = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
    assert_eq!(handler.handle_key(g1), None); // Waiting

    let g2 = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
    assert_eq!(handler.handle_key(g2), Some(Action::JumpTop));
}

#[test]
fn app_move_down_bounds() {
    let mut app = App::new();
    app.envelopes = vec![/* 3 envelopes */];
    app.apply(Action::MoveDown);
    assert_eq!(app.selected_index, 1);
    app.apply(Action::MoveDown);
    assert_eq!(app.selected_index, 2);
    app.apply(Action::MoveDown); // At end, should not go past
    assert_eq!(app.selected_index, 2);
}
```

---

## Workspace Cargo.toml

```
Cargo.toml  (workspace root)
```

```toml
[workspace]
resolver = "2"
members = [
    "crates/core",
    "crates/store",
    "crates/search",
    "crates/protocol",
    "crates/providers/fake",
    "crates/sync",
    "crates/daemon",
    "crates/tui",
]

[workspace.package]
edition = "2021"
license = "MIT OR Apache-2.0"
```

---

## Definition of Done

### Verification Steps

1. **`cargo build --workspace`** compiles without errors or warnings.
2. **`cargo test --workspace`** passes all tests (unit + integration).
3. **`cargo clippy --workspace`** produces no warnings.
4. **`cargo fmt --all -- --check`** passes.
5. **Run `mxr daemon --foreground`** in one terminal:
   - Daemon starts, logs to both stdout and `$XDG_DATA_HOME/mxr/logs/mxr.log`
   - Prints "Daemon listening on ..."
   - Prints "Initial sync complete: 55 messages"
   - Does not crash or panic.
   - Log file is created and written to.
6. **Run `mxr daemon`** (background mode):
   - Daemon logs only to file (no stdout output).
7. **In another terminal, run `mxr`** (TUI mode):
   - TUI renders with two-pane layout (sidebar + message list)
   - Sidebar shows 8 labels with names
   - Message list shows 55 messages with star indicator, from name, subject, date
   - Unread messages appear bold
   - `j`/`k` moves selection up/down
   - `gg` jumps to top, `G` jumps to bottom
   - `Ctrl-d`/`Ctrl-u` pages down/up
   - `H`/`M`/`L` jumps to top/middle/bottom of visible area
   - `zz` centers current item in viewport
   - `Enter` or `o` opens selected (no-op in Phase 0, but mapped)
   - `Escape` triggers Back action (no-op in Phase 0, but mapped)
   - `Tab` switches focus between sidebar and message list
   - `q` exits cleanly (terminal restored)
   - Status bar shows `[INBOX] N unread | 55 total`
8. **SQLite schema**: `event_log` table exists in the database after daemon startup.
9. **Search works**: After daemon startup, Tantivy index contains all 55 messages. A search request (via code test or future CLI) for "deployment" returns the deployment thread messages.
10. **IPC works**: Sending a `Ping` to the socket returns `Pong`. Sending `ListEnvelopes` returns envelope data.

---

## Risks and Mitigations

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| sqlx runtime queries have no compile-time safety | Bugs from typos in SQL strings caught only at runtime | Medium | Comprehensive integration tests for every query. Run tests on every change. Consider adding `sqlx::test` attribute for automatic DB setup/teardown. |
| Tantivy API changes between 0.22 and future versions | Breaking changes in schema builder or query API | Low | Pin exact tantivy version in Cargo.toml. Review changelog before upgrading. |
| Two-pool SQLite WAL contention | Writer blocks readers or vice versa under heavy sync | Low (Phase 0 is single-provider, low volume) | WAL mode specifically avoids this. Monitor with `PRAGMA wal_checkpoint` if issues arise. |
| Unix socket path differences across platforms | Socket not found on Linux vs macOS | Medium | Centralize socket path logic in one function (`AppState::socket_path`). Test on both platforms in CI. |
| FakeProvider fixture data insufficient for edge cases | TUI renders incorrectly for empty strings, very long subjects, etc. | Medium | Include edge cases in fixtures: empty subject, very long from name, message with all flags set, message with no flags. |
| Length-delimited codec max frame size | Large responses (many envelopes) exceed 16MB limit | Low | 16MB is generous for Phase 0 (55 messages). Increase if needed. Add pagination to ListEnvelopes. |
| Ratatui rendering across different terminal sizes | Layout breaks on very small terminals | Medium | Test with minimum 80x24 terminal. Add graceful degradation (hide sidebar below certain width). |
| Multi-key timeout feels sluggish | 500ms wait after pressing `g` before giving up | Low | 500ms is standard vim timeout. Document that `gg` requires quick double-tap. Make timeout configurable in future phases. |
| Daemon process management (orphan processes) | Daemon stays running after TUI exits, or multiple daemons start | Medium | Check for stale socket on startup (try connect, remove if dead). Write PID file alongside socket. `mxr doctor` checks for orphan processes. |
| `Store::in_memory()` shares pool for reader/writer | Tests don't exercise the actual two-pool behavior | Low | Acceptable for Phase 0. Add file-based integration tests in Phase 1 that test true two-pool behavior. |
