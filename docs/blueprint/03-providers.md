# mxr — Providers & Adapter Strategy

## Provider philosophy

mxr ships with first-party Gmail sync and SMTP send support. Other providers are enabled through a stable adapter interface. The core project is designed to make third-party adapters straightforward to implement. First-party support is driven by actual maintainer usage, not checkbox coverage.

### What we considered and rejected

We initially considered:
- **Google Workspace CLI (gws)**: Rejected because it's a CLI wrapper (shell out + parse stdout = fragile), pre-1.0 with expected breaking changes, adds an external binary dependency, and prevents efficient delta sync via Gmail's history.list API.
- **Single unified `EmailProvider` trait**: Rejected because it forces SMTP to implement sync methods it can't support. Split traits are more honest.
- **IMAP as first adapter**: Considered because it's the open standard. Rejected for v1 because Gmail is the maintainer's actual use case, and Gmail's API is significantly better for sync (delta via history.list vs IMAP full re-scan). IMAP is a great community adapter candidate.
- **Generated Google API crate (`google-gmail1`)**: Rejected because it's bloated and awkward. Raw `reqwest` + serde gives full control with less code.

### Provider support levels

**Official adapters** (maintained in main repo):
- Gmail (MailSyncProvider + MailSendProvider)
- SMTP (MailSendProvider only)
- Fake/Testing (both traits, in-memory, deterministic)

**Community adapter candidates** (supported by interface/docs, not maintained by us):
- IMAP (+ SMTP for send)
- Outlook / Microsoft Graph
- JMAP (Fastmail, etc.)
- Proton Bridge
- Exchange (ActiveSync)

## Split traits

### MailSyncProvider (inbox access, reading, mutations)

```rust
#[async_trait]
pub trait MailSyncProvider: Send + Sync {
    /// Human-readable provider name (for logging and UI).
    fn name(&self) -> &str;

    /// Which account this provider serves.
    fn account_id(&self) -> &AccountId;

    /// What this provider can do.
    fn capabilities(&self) -> SyncCapabilities;

    // -- Authentication ---------------------------------------------------

    /// Perform initial authentication or re-authenticate.
    async fn authenticate(&mut self) -> Result<()>;

    /// Refresh an expired token. No-op for password-based providers.
    async fn refresh_auth(&mut self) -> Result<()>;

    // -- Sync -------------------------------------------------------------

    /// Fetch the list of labels/folders from the provider.
    async fn sync_labels(&self) -> Result<Vec<Label>>;

    /// Fetch message changes since the given cursor.
    /// If cursor is Initial, performs full initial sync.
    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch>;

    // -- On-demand fetch --------------------------------------------------

    /// Fetch the full body of a single message.
    async fn fetch_body(&self, provider_message_id: &str) -> Result<MessageBody>;

    /// Download an attachment's raw bytes.
    async fn fetch_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<Vec<u8>>;

    // -- Mutations ---------------------------------------------------------

    /// Add or remove labels/flags on a message.
    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<()>;

    /// Move a message to trash.
    async fn trash(&self, provider_message_id: &str) -> Result<()>;

    /// Mark a message as read or unread.
    async fn set_read(&self, provider_message_id: &str, read: bool) -> Result<()>;

    /// Mark a message as starred or unstarred.
    async fn set_starred(&self, provider_message_id: &str, starred: bool) -> Result<()>;

    // -- Optional: server-side search ------------------------------------

    /// Search on the server. Default: not supported.
    async fn search_remote(&self, _query: &str) -> Result<Vec<String>> {
        Err(Error::Provider("Server-side search not supported".into()))
    }
}

pub struct SyncCapabilities {
    pub labels: bool,           // Multi-assign labels (Gmail: yes, IMAP: no)
    pub server_search: bool,    // Server-side search (Gmail: yes, IMAP: partial)
    pub delta_sync: bool,       // Incremental sync (Gmail: yes via history.list, IMAP: partial via UID)
    pub push: bool,             // Push notifications (Gmail: pub/sub, IMAP: IDLE)
    pub batch_operations: bool, // Batch API calls (Gmail: yes, IMAP: no)
}
```

### MailSendProvider (outbound mail only)

```rust
#[async_trait]
pub trait MailSendProvider: Send + Sync {
    /// Human-readable provider name.
    fn name(&self) -> &str;

    /// Send a composed draft. Returns a receipt with the sent message ID.
    async fn send(&self, draft: &Draft, from: &Address) -> Result<SendReceipt>;
}

pub struct SendReceipt {
    pub provider_message_id: Option<String>,  // Some providers return an ID
    pub sent_at: DateTime<Utc>,
}
```

### Why split traits?

- Gmail adapter implements both `MailSyncProvider` + `MailSendProvider`
- SMTP adapter implements only `MailSendProvider`
- Future IMAP adapter implements `MailSyncProvider` while SMTP handles send
- Outlook adapter could implement both
- The type system prevents you from accidentally calling sync methods on an SMTP backend

## Gmail adapter details

### Authentication

Gmail requires OAuth2. The flow:

1. User runs `mxr accounts add gmail`
2. mxr opens the system browser to Google's OAuth2 consent page
3. Google redirects to `http://localhost:{port}/callback` (mxr runs a temporary local HTTP server)
4. mxr exchanges the authorization code for access + refresh tokens
5. Tokens are stored in the system keyring (via `keyring` crate), NOT in config files
6. On subsequent runs, the refresh token is used to get new access tokens silently

**Google Cloud Console requirement**: Using the Gmail API requires a Google Cloud project with the Gmail API enabled. For personal use, the app runs in "test mode" (limited to 100 users). For public distribution, Google requires app verification. This is a known friction point and should be documented clearly for users.

### API usage

We use the Gmail API directly via `reqwest`, not through the generated `google-gmail1` crate or the gws CLI.

Key endpoints:

```
# List messages
GET /gmail/v1/users/me/messages?labelIds=INBOX&maxResults=100

# Get message (metadata only, for sync)
GET /gmail/v1/users/me/messages/{id}?format=metadata

# Get message (full, for body fetch)
GET /gmail/v1/users/me/messages/{id}?format=full

# Delta sync (the killer feature)
GET /gmail/v1/users/me/history?startHistoryId={id}&historyTypes=messageAdded,messageDeleted,labelAdded,labelRemoved

# Modify labels
POST /gmail/v1/users/me/messages/{id}/modify
{ "addLabelIds": ["INBOX"], "removeLabelIds": ["UNREAD"] }

# Send
POST /gmail/v1/users/me/messages/send
(raw RFC 2822 message as base64url)

# Batch (up to 100 operations in one HTTP request)
POST /gmail/v1/users/me/messages/batchModify

# Labels
GET /gmail/v1/users/me/labels
```

### Delta sync via history.list

This is Gmail's killer sync feature. Instead of re-listing all messages, you ask "what changed since historyId X?" and get back only the deltas: messages added, deleted, label changes. This makes subsequent syncs extremely fast (often just a handful of API calls even for active inboxes).

The sync loop:
1. First sync: list all messages, store the latest `historyId`
2. Subsequent syncs: call `history.list` with the stored `historyId`
3. Apply the returned deltas to local store
4. Update the stored `historyId`

If `historyId` is invalid (too old, account changes), fall back to a full re-sync. This should be rare.

### Snooze integration with Gmail

When the user snoozes a message:
1. Daemon calls Gmail API to remove INBOX label (archive): `POST /messages/{id}/modify { removeLabelIds: ["INBOX"] }`
2. Stores snooze state locally (wake_at, original labels)
3. Message disappears from Gmail web UI inbox too

When snooze wakes:
1. Daemon calls Gmail API to re-add INBOX label: `POST /messages/{id}/modify { addLabelIds: ["INBOX"] }`
2. Restores local labels
3. Message reappears in both mxr and Gmail web UI

This keeps inbox-zero state consistent across clients.

## SMTP adapter details

### Purpose

SMTP is a sending transport only. It cannot sync, list messages, or read mail. It implements `MailSendProvider` and nothing else.

### Implementation

Uses the `lettre` crate. Configuration:

```toml
[accounts.work.send]
provider = "smtp"
host = "smtp.company.com"
port = 587
username = "bk@company.com"
password_ref = "keyring:mxr/work-smtp"  # Stored in system keyring
use_tls = true
```

### When SMTP is used

A typical configuration might be:
- Sync: Gmail (read inbox, sync labels, fetch messages)
- Send: SMTP (deliver outbound mail through company relay)

Or:
- Sync: Gmail
- Send: Gmail API (simpler, no separate SMTP config needed)

The user chooses based on their needs.

## Fake/Testing adapter

### Purpose

The fake adapter is critical for:
- Running integration tests without hitting real servers
- Adapter conformance testing (does your adapter behave correctly?)
- Local development without network access
- Demo mode for screenshots and videos

### Implementation

In-memory storage. Deterministic behavior. Both `MailSyncProvider` and `MailSendProvider`. Supports:
- Pre-loaded fixture messages
- Simulated sync cycles (returns predefined deltas)
- Controllable failures (test error handling)
- Inspectable state (verify what was sent, what was mutated)

```rust
pub struct FakeProvider {
    messages: Vec<Envelope>,
    bodies: HashMap<String, MessageBody>,
    labels: Vec<Label>,
    sent: Vec<Draft>,          // Inspect what was sent
    mutations: Vec<Mutation>,  // Inspect what was mutated
}
```

## Adapter kit (for community adapter authors)

If we want others to build adapters, we need more than a trait definition. We need:

1. **Clear trait definitions** (above)
2. **Conformance test suite**: A set of tests any adapter can run to verify correctness. "Given these inputs, your adapter should produce these outputs."
3. **Fixture data**: Canonical test messages, threads, labels in the internal model format.
4. **Fake provider as reference implementation**: Shows exactly how to implement both traits.
5. **Documentation**: A "How to build an mxr adapter" guide covering:
   - Which traits to implement
   - How to map provider concepts to the internal model
   - How to handle auth
   - How to implement delta sync (if supported)
   - How to store provider metadata
   - How to run the conformance tests
   - How to package as a standalone crate that depends on `mxr-core`

### Out-of-tree adapters

Community adapters should be buildable as standalone crates that depend on `mxr-core` only. They don't need to fork or live inside the main repo. The user would:

```toml
# Cargo.toml of a hypothetical community IMAP adapter
[dependencies]
mxr-core = "0.1"
async-imap = "0.10"
async-native-tls = "0.5"
```

Then in mxr's config, the user registers the adapter. Exact mechanism TBD (dynamic loading is complex in Rust; more likely a feature-flag or compile-time selection initially).
