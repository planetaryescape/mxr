---
title: MSP ↔ mxr alignment audit
status: discovery
last_updated: 2026-05-17
companion: spec.md
---

# MSP ↔ mxr alignment audit

> Gap analysis between mxr's current sync architecture and the MSP
> v0.1 draft. Each gap classified **cheap** (< 4 hours), **medium**
> (1-2 days), or **expensive** (> 1 week).
>
> **Goal:** identify which refactors would let mxr serve as a
> credible MSP reference implementation — without committing to
> doing them in this session.

## TL;DR

mxr is **closer to MSP than expected**. The daemon already has the
protocol's three-layer shape (client ↔ daemon ↔ provider adapter
trait). The biggest gaps are:

1. **`SyncCursor` is a tagged union over provider variants** rather
   than opaque to the client (mxr's daemon switches on the variant).
   Medium refactor.
2. **`SyncCapabilities` is a flat boolean soup** (DAP-style) rather
   than namespaced (MSP/LSP-style). Cheap rename.
3. **`fetch_message` is optional** with a `None` default; MSP makes
   `fetch_body` foundational. Medium refactor — needs Gmail/IMAP/
   Outlook adapters to implement it consistently.
4. **No `mutation_id` for idempotent mutation replay.** Medium
   addition.
5. **Push: mxr's `IdleWatcher` is provider-internal**; MSP's
   `subscribe_changes` returns an opaque subscription with a
   resume token. Medium refactor.
6. **Bodies stream alongside envelopes in `SyncBatch`.** MSP
   separates them. Expensive refactor — affects every sync codepath.
7. **Authentication is provider-trait-internal.** MSP punts on
   auth in v0.1 anyway, so this gap is "by design" until we spec it.

Total estimate to align mxr fully: ~2-3 weeks of focused refactor
work. Most of it falls out of normal sync hygiene improvements that
would benefit mxr regardless of MSP.

---

## Method

For each MSP section, two passes:

1. **Read pass:** where in mxr does the equivalent concept live?
   Cite file paths.
2. **Gap pass:** what's different? Classify the refactor.

CLI-driven validation deferred. The static analysis below is
sufficient for v0.1 of the alignment audit. A future revision can
add `mxr events --tail` traces against the fake provider to verify
the runtime behaviour matches the static reading.

---

## Section-by-section audit

### MSP §2.1 — Account

**mxr today:** `AccountId` newtype in `crates/core/src/id.rs`.
Adapter-owned via the `MailSyncProvider::account_id()` method
(`crates/core/src/provider.rs:11`).

**Gap:** None of substance. mxr's `AccountId` is opaque to consumers.

**Refactor cost:** zero.

### MSP §2.2 — AccountState (opaque cursor)

**mxr today:** `SyncCursor` enum at
`crates/core/src/types.rs:1751-1768`:

```rust
pub enum SyncCursor {
    Gmail { history_id: u64 },
    GmailBackfill { history_id: u64, page_token: String },
    Imap { uid_validity: u32, uid_next: u32, mailboxes: Vec<...>, capabilities: ... },
    Initial,
}
```

The cursor is a **tagged union** the daemon pattern-matches on
(`crates/sync/src/engine.rs:243-264` switches on Gmail vs Imap).
That's the opposite of MSP's "opaque to the client" rule.

**Gap:** the daemon shouldn't know the cursor's shape. The cursor
should be opaque bytes (serialised by the adapter, persisted by the
daemon, passed back unchanged).

**Refactor:** change `SyncCursor` to `pub struct SyncCursor(Vec<u8>)`
(or a wrapper around `serde_json::Value`). Move the variant-specific
logic into the adapters. The daemon does NOT switch on it.

The Gmail "cursor not found, reset to Initial" recovery
(`crates/sync/src/engine.rs:243`) can become a typed error
(`MxrError::SyncCursorExpired`) the adapter returns, mapping cleanly
to MSP's `msp.sync.cannot_calculate_changes`.

**Cost: medium.** ~1-2 days. Touches `mxr-core`, `mxr-sync`,
`mxr-store`, all three provider crates, and one daemon handler. No
schema migration (the persisted form is already JSON).

**Side benefit:** the daemon stops being coupled to which providers
exist. Adding a new provider (e.g. JMAP) doesn't require a new
`SyncCursor` variant; the adapter chooses its own serialisation.

### MSP §2.3 — Folder

**mxr today:** `Label` type in `crates/core/src/types.rs`. The
`MailSyncProvider::sync_labels()` returns `Vec<Label>`. Provider-
side, IMAP has SPECIAL-USE handling in
`crates/provider-imap/src/session.rs:796-820`; Gmail flattens labels
into the same Label shape.

**Gap:** mxr's `Label` type lacks an explicit `role` enum. The
SPECIAL-USE strings (`\Inbox`, `\Sent`, etc.) live in a `special_use:
Option<String>` field. MSP wants a typed `Role` enum.

**Refactor:** add `Role` enum to mxr-core; populate from the
existing `special_use` strings in the adapters.

**Cost: cheap.** ~2-3 hours. The `role` is derived, not new state.
Pure addition.

### MSP §2.4 — Message + lazy body fetch

**mxr today:** `SyncedMessage` carries `envelope + body` together
(`crates/core/src/types.rs:1776-1779`). Bodies fetch eagerly during
sync (the comment at line 1773 says "no lazy hydration").

**Gap:** MSP separates body from sync delta. Clients fetch bodies
lazily via `fetch_body`. mxr's current model streams bodies eagerly.

**Why mxr does it eagerly:** the comment says "opening a message is
a pure SQLite read — no network call, no loading state." This is a
deliberate UX choice (the local-first principle in `AGENTS.md` rule
3).

**Refactor consideration:** mxr's choice is good for mxr but not
necessarily for every MSP client. A web client may want lazy bodies;
a Maildir-sync tool may want eager.

**Resolution:** make body fetch a **capability**. Adapters that
support lazy fetch advertise it; clients that want eager fetch
either don't claim the capability or ignore it and `fetch_body`
immediately. mxr keeps its eager behaviour internally by setting
its client capability accordingly.

**Cost: expensive.** ~1-2 weeks. Affects every sync codepath and
the daemon's body-cache logic. But it's optional — mxr can keep
eager fetch and still be MSP-compatible by negotiating that
capability.

### MSP §2.5 — Thread

**mxr today:** `Envelope` carries `thread_id`. mail-threading
(extracted earlier) does the JWZ-style fallback when adapters don't
provide native threading. Gmail provides `threadId`; IMAP doesn't.

**Gap:** mxr's thread is implicit (envelope carries thread_id);
MSP's Thread is explicit (`{id, message_ids}`). Adapters that don't
support native threading would still need to surface thread shape to
clients.

**Refactor:** introduce a `Thread` type alongside `Envelope`;
populate it from native provider data where available, from
mail-threading otherwise.

**Cost: medium.** ~1 day. mail-threading already does the work; we
just need to expose it through the protocol.

### MSP §2.6 — Flag

**mxr today** (post-Phase-E): `Envelope.flags: MessageFlags` (u32
bitfield for the 8 system flags) + parallel
`Envelope.keywords: BTreeSet<String>` for free-form IMAP atoms.
IMAP adapter captures keywords from FETCH FLAGS and emits them via
`UID STORE +/-FLAGS (...)`. Gmail adapter ignores keywords and
advertises `capabilities.mutate.custom_keywords = false`.

**Resolved in Phase E (2026-05-18):**

- New `keywords: BTreeSet<String>` field on Envelope. New
  `message_keywords` junction table (migration 041, foreign-keyed
  to messages so deletes cascade). Hydration happens batch-wise on
  every envelope read.
- `Mutation::SetKeywords { add, remove }` enum variant; IMAP emits
  it as `UID STORE`, Gmail returns a typed error, Fake records it.
- New `MutateCaps.custom_keywords: bool` capability. IMAP + Fake =
  true; Gmail = false.
- IMAP parse split: `flags_from_imap` → `flags_and_keywords_from_imap`
  returning a tuple. The old function stays as a one-line wrapper for
  callers that only care about system flags.
- Search: new `keywords` STRING field in the Tantivy schema, indexed
  during ingest, queryable via `is:$foo` syntax which routes through
  `FilterKind::Custom` when the name starts with `$`.
- **Decision:** kept the bitfield + parallel keyword set rather than
  unifying into one JMAP-style keyword set. Local-store clients
  upsert-by-id; the split keeps existing `envelope.flags.contains(...)`
  call sites unchanged.
- **Decision:** Gmail drops keywords on the floor (no
  `mxr/keywords/<name>` label-namespace synthesis). The capability
  bit makes the limitation explicit; cross-provider sync that
  involves Gmail loses keywords.

**Cost (delivered): medium.** Single atomic commit covering the
core type, migration, all three adapters, sync engine, search,
and ~25 test mocks.

### MSP §2.7 — Mutation

**mxr today** (post-Phase-D): single trait method
`apply_mutation(mutation_id, Mutation)` on `MailSyncProvider`.
`Mutation` is the batched per-message enum
(`ModifyLabels`/`Trash`/`SetRead`/`SetStarred`); MSP spec §2.7
updated to match.

**Resolved in Phase D (2026-05-17):**

- Four per-method trait functions (`modify_labels`, `trash`,
  `set_read`, `set_starred`) collapsed into one
  `apply_mutation(mutation_id, &Mutation)`.
- New `mutation_dedup_log` SQLite table (migration 040, 24h TTL)
  keyed on `(mutation_id, provider_message_id)`. Daemon checks
  the log before each provider call; replays of an
  already-applied mutation become no-ops.
- `ReadAndArchive` fans out into SetRead + ModifyLabels under one
  mutation_id; daemon disambiguates the two dedup rows by
  suffixing the provider_message_id (`${pid}#read` /
  `${pid}#labels`).
- 8-protocol survey (JMAP, MS Graph, Drive, WebDAV, CloudKit,
  Matrix, Notion, Stripe) confirmed: batched per-message enum is
  idiomatic for local-store clients; granular per-flag variants
  only help stateless protocols.
- Adapter-side in-memory dedup rejected in favour of daemon-side
  persisted dedup for cross-provider consistency and crash-safety.

**Cost (delivered): medium.** Single atomic commit covering the
trait change, migration, daemon dispatch, and three adapters.

### MSP §2.8 — SyncDelta

**mxr today** (post-Phase-C): `SyncBatch` at
`crates/core/src/types.rs`:

```rust
pub struct SyncBatch {
    pub upserted: Vec<SyncedMessage>,
    pub deleted_provider_ids: Vec<String>,
    pub label_changes: Vec<LabelChange>,
    pub next_cursor: SyncCursor,
    pub has_more: bool,
}
```

**Gap:** Close to MSP's `SyncDelta`. Remaining differences:

- No `threads_changed` or `folders_changed` — mxr handles labels via
  `label_changes` and threads implicitly via envelope `thread_id`.

**Resolved in Phase C (2026-05-17):**

- `has_more: bool` added; daemon's sync loop now sets
  `skip_sleep = outcome.has_more` and re-polls immediately on
  truncated batches (multi-page Gmail backfill finishes in minutes
  rather than hours).
- Decision: **do not split `upserted` into added vs changed.**
  Survey of 8 sync protocols (JMAP, MS Graph, Drive, WebDAV,
  CloudKit, Matrix, Notion, Linear) showed only JMAP splits, and
  only because JMAP returns IDs alone and defers object retrieval.
  Every protocol with a local id-keyed store merges added+changed
  because the client upserts-by-id anyway — which mxr already does
  via `(account_id, provider_id)` upsert in
  `crates/store/src/message.rs`. Splitting would double array
  counts for no semantic gain. MSP spec §2.8 was updated to match.

**Cost (delivered): small.** ~half-day, single atomic commit.

### MSP §3 — Wire protocol (JSON-RPC framing)

**mxr today:** `IpcMessage { id, payload }` with length-delimited
JSON over Unix socket (`crates/protocol/src/types.rs:306-320`). Not
quite JSON-RPC 2.0 — closer to a custom envelope.

**Gap:** mxr's IPC isn't JSON-RPC. It's a custom request/response
shape on top of `IpcPayload`.

**Refactor:** the daemon's IPC contract can stay as-is for now;
MSP is a different layer. If/when mxr's daemon becomes an MSP
client (subprocessing adapters), the daemon converts its internal
IPC requests into MSP method calls. The internal IPC doesn't need
to change.

**Cost: zero for alignment.** mxr's daemon-internal protocol is
private. MSP only affects the adapter-facing edge.

### MSP §4 — Capabilities

**mxr today:** `SyncCapabilities` struct
(`crates/core/src/types.rs:1833-1843`):

```rust
pub struct SyncCapabilities {
    pub labels: bool,
    pub server_search: bool,
    pub delta_sync: bool,
    pub push: bool,
    pub batch_operations: bool,
    pub native_thread_ids: bool,
}
```

Flat boolean soup. Same shape DAP used; same problems.

**Gap:** MSP wants namespaced capabilities (LSP-style):
`{sync: {delta, native_threading, ...}, mutate: {labels,
atomic_move, ...}, push: {streaming, ...}}`.

**Refactor:** restructure `SyncCapabilities` into a nested struct.

**Cost: cheap.** ~2-3 hours. Pure rename/restructure; same
information, different shape.

### MSP §5 — Resumability

**mxr today:** Read resumability: `SyncCursor` persisted in
`crates/store/src/sync_cursor.rs`. Re-loaded on daemon startup;
passed into next sync. Matches MSP's "state is the resumability
story."

Write resumability: mxr has a mutation queue for offline mutations
(check `crates/daemon/`). The daemon flushes queued mutations on
reconnect.

**Gap:** the cursor-too-old recovery is special-cased for Gmail
(`crates/sync/src/engine.rs:243-264`). MSP wants a typed error
that adapters return when their state is unrecoverable.

**Refactor:** add `MxrError::SyncCursorExpired` variant; adapters
return it instead of `NotFound`; the daemon's recovery code becomes
provider-agnostic.

**Cost: cheap.** ~3-4 hours. Single error variant + a
match-case-removal.

### MSP §6 — Push notifications

**mxr today:** `MailSyncProvider::idle_watch()` returns
`Option<Box<dyn IdleWatcher>>` (`crates/core/src/provider.rs:78-80`).
The watcher emits `Result<()>` via `next_event()` — no payload.
Daemon calls `sync_account` on each event.

**Gap:** mxr's shape is **already very close to MSP's
`subscribe_changes`** — events carry no payload, client fetches
state via the sync method. The differences:

- mxr's watcher is provider-side (returned by `idle_watch`); MSP's
  is a protocol-level subscription with an opaque id + resume token.
- mxr doesn't have a `state_hint` in the event (the daemon just
  triggers a full delta sync).

**Refactor:**
- Add an opaque `subscription_id: String` to the watcher.
- Add a `pushState` resume token (JMAP RFC 8887's contribution).
- Optionally add a `scope` field to events for fine-grained
  invalidation.

**Cost: medium.** ~1 day. Tweaks to `IdleWatcher` trait + Gmail/
IMAP implementations.

### MSP §7 — Conformance

**mxr today:** No conformance harness. Provider implementations are
tested ad-hoc (Gmail tests, IMAP tests).

**Gap:** MSP requires a conformance harness that adapters run
against their binary.

**Refactor:** N/A — this is greenfield work for the protocol, not
something mxr currently has.

**Cost:** out of scope for alignment. If we ship the protocol, the
conformance harness ships with it.

### MSP §8 — Open questions

**Authentication.** mxr handles auth provider-internally
(`authenticate` / `refresh_auth` on `MailSyncProvider`). MSP punts
on auth in v0.1. The current behaviour stays.

**Calendar + contacts.** mxr has calendar-invite stubs (added
recently); contacts via `mxr-relationship` (mxr-shaped). Neither
fits MSP's mail-only scope.

**Compose / send.** mxr has `MailSendProvider` as a separate trait;
MSP punts on this too. Symmetric story for a future "Mail Submit
Protocol".

---

## Cost summary

| Gap | Cost | Side benefit for mxr |
|-----|------|----------------------|
| Opaque SyncCursor | medium (1-2 days) | provider-agnostic daemon |
| Namespaced capabilities | cheap (2-3 hours) | clearer code |
| Lazy fetch capability | expensive (1-2 weeks) | optional — keep eager |
| Thread shape | medium (1 day) | better external API |
| Custom keywords | medium (1-2 days) | feature parity with Dovecot |
| Unified Mutation + ID | medium (1-2 days) | idempotent retries |
| Split SyncDelta added/changed | medium (1-2 days) | clearer semantics |
| Restructure capabilities | cheap (2-3 hours) | clarity |
| Typed SyncCursorExpired | cheap (3-4 hours) | removes Gmail special case |
| Push subscription_id + resume | medium (1 day) | resilient push |

**Realistic phasing:**

- **Phase A — Cheap wins** (~1 day total): namespaced capabilities,
  `SyncCursorExpired` error, `Role` enum on Folder. Pure clean-ups
  with no behaviour change. mxr is strictly better afterwards.
- **Phase B — Opaque cursor** (~2 days): the biggest single
  architectural win. Decouples daemon from provider specifics.
  Strict win for mxr regardless of MSP.
- **Phase C — Mutation unification** (~2 days): collapse the four
  mutation methods into `apply_mutation`. Adds `mutation_id`.
- **Phase D — Push subscription** (~1 day): subscription_id +
  resume token on `IdleWatcher`.
- **Phase E — Schema for keywords** (~2 days): real custom-keyword
  support.
- **Phase F — Sync delta shape** (~2 days): split added/changed,
  add has_more.
- **Phase G — Lazy body fetch** (~1-2 weeks): optional. mxr can
  skip indefinitely while still being MSP-shaped (eager-fetch is a
  valid client capability).

Phases A-F are all things mxr would benefit from doing regardless
of whether MSP ships as a public protocol. They're architectural
hygiene improvements that fall out of "design from the contract
inward."

**Total estimated cost for MSP-shape mxr (excluding G):** ~10
focused days. Reasonable to spread across 1-2 months alongside
normal work.
