---
candidate: sync-engine
status: tier-3
decision: investigate-later
mxr_source: crates/sync/, crates/store/, crates/provider-gmail/, crates/provider-imap/, crates/provider-outlook/
last_reviewed: 2026-05-16
audit_notes: |
  Held as investigate (not won't-do) because impact is highest of any
  candidate — every Rust mail client reinvents this. But extraction risk
  is also highest: tight coupling to mxr's MailSyncProvider trait, store
  schema, and search indexing. Do not commit until extraction boundaries
  are proven via a 2-3 day discovery. The right trigger is "mxr's own sync
  refactor has stabilised and the trait surface is clean enough that
  carving doesn't fight the architecture."
---

# `mail-sync-engine` (proposed name, very provisional)

> A local-first email synchronisation engine. Reconcile remote provider
> state (IMAP UIDVALIDITY/CONDSTORE/QRESYNC, Gmail historyId, JMAP state
> cursors) with a local SQLite store. Handle offline queues, conflict
> resolution, and resumable sync. Like `matrix-sdk-store-sqlite` but for
> mail.

## Decision: **Tier 3 — investigate, do not commit**

By **impact**, this is the largest unfilled gap in the Rust email
ecosystem. No published crate solves it. Every Rust mail client that
wants offline-capable sync (mxr included) rolls its own.

By **extraction risk**, this is also the highest. mxr's sync code is
deeply intertwined with its store schema, provider trait, and protocol
types. Lifting it out cleanly is non-trivial and might force compromises
that hurt mxr's internal velocity.

**Action:** investigate. Don't commit to shipping until we've done a
dedicated discovery pass to answer: is there a clean abstraction
boundary, or is the cost of refactoring to it higher than its
external value?

## What mxr has today

**Sources:**
- `crates/sync/` — orchestration, change reconciliation, threading
- `crates/store/` — SQLite schema, message/envelope/attachment tables,
  account state, sync cursors
- `crates/provider-gmail/`, `crates/provider-imap/`,
  `crates/provider-outlook/`, `crates/provider-fake/` — provider
  adapters implementing the `MailSyncProvider` trait from `mxr-core`

Behaviours implemented:

- IMAP: UIDVALIDITY drift detection, CONDSTORE-based incremental sync,
  QRESYNC where supported, EXPUNGE handling, label/flag sync
- Gmail: `historyId` cursor, batched message GETs, label change
  reconciliation, threadId stitching
- Outlook: delta-query cursors via Microsoft Graph
- Local store: append-only-style envelope storage with idempotent
  inserts, attachment indirection, full-text index hooks, semantic
  index hooks
- Offline queue: actions taken offline (archive, label, flag) queued and
  flushed on reconnect
- Conflict resolution: last-writer-wins per field with provider-side
  authority
- Resumability: every sync cycle is checkpointed; restart picks up where
  it left off

This is the work of multiple person-months. It is also where most of
the bugs in mxr have lived; the surface area is large.

## Ecosystem state

| Candidate | Coverage |
|---|---|
| `async-imap` / `imap` | Protocol primitives only; no sync engine |
| `jmap-client` (stalwart) | Protocol client only; no local persistence |
| `google-gmail1` | Auto-generated REST bindings; no sync engine |
| `matrix-sdk-store-sqlite` | Matrix-specific; not email |
| `notmuch` | C++, file-based, no remote sync — purely local indexing |
| `offlineimap` / `mbsync` | C/Python apps, not Rust libraries |

**No published Rust crate provides a complete sync engine.** The gap is
large by impact (any Rust mail client benefits) and unfilled.

## Why extraction is hard

Three coupling axes make this expensive to lift:

### 1. The `MailSyncProvider` trait is mxr-shaped

The trait sits in `mxr-core` and bakes in mxr's view of an account, an
envelope, a flag set, and a thread. Extracting the engine without the
trait means re-inventing the trait. The new trait might not match
`MailSyncProvider` 1:1, which would force a refactor on mxr's side.

### 2. The store schema is mxr-specific

mxr's SQLite schema includes columns for safety scores, relationship
metadata, owed-reply state, reply-later queues — features that don't
belong in a generic sync engine. Either:

- The extracted engine assumes its own minimal schema and mxr keeps
  mxr-specific tables in a separate database (split-store overhead), or
- The engine becomes parameterisable over schema (significant
  abstraction cost), or
- The engine carries the mxr schema (defeats the point of extraction).

None of these are obviously cheap.

### 3. Tight coupling to search and semantic indexing

Sync drives full-text and semantic indexing in mxr. The lifecycle hooks
("a new message was synced", "a message was expunged") are part of
mxr's sync orchestration. Cleanly separating these from raw sync logic
is more refactoring.

## What investigation needs to answer

Before committing to extraction, a focused 2–3 day discovery should
answer:

1. **Is there a stable provider trait the engine could expose** that
   mxr can still implement underneath without contortions?
2. **Is the schema separable?** Can we draw a clean line between
   "generic mail sync schema" (envelopes, threads, flags, cursors) and
   "mxr application schema" (safety, relationships, queues)?
3. **What's the right backend abstraction?** Force SQLite, or expose a
   `Storage` trait users implement against their own DB?
4. **Async runtime assumptions.** mxr is `tokio`. A generic crate should
   probably be runtime-agnostic via `async-trait` or async-fn-in-trait.
   Is the current code structured to allow this?
5. **What's the realistic adopter list?** If only mxr would use it,
   extraction is busywork. If we can identify 3+ plausible external
   adopters (e.g. a TUI mail client like himalaya, a desktop client
   experiment, a CLI archive sync tool), the case strengthens.

## Provisional shape (if/when we ship)

```rust
// What the engine owns.
pub struct SyncEngine<S: Storage, P: Provider> {
    storage: S,
    provider: P,
    // ...
}

// What providers implement.
pub trait Provider: Send + Sync {
    type AccountState: Serialize + DeserializeOwned;
    async fn list_changes(&self, state: &Self::AccountState)
        -> Result<ChangeBatch>;
    async fn fetch(&self, ids: &[MessageId]) -> Result<Vec<Envelope>>;
    async fn apply(&self, action: Action) -> Result<()>;
}

// What storage backends implement.
pub trait Storage: Send + Sync {
    async fn upsert_envelope(&self, env: Envelope) -> Result<()>;
    async fn delete_envelope(&self, id: MessageId) -> Result<()>;
    async fn cursor_for(&self, account: AccountId)
        -> Result<Option<SerializedCursor>>;
    async fn set_cursor(&self, account: AccountId, cursor: SerializedCursor)
        -> Result<()>;
    // ...
}

// Engine API.
impl<S, P> SyncEngine<S, P> {
    pub async fn sync_once(&self, account: AccountId) -> Result<SyncReport>;
    pub async fn enqueue(&self, action: Action) -> Result<()>;
    pub async fn flush_queue(&self, account: AccountId) -> Result<()>;
}
```

The cost: two big trait surfaces that need to be stable. Get them
wrong and every release is a breaking change.

## Risks specific to this candidate

- **Refactor cost may exceed external value.** If mxr's internal trait
  is awkward to share, we're refactoring our own working system to pay
  for a hypothetical external user base.
- **Maintenance load is high.** Sync engines accumulate edge cases
  forever. Every new provider quirk lands in issues.
- **API stability pressure.** Once published, breaking changes to the
  Provider/Storage traits hurt every downstream user. A reckless 1.0 is
  worse than not shipping.

## Risks of *not* shipping

- mxr's sync logic stays buried and undertested compared to a public
  crate.
- Other Rust mail-client projects continue to reinvent the same wheel
  with the same bugs.
- We forfeit the credibility signal a well-known engine would give mxr.

## When to re-evaluate

Trigger conditions to move this to Tier 1 or Tier 2:

1. mxr's sync internals stabilise and we go a quarter without major
   refactors. (Currently the area is too live.)
2. A clear external user appears who would adopt our engine.
3. We're doing a non-trivial sync refactor anyway, at which point the
   marginal cost of a clean public API is small.
4. Someone else starts publishing a competing crate. Decide whether to
   contribute there or compete.

## Naming (when we get there)

Candidates:

- `mail-sync-engine` — descriptive
- `mailsync` — taken (a different project)
- `mxr-sync` — too internal
- `mailbox-sync` — okay
- `imap-sync` — too narrow

To be decided during the investigation phase.

## TL;DR

Highest impact, highest risk. Don't ship today. Do a focused investigation
when sync internals stabilise. Until then, keep extracting the small
clean pieces (threading, query parser, list-unsubscribe) that demonstrate
quality without committing to maintaining a sprawling engine.
