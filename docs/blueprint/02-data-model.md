# mxr — Data Model

## Design philosophy

The internal model is the most important design decision in mxr. All application logic speaks this language. Gmail and IMAP (and any future provider) map INTO this model. The model never bends to accommodate provider quirks — that's the adapter's job.

### Key principles

1. **Provider-agnostic**: No Gmail-specific or IMAP-specific concepts in the core types.
2. **Correctness over cleverness**: We store enough data to round-trip back to the provider without loss.
3. **Lazy hydration**: Envelopes (headers/metadata) are always cached. Bodies are fetched on demand and cached after first access. This keeps sync fast and storage manageable.
4. **Typed IDs**: Newtypes prevent mixing up account IDs with message IDs at compile time.
5. **Time-sortable IDs**: UUIDv7 gives naturally ordered primary keys.

## Typed IDs

Every entity has a strongly-typed ID to prevent accidental mixing at compile time.

```rust
// Macro generates: struct, new(), from_uuid(), as_str(), Display, Default
macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);
        // ... new() creates UUIDv7, Display delegates to inner Uuid
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

We use UUIDv7 (not v4) because v7 is time-sortable. This means our primary keys are naturally ordered by creation time, which gives efficient range queries and natural ordering without a separate timestamp index.

## Core types

### Address

```rust
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}
```

Simple. Used everywhere: from, to, cc, bcc, thread participants.

### Account

```rust
pub struct Account {
    pub id: AccountId,
    pub name: String,              // Display name ("Work Gmail", "Personal")
    pub email: String,             // Primary email address
    pub sync_backend: Option<BackendRef>,  // Which provider syncs inbox
    pub send_backend: Option<BackendRef>,  // Which provider sends mail
    pub enabled: bool,
}

pub struct BackendRef {
    pub provider_kind: ProviderKind,
    pub config_key: String,  // Reference to config section
}

pub enum ProviderKind {
    Gmail,
    Smtp,
    // Future: Imap, Outlook, Jmap, etc.
}
```

**Critical design decision**: An account has SEPARATE sync and send backends. This is because:

- Gmail can do both sync and send
- SMTP can only send
- A user might use Gmail for inbox sync but their company's SMTP relay for sending
- Future IMAP adapter handles sync while SMTP handles send

We considered a single `provider` field per account but rejected it because it forces every provider to implement both sync and send, which SMTP cannot do. The split model reflects reality.

### Label (the universal organizer)

```rust
pub struct Label {
    pub id: LabelId,
    pub account_id: AccountId,
    pub name: String,
    pub kind: LabelKind,
    pub color: Option<String>,
    pub provider_id: String,     // Remote label/folder ID
    pub unread_count: u32,
    pub total_count: u32,
}

pub enum LabelKind {
    System,   // Built-in: inbox, sent, trash, spam, drafts, starred
    Folder,   // IMAP folder mapped as label
    User,     // User-created labels/tags
}
```

**How labels unify Gmail and IMAP:**

Gmail has labels (multi-assign: a message can have multiple labels). IMAP has folders (single-assign: a message lives in one folder) plus flags (read, starred, etc.).

Our model uses labels as the universal concept:
- Gmail labels → Label { kind: User | System } — maps directly
- IMAP folders → Label { kind: Folder }
- IMAP flags → Label { kind: System } (e.g., \Seen → system label "read")

Messages can have multiple labels. This is the superset model.

**Important nuance**: We initially considered flattening everything into just labels, but the feedback was that this flattens too aggressively. IMAP folder membership has different semantics than Gmail labeling (an IMAP COPY+DELETE has different failure modes than a Gmail label add/remove). So internally we also maintain:

- **labels**: the universal organizer (what the UI shows)
- **mailbox_memberships**: which IMAP mailbox a message belongs to (provider-specific, stored in ProviderMeta)
- **flags**: IMAP flags as distinct from labels (also in ProviderMeta)

The app logic sees labels. The sync engine sees the full picture including mailbox membership and flags through ProviderMeta.

### Message flags (bitfield)

```rust
bitflags! {
    pub struct MessageFlags: u32 {
        const READ       = 0b0000_0001;
        const STARRED    = 0b0000_0010;
        const DRAFT      = 0b0000_0100;
        const SENT       = 0b0000_1000;
        const TRASH      = 0b0001_0000;
        const SPAM       = 0b0010_0000;
        const ARCHIVED   = 0b0100_0000;
        const ANSWERED   = 0b1000_0000;
    }
}
```

Stored as a single integer in SQLite. Fast bitwise checks for common queries like "all unread" or "starred and not archived."

### Envelope (message metadata — always cached)

```rust
pub struct Envelope {
    pub id: MessageId,
    pub account_id: AccountId,
    pub provider_id: String,         // Gmail msg ID or IMAP UID
    pub thread_id: ThreadId,
    pub message_id_header: Option<String>,  // RFC 2822 Message-ID
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,     // RFC 2822 References header
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub flags: MessageFlags,
    pub snippet: String,             // Preview text
    pub has_attachments: bool,
    pub size_bytes: u64,
    pub unsubscribe: UnsubscribeMethod,  // Parsed at sync time
}
```

Envelopes are synced eagerly. Every message in the account has an envelope in SQLite. This is what powers fast list views, search results, and thread summaries. The body is NOT here — it's fetched lazily.

### UnsubscribeMethod

```rust
pub enum UnsubscribeMethod {
    /// RFC 8058 one-click POST. Best case: no browser needed.
    /// The daemon fires the POST request directly.
    OneClick { url: String },
    /// HTTP link. Opens in browser.
    HttpLink { url: String },
    /// Send an email to unsubscribe.
    Mailto { address: String, subject: Option<String> },
    /// Extracted from HTML body (lower confidence than header-based).
    BodyLink { url: String },
    /// No unsubscribe method found.
    None,
}
```

**Where this comes from**: The `List-Unsubscribe` header (RFC 2369) is a standard header that most legitimate newsletters include. It's machine-readable by design. Gmail and Apple Mail already use it for their unsubscribe buttons. We parse it at sync time and store it on the envelope so that the one-key unsubscribe feature (`U`) is instant, not a runtime header scan.

If the header isn't present, we fall back to scanning the HTML body for common unsubscribe link patterns (href containing "unsubscribe", "opt-out", "manage preferences"). This is fuzzier but catches stragglers.

### MessageBody (fetched on demand, cached)

```rust
pub struct MessageBody {
    pub message_id: MessageId,
    pub text_plain: Option<String>,
    pub text_html: Option<String>,
    pub attachments: Vec<AttachmentMeta>,
    pub fetched_at: DateTime<Utc>,
}
```

Bodies are fetched the first time a user opens a message and then cached in SQLite. This means:
- Initial sync is fast (headers only)
- Storage grows incrementally as the user reads messages
- Offline access to read messages works after first view

### AttachmentMeta

```rust
pub struct AttachmentMeta {
    pub id: AttachmentId,
    pub message_id: MessageId,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub local_path: Option<PathBuf>,  // None until downloaded
    pub provider_id: String,          // Provider's attachment ref for fetching
}
```

Attachments are metadata-only until explicitly downloaded. The `local_path` is populated when the user triggers a download.

### Thread (conversation)

```rust
pub struct Thread {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub subject: String,
    pub participants: Vec<Address>,
    pub message_count: u32,
    pub unread_count: u32,
    pub latest_date: DateTime<Utc>,
    pub snippet: String,             // From latest message
}
```

Threads are computed/aggregated from messages sharing a thread_id. Gmail provides thread IDs natively. For IMAP, threads are constructed from `In-Reply-To` and `References` headers using JWZ threading algorithm.

### Draft (compose state)

```rust
pub struct Draft {
    pub id: DraftId,
    pub account_id: AccountId,
    pub in_reply_to: Option<MessageId>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub body_markdown: String,       // User writes markdown
    pub attachments: Vec<PathBuf>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

Drafts are local-first. They exist in SQLite before they're sent. The compose flow creates a draft, opens $EDITOR with a temp file, and updates the draft on save. Sending converts the markdown body to multipart and dispatches via the send provider.

### SavedSearch

```rust
pub struct SavedSearch {
    pub id: SavedSearchId,
    pub account_id: Option<AccountId>,  // None = all accounts
    pub name: String,
    pub query: String,                  // mxr query syntax
    pub sort: SortOrder,
    pub icon: Option<String>,
    pub position: i32,                  // Sidebar ordering
    pub created_at: DateTime<Utc>,
}

pub enum SortOrder {
    DateDesc,
    DateAsc,
    Relevance,
}
```

Saved searches are a core primitive. They appear in the sidebar, in the command palette, and are the primary way users organize their view. They are stored queries, not materialized views — results are computed live from the search index.

### ProviderMeta (the ugly truth the sync engine needs)

```rust
pub struct ProviderMeta {
    pub message_id: MessageId,
    pub provider: ProviderKind,
    pub remote_id: String,
    pub thread_remote_id: Option<String>,
    pub sync_token: Option<String>,
    pub raw_labels: Option<String>,     // JSON: provider's native label/flag state
    pub mailbox_id: Option<String>,     // IMAP: which mailbox this lives in
    pub uid_validity: Option<u32>,      // IMAP: UIDVALIDITY for this mailbox
    pub raw_json: Option<String>,       // Full provider response, for debugging
}
```

**Why this exists**: The canonical model (Envelope, Label, etc.) is what the app sees. But sync engines always need the ugly truth: the exact provider state as it was last seen. This is essential for:
- Detecting remote changes during delta sync
- Resolving conflicts between local and remote state
- Debugging sync issues ("what did Gmail actually return?")
- Round-tripping mutations back to the provider accurately

ProviderMeta lives in its own table. Application logic NEVER reads it. Only the sync engine and provider adapters touch it.

We initially tried to stuff everything into the Envelope type but realized this polluted the canonical model with provider-specific concerns. Separating ProviderMeta keeps the internal model clean while preserving the data the sync engine needs.

### SyncCursor

```rust
pub enum SyncCursor {
    Gmail { history_id: u64 },
    Imap { uid_validity: u32, uid_next: u32 },
    Initial,  // Fresh account, no sync yet
}
```

Opaque cursor stored per-account. The sync engine passes it to the provider on each sync cycle. The provider returns an updated cursor with the response.

### SyncBatch

```rust
pub struct SyncBatch {
    pub upserted: Vec<Envelope>,
    pub deleted_provider_ids: Vec<String>,
    pub label_changes: Vec<LabelChange>,
    pub next_cursor: SyncCursor,
}

pub struct LabelChange {
    pub provider_message_id: String,
    pub added_labels: Vec<String>,
    pub removed_labels: Vec<String>,
}
```

This is what a provider returns from `sync_messages()`. The sync engine applies these changes to the local store and search index.

### Snoozed (local-first snooze)

```rust
pub struct Snoozed {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub snoozed_at: DateTime<Utc>,
    pub wake_at: DateTime<Utc>,
    pub original_labels: Vec<LabelId>,  // Labels to restore on wake
}
```

**Why local snooze**: Gmail's API has no snooze endpoint. Snooze in Gmail's web UI is an internal feature never exposed to third parties. So we implement it locally, which is actually better because we control the behavior entirely.

The flow:
1. User hits `Z` (snooze), selects a time
2. Daemon archives the message on Gmail (removes INBOX label via API) and stores snooze state locally
3. Message disappears from inbox view
4. Daemon runs a wake loop (checking every 60 seconds)
5. When `wake_at` is reached: daemon re-applies INBOX label on Gmail via API AND restores local labels
6. Message reappears in inbox in both mxr and Gmail's web UI

This means inbox-zero state is consistent across mxr and Gmail web. When you snooze in mxr, it's gone from Gmail too. When it wakes, it's back in both places.

### ExportFormat

```rust
pub enum ExportFormat {
    Markdown,     // Clean readable thread
    Json,         // Structured, for programmatic use
    Mbox,         // Standard email format
    LlmContext,   // Optimized: stripped signatures, collapsed quotes, minimal tokens
}
```

The `LlmContext` format is specifically designed for feeding email threads to AI. It strips quoted replies, removes signatures, collapses forwarded chains, and outputs a clean chronological thread with minimal tokens. This pairs with reader mode — if the rendering pipeline already strips emails to clean text, the LLM export uses the same pipeline.

## SQLite schema

```sql
-- =========================================================================
-- Accounts
-- =========================================================================
CREATE TABLE IF NOT EXISTS accounts (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    email           TEXT NOT NULL,
    sync_provider   TEXT,                    -- 'gmail' | NULL (if no sync)
    send_provider   TEXT,                    -- 'gmail' | 'smtp' | NULL
    sync_config     TEXT,                    -- JSON: provider-specific sync config
    send_config     TEXT,                    -- JSON: provider-specific send config
    enabled         INTEGER NOT NULL DEFAULT 1,
    sync_cursor     TEXT,                    -- JSON: opaque SyncCursor
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- =========================================================================
-- Labels (unified: gmail labels + imap folders + flags)
-- =========================================================================
CREATE TABLE IF NOT EXISTS labels (
    id              TEXT PRIMARY KEY,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('system', 'folder', 'user')),
    color           TEXT,
    provider_id     TEXT NOT NULL,
    unread_count    INTEGER NOT NULL DEFAULT 0,
    total_count     INTEGER NOT NULL DEFAULT 0,
    UNIQUE (account_id, provider_id)
);

CREATE INDEX idx_labels_account ON labels(account_id);

-- =========================================================================
-- Messages (envelope/headers, always cached)
-- =========================================================================
CREATE TABLE IF NOT EXISTS messages (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    provider_id         TEXT NOT NULL,
    thread_id           TEXT NOT NULL,
    message_id_header   TEXT,
    in_reply_to         TEXT,
    reference_headers   TEXT,           -- JSON array
    from_name           TEXT,
    from_email          TEXT NOT NULL,
    to_addrs            TEXT NOT NULL,  -- JSON array of {name, email}
    cc_addrs            TEXT NOT NULL DEFAULT '[]',
    bcc_addrs           TEXT NOT NULL DEFAULT '[]',
    subject             TEXT NOT NULL DEFAULT '',
    date                INTEGER NOT NULL,
    flags               INTEGER NOT NULL DEFAULT 0,
    snippet             TEXT NOT NULL DEFAULT '',
    has_attachments     INTEGER NOT NULL DEFAULT 0,
    size_bytes          INTEGER NOT NULL DEFAULT 0,
    unsubscribe_method  TEXT,           -- JSON: UnsubscribeMethod enum
    UNIQUE (account_id, provider_id)
);

CREATE INDEX idx_messages_account ON messages(account_id);
CREATE INDEX idx_messages_thread ON messages(thread_id);
CREATE INDEX idx_messages_date ON messages(date DESC);
CREATE INDEX idx_messages_from ON messages(from_email);
CREATE INDEX idx_messages_flags ON messages(flags);

-- =========================================================================
-- Message-label junction (many-to-many)
-- =========================================================================
CREATE TABLE IF NOT EXISTS message_labels (
    message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    label_id    TEXT NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
    PRIMARY KEY (message_id, label_id)
);

CREATE INDEX idx_message_labels_label ON message_labels(label_id);

-- =========================================================================
-- Message bodies (fetched on demand, cached)
-- =========================================================================
CREATE TABLE IF NOT EXISTS bodies (
    message_id  TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    text_plain  TEXT,
    text_html   TEXT,
    fetched_at  INTEGER NOT NULL
);

-- =========================================================================
-- Attachments (metadata; actual files stored on disk)
-- =========================================================================
CREATE TABLE IF NOT EXISTS attachments (
    id          TEXT PRIMARY KEY,
    message_id  TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    filename    TEXT NOT NULL,
    mime_type   TEXT NOT NULL,
    size_bytes  INTEGER NOT NULL DEFAULT 0,
    local_path  TEXT,
    provider_id TEXT NOT NULL
);

CREATE INDEX idx_attachments_message ON attachments(message_id);

-- =========================================================================
-- Provider metadata (sync engine's private state)
-- =========================================================================
CREATE TABLE IF NOT EXISTS provider_meta (
    message_id      TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    provider        TEXT NOT NULL,
    remote_id       TEXT NOT NULL,
    thread_remote_id TEXT,
    sync_token      TEXT,
    raw_labels      TEXT,       -- JSON: provider's native label/flag state
    mailbox_id      TEXT,       -- IMAP: which mailbox
    uid_validity    INTEGER,    -- IMAP: UIDVALIDITY
    raw_json        TEXT,       -- Full provider response for debugging
    PRIMARY KEY (message_id, provider)
);

-- =========================================================================
-- Drafts (local compose state)
-- =========================================================================
CREATE TABLE IF NOT EXISTS drafts (
    id              TEXT PRIMARY KEY,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    in_reply_to     TEXT,
    to_addrs        TEXT NOT NULL DEFAULT '[]',
    cc_addrs        TEXT NOT NULL DEFAULT '[]',
    bcc_addrs       TEXT NOT NULL DEFAULT '[]',
    subject         TEXT NOT NULL DEFAULT '',
    body_markdown   TEXT NOT NULL DEFAULT '',
    attachments     TEXT NOT NULL DEFAULT '[]',
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- =========================================================================
-- Snoozed messages
-- =========================================================================
CREATE TABLE IF NOT EXISTS snoozed (
    message_id      TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    snoozed_at      INTEGER NOT NULL,
    wake_at         INTEGER NOT NULL,
    original_labels TEXT NOT NULL    -- JSON: labels to restore
);

CREATE INDEX idx_snoozed_wake ON snoozed(wake_at);

-- =========================================================================
-- Saved searches
-- =========================================================================
CREATE TABLE IF NOT EXISTS saved_searches (
    id          TEXT PRIMARY KEY,
    account_id  TEXT,
    name        TEXT NOT NULL,
    query       TEXT NOT NULL,
    sort_order  TEXT NOT NULL DEFAULT 'date_desc',
    icon        TEXT,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);

-- =========================================================================
-- Rules (deterministic rules engine)
-- =========================================================================
CREATE TABLE IF NOT EXISTS rules (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    priority    INTEGER NOT NULL DEFAULT 0,
    conditions  TEXT NOT NULL,   -- JSON: serialized Conditions struct
    actions     TEXT NOT NULL,   -- JSON: serialized Vec<Action>
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

-- =========================================================================
-- FTS5 (lightweight fallback; Tantivy is primary search)
-- =========================================================================
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    subject, from_email, from_name, snippet,
    content='messages',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, subject, from_email, from_name, snippet)
    VALUES (new.rowid, new.subject, new.from_email, new.from_name, new.snippet);
END;

CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, subject, from_email, from_name, snippet)
    VALUES ('delete', old.rowid, old.subject, old.from_email, old.from_name, old.snippet);
END;

CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, subject, from_email, from_name, snippet)
    VALUES ('delete', old.rowid, old.subject, old.from_email, old.from_name, old.snippet);
    INSERT INTO messages_fts(rowid, subject, from_email, from_name, snippet)
    VALUES (new.rowid, new.subject, new.from_email, new.from_name, new.snippet);
END;

-- =========================================================================
-- Sync log (diagnostics)
-- =========================================================================
CREATE TABLE IF NOT EXISTS sync_log (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id      TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    status          TEXT NOT NULL CHECK (status IN ('running', 'success', 'error')),
    messages_synced INTEGER NOT NULL DEFAULT 0,
    error_message   TEXT
);

CREATE INDEX idx_sync_log_account ON sync_log(account_id, started_at DESC);
```

### Why both FTS5 and Tantivy?

Tantivy is the primary search engine. FTS5 is a lightweight fallback for:
- Simple substring queries in the daemon when Tantivy isn't available
- Migration/reindexing scenarios
- Diagnostic queries

FTS5 costs almost nothing to maintain (triggers handle sync automatically) and provides a safety net. Tantivy is always rebuildable from SQLite, so if the Tantivy index gets corrupted, we reindex from the FTS5/SQLite source of truth.
