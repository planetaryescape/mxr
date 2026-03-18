# 05 — Phase 4: Community & Release

## Goal

Ready for public release. Adapter kit for community providers, binary releases, install methods, documentation site, contributor guide. After this phase, mxr is a real open-source project that people can discover, install, use, and contribute to.

## Prerequisites

Phase 3 complete:
- Export working (Markdown, JSON, Mbox, LLM Context)
- Rules engine operational (TOML definitions, dry-run, shell hooks)
- Multi-account support (multiple Gmail + SMTP configs, account switcher)
- HTML rendering config (external html_command)
- Shell completions (bash, zsh, fish)
- Performance optimized for 10k+ message mailboxes
- `mxr doctor --reindex` working

---

## Step 1: Adapter Kit

The adapter kit enables community adapter development. It consists of conformance tests, fixture data, reference documentation, and `mxr-core` published as a stable dependency. With IMAP promoted to first-party (A008), the adapter kit now has two first-party adapters (Gmail and IMAP) to validate against, ensuring the conformance suite covers genuinely different protocol semantics.

### 1.1 Fixture Data Module

Create a `fixtures` module inside `mxr-provider-fake` that exports canonical test data. This data represents the "golden" dataset that conformance tests validate against.

`crates/providers/fake/src/fixtures.rs`:
```rust
use mxr_core::types::{
    AccountId, Envelope, Label, LabelId, MessageBody, MessageFlags,
    ThreadId, Attachment, Address,
};
use chrono::{Utc, TimeZone};

/// Canonical fixture labels that every adapter should map to.
pub fn canonical_labels() -> Vec<Label> {
    vec![
        Label {
            id: LabelId::new("INBOX"),
            name: "Inbox".into(),
            label_type: LabelType::System,
            ..Default::default()
        },
        Label {
            id: LabelId::new("SENT"),
            name: "Sent".into(),
            label_type: LabelType::System,
            ..Default::default()
        },
        Label {
            id: LabelId::new("TRASH"),
            name: "Trash".into(),
            label_type: LabelType::System,
            ..Default::default()
        },
        Label {
            id: LabelId::new("STARRED"),
            name: "Starred".into(),
            label_type: LabelType::System,
            ..Default::default()
        },
        Label {
            id: LabelId::new("user/projects"),
            name: "Projects".into(),
            label_type: LabelType::User,
            ..Default::default()
        },
        Label {
            id: LabelId::new("user/newsletters"),
            name: "Newsletters".into(),
            label_type: LabelType::User,
            ..Default::default()
        },
    ]
}

/// Canonical fixture messages (50+ messages across 10+ threads).
/// Covers: plain text, HTML, multipart, attachments, threads,
/// various flag states, List-Unsubscribe headers, non-ASCII subjects.
pub fn canonical_messages() -> Vec<FixtureMessage> {
    // ... structured fixture data
    // Each FixtureMessage bundles envelope + body + expected behaviors
}

/// A fixture message with its envelope, body, and expected test outcomes.
pub struct FixtureMessage {
    pub envelope: Envelope,
    pub body: MessageBody,
    pub attachments: Vec<Attachment>,
    /// Expected thread grouping (messages with same thread_key should thread together)
    pub thread_key: String,
    /// Expected label mappings after normalization
    pub expected_labels: Vec<LabelId>,
}

/// Canonical fixture threads for testing thread assembly.
pub fn canonical_threads() -> Vec<FixtureThread> {
    // 10+ threads: single-message, multi-message, deeply nested,
    // cross-label threads
}

pub struct FixtureThread {
    pub thread_id: ThreadId,
    pub message_keys: Vec<String>,
    pub expected_subject: String,
    pub expected_message_count: usize,
}
```

Export from the fake provider crate's public API:

`crates/providers/fake/src/lib.rs`:
```rust
pub mod fixtures;
pub mod conformance;
// ... existing FakeProvider code
```

### 1.2 Conformance Test Suite

Create a `conformance` module that exports test functions any adapter can call. These are not `#[test]` functions themselves — they are assertion functions that adapter authors call from their own test suites. The conformance suite must be tested against BOTH first-party adapters (Gmail and IMAP) to ensure it validates real-world protocol differences, not just FakeProvider's in-memory behavior.

`crates/providers/fake/src/conformance.rs`:
```rust
use mxr_core::provider::{MailSyncProvider, MailSendProvider};
use mxr_core::types::*;
use crate::fixtures;

/// Run all conformance tests against the given sync provider.
/// Adapter authors call this from their own `#[tokio::test]` functions.
pub async fn run_sync_conformance<P: MailSyncProvider>(provider: &P) {
    test_name_returns_non_empty(provider);
    test_account_id_returns_valid(provider);
    test_capabilities_are_consistent(provider).await;
    test_sync_labels(provider).await;
    test_initial_sync(provider).await;
    test_delta_sync(provider).await;
    test_fetch_body(provider).await;
    test_fetch_attachment(provider).await;
    test_modify_labels(provider).await;
    test_trash(provider).await;
    test_set_read(provider).await;
    test_set_starred(provider).await;
}

/// Run all conformance tests against the given send provider.
pub async fn run_send_conformance<P: MailSendProvider>(provider: &P) {
    test_send_plain_text(provider).await;
    test_send_html(provider).await;
    test_send_with_attachments(provider).await;
    test_send_returns_receipt(provider).await;
}

// --- Individual test functions ---

fn test_name_returns_non_empty<P: MailSyncProvider>(provider: &P) {
    let name = provider.name();
    assert!(!name.is_empty(), "Provider name must not be empty");
}

fn test_account_id_returns_valid<P: MailSyncProvider>(provider: &P) {
    let id = provider.account_id();
    assert!(!id.as_str().is_empty(), "Account ID must not be empty");
}

async fn test_capabilities_are_consistent<P: MailSyncProvider>(provider: &P) {
    let caps = provider.capabilities();
    // If delta_sync is false, sync_messages with a non-Initial cursor
    // should return an error or a full re-sync, not partial results.
    // If server_search is false, search_remote should return an error.
    if !caps.server_search {
        let result = provider.search_remote("test").await;
        assert!(result.is_err(), "search_remote should error when capability is false");
    }
}

async fn test_sync_labels<P: MailSyncProvider>(provider: &P) {
    let labels = provider.sync_labels().await.expect("sync_labels should succeed");
    assert!(!labels.is_empty(), "Provider should return at least one label");
    // Every label must have a non-empty id and name
    for label in &labels {
        assert!(!label.id.as_str().is_empty(), "Label ID must not be empty");
        assert!(!label.name.is_empty(), "Label name must not be empty");
    }
}

async fn test_initial_sync<P: MailSyncProvider>(provider: &P) {
    let batch = provider
        .sync_messages(&SyncCursor::Initial)
        .await
        .expect("Initial sync should succeed");
    assert!(!batch.messages.is_empty(), "Initial sync should return messages");
    // Each message must have required fields
    for msg in &batch.messages {
        assert!(!msg.provider_message_id.is_empty());
        assert!(!msg.subject.is_empty() || msg.subject == ""); // allow empty subjects
        assert!(msg.date.is_some() || msg.internal_date.is_some());
    }
    // Must return a cursor for subsequent delta sync
    assert!(
        batch.next_cursor.is_some(),
        "Initial sync must return a cursor for delta sync"
    );
}

async fn test_delta_sync<P: MailSyncProvider>(provider: &P) {
    // First do initial sync to get a cursor
    let initial = provider
        .sync_messages(&SyncCursor::Initial)
        .await
        .expect("Initial sync should succeed");

    if let Some(cursor) = initial.next_cursor {
        // Delta sync with the cursor should succeed (may return empty batch)
        let delta = provider
            .sync_messages(&cursor)
            .await
            .expect("Delta sync should succeed");
        // Delta batch may be empty, that's fine
        // But it should not error
    }
}

async fn test_fetch_body<P: MailSyncProvider>(provider: &P) {
    let batch = provider
        .sync_messages(&SyncCursor::Initial)
        .await
        .expect("sync failed");
    if let Some(msg) = batch.messages.first() {
        let body = provider
            .fetch_body(&msg.provider_message_id)
            .await
            .expect("fetch_body should succeed");
        // Body should have at least plain text or HTML
        assert!(
            body.text.is_some() || body.html.is_some(),
            "Body must have text or HTML content"
        );
    }
}

async fn test_fetch_attachment<P: MailSyncProvider>(provider: &P) {
    // Find a message with attachments from fixtures
    let batch = provider
        .sync_messages(&SyncCursor::Initial)
        .await
        .expect("sync failed");
    for msg in &batch.messages {
        if !msg.attachments.is_empty() {
            let att = &msg.attachments[0];
            let bytes = provider
                .fetch_attachment(&msg.provider_message_id, &att.provider_attachment_id)
                .await
                .expect("fetch_attachment should succeed");
            assert!(!bytes.is_empty(), "Attachment bytes should not be empty");
            return;
        }
    }
    // If no messages with attachments exist, skip (not a failure)
}

async fn test_modify_labels<P: MailSyncProvider>(provider: &P) {
    let batch = provider
        .sync_messages(&SyncCursor::Initial)
        .await
        .expect("sync failed");
    if let Some(msg) = batch.messages.first() {
        // Add a label
        provider
            .modify_labels(&msg.provider_message_id, &["STARRED".into()], &[])
            .await
            .expect("modify_labels (add) should succeed");
        // Remove the label
        provider
            .modify_labels(&msg.provider_message_id, &[], &["STARRED".into()])
            .await
            .expect("modify_labels (remove) should succeed");
    }
}

async fn test_trash<P: MailSyncProvider>(provider: &P) {
    let batch = provider
        .sync_messages(&SyncCursor::Initial)
        .await
        .expect("sync failed");
    if let Some(msg) = batch.messages.first() {
        provider
            .trash(&msg.provider_message_id)
            .await
            .expect("trash should succeed");
    }
}

async fn test_set_read<P: MailSyncProvider>(provider: &P) {
    let batch = provider
        .sync_messages(&SyncCursor::Initial)
        .await
        .expect("sync failed");
    if let Some(msg) = batch.messages.first() {
        provider
            .set_read(&msg.provider_message_id, true)
            .await
            .expect("set_read(true) should succeed");
        provider
            .set_read(&msg.provider_message_id, false)
            .await
            .expect("set_read(false) should succeed");
    }
}

async fn test_set_starred<P: MailSyncProvider>(provider: &P) {
    let batch = provider
        .sync_messages(&SyncCursor::Initial)
        .await
        .expect("sync failed");
    if let Some(msg) = batch.messages.first() {
        provider
            .set_starred(&msg.provider_message_id, true)
            .await
            .expect("set_starred(true) should succeed");
        provider
            .set_starred(&msg.provider_message_id, false)
            .await
            .expect("set_starred(false) should succeed");
    }
}

async fn test_send_plain_text<P: MailSendProvider>(provider: &P) {
    let draft = Draft {
        to: vec![Address::new("test@example.com")],
        subject: "Conformance test: plain text".into(),
        body: "Hello from conformance test.".into(),
        ..Default::default()
    };
    let receipt = provider
        .send(&draft, &Address::new("sender@example.com"))
        .await
        .expect("send plain text should succeed");
    assert!(receipt.sent_at <= Utc::now());
}

async fn test_send_html<P: MailSendProvider>(provider: &P) {
    let draft = Draft {
        to: vec![Address::new("test@example.com")],
        subject: "Conformance test: HTML".into(),
        body: "**Bold** text".into(),
        html_body: Some("<p><strong>Bold</strong> text</p>".into()),
        ..Default::default()
    };
    let receipt = provider
        .send(&draft, &Address::new("sender@example.com"))
        .await
        .expect("send HTML should succeed");
    assert!(receipt.sent_at <= Utc::now());
}

async fn test_send_with_attachments<P: MailSendProvider>(provider: &P) {
    let draft = Draft {
        to: vec![Address::new("test@example.com")],
        subject: "Conformance test: attachment".into(),
        body: "See attached.".into(),
        attachments: vec![DraftAttachment {
            filename: "test.txt".into(),
            mime_type: "text/plain".into(),
            data: b"attachment content".to_vec(),
        }],
        ..Default::default()
    };
    let receipt = provider
        .send(&draft, &Address::new("sender@example.com"))
        .await
        .expect("send with attachment should succeed");
    assert!(receipt.sent_at <= Utc::now());
}

async fn test_send_returns_receipt<P: MailSendProvider>(provider: &P) {
    let draft = Draft {
        to: vec![Address::new("test@example.com")],
        subject: "Conformance test: receipt".into(),
        body: "Receipt test.".into(),
        ..Default::default()
    };
    let receipt = provider
        .send(&draft, &Address::new("sender@example.com"))
        .await
        .expect("send should succeed");
    // sent_at should be recent (within last 60 seconds)
    let elapsed = Utc::now() - receipt.sent_at;
    assert!(elapsed.num_seconds() < 60, "sent_at should be recent");
}
```

### 1.3 Adapter Usage Example

Show how an adapter author uses the conformance suite in their own crate. Since IMAP is now first-party (A008), the IMAP adapter itself serves as a real-world example of conformance test usage — not a hypothetical:

```rust
// In crates/providers/imap/ (first-party IMAP adapter)
#[cfg(test)]
mod tests {
    use super::ImapProvider;
    use mxr_provider_fake::conformance;

    #[tokio::test]
    async fn imap_passes_sync_conformance() {
        let provider = ImapProvider::new_test_instance().await;
        conformance::run_sync_conformance(&provider).await;
    }
}
```

Community adapter authors follow the same pattern — the IMAP adapter demonstrates this with a genuinely different protocol (folder-based, CONDSTORE/UID sync) compared to Gmail's label-based API.

### 1.4 Reference Implementations

The adapter kit provides two reference implementations for community adapter authors:

1. **FakeProvider** — Canonical in-memory reference. Shows the simplest possible implementation of both traits. Best for understanding the API surface without protocol complexity.
2. **IMAP adapter** (`crates/providers/imap/`) — Real-world reference. Shows how to map a genuinely different protocol (folder-based, stateful connections, CONDSTORE/UID sync) to the mxr internal model. Best for understanding the mapping challenges adapter authors will face.

The existing `FakeProvider` already implements both traits. Document it as the canonical reference:

`crates/providers/fake/src/lib.rs` — add module-level doc comment:
```rust
//! # mxr-provider-fake
//!
//! Reference implementation of the mxr provider traits.
//!
//! This crate serves three purposes:
//! 1. **Testing**: Integration tests use FakeProvider instead of hitting real servers.
//! 2. **Reference**: Community adapter authors can study this implementation to
//!    understand how to map provider concepts to the mxr internal model.
//! 3. **Conformance**: The conformance test suite lives here, and FakeProvider
//!    is the first adapter to pass it.
//!
//! ## For adapter authors
//!
//! See the `conformance` module for the test suite you should run against your adapter.
//! See the `fixtures` module for canonical test data.
//! See `FakeProvider`'s implementation of `MailSyncProvider` and `MailSendProvider`
//! for a complete working example.
```

### 1.5 Publish mxr-core to crates.io

Before community adapters can exist as standalone crates, `mxr-core` must be published.

**Pre-publish checklist** (file: `crates/core/Cargo.toml`):
```toml
[package]
name = "mxr-core"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
description = "Core types and provider traits for the mxr email client"
repository = "https://github.com/planetaryescape/mxr"
homepage = "https://github.com/planetaryescape/mxr"
keywords = ["email", "mail", "provider", "adapter"]
categories = ["email"]
readme = "README.md"
```

**Steps:**
1. Audit `mxr-core` public API surface — every `pub` item is now a semver commitment
2. Add `#[non_exhaustive]` to enums and structs that may grow (e.g., `SyncCapabilities`, `MessageFlags`)
3. Ensure no path dependencies leak into the published crate
4. Write `crates/core/README.md` (minimal: what it is, link to main repo, link to adapter guide)
5. `cargo publish --dry-run -p mxr-core` to validate
6. `cargo publish -p mxr-core`

**API stability rules** (document in `crates/core/README.md`):
- Provider traits (`MailSyncProvider`, `MailSendProvider`) are semver-guarded
- Breaking changes to traits require a major version bump + migration guide
- New default methods on traits are minor version bumps (non-breaking)
- New fields on `#[non_exhaustive]` structs are minor version bumps

### 1.6 Out-of-Tree Adapter Skeleton

Create an example skeleton that adapter authors can copy:

`examples/adapter-skeleton/Cargo.toml`:
```toml
[package]
name = "mxr-provider-example"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
description = "Example mxr provider adapter — use as a template"

[dependencies]
mxr-core = "0.1"
async-trait = "0.1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"

[dev-dependencies]
mxr-provider-fake = "0.1"  # For conformance tests
tokio = { version = "1", features = ["full", "test-util"] }
```

`examples/adapter-skeleton/src/lib.rs`:
```rust
//! Example mxr provider adapter.
//!
//! Copy this skeleton and implement the trait methods for your email provider.
//! Run the conformance tests to verify correctness.

use async_trait::async_trait;
use mxr_core::provider::{MailSyncProvider, MailSendProvider};
use mxr_core::types::*;
use mxr_core::error::Result;

pub struct ExampleProvider {
    account_id: AccountId,
    // Your provider-specific state here
}

impl ExampleProvider {
    pub fn new(account_id: AccountId) -> Self {
        Self { account_id }
    }
}

#[async_trait]
impl MailSyncProvider for ExampleProvider {
    fn name(&self) -> &str {
        "example"
    }

    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn capabilities(&self) -> SyncCapabilities {
        SyncCapabilities {
            labels: false,
            server_search: false,
            delta_sync: false,
            push: false,
            batch_operations: false,
        }
    }

    async fn authenticate(&mut self) -> Result<()> {
        todo!("Implement authentication for your provider")
    }

    async fn refresh_auth(&mut self) -> Result<()> {
        todo!("Implement token refresh (no-op for password-based)")
    }

    async fn sync_labels(&self) -> Result<Vec<Label>> {
        todo!("Fetch labels/folders from your provider")
    }

    async fn sync_messages(&self, cursor: &SyncCursor) -> Result<SyncBatch> {
        todo!("Fetch messages (initial or delta) from your provider")
    }

    async fn fetch_body(&self, provider_message_id: &str) -> Result<MessageBody> {
        todo!("Fetch full message body")
    }

    async fn fetch_attachment(
        &self,
        provider_message_id: &str,
        provider_attachment_id: &str,
    ) -> Result<Vec<u8>> {
        todo!("Download attachment bytes")
    }

    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<()> {
        todo!("Add/remove labels on a message")
    }

    async fn trash(&self, provider_message_id: &str) -> Result<()> {
        todo!("Move message to trash")
    }

    async fn set_read(&self, provider_message_id: &str, read: bool) -> Result<()> {
        todo!("Mark message read/unread")
    }

    async fn set_starred(&self, provider_message_id: &str, starred: bool) -> Result<()> {
        todo!("Star/unstar message")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_provider_fake::conformance;

    #[tokio::test]
    async fn passes_sync_conformance() {
        let provider = ExampleProvider::new(AccountId::new("test"));
        conformance::run_sync_conformance(&provider).await;
    }
}
```

### 1.7 Adapter Development Guide

File: `docs/guide/adapter-development.md` (also rendered in the mdBook documentation site, Step 5).

**Content outline:**

1. **Overview** — What adapters are, the split-trait design, support levels
2. **Prerequisites** — Rust toolchain, familiarity with async Rust, your provider's API docs
3. **Getting started** — Copy the skeleton, set up Cargo.toml with `mxr-core` dependency
4. **Implementing MailSyncProvider**
   - `name()` and `account_id()`: trivial, just return identifiers
   - `capabilities()`: declare what your provider supports
   - `authenticate()` / `refresh_auth()`: OAuth2 vs password vs API key patterns
   - `sync_labels()`: mapping provider folders/labels to `Label` type
   - `sync_messages()`: initial sync vs delta sync, `SyncCursor` usage, `SyncBatch` construction
   - `fetch_body()`: lazy hydration pattern, mapping to `MessageBody`
   - `fetch_attachment()`: downloading raw bytes
   - Mutations: `modify_labels`, `trash`, `set_read`, `set_starred`
   - `search_remote()`: optional, default returns error
5. **Implementing MailSendProvider**
   - `send()`: converting `Draft` to provider-specific format, returning `SendReceipt`
6. **Mapping provider concepts to internal model**
   - Gmail labels vs IMAP folders vs Exchange categories
   - Thread ID resolution (References/In-Reply-To header vs provider threading)
   - Flag mapping (Seen → read, Flagged → starred, Deleted → trashed)
   - Date handling (provider date vs internal UTC)
7. **Handling auth**
   - OAuth2 pattern (browser redirect, token storage, refresh)
   - Password/API key pattern (keyring storage)
   - Testing auth without real credentials
8. **Delta sync strategies**
   - Gmail-style: history.list with historyId cursor
   - IMAP-style: UID-based comparison
   - No delta sync: full re-list with local diff
9. **Provider metadata**
   - Using `provider_metadata: serde_json::Value` on `Envelope` for provider-specific data
   - When to store metadata vs re-derive it
10. **Running conformance tests** — import `mxr_provider_fake::conformance`, call `run_sync_conformance` / `run_send_conformance`
11. **Reference implementations** — FakeProvider for API surface understanding, IMAP adapter (`crates/providers/imap/`) for real-world protocol mapping (folder→label, CONDSTORE sync, connection management, JWZ threading)
12. **Packaging** — standalone crate, depend on `mxr-core` only, publish to crates.io if desired
13. **Registering your adapter with mxr** — config format, feature flags, compilation

---

## Step 2: CONTRIBUTING.md

File: `CONTRIBUTING.md` (project root)

### Content

```markdown
# Contributing to mxr

## Non-Negotiable Principles

These guide every design decision. Features that conflict with these lose.

- Local-first by default
- SQLite is the canonical state store
- Search index is rebuildable from SQLite
- Provider adapters are replaceable
- No provider-specific logic outside adapter crates
- Compose uses $EDITOR
- Core features do not depend on proprietary services
- Rules are deterministic before they are intelligent
- TUI is a client of the daemon, not the system itself
- Distraction-free rendering: plain text first, reader mode, no inline images

## Development Setup

### Prerequisites

- Rust stable (latest)
- SQLite3 (system library, usually pre-installed)
- A Gmail account (for integration testing with real provider)
  - Or use the fake provider for all development

### Build

    git clone https://github.com/planetaryescape/mxr.git
    cd mxr
    cargo build

### Run with Fake Provider

    cargo run -- --provider fake

This starts the daemon with in-memory test data. No network, no auth required.

### Run Tests

    cargo test                    # All workspace tests
    cargo test -p mxr-core        # Single crate
    cargo test -p mxr-store       # Single crate

### Linting

    cargo fmt --check
    cargo clippy -- -D warnings

## Code Style

- Plain, legible Rust. No clever macro towers.
- Comments explain WHY, not WHAT.
- Explicit error types (no `.unwrap()` in library code, `anyhow` in binary crates only).
- Compile-time checked SQL queries via sqlx.
- Use `tracing` for logging, not `println!`.
- Follow existing patterns in the crate you're modifying.

## Keybinding Convention

mxr follows a strict keybinding hierarchy (see A005):
1. **Vim-native first** — navigation uses vim conventions (j/k, gg/G, Ctrl-d/u, etc.)
2. **Gmail second** — email actions use Gmail keyboard shortcuts (e for archive, # for trash, s for star, etc.)
3. **Custom last** — only invent a keybinding when neither vim nor Gmail has a relevant convention.

When adding a new TUI action, follow this hierarchy. Check Gmail's keyboard shortcuts before inventing a new binding. Document the rationale in the PR if the binding is custom.

## How to Add a Feature

1. Check if the feature aligns with the non-negotiable principles.
2. Open an issue describing the feature and your proposed approach.
3. Wait for feedback before starting large changes.
4. Implement in the smallest scope possible.
5. Add tests.
6. Submit a PR.

## How to Add a Provider Adapter

See the [Adapter Development Guide](docs/guide/adapter-development.md).

Short version:
1. Create a standalone crate depending on `mxr-core`.
2. Implement `MailSyncProvider` and/or `MailSendProvider`.
3. Run the conformance test suite.
4. See `mxr-provider-fake` as a reference implementation.

Community adapters live in their own repositories, not in the main repo.

## How to Add a CLI Command

1. Add a variant to the `Commands` enum in `crates/cli/src/main.rs`.
2. Add the corresponding `Command` variant in `mxr-protocol`.
3. Handle the command in the daemon's command dispatcher.
4. Add tests.

## How to Add an Export Format

1. Add a variant to the `ExportFormat` enum in `mxr-export`.
2. Implement the `Exporter` trait for the new format.
3. Add tests with snapshot assertions.

## PR Guidelines

- Keep PRs focused. One feature or fix per PR.
- Include tests for new functionality.
- Update relevant documentation.
- Ensure CI passes: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.
- Write a clear PR description explaining what and why.

## Licensing

All contributions are licensed under MIT OR Apache-2.0 (same as the project).
By submitting a PR, you agree to license your contribution under these terms.
```

### Issue Templates

`.github/ISSUE_TEMPLATE/bug_report.yml` (updated per A009 to integrate with `mxr bug-report`):
```yaml
name: Bug Report
description: Report a bug in mxr
labels: [bug]
body:
  - type: textarea
    id: report
    attributes:
      label: Bug Report
      description: |
        Paste the output of `mxr bug-report --stdout` below,
        or describe the issue manually.
      placeholder: |
        Run `mxr bug-report --stdout` and paste the output here,
        or describe the bug manually.
    validations:
      required: true

  - type: textarea
    id: description
    attributes:
      label: Description
      description: What happened? What did you expect?
    validations:
      required: true

  - type: textarea
    id: reproduce
    attributes:
      label: Steps to Reproduce
      description: How can we reproduce this?
    validations:
      required: true

  - type: textarea
    id: additional
    attributes:
      label: Additional Context
      description: Screenshots, additional logs, related issues, etc.
    validations:
      required: false
```

`.github/ISSUE_TEMPLATE/feature_request.yml`:
```yaml
name: Feature Request
description: Suggest a feature for mxr
labels: ["feature"]
body:
  - type: textarea
    id: problem
    attributes:
      label: Problem
      description: What problem does this solve?
    validations:
      required: true
  - type: textarea
    id: solution
    attributes:
      label: Proposed Solution
      description: How should this work?
    validations:
      required: true
  - type: textarea
    id: alternatives
    attributes:
      label: Alternatives Considered
```

`.github/ISSUE_TEMPLATE/adapter_proposal.yml`:
```yaml
name: Provider Adapter Proposal
description: Propose a new provider adapter
labels: ["adapter"]
body:
  - type: input
    id: provider
    attributes:
      label: Provider
      placeholder: "e.g., IMAP, Outlook, JMAP, Proton Bridge"
    validations:
      required: true
  - type: textarea
    id: api
    attributes:
      label: API / Protocol
      description: "What API or protocol will this adapter use?"
    validations:
      required: true
  - type: checkboxes
    id: traits
    attributes:
      label: Traits to implement
      options:
        - label: MailSyncProvider
        - label: MailSendProvider
  - type: textarea
    id: notes
    attributes:
      label: Notes
      description: "Delta sync strategy, auth mechanism, known limitations"
```

### `mxr bug-report` Command (A009, D072-D074)

A single command that generates a sanitized diagnostic bundle ready to paste into a GitHub issue. Builds on top of the logging infrastructure from A006 (Phase 0/3) and the `mxr doctor`/`mxr status`/`mxr logs` commands from Phase 3.

#### 2.1 BugReportSanitizer

`crates/cli/src/bug_report.rs` (or `crates/daemon/src/commands/bug_report.rs` if executed daemon-side):

```rust
use regex::Regex;

pub struct BugReportSanitizer;

impl BugReportSanitizer {
    /// Sanitize text by redacting sensitive information.
    /// Errs on the side of over-redacting (D073).
    pub fn sanitize(text: &str) -> String {
        let text = Self::redact_emails(text);
        let text = Self::redact_tokens(&text);
        let text = Self::redact_passwords(&text);
        let text = Self::redact_api_keys(&text);
        let text = Self::redact_subjects(&text);
        let text = Self::redact_bodies(&text);
        let text = Self::redact_ips(&text);
        Self::anonymize_paths(&text)
    }

    fn redact_emails(text: &str) -> String {
        let re = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
        re.replace_all(text, "[REDACTED_EMAIL]").into_owned()
    }

    fn redact_tokens(text: &str) -> String {
        // Match common token patterns: Bearer tokens, OAuth tokens, etc.
        let re = Regex::new(r"(?i)(bearer|token|oauth|access_token|refresh_token)\s*[=:]\s*\S+").unwrap();
        re.replace_all(text, "$1=[REDACTED]").into_owned()
    }

    fn redact_passwords(text: &str) -> String {
        let re = Regex::new(r"(?i)(password|password_ref|secret)\s*[=:]\s*\S+").unwrap();
        re.replace_all(text, "$1=[REDACTED]").into_owned()
    }

    fn redact_api_keys(text: &str) -> String {
        let re = Regex::new(r"(?i)(api_key|apikey|client_secret)\s*[=:]\s*\S+").unwrap();
        re.replace_all(text, "$1=[REDACTED]").into_owned()
    }

    fn redact_subjects(text: &str) -> String {
        let re = Regex::new(r"(?i)subject\s*[=:]\s*.*").unwrap();
        re.replace_all(text, "subject=[REDACTED_SUBJECT]").into_owned()
    }

    fn redact_bodies(text: &str) -> String {
        let re = Regex::new(r"(?i)body\s*[=:]\s*.*").unwrap();
        re.replace_all(text, "body=[REDACTED_BODY]").into_owned()
    }

    fn redact_ips(text: &str) -> String {
        let re = Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap();
        re.replace_all(text, "[REDACTED_IP]").into_owned()
    }

    fn anonymize_paths(text: &str) -> String {
        // Replace absolute home paths with ~/
        if let Some(home) = std::env::var("HOME").ok() {
            text.replace(&home, "~")
        } else {
            text.to_string()
        }
    }
}
```

#### 2.2 Report Generator

The report generator collects data from multiple sources and assembles the markdown report:

```rust
pub struct BugReportGenerator {
    daemon_client: Option<DaemonClient>,
}

impl BugReportGenerator {
    pub async fn generate(&self, opts: &BugReportOpts) -> Result<String> {
        let mut report = String::new();

        report.push_str("# mxr Bug Report\n\n");

        // System info (always available, no daemon needed)
        report.push_str(&self.system_info());

        // Config summary (read from config file, sanitized)
        report.push_str(&self.config_summary()?);

        // Daemon-dependent sections (gracefully degrade if daemon not running)
        if let Some(client) = &self.daemon_client {
            report.push_str(&self.daemon_status(client).await?);
            report.push_str(&self.account_health(client).await?);
            report.push_str(&self.sync_history(client).await?);
            report.push_str(&self.recent_errors(client).await?);
        } else {
            report.push_str("\n## Daemon Status\nDaemon not running.\n");
        }

        // Recent logs (read directly from log file)
        let log_lines = match (opts.full_logs, opts.verbose) {
            (true, _) => usize::MAX,
            (_, true) => 500,
            _ => 100,
        };
        report.push_str(&self.recent_logs(log_lines, opts.since.as_deref())?);

        // User sections
        report.push_str("\n## User Description\n[Please describe the bug here]\n\n");
        report.push_str("## Steps to Reproduce\n[Please describe how to reproduce]\n\n");
        report.push_str("## Expected Behavior\n[What did you expect to happen?]\n\n");
        report.push_str("## Actual Behavior\n[What actually happened?]\n");

        // Sanitize unless explicitly opted out
        if !opts.no_sanitize {
            Ok(BugReportSanitizer::sanitize(&report))
        } else {
            Ok(report)
        }
    }

    fn system_info(&self) -> String {
        format!(
            "## System\n\
             - mxr version: {version} (built {build_date}, commit {commit})\n\
             - OS: {os}\n\
             - Architecture: {arch}\n\
             - Terminal: {term}\n\
             - Shell: {shell}\n\
             - $EDITOR: {editor}\n\
             - Rust: {rust}\n\n",
            version = env!("CARGO_PKG_VERSION"),
            build_date = option_env!("MXR_BUILD_DATE").unwrap_or("unknown"),
            commit = option_env!("MXR_BUILD_COMMIT").unwrap_or("unknown"),
            os = Self::detect_os(),
            arch = std::env::consts::ARCH,
            term = std::env::var("TERM").unwrap_or_else(|_| "unknown".into()),
            shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".into()),
            editor = std::env::var("EDITOR").unwrap_or_else(|_| "not set".into()),
            rust = Self::detect_rust_version(),
        )
    }
}
```

#### 2.3 CLI Subcommand

Add `bug-report` as a clap subcommand:

```rust
/// Generate a sanitized diagnostic bundle for bug reports
#[derive(Parser)]
pub struct BugReportOpts {
    /// Open report in $EDITOR for review before sharing
    #[arg(long)]
    edit: bool,

    /// Print report to stdout (for piping)
    #[arg(long)]
    stdout: bool,

    /// Copy report to clipboard
    #[arg(long)]
    clipboard: bool,

    /// Open GitHub issue with report pre-filled
    #[arg(long)]
    github: bool,

    /// Save report to specific path
    #[arg(long)]
    output: Option<PathBuf>,

    /// Include debug-level logs (last 500 lines instead of 100)
    #[arg(long)]
    verbose: bool,

    /// Include ALL logs from today
    #[arg(long)]
    full_logs: bool,

    /// Skip sanitization (warned about risks)
    #[arg(long)]
    no_sanitize: bool,

    /// Only include logs from the given duration (e.g., "2h", "30m")
    #[arg(long)]
    since: Option<String>,
}
```

#### 2.4 `--github` Flag Implementation

```rust
fn open_github_issue(report: &str) -> Result<()> {
    let encoded = urlencoding::encode(report);
    let url = format!(
        "https://github.com/planetaryescape/mxr/issues/new?template=bug_report.yml&body={}",
        encoded
    );

    // URL length limit (~8000 chars for most browsers)
    if url.len() > 8000 {
        let path = save_to_temp(report)?;
        eprintln!("Report too large for URL pre-fill.");
        eprintln!("Report saved to: {}", path.display());
        eprintln!("Please paste it into the issue.");
        // Open issue page without pre-fill
        opener::open("https://github.com/planetaryescape/mxr/issues/new?template=bug_report.yml")?;
    } else {
        opener::open(&url)?;
    }
    Ok(())
}
```

#### 2.5 `--clipboard` Flag Implementation

```rust
fn copy_to_clipboard(text: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::{Command, Stdio};
        let mut child = Command::new("pbcopy")
            .stdin(Stdio::piped())
            .spawn()?;
        child.stdin.as_mut().unwrap().write_all(text.as_bytes())?;
        child.wait()?;
    }
    #[cfg(target_os = "linux")]
    {
        // Try xclip first, fall back to xsel
        use std::process::{Command, Stdio};
        let result = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn();
        match result {
            Ok(mut child) => {
                child.stdin.as_mut().unwrap().write_all(text.as_bytes())?;
                child.wait()?;
            }
            Err(_) => {
                let mut child = Command::new("xsel")
                    .arg("--clipboard")
                    .stdin(Stdio::piped())
                    .spawn()?;
                child.stdin.as_mut().unwrap().write_all(text.as_bytes())?;
                child.wait()?;
            }
        }
    }
    Ok(())
}
```

#### 2.6 Log Retention and Pruning

The daemon runs a daily cleanup job (or on startup) that prunes old rows per D074:

```rust
/// Prune old log entries based on retention config.
/// Called on daemon startup and daily by the background scheduler.
pub async fn prune_logs(pool: &SqlitePool, retention_days: u32) -> Result<()> {
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    let cutoff_ts = cutoff.timestamp();

    sqlx::query("DELETE FROM event_log WHERE timestamp < ?")
        .bind(cutoff_ts)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM sync_log WHERE started_at < ?")
        .bind(cutoff_ts)
        .execute(pool)
        .await?;

    sqlx::query("DELETE FROM ai_usage WHERE timestamp < ?")
        .bind(cutoff_ts)
        .execute(pool)
        .await?;

    Ok(())
}
```

`mxr logs --purge` CLI flags:

```rust
/// Purge log entries
#[derive(Parser)]
pub struct LogsPurgeOpts {
    /// Delete logs older than retention period
    #[arg(long)]
    purge: bool,

    /// Delete logs before specific date (YYYY-MM-DD)
    #[arg(long)]
    before: Option<String>,

    /// Delete ALL logs (requires confirmation)
    #[arg(long)]
    all: bool,
}
```

`mxr doctor --store-stats` extends the existing doctor command to report log disk usage:
- Text log file sizes (current + rotated)
- event_log row count and approximate size
- sync_log row count
- Total log disk usage

---

## Step 3: Binary Releases

### 3.1 Release Workflow

Uses `musl` for Linux targets to produce fully static binaries (no glibc dependency — runs on any Linux distro). Uses `cross` for containerized cross-compilation on Linux. Native compilation for macOS targets on macOS runners (D067).

Binary naming: `mxr-v{version}-{target}.tar.gz` — follows cargo-binstall convention for free compatibility (D071).

Each archive contains: `mxr` binary, `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`.

`.github/workflows/release.yml`:
```yaml
name: Release binaries
on:
  push:
    tags: ['v*']

permissions:
  contents: write

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
            archive: mxr-linux-x86_64.tar.gz
          - target: aarch64-unknown-linux-musl
            os: ubuntu-latest
            archive: mxr-linux-aarch64.tar.gz
          - target: x86_64-apple-darwin
            os: macos-latest
            archive: mxr-macos-x86_64.tar.gz
          - target: aarch64-apple-darwin
            os: macos-latest
            archive: mxr-macos-aarch64.tar.gz

    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      # For Linux cross-compilation
      - name: Install cross-compilation tools
        if: matrix.os == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y musl-tools
          if [ "${{ matrix.target }}" = "aarch64-unknown-linux-musl" ]; then
            sudo apt-get install -y gcc-aarch64-linux-gnu
          fi

      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      # Build with cross for Linux targets (handles cross-compilation toolchains)
      - name: Install cross
        if: matrix.os == 'ubuntu-latest'
        run: cargo install cross

      - name: Build (Linux)
        if: matrix.os == 'ubuntu-latest'
        run: cross build --release --target ${{ matrix.target }} --all-features

      - name: Build (macOS)
        if: matrix.os == 'macos-latest'
        run: cargo build --release --target ${{ matrix.target }} --all-features

      - name: Package
        run: |
          VERSION="${GITHUB_REF#refs/tags/v}"
          ARCHIVE="mxr-v${VERSION}-${{ matrix.archive }}"
          mkdir -p release
          cp target/${{ matrix.target }}/release/mxr release/
          cp LICENSE-MIT LICENSE-APACHE README.md release/
          cd release
          tar czf "../${ARCHIVE}" *
          cd ..
          echo "ARCHIVE=${ARCHIVE}" >> $GITHUB_ENV

      - name: Generate SHA256 checksum
        run: |
          sha256sum "${{ env.ARCHIVE }}" > "${{ env.ARCHIVE }}.sha256"

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.target }}
          path: |
            ${{ env.ARCHIVE }}
            ${{ env.ARCHIVE }}.sha256

  publish-crates:
    name: Publish to crates.io
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Verify version matches tag
        run: |
          TAG_VERSION="${GITHUB_REF#refs/tags/v}"
          CARGO_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
          if [ "$TAG_VERSION" != "$CARGO_VERSION" ]; then
            echo "Tag version ($TAG_VERSION) does not match Cargo.toml version ($CARGO_VERSION)"
            exit 1
          fi

      - name: Install cargo-workspaces
        run: cargo install cargo-workspaces

      - name: Publish all crates
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: cargo workspaces publish --from-git --yes

  release:
    name: Create GitHub Release
    needs: [build, publish-crates]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts
          merge-multiple: true

      - name: Generate changelog for this release
        id: changelog
        run: |
          # Get commits since last tag
          PREV_TAG=$(git describe --tags --abbrev=0 HEAD^ 2>/dev/null || echo "")
          if [ -n "$PREV_TAG" ]; then
            CHANGES=$(git log --pretty=format:"- %s (%h)" "${PREV_TAG}..HEAD")
          else
            CHANGES=$(git log --pretty=format:"- %s (%h)")
          fi
          echo "changes<<EOF" >> $GITHUB_OUTPUT
          echo "$CHANGES" >> $GITHUB_OUTPUT
          echo "EOF" >> $GITHUB_OUTPUT

      - name: Create release
        uses: softprops/action-gh-release@v2
        with:
          files: artifacts/*
          generate_release_notes: false
          body: |
            ## Installation

            **Cargo (from source):**
            ```bash
            cargo install mxr
            # With AI features:
            cargo install mxr --features ai
            ```

            **Pre-built binaries:**
            Download the appropriate binary for your platform below, extract, and place `mxr` in your `$PATH`.

            **Homebrew (macOS/Linux):**
            ```bash
            brew install mxr
            ```

            **cargo-binstall (pre-built, no compile):**
            ```bash
            cargo binstall mxr
            ```

            ## Checksums
            SHA256 checksums are provided alongside each binary. Verify with:
            ```bash
            sha256sum -c mxr-v*.sha256
            ```

            ## What's changed
            ${{ steps.changelog.outputs.changes }}

  update-homebrew:
    name: Update Homebrew formula
    needs: release
    runs-on: ubuntu-latest
    steps:
      - name: Update Homebrew formula
        uses: mislav/bump-homebrew-formula-action@v3
        with:
          formula-name: mxr
          homebrew-tap: planetaryescape/homebrew-mxr
          download-url: https://github.com/planetaryescape/mxr/releases/download/${{ github.ref_name }}/mxr-${{ github.ref_name }}-macos-aarch64.tar.gz
        env:
          COMMITTER_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}

  deploy-docs:
    name: Deploy docs site
    needs: release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - working-directory: site
        run: npm ci
      - working-directory: site
        run: npm run build
      - name: Deploy to Cloudflare Pages
        uses: cloudflare/pages-action@v1
        with:
          apiToken: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          accountId: ${{ secrets.CLOUDFLARE_ACCOUNT_ID }}
          projectName: mxr-docs
          directory: site/dist
```

### Required GitHub Secrets

| Secret | Purpose |
|---|---|
| `CARGO_REGISTRY_TOKEN` | crates.io API token for publishing |
| `HOMEBREW_TAP_TOKEN` | GitHub PAT with push access to the homebrew-tap repo |
| `CLOUDFLARE_API_TOKEN` | Cloudflare Pages deployment |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare account identifier |

### 3.2 CI Workflow (existing, updated)

Every pull request runs these checks. All must pass before merge. Jobs are split for clear failure diagnosis.

`.github/workflows/ci.yml`:
```yaml
name: CI
on:
  pull_request:
  push:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -Dwarnings

jobs:
  fmt:
    name: Formatting
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all --check

  clippy:
    name: Lint
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings

  test:
    name: Test
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --all-features

  build:
    name: Build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --workspace --all-features

  # Verify sqlx compile-time checked queries
  sqlx-check:
    name: SQLx Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo sqlx prepare --check --workspace

  # Docs site build check
  docs:
    name: Docs Build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: site/package-lock.json
      - working-directory: site
        run: npm ci
      - working-directory: site
        run: npm run build

  # Privacy/terms sync check
  policy-sync:
    name: Policy Sync
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check privacy policy sync
        run: diff PRIVACY.md site/src/pages/privacy.md
      - name: Check terms sync
        run: diff TERMS.md site/src/pages/terms.md

  # Optional: enforce conventional commit messages
  commit-lint:
    name: Commit Message Lint
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: wagoid/commitlint-github-action@v5
        with:
          configFile: .commitlintrc.yml
```

### 3.3 Versioning (D066)

Semantic versioning with conventional commits for automated changelog generation.

- **PATCH** (0.1.0 → 0.1.1): Bug fixes, dependency updates, docs fixes. No new features. No breaking changes.
- **MINOR** (0.1.1 → 0.2.0): New features, non-breaking additions. New CLI commands, new config options, new keybindings.
- **MAJOR** (0.x → 1.0.0): Breaking changes to CLI interface, config format, IPC protocol, or provider trait API. Pre-1.0, minor versions can include breaking changes (standard Rust convention).
- Tag format: `v{major}.{minor}.{patch}` (e.g., `v0.1.0`)
- Initial public release: `v0.1.0` (not 1.0 — signals API may still evolve)
- `mxr-core` version tracks independently from the main binary (it has its own semver commitments for adapter authors)

### 3.4 Commit Convention

Use conventional commits for clean changelog generation:

```
feat: add IMAP adapter
fix: handle expired OAuth token gracefully
docs: add search syntax reference
refactor: simplify sync engine delta tracking
perf: batch Tantivy index commits
ci: add aarch64 Linux build target
chore: update dependencies
```

Document in `CONTRIBUTING.md` and enforce with commit-lint in CI (see 3.2).

### 3.5 Changelog Generation (git-cliff)

`git-cliff` generates changelogs from conventional commits. Rust-native tool.

`cliff.toml`:
```toml
[changelog]
header = "# Changelog\n\n"
body = """
{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}
- {{ commit.message | upper_first }} ({{ commit.id | truncate(length=7, end="") }})\
{% endfor %}
{% endfor %}
"""
trim = true

[git]
conventional_commits = true
filter_unconventional = true
commit_parsers = [
    { message = "^feat", group = "Features" },
    { message = "^fix", group = "Bug Fixes" },
    { message = "^doc", group = "Documentation" },
    { message = "^perf", group = "Performance" },
    { message = "^refactor", group = "Refactoring" },
    { message = "^ci", group = "CI" },
    { message = "^chore", skip = true },
    { message = "^style", skip = true },
]
```

### 3.6 Crates.io Workspace Publishing (D068)

Crates must be published in dependency order (leaf crates first). Use `cargo-workspaces` which resolves the dependency graph automatically and waits for crates.io index propagation.

Publish order (for reference — `cargo-workspaces` handles this automatically):

```
1. mxr-core           # No internal dependencies
2. mxr-protocol       # Depends on: core
3. mxr-store          # Depends on: core
4. mxr-search         # Depends on: core
5. mxr-reader         # Depends on: core
6. mxr-provider-fake  # Depends on: core
7. mxr-provider-gmail # Depends on: core
8. mxr-provider-imap  # Depends on: core
9. mxr-provider-smtp  # Depends on: core
10. mxr-compose       # Depends on: core, store
11. mxr-rules         # Depends on: core, store
12. mxr-export        # Depends on: core, store, reader
13. mxr-sync          # Depends on: core, store, search
14. mxr-ai            # Depends on: core (behind feature flag)
15. mxr-daemon        # Depends on: most crates
16. mxr-tui           # Depends on: core, protocol
17. mxr-cli           # Depends on: core, protocol (the binary crate)
```

Why publish individual crates: `mxr-core` is the stable API for community adapters. Individual crates let users depend on just what they need. Rust ecosystem convention.

### 3.7 Complete Release Flow (End to End)

```
1. Developer finishes work, merges to main
2. CI runs on main: fmt, clippy, test, build, sqlx-check, docs build, policy sync
3. Developer updates version in Cargo.toml
4. Developer runs: git cliff --output CHANGELOG.md
5. Developer commits: git commit -m "chore: release v0.1.0"
6. Developer tags: git tag v0.1.0
7. Developer pushes: git push origin main v0.1.0
8. Tag triggers release pipeline:
   a. Verify tag version matches Cargo.toml version
   b. Build cross-compiled binaries (4 targets)
   c. Generate SHA256 checksums
   d. Publish crates to crates.io (dependency order via cargo-workspaces)
   e. Create GitHub Release with binaries, checksums, and changelog
   f. Update Homebrew formula (auto-PR to tap repo)
   g. Deploy docs site to Cloudflare Pages
9. Done. Users can now:
   - cargo install mxr
   - cargo binstall mxr
   - brew install mxr
   - Download binary from GitHub Releases
```

### 3.8 Pre-release Checklist (manual, before tagging)

1. Update version in root `Cargo.toml` (workspace version)
2. Run `git cliff --output CHANGELOG.md`
3. Verify CI is green on main
4. Commit version bump + changelog
5. Tag and push

---

## Step 4: Install Methods

### 4.1 cargo install

Requires publishing the main `mxr` binary crate to crates.io.

`Cargo.toml` (workspace root or binary crate):
```toml
[package]
name = "mxr"
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
description = "Local-first, keyboard-native terminal email client"
repository = "https://github.com/planetaryescape/mxr"
homepage = "https://mxr.dev"
keywords = ["email", "mail", "terminal", "tui", "cli"]
categories = ["email", "command-line-utilities"]
```

User installs with:
```bash
cargo install mxr
```

### 4.2 Homebrew Tap

Create repository: `planetaryescape/homebrew-tap`

`Formula/mxr.rb`:
```ruby
class Mxr < Formula
  desc "Local-first, keyboard-native terminal email client"
  homepage "https://github.com/planetaryescape/mxr"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/planetaryescape/mxr/releases/download/v#{version}/mxr-macos-aarch64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    end
    on_intel do
      url "https://github.com/planetaryescape/mxr/releases/download/v#{version}/mxr-macos-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/planetaryescape/mxr/releases/download/v#{version}/mxr-linux-aarch64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    end
    on_intel do
      url "https://github.com/planetaryescape/mxr/releases/download/v#{version}/mxr-linux-x86_64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"
    end
  end

  def install
    bin.install "mxr"
  end

  test do
    assert_match "mxr", shell_output("#{bin}/mxr --version")
  end
end
```

User installs with:
```bash
brew tap planetaryescape/tap
brew install mxr
```

**Automation**: The release workflow (3.1) includes an `update-homebrew` job that uses `mislav/bump-homebrew-formula-action` to automatically create a PR on the tap repo with updated URLs and SHA256 checksums whenever a new release is published (D069).

### 4.3 cargo-binstall (D071)

Zero extra work. If GitHub Release assets follow the naming pattern `{name}-v{version}-{target}.tar.gz` (which ours do), cargo-binstall installs pre-built binaries automatically:

```bash
cargo binstall mxr
```

This gives users a fast install path without the 5+ minute compile time of `cargo install mxr`, and requires no infrastructure beyond what the release workflow already builds.

### 4.4 AUR Package

`PKGBUILD` (hosted in AUR or a separate `mxr-aur` repo):
```bash
# Maintainer: planetaryescape
pkgname=mxr
pkgver=0.1.0
pkgrel=1
pkgdesc="Local-first, keyboard-native terminal email client"
arch=('x86_64' 'aarch64')
url="https://github.com/planetaryescape/mxr"
license=('MIT' 'Apache-2.0')
depends=('sqlite')
makedepends=('rust' 'cargo')
source=("$pkgname-$pkgver.tar.gz::https://github.com/planetaryescape/mxr/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('PLACEHOLDER')

build() {
    cd "$pkgname-$pkgver"
    cargo build --release --locked
}

package() {
    cd "$pkgname-$pkgver"
    install -Dm755 "target/release/mxr" "$pkgdir/usr/bin/mxr"
    install -Dm644 "LICENSE-MIT" "$pkgdir/usr/share/licenses/$pkgname/LICENSE-MIT"
    install -Dm644 "LICENSE-APACHE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE-APACHE"

    # Shell completions
    install -Dm644 "completions/mxr.bash" "$pkgdir/usr/share/bash-completion/completions/mxr"
    install -Dm644 "completions/mxr.zsh" "$pkgdir/usr/share/zsh/site-functions/_mxr"
    install -Dm644 "completions/mxr.fish" "$pkgdir/usr/share/fish/vendor_completions.d/mxr.fish"
}
```

There is also a binary variant (`mxr-bin`) that downloads pre-built binaries instead of compiling from source.

### 4.4.1 Packaging Directory Structure

```
packaging/
├── aur/
│   ├── mxr-bin/PKGBUILD       # Pre-built binary
│   └── mxr/PKGBUILD           # Build from source
├── homebrew/
│   └── mxr.rb                  # Formula template
└── deb/                         # Future: .deb package
```

### 4.5 Nix Package

`flake.nix` at project root:
```nix
{
  description = "mxr - Local-first, keyboard-native terminal email client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rustPlatform = pkgs.makeRustPlatform {
          cargo = pkgs.rust-bin.stable.latest.default;
          rustc = pkgs.rust-bin.stable.latest.default;
        };
      in {
        packages.default = rustPlatform.buildRustPackage {
          pname = "mxr";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ sqlite openssl ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.darwin.apple_sdk.frameworks.Security
              pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            ];

          meta = with pkgs.lib; {
            description = "Local-first, keyboard-native terminal email client";
            homepage = "https://github.com/planetaryescape/mxr";
            license = with licenses; [ mit asl20 ];
            mainProgram = "mxr";
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            (rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "rust-analyzer" ];
            })
            sqlite
            pkg-config
            openssl
          ];
        };
      });
}
```

User installs with:
```bash
# Direct run
nix run github:planetaryescape/mxr

# Install to profile
nix profile install github:planetaryescape/mxr

# Development shell
nix develop github:planetaryescape/mxr
```

### 4.6 Install Script

`install.sh` (hosted at project root, served from GitHub raw URL or docs site):
```bash
#!/usr/bin/env bash
set -euo pipefail

VERSION="${MXR_VERSION:-latest}"
INSTALL_DIR="${MXR_INSTALL_DIR:-$HOME/.local/bin}"

# Detect OS and architecture
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
    linux) OS="linux" ;;
    darwin) OS="macos" ;;
    *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

ARTIFACT="mxr-${OS}-${ARCH}"

# Resolve version
if [ "$VERSION" = "latest" ]; then
    VERSION=$(curl -sSf https://api.github.com/repos/planetaryescape/mxr/releases/latest | grep '"tag_name"' | sed 's/.*"v\(.*\)".*/\1/')
fi

URL="https://github.com/planetaryescape/mxr/releases/download/v${VERSION}/${ARTIFACT}.tar.gz"
CHECKSUM_URL="${URL}.sha256"

echo "Installing mxr v${VERSION} (${OS}/${ARCH})..."

# Download
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -sSfL "$URL" -o "$TMPDIR/${ARTIFACT}.tar.gz"
curl -sSfL "$CHECKSUM_URL" -o "$TMPDIR/${ARTIFACT}.tar.gz.sha256"

# Verify checksum
cd "$TMPDIR"
if command -v sha256sum &> /dev/null; then
    sha256sum -c "${ARTIFACT}.tar.gz.sha256"
elif command -v shasum &> /dev/null; then
    shasum -a 256 -c "${ARTIFACT}.tar.gz.sha256"
else
    echo "Warning: no sha256 tool found, skipping checksum verification"
fi

# Extract and install
tar xzf "${ARTIFACT}.tar.gz"
mkdir -p "$INSTALL_DIR"
mv "$ARTIFACT" "$INSTALL_DIR/mxr"
chmod +x "$INSTALL_DIR/mxr"

echo "Installed mxr to $INSTALL_DIR/mxr"

# Check if install dir is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo ""
    echo "Add $INSTALL_DIR to your PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi
```

User installs with:
```bash
curl -sSf https://raw.githubusercontent.com/planetaryescape/mxr/main/install.sh | bash
```

---

## Step 5: Documentation Site

### 5.1 Technology: mdBook

mdBook is Rust-native, simple, has built-in search, and fits the ecosystem. No JavaScript framework overhead.

**Setup:**
```bash
cargo install mdbook
```

### 5.2 Directory Structure

```
docs/
├── book/                          # mdBook source
│   ├── book.toml                  # mdBook config
│   └── src/
│       ├── SUMMARY.md             # Table of contents (required by mdBook)
│       ├── introduction.md        # What mxr is, why it exists
│       ├── installation/
│       │   ├── README.md          # Overview of install methods
│       │   ├── cargo.md           # cargo install
│       │   ├── homebrew.md        # brew install
│       │   ├── binary.md          # Download pre-built binary
│       │   ├── aur.md             # AUR package
│       │   ├── nix.md             # Nix flake
│       │   └── source.md          # Build from source
│       ├── getting-started/
│       │   ├── README.md          # Quick start overview
│       │   ├── gmail-setup.md     # Adding Gmail account
│       │   ├── first-sync.md      # Running first sync
│       │   ├── basic-usage.md     # TUI basics: navigation, reading, search
│       │   └── configuration.md   # Initial config.toml setup
│       ├── user-guide/
│       │   ├── README.md          # Guide overview
│       │   ├── reading.md         # Reading messages, reader mode, thread view
│       │   ├── composing.md       # Compose, reply, forward with $EDITOR
│       │   ├── searching.md       # Search workflow, saved searches
│       │   ├── organizing.md      # Labels, archive, trash, star
│       │   ├── snooze.md          # Snooze workflow
│       │   ├── unsubscribe.md     # One-key unsubscribe
│       │   ├── export.md          # Export threads (Markdown, JSON, Mbox, LLM)
│       │   ├── rules.md           # Rules engine, shell hooks
│       │   └── multi-account.md   # Multiple accounts
│       ├── reference/
│       │   ├── README.md
│       │   ├── config.md          # config.toml: all options with defaults (incl. [logging] section from A006)
│       │   ├── keybindings.md     # Full keybinding reference: vim+Gmail scheme (A005), hierarchy explained
│       │   ├── search-syntax.md   # Query syntax: fields, operators, examples
│       │   ├── rules-syntax.md    # Rules TOML format, conditions, actions
│       │   ├── cli.md             # Complete CLI reference from A004: all subcommands, flags, output formats
│       │   ├── observability.md   # mxr logs, mxr status, mxr events, monitoring integration (A006)
│       │   └── environment.md     # Environment variables (MXR_EDITOR, XDG paths)
│       ├── adapters/
│       │   ├── README.md          # Adapter overview, support levels
│       │   ├── development.md     # "How to build an adapter" (from Step 1.7)
│       │   ├── conformance.md     # Conformance test suite reference
│       │   └── examples.md        # Links to example skeleton, FakeProvider
│       └── faq.md                 # Troubleshooting, common issues
```

### 5.3 book.toml

`docs/book/book.toml`:
```toml
[book]
title = "mxr — Terminal Email Client"
authors = ["planetaryescape"]
language = "en"
multilingual = false
src = "src"

[build]
build-dir = "../../target/book"

[output.html]
default-theme = "coal"
preferred-dark-theme = "coal"
git-repository-url = "https://github.com/planetaryescape/mxr"
edit-url-template = "https://github.com/planetaryescape/mxr/edit/main/docs/book/src/{path}"
site-url = "/mxr/"

[output.html.search]
enable = true
limit-results = 20
use-hierarchical-headings = true
```

### 5.4 SUMMARY.md

`docs/book/src/SUMMARY.md`:
```markdown
# Summary

[Introduction](introduction.md)

# Getting Started

- [Installation](installation/README.md)
  - [cargo install](installation/cargo.md)
  - [Homebrew](installation/homebrew.md)
  - [Pre-built Binary](installation/binary.md)
  - [AUR](installation/aur.md)
  - [Nix](installation/nix.md)
  - [Build from Source](installation/source.md)
- [Quick Start](getting-started/README.md)
  - [Gmail Setup](getting-started/gmail-setup.md)
  - [First Sync](getting-started/first-sync.md)
  - [Basic Usage](getting-started/basic-usage.md)
  - [Configuration](getting-started/configuration.md)

# User Guide

- [Overview](user-guide/README.md)
- [Reading Email](user-guide/reading.md)
- [Composing](user-guide/composing.md)
- [Searching](user-guide/searching.md)
- [Organizing](user-guide/organizing.md)
- [Snooze](user-guide/snooze.md)
- [Unsubscribe](user-guide/unsubscribe.md)
- [Export](user-guide/export.md)
- [Rules & Automation](user-guide/rules.md)
- [Multi-Account](user-guide/multi-account.md)

# Reference

- [Configuration (config.toml)](reference/config.md)
- [Keybindings](reference/keybindings.md)
- [Search Query Syntax](reference/search-syntax.md)
- [Rules Syntax](reference/rules-syntax.md)
- [CLI Reference](reference/cli.md)
- [Observability & Monitoring](reference/observability.md)
- [Environment Variables](reference/environment.md)

# Adapter Development

- [Overview](adapters/README.md)
- [Building an Adapter](adapters/development.md)
- [Conformance Tests](adapters/conformance.md)
- [Examples](adapters/examples.md)

---

[FAQ & Troubleshooting](faq.md)
```

### 5.5 Key Reference Pages Content

**`reference/cli.md`** — Complete CLI command reference from A004. Includes all command groups (system/daemon, accounts, sync, reading, search, compose, mutations, batch operations, snooze, attachments, labels, export, rules, notifications), universal flags, TUI-to-CLI cross-reference table, and auto-format detection behavior.

**`reference/keybindings.md`** — Full keybinding reference from A005. Explains the vim+Gmail hierarchy (vim-native first for navigation, Gmail second for email actions, custom last). Documents all navigation keys, email action keys, `g` prefix go-to navigation, mxr-specific actions, attachment handling, multi-select with `x`, visual line mode, pattern select with `*` prefix, and vim count support.

**`reference/observability.md`** — Daemon observability guide from A006. Covers `mxr logs` (filtering by level, time, grep, category), `mxr status` (single-command overview, `--watch` mode), `mxr events` (real-time daemon event stream, JSONL output for piping), `mxr doctor --check` for monitoring integration. Documents the `[logging]` config section (level, max_size_mb, max_files, stderr, event_retention_days). Includes examples for integrating with external monitoring (health check scripts, status bar integration via `mxr notify`).

**`reference/config.md`** — Updated to include the `[logging]` configuration section from A006.

### 5.6 Deploy to GitHub Pages

`.github/workflows/docs.yml`:
```yaml
name: Deploy Docs

on:
  push:
    branches: [main]
    paths:
      - "docs/book/**"
  workflow_dispatch:

permissions:
  pages: write
  id-token: write
  contents: read

concurrency:
  group: "pages"
  cancel-in-progress: true

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install mdBook
        run: |
          curl -sSL https://github.com/rust-lang/mdBook/releases/latest/download/mdbook-x86_64-unknown-linux-gnu.tar.gz | tar xz
          sudo mv mdbook /usr/local/bin/

      - name: Build book
        run: mdbook build docs/book

      - name: Upload Pages artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: target/book

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

---

## Step 6: README Overhaul

File: `README.md` (project root)

### Content Outline

```markdown
# mxr

A local-first, keyboard-native terminal email client.

[screenshot or GIF of TUI — inbox view with reader mode]

## What it does

mxr syncs your email to a local SQLite database, indexes it with a full-text
search engine, and presents it in a fast terminal UI with vim keybindings.
Compose in your $EDITOR with markdown. Automate with declarative rules and
shell hooks. Export threads for LLM context.

## Why mxr

| | mutt/neomutt | aerc | himalaya | mxr |
|---|---|---|---|---|
| Architecture | Monolith | Monolith | CLI wrapper | Daemon + clients |
| Local store | Maildir | Maildir | None | SQLite |
| Search | strstrstr | basic | none | Tantivy BM25 |
| Compose | $EDITOR | built-in | $EDITOR | $EDITOR + markdown |
| Reader mode | No | No | No | Yes |
| Rules engine | procmail | No | No | Built-in |

## Quick Start

    # Install
    cargo install mxr

    # Add your Gmail account
    mxr accounts add gmail

    # Start
    mxr

## Install

- **cargo**: `cargo install mxr`
- **Homebrew**: `brew tap planetaryescape/tap && brew install mxr`
- **AUR**: `yay -S mxr`
- **Nix**: `nix run github:planetaryescape/mxr`
- **Binary**: Download from [Releases](https://github.com/planetaryescape/mxr/releases)
- **Script**: `curl -sSf https://raw.githubusercontent.com/planetaryescape/mxr/main/install.sh | bash`

## Features

- **Daemon architecture** — background sync, TUI is a client
- **Local-first** — SQLite store, works offline
- **Tantivy search** — BM25 ranked, field queries, sub-second results
- **$EDITOR compose** — markdown to multipart, YAML frontmatter
- **Reader mode** — strip signatures, quotes, boilerplate
- **Saved searches** — programmable inbox lenses
- **Command palette** — Ctrl-P fuzzy search for everything
- **One-key unsubscribe** — RFC 2369/8058 support
- **Local snooze** — snooze with Gmail inbox-zero sync
- **Rules engine** — deterministic, dry-runnable, shell hooks
- **Thread export** — Markdown, JSON, Mbox, LLM Context
- **Multi-account** — multiple Gmail + IMAP + SMTP configs
- **Fully scriptable** — every TUI action has a CLI equivalent

## Scriptability

Every action you can do in the TUI, you can script from the shell:

    # Batch archive read newsletters
    mxr archive --search "label:newsletters is:read" --yes

    # Daily digest via cron
    mxr search "label:alerts date:today" --format json | \
      jq -r '[.[].subject] | join("\n- ")' | \
      mxr compose --to "me@example.com" --subject "Today's alerts" --body-stdin --yes

    # CI/CD: notify on deploy
    mxr compose --from work --to "team@company.com" \
      --subject "v2.3 deployed" \
      --body "Deployment completed at $(date). All health checks passing." \
      --yes

    # Status bar integration
    mxr notify --format json

    # Monitor daemon health
    mxr doctor --check && echo "healthy" || echo "unhealthy"

See the [CLI Reference](https://planetaryescape.github.io/mxr/reference/cli.html) for the complete command surface.

## Screenshots

[GIF: browsing inbox with j/k navigation]
[GIF: search with field queries]
[GIF: compose in $EDITOR with markdown preview]
[GIF: reader mode toggle]
[GIF: command palette with fuzzy search]

## Documentation

Full docs at [mxr.dev](https://planetaryescape.github.io/mxr/) (or wherever hosted).

## Building Adapters

mxr ships with Gmail, IMAP, and SMTP support. Community adapters can be built as
standalone crates depending on `mxr-core`. See the
[Adapter Development Guide](https://planetaryescape.github.io/mxr/adapters/development.html).
The IMAP adapter serves as a real-world reference implementation alongside the
FakeProvider.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT OR Apache-2.0
```

### Screenshots and GIFs

**Tools**: Use `vhs` (charmbracelet/vhs) to create reproducible terminal GIFs from a `.tape` script.

`docs/assets/demo.tape` (vhs script for the main demo GIF):
```
Output docs/assets/demo.gif

Set FontSize 14
Set Width 1200
Set Height 800
Set Theme "Catppuccin Mocha"

Type "mxr"
Enter
Sleep 2s

# Navigate inbox
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms
Type "j"
Sleep 500ms

# Open message
Enter
Sleep 1s

# Toggle reader mode
Type "R"
Sleep 1s

# Search
Type "/"
Sleep 500ms
Type "from:alice subject:project"
Enter
Sleep 1s

# Command palette
Type@100ms ""  # Ctrl-P
Sleep 500ms
Type "compose"
Enter
Sleep 1s

# Back to inbox
Type "q"
Sleep 500ms
Type "q"
```

Create individual GIFs for each feature highlight (browse, search, compose, reader mode, command palette). Store in `docs/assets/`.

---

## Step 7: Announcement Preparation

### 7.1 Blog Post Draft

File: `docs/announcement/launch-post.md` (not published to repo, used as source for blog post)

**Outline:**

1. **Opening** — One-line pitch: "mxr is a local-first, keyboard-native terminal email client built in Rust."
2. **The problem** — Terminal email clients are either legacy (mutt) or half-measures (aerc, himalaya). The gap: modern UX + local-first + daemon architecture + fast search.
3. **What mxr does differently**
   - Daemon architecture (TUI is a client, not the system)
   - SQLite + Tantivy (your email is queryable structured data)
   - $EDITOR compose with markdown
   - Reader mode (strip the noise)
   - Deterministic rules engine
   - Thread export for LLM context
4. **Architecture overview** — Diagram: Provider ↔ Sync ↔ SQLite ↔ Daemon ↔ TUI/CLI/Scripts. Explain the crate structure briefly.
5. **Demo** — Embedded GIF showing key workflow (sync → browse → search → compose → send)
6. **For adapter authors** — Stable trait interface, conformance tests, mxr-core on crates.io. Call to action for IMAP, Outlook, JMAP adapters.
7. **Install** — Three ways to get running in 30 seconds.
8. **What's next** — Post-v0.1 roadmap (hybrid search, notifications, IMAP adapter, scripting runtime).
9. **Call to action** — Star the repo, try it out, file issues, build adapters.

### 7.2 Target Channels

| Channel | Format | Timing |
|---------|--------|--------|
| Hacker News | "Show HN: mxr — local-first terminal email client in Rust" | Primary launch |
| r/rust | Cross-post with Rust-specific details (crate structure, trait design) | Same day |
| r/commandline | Focus on workflow, keybindings, composability | Same day |
| r/linux | Focus on install methods, local-first, privacy | Same day |
| Rust Community Discord | #showcase channel | Same day |
| This Week in Rust | Submit for newsletter inclusion | Submit week before |
| Lobsters | Post link | Same day |

### 7.3 Launch Checklist

Before announcing, verify ALL of the following:

- [ ] `cargo install mxr` works from a clean machine
- [ ] Homebrew formula installs and runs
- [ ] Binary downloads work for all 4 targets
- [ ] Install script works on macOS and Linux
- [ ] `mxr accounts add gmail` completes OAuth flow
- [ ] `mxr` launches TUI, shows synced messages
- [ ] README renders correctly on GitHub (images load, links work)
- [ ] Documentation site is live and searchable
- [ ] CONTRIBUTING.md is complete
- [ ] Issue templates are created
- [ ] CI is green on main
- [ ] GitHub releases page has binaries with checksums
- [ ] `mxr --version` shows correct version
- [ ] `mxr doctor` passes all checks
- [ ] `mxr bug-report --stdout` generates sanitized report without errors
- [ ] License files present (LICENSE-MIT, LICENSE-APACHE)
- [ ] Demo GIF is current and looks good
- [ ] Blog post is reviewed
- [ ] Hacker News title is ready

---

## Definition of Done

Phase 4 is complete when ALL of the following are true:

1. **Adapter kit**: Conformance test suite exists in `mxr-provider-fake`. FakeProvider AND IMAP adapter both pass all conformance tests (validating the suite against two genuinely different protocols). Fixture data module exports canonical test messages/threads/labels. Adapter skeleton in `examples/adapter-skeleton/` compiles and shows structure. Adapter development guide covers all topics listed in Step 1.7, referencing IMAP as a second real-world reference implementation alongside FakeProvider.
2. **mxr-core published**: `mxr-core` is on crates.io with stable provider traits. `#[non_exhaustive]` on extensible types. Public API audited.
3. **CONTRIBUTING.md**: Complete with dev setup, code style, how-to sections, non-negotiable principles, PR guidelines. Issue templates created for bug reports (integrated with `mxr bug-report`), feature requests, and adapter proposals.
4. **Bug reporting (A009)**: `mxr bug-report` generates sanitized diagnostic bundle (system info, config, account health, sync history, errors, logs). Auto-sanitization redacts emails, tokens, passwords, API keys, subjects, bodies (D073). `--github` opens pre-filled issue. `--clipboard`/`--stdout`/`--edit` output modes work. Log retention defaults configured (D074). `mxr logs --purge` works. `mxr doctor --store-stats` reports log disk usage.
5. **Binary releases**: GitHub Actions release workflow runs on tag push. Produces static musl binaries for Linux x86_64, Linux aarch64, and native macOS x86_64, macOS aarch64. SHA256 checksums generated. GitHub Release created with binaries, checksums, and changelog. Homebrew formula auto-updated via PR. Docs site deployed on release.
6. **Install methods**: `cargo install mxr` works. `cargo binstall mxr` installs pre-built binary. Homebrew formula in `planetaryescape/homebrew-mxr` installs correctly with auto-update on release. AUR PKGBUILD builds and installs. Nix flake builds and runs. Install script works on macOS and Linux.
7. **Crates.io publishing**: All workspace crates publish in dependency order via `cargo-workspaces`. Version verification ensures tag matches Cargo.toml. `CARGO_REGISTRY_TOKEN` secret configured.
8. **Changelog**: `cliff.toml` configured. `git-cliff` generates grouped changelog from conventional commits. Commit-lint enforced in CI.
9. **Documentation site**: mdBook site builds from `docs/book/`. Deployed to GitHub Pages via CI and on release to Cloudflare Pages. Contains: installation, getting started, user guide, configuration reference (incl. `[logging]` section), keybinding reference (vim+Gmail hierarchy explained per A005), search syntax, rules syntax, complete CLI reference (all commands from A004), observability & monitoring guide (`mxr logs`/`status`/`events` per A006), adapter development guide, FAQ. Search works.
10. **README**: Project description, differentiation table, screenshots/GIFs, quick start, install methods, feature highlights, CLI scriptability examples (from A004), links to docs, license, contributing link.
11. **Announcement ready**: Blog post drafted. Demo GIF created. Launch checklist passed. Target channels identified.
12. **CI passes**: All workflows green — `ci.yml` (fmt, clippy, test, build, sqlx-check, docs build, policy sync, commit lint), `release.yml` (binary builds, crates.io publish, GitHub Release, Homebrew update, docs deploy), `docs.yml` (mdBook deploy). All required GitHub secrets configured.

### User Acceptance Test

A new user can:
- Discover mxr via README or blog post
- Install via their preferred method (cargo, brew, binary, nix)
- Add their Gmail account following the getting-started guide
- Use mxr as their email client, referring to the docs site for advanced features
- A Rust developer can build a community adapter by following the adapter guide, running conformance tests, and publishing a standalone crate

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Google OAuth verification required for >100 users | Users can't authenticate without creating their own GCP project | Document self-hosted OAuth setup clearly. Provide instructions to create own GCP project + credentials. Long-term: apply for Google verification. |
| `mxr-core` API instability after publish | Breaking changes force community adapters to update | Audit API surface thoroughly before publish. Use `#[non_exhaustive]` liberally. Start at `0.1.0` to signal instability. Add default trait method implementations for new methods (non-breaking). |
| Cross-compilation failures for Linux aarch64 | Missing release binary for ARM Linux | Use cargo-zigbuild which handles cross-compilation reliably. Test in CI before first release. Fallback: `cargo install` always works from source. |
| Homebrew formula SHA256 mismatch after release | Brew install fails | Automate SHA256 update via CI. Manual fallback: update formula within hours of release. |
| mdBook site search inadequate for large docs | Users can't find what they need | mdBook search is good enough for this doc size. If inadequate, add Algolia DocSearch later. |
| Demo GIF becomes stale as TUI evolves | README shows outdated UI | Use vhs tape scripts (reproducible). Re-record as part of release checklist. |
| Hacker News launch gets no traction | Low visibility | Have concrete differentiators in the post (not "yet another email client"). Show real workflows, not architecture diagrams. Post at optimal HN time (Tuesday-Thursday, 9-11am ET). Have friends upvote early. |
| AUR/Nix packages break on updates | Users on those platforms get broken installs | Automated testing in CI for flake.nix builds. AUR: include checksums, test build before publishing. Pin dependencies. |
| Install script security concerns (curl pipe sh) | Users wary of the pattern | Script verifies SHA256 checksums. Offer alternative install methods prominently. Script is auditable (simple bash, no obfuscation). |
| crates.io name squatting or conflict | Can't publish as `mxr` | Name `mxr` is already confirmed available on crates.io (per 00-overview.md). Publish placeholder early if concerned. |

---

## File Summary

Files created or modified in this phase:

| File | Action | Description |
|------|--------|-------------|
| `crates/providers/fake/src/fixtures.rs` | Create | Canonical fixture data (messages, threads, labels) |
| `crates/providers/fake/src/conformance.rs` | Create | Conformance test suite for adapter validation |
| `crates/providers/fake/src/lib.rs` | Modify | Export fixtures and conformance modules, add crate docs |
| `crates/core/Cargo.toml` | Modify | Add crates.io metadata, prepare for publish |
| `crates/core/README.md` | Create | Crate-level README for crates.io |
| `examples/adapter-skeleton/Cargo.toml` | Create | Out-of-tree adapter template |
| `examples/adapter-skeleton/src/lib.rs` | Create | Skeleton adapter implementation with todo!() stubs |
| `CONTRIBUTING.md` | Create | Full contributor guide |
| `.github/ISSUE_TEMPLATE/bug_report.yml` | Create | Bug report template (updated for `mxr bug-report` integration, A009) |
| `crates/cli/src/bug_report.rs` | Create | Bug report generator, sanitizer, CLI subcommand (A009, D072-D073) |
| `.github/ISSUE_TEMPLATE/feature_request.yml` | Create | Feature request template |
| `.github/ISSUE_TEMPLATE/adapter_proposal.yml` | Create | Adapter proposal template |
| `.github/workflows/release.yml` | Create | Full release pipeline: musl binaries (4 targets), crates.io publish, GitHub Release, Homebrew update, docs deploy |
| `.github/workflows/ci.yml` | Modify | Comprehensive CI: fmt, clippy, test (multi-OS), build, sqlx-check, docs build, policy sync, commit lint |
| `.github/workflows/docs.yml` | Create | mdBook deploy to GitHub Pages |
| `cliff.toml` | Create | git-cliff configuration for changelog generation |
| `.commitlintrc.yml` | Create | Conventional commit message lint config |
| `packaging/aur/mxr/PKGBUILD` | Create | AUR package (build from source) |
| `packaging/aur/mxr-bin/PKGBUILD` | Create | AUR package (pre-built binary) |
| `packaging/homebrew/mxr.rb` | Create | Homebrew formula template |
| `docs/book/book.toml` | Create | mdBook configuration |
| `docs/book/src/SUMMARY.md` | Create | Documentation table of contents |
| `docs/book/src/reference/observability.md` | Create | Observability & monitoring guide (A006) |
| `docs/book/src/**/*.md` | Create | All documentation pages (~26 files) |
| `docs/guide/adapter-development.md` | Create | Adapter development guide (source for mdBook) |
| `docs/assets/demo.tape` | Create | vhs script for demo GIF |
| `docs/assets/*.gif` | Create | Screenshots and demo GIFs |
| `docs/announcement/launch-post.md` | Create | Blog post draft |
| `README.md` | Rewrite | Full README overhaul |
| `install.sh` | Create | Quick install script |
| `flake.nix` | Create | Nix flake for build + dev shell |
| `Cargo.toml` (root) | Modify | Add crates.io metadata for binary crate |
