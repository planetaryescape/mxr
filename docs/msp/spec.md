---
title: Mail Sync Protocol (MSP) v0.1 draft
status: draft
authors: planetaryescape/mxr
license: CC BY 4.0
last_updated: 2026-05-17
---

# Mail Sync Protocol (MSP) v0.1 — draft

> A wire protocol between a mail **client** (UI, daemon, CLI) and a
> **provider adapter** (a process that speaks one provider's native
> API and translates to MSP). Inspired by the
> [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/specification).

This is a draft. Section 8 lists what v0.1 deliberately does not cover.

---

## 1. Motivation

Every mail client reinvents sync. Gmail's REST API, IMAP's UID matrix,
JMAP's state cursors, Microsoft Graph's deltas — each is wrapped
individually inside every mail client that exists, in every language.
The N×M problem (every client × every provider) currently has no
shared solution at the client layer.

DAP showed that a wire protocol can collapse this problem. Each
language ships one debug adapter; each editor speaks DAP; debugging
works everywhere. The N×M problem becomes N+M.

MSP is the same shape for mail. A protocol the client speaks; per-
provider adapter binaries on the other side; mail clients in any
language, written by anyone, talk to any provider.

**Three reasons a protocol beats a Rust library here:**

1. **Language reach.** Mail clients exist in Rust, Go, Swift,
   TypeScript, Python, C++, ObjC. A Rust library locks out the
   majority of the audience. A wire protocol is a JSON contract any
   language can implement.

2. **Provider SDK fit.** Each provider has a canonical SDK in some
   native language (Google's Gmail SDK in Go/Java/Python; Microsoft
   Graph in C#/.NET; Apple's MailKit in Objective-C). An adapter
   written in the provider's native language leans on the official
   SDK rather than reverse-engineering its REST shape.

3. **Adoption gradient.** Protocol adoption is "ship one adapter,
   unlock every client." Library adoption is "rewrite your sync loop
   in our trait shape." The first is a lower switching cost for
   everyone who isn't us.

The DAP analogy isn't perfect. Mail is statefuller (long-lived
sessions, bodies streaming over hours, push notifications) and the
provider semantics diverge harder (Gmail's labels are not IMAP
folders are not JMAP roles). But the protocol shape — JSON-RPC,
capabilities negotiation, opaque state cursors — transfers cleanly.

---

## 2. Abstract model

Every adapter maps its provider into these concepts. Clients only see
these. The set is intentionally small; anything mxr-specific (or any
client-specific) lives **above** the protocol.

### Account

Opaque identifier scoped to one provider instance for one user.
Clients persist `(adapter_id, account_id)` to address a given account
between sessions. The adapter chooses the format of `account_id`.

### AccountState

An opaque adapter-owned cursor blob. The client persists it but never
parses it. It carries whatever the adapter needs to compute the next
sync delta (Gmail's `historyId`, IMAP's `(uid_validity, modseq)`,
JMAP's `state` token, Outlook's delta link, etc.).

**Rule:** if the adapter cannot compute changes from a given
`AccountState` (e.g. cursor too old), it returns the well-known
`cannotCalculateChanges` error and the client triggers a full
re-sync.

### Folder

Has a stable `id` (adapter-owned), a `display_name`, a `role`, and an
optional `parent_id` (for hierarchy).

```
Role :: Inbox | Sent | Drafts | Trash | Spam | Archive
      | Important | AllMail | Outbox | Custom
```

The Gmail-labels-vs-IMAP-folders divergence resolves here: adapters
either flatten labels into folders (Gmail) or expose folders directly
(IMAP). Roles are how clients identify the conceptual location of a
message regardless of how the provider models it.

### Message

```
Message {
  id:                  String            // adapter-owned, stable
  envelope:            Envelope          // from/to/cc/bcc/subject/date/message_id/in_reply_to/references
  body_refs:           Option<BodyRefs>  // present if previously fetched; null otherwise
  flags:               Set<Flag>
  folder_ids:          Vec<FolderId>     // membership; multi-entry for label-style providers
  thread_id:           Option<ThreadId>
  size_bytes:          Option<u64>
  attachments:         Vec<AttachmentMeta>
}
```

Body delivery is **a capability the adapter and client negotiate**,
not a spec mandate. Two valid modes:

- **Eager (default).** The adapter includes the body alongside the
  envelope in `SyncDelta.messages_changed`. Opening a message becomes
  a local read — no spinner, no per-message round-trip. Best for
  local-first clients (mxr, mutt-style TUIs, Maildir tooling) and
  for offline-first UX. Adapters that fetch bodies cheaply during
  sync anyway (Gmail's `format=full`, IMAP `BODY[]` while the
  session is open) have no reason to defer.
- **Lazy.** The adapter omits bodies from the delta and the client
  calls `fetch_body(message_id)` on demand. Best for thin-web clients
  that don't want to persist full corpora, for cursor catch-up on
  massive backlogs, and for adapters whose body fetch is metered or
  rate-limited separately from envelope listing.

Negotiation:

- Client advertises `bodies.prefer = "eager" | "lazy"` in
  `initialize`.
- Adapter advertises `bodies.modes = ["eager", "lazy"]` (subset it
  supports) and the client picks one for the session.
- If the adapter can't satisfy the client's preference, it falls
  back to whichever mode it does support and the client adapts.

`fetch_body(message_id)` is always callable (the eager-mode client
may still need it for cursor-reset re-hydration, attachment streams,
or messages older than the local body cache). Adapters MAY omit
support only if they ALSO advertise `bodies.modes = ["eager"]`
exclusively — in which case the body is always present in the
delta and there is nothing to fetch.

MSP takes no position on which mode is "correct." Local-first
clients are not a degenerate case to be retrofitted; they are
first-class. Equally, lazy-fetch clients are first-class.

### Thread

```
Thread {
  id:           ThreadId
  message_ids:  Vec<MessageId>   // ordered by date
}
```

Whether the adapter computes threading server-side (Gmail's `threadId`)
or constructs it from References/In-Reply-To (IMAP) is opaque to the
client. Adapters that can't thread MUST still return a Thread per
message (a singleton thread with `message_ids = [msg.id]`).

`message_ids` is a **flat list ordered by date ascending**; equal-date
ties broken by `MessageId` ascending (RFC 8621 §5.1 convention).
A Thread whose `message_ids` is empty is a **tombstone** — clients
SHOULD drop any cached state for that id. Tombstones are typically
emitted when a server-side or local-threading merge moves all of a
thread's messages into another thread; `SyncDelta.threads_changed`
carries both the survivor (with its new `message_ids`) and the
tombstoned loser (with `message_ids: []`).

Resolved 2026-05-18 in mxr (Phase F): extended the internal Thread
to carry `message_ids` + emitted `threads_changed` in `SyncBatch`;
added `Request::ListThreads` for paginated thread listing.

### Flag

Envelope flag state is split into two parallel facets to match the
local-store model most clients (mxr, Apple Mail, Thunderbird) already
use:

```
flags    : Bitfield of system flags
           Seen / Answered / Flagged / Draft / Deleted / Sent / Trash / Spam / Archived
keywords : Set<String> of free-form IMAP-style keywords
           ($Forwarded, $NotJunk, $MDNSent, user-defined $Work, ...)
```

Adapters whose backend doesn't support keywords advertise
`capabilities.mutate.custom_keywords = false`; clients MUST NOT
issue `SetKeywords` mutations against such accounts. (The earlier
draft modelled this as a single `Flag :: System | Keyword(String)`
enum; an 8-protocol survey showed every local-store client splits,
and the per-flag granular Mutation enum only helps stateless
protocols like JMAP that defer body retrieval.)

Adapters whose backend doesn't support a given system flag map it
to the closest available semantics or drop it. Keywords are
preserved verbatim — IMAP atoms are case-sensitive on the wire,
and round-trip integrity matters more than canonicalising case.

### Mutation

A request to change server state. Each `apply_mutation` call carries
a client-supplied `mutation_id` (UUIDv7); the adapter (or its
client-side wrapper) dedupes on retry within a 24h window:

```
Mutation ::
    ModifyLabels { message_id, add: Vec<FolderId>, remove: Vec<FolderId> }
  | Trash        { message_id }
  | SetRead      { message_id, read: bool }
  | SetStarred   { message_id, starred: bool }
  | SetKeywords  { message_id, add: Vec<String>, remove: Vec<String> }
```

`SetKeywords` is rejected by adapters with
`capabilities.mutate.custom_keywords = false` (e.g. Gmail).

`ModifyLabels` is batched per-message so a single user intent
("archive this") maps to one provider call instead of N. The earlier
draft prescribed per-flag granular variants (`AddFlag`, `RemoveFlag`,
`Move`, `Copy`, `Delete`, `LabelApply`, `LabelRemove`); the batched
shape matches MS Graph, Drive, CloudKit, and WebDAV convention.
Local-store clients upsert-by-id anyway, so the granular split only
helps stateless protocols (none of MSP's target audience).

### SyncDelta

The output of `sync` and the payload of push notifications:

```
SyncDelta {
  messages_changed:  Vec<Message>           // additions + mutations; merged
  messages_removed:  Vec<MessageId>
  threads_changed:   Vec<Thread>            // composition or ordering changed
  folders_changed:   Vec<Folder>            // additions, renames, deletions
  new_state:         AccountState
  has_more:          bool                   // true ⇒ call sync again immediately
}
```

The merged `messages_changed` follows MS Graph, Drive, CloudKit, and
WebDAV convention. Local-store clients upsert-by-id; splitting
`created/updated` doubles array counts without semantic gain (only
JMAP splits, and only because it returns IDs alone and defers object
retrieval). `has_more` enables paginated catch-up.

---

## 3. Wire protocol

### Framing

**JSON-RPC 2.0** ([RFC](https://www.jsonrpc.org/specification)),
framed with `Content-Length: N\r\n\r\n` headers (same as LSP). UTF-8
mandatory. The framing is identical across transports.

```
Content-Length: 142
Content-Type: application/vscode-jsonrpc; charset=utf-8

{"jsonrpc":"2.0","id":1,"method":"initialize","params":{...}}
```

### Transports

- **stdio** — the default. The client launches the adapter as a
  subprocess; reads/writes JSON-RPC over its stdin/stdout. stderr is
  reserved for adapter logs.
- **WebSocket** — for adapters hosted out-of-process or remotely.
  One MSP session per WebSocket connection.
- **Unix domain socket** — for adapters hosted on the same machine
  but in a separate daemon process.

Transports are negotiated out-of-band. The protocol itself doesn't
specify how a client finds its adapter; convention is "adapters are
binaries on `$PATH` named `msp-<provider>` (e.g. `msp-gmail`,
`msp-imap`); clients launch them as subprocesses by default."

### Message categories

**Requests** (client → adapter): a `method`, `params`, and `id`.
Adapter MUST respond with the same `id`.

**Responses** (adapter → client): correlate via `id`. Either
`result` or `error`, per JSON-RPC.

**Notifications** (either direction, no `id`): fire-and-forget.
Adapter-side notifications are how push events arrive at the client.

**Server-initiated requests** (adapter → client): allowed (à la
LSP) for callbacks like reauth prompts. Hard rule: notifications and
server requests are unsolicited; the adapter MUST NOT use them as
the only way to deliver routine sync state.

### Error shape

```
error: {
  code:    Int       // JSON-RPC code (see below)
  message: String    // short, human-readable
  data:    {
    kind:  String    // namespaced enum, e.g. "msp.sync.cannot_calculate_changes"
    retry: Option<{ after_ms: u64 }>   // hint for retryable errors
    ...    // method-specific extra fields
  }
}
```

JSON-RPC reserves codes -32700 to -32000 (parse errors, invalid
request, etc.). MSP uses -32000 to -32099 for protocol-level errors
and positive codes for adapter-domain errors. The `data.kind` string
is the durable identifier; clients SHOULD match on it, not on `code`.

**Forward compatibility rule** (borrowed from LSP): receivers MUST
ignore unknown fields and unknown enum variants. Never reject a
message because a field you don't recognise is present.

---

## 4. Capabilities

### Negotiation

Two-phase, namespaced.

**Phase 1: client → adapter, in `initialize`:**

```json
{
  "client_info": {"name": "mxr", "version": "0.6.0"},
  "client_capabilities": {
    "core": {"version": "0.1"},
    "push": {"streaming": true},
    "search": {"server_side": true}
  }
}
```

**Phase 2: adapter → client, in the initialize response:**

```json
{
  "adapter_info": {"name": "msp-gmail", "version": "1.0.0", "provider": "Gmail"},
  "server_capabilities": {
    "core": {"version": "0.1"},
    "sync": {"native_threading": true, "delta": true, "bulk_fetch": true},
    "mutate": {"labels": true, "atomic_move": true, "custom_keywords": true},
    "search": {"server_side": true, "query_dialect": "gmail"},
    "push": {"streaming": true, "max_idle_seconds": 1700}
  }
}
```

Capabilities are **namespaced** (LSP-style). Each request is paired
with the capability it requires; clients check before calling.
Unsupported methods return `error.data.kind = "msp.unsupported"`.

**No dynamic registration.** If the adapter's capability set changes
mid-session, it MUST end the session and the client MUST re-initialize.
This is LSP's biggest interop bug; we skip it.

### Foundational capabilities

Every adapter MUST support:

- `core` namespace (initialize, shutdown, ping)
- `sync.delta` (sync_account method with state cursor)
- `bodies` namespace with at least one of `eager` or `lazy`
  declared in `bodies.modes`

`fetch_body` is REQUIRED only when the adapter advertises
`bodies.modes` containing `"lazy"`. Eager-only adapters bundle
bodies into `SyncDelta` and do not need a separate fetch path.

Everything else is optional but discoverable.

---

## 5. Resumability and offline mutations

### Read resumability

The `AccountState` IS the resumability story. Every `sync_account`
returns a new state. Clients persist the latest state per account.
On startup, clients pass the persisted state back to sync.

If the adapter cannot compute changes from that state (e.g. cursor
too old, provider's history window expired), it returns:

```json
{
  "error": {
    "code": 100,
    "message": "cannot calculate changes",
    "data": {
      "kind": "msp.sync.cannot_calculate_changes",
      "reason": "provider history expired (gmail historyId 30 days old)"
    }
  }
}
```

Clients respond by clearing local state for that account and calling
`sync_account` with a fresh (null) state to trigger a full re-sync.

### Write resumability (offline queue)

Clients buffer mutations locally while disconnected. On reconnect,
they replay in order via `mutate`. Idempotency is the **adapter's**
responsibility — adapters MUST be safe against duplicate mutation
delivery.

The protocol does not specify queue persistence; that's the client's
concern. The protocol does specify mutation idempotency: every
mutation includes a client-generated `mutation_id` (UUID); the
adapter SHOULD reject duplicates within a 24h window.

---

## 6. Push notifications

Unified shape: `subscribe_changes` returns an opaque subscription
that emits `state_changed` notifications.

```json
// Client → adapter
{"method": "subscribe_changes", "params": {"account_id": "alice"}}

// Adapter → client (response)
{"result": {"subscription_id": "s-1234"}}

// Adapter → client (notification, later, repeatedly)
{
  "method": "state_changed",
  "params": {
    "account_id": "alice",
    "new_state_hint": "<opaque>",
    "scope": "messages"  // or "threads", "folders", "labels"
  }
}
```

Notifications carry **only** `(account, new_state_hint, scope)` —
never payloads. Clients reconcile by calling `sync_account` with
their persisted state. This makes push idempotent, replayable, and
transport-agnostic: the same notification works over WebSocket, SSE,
or WebPush.

Adapters handle the provider-specific mechanism internally
(IMAP IDLE long-polling, Gmail history watch, JMAP push, Outlook
webhook). Clients never see it.

`pushState` resume tokens (JMAP RFC 8887's contribution) — adapter
returns one in `subscribe_changes`; client passes it back to resume
a subscription after a disconnect.

---

## 7. Conformance

Three test categories. An adapter MAY claim to support a category;
if it does, the conformance harness verifies its behaviour against a
recorded transcript corpus.

**`msp-conformance` test harness:**

```bash
msp-conformance --adapter msp-gmail --category sync.delta --account test@example.com
# runs a sequence of sync_account calls and verifies the expected
# SyncDelta shape against the recorded reference
```

Categories:

- `sync.delta` — full + incremental sync correctness
- `mutate.idempotent` — mutation replay safety
- `push.streaming` — state_changed notification delivery
- `capabilities.honest` — every capability claimed is exercised
- `errors.spec` — error shapes match the spec; required errors fire
  on the documented triggers

Reference adapters (the IMAP one in particular, since it's open)
ship with their own test fixtures and document which categories
pass. The harness ships as part of the protocol's repo. No
"Certified™" stamp in v0.1.

---

## 8. Open questions (v0.1 punts)

Honest list of what this draft doesn't cover. Each is a candidate for
v0.2 or later, or a separate companion protocol.

- **Authentication.** Provider-specific (OAuth2 PKCE for Gmail/
  Outlook, password/app-password for IMAP, SASL XOAUTH2 mechanics,
  etc.). v0.1 assumes the adapter handles auth and asks the client
  for credentials/tokens via a server-initiated `prompt_auth`
  request. The exact shape of `prompt_auth` is TBD.

- **Calendar + contacts.** JMAP includes them; MSP v0.1 does not. A
  parallel `csp` (Calendar Sync Protocol) could share the same wire
  shape and capability negotiation; out of scope here.

- **Compose / send.** The "outgoing" direction is symmetric but
  separate. The Mail Submit Protocol (MSP-Send? MSubP? naming TBD) is
  a companion spec.

- **Server-side rules / Sieve.** RFC 5228 is the existing standard
  for server-side rule programming; MSP doesn't redo it. Adapters
  that support it expose a `rules` capability and method set TBD.

- **Encryption.** PGP/SMIME message decryption + key management are
  client-side concerns (the protocol passes ciphertext through).

- **Push delivery semantics.** At-least-once vs exactly-once is
  unspecified in this draft. We probably want at-least-once + client
  idempotency (matches the read model).

- **Backpressure.** What if the client can't keep up with push
  notifications? Adapter-side buffering policy is TBD.

- **Adapter discovery.** v0.1 assumes the client knows where to find
  its adapters (binaries on `$PATH`, WebSocket URLs in config). A
  discovery mechanism could come later.

- **Bidirectional sync vs read-mostly.** v0.1 supports both, but
  some use cases (archive viewers, forensic analysers) only need
  read. Should adapters expose a read-only mode? Probably yes; spec
  it explicitly in v0.2.

---

## 9. Comparison with prior art

**JMAP** ([RFC 8620](https://datatracker.ietf.org/doc/html/rfc8620))
is the closest analog. It already has opaque state cursors, the
`created/updated/destroyed` triple, capabilities-per-request via
`using`, and push notifications. Its abstract model is well-designed.

JMAP's adoption stalled at the **server** side. Fastmail and Stalwart
support it; Apache James and Cyrus do. But Gmail, Outlook 365, and
iCloud — the three providers that hold ~95% of consumer mail — don't.
Their incentive to add a third protocol on top of IMAP + proprietary
APIs is zero.

**MSP sits one layer down from JMAP.** Where JMAP says "the server
speaks this protocol natively," MSP says "an adapter translates the
server's native API to a protocol the client speaks." A JMAP-aware
adapter can implement MSP by forwarding the methods one-to-one. An
IMAP-aware adapter implements MSP by translating from IMAP commands.
A Gmail-aware adapter implements MSP by translating from Gmail's
REST API. **The wire surface area is the same; the implementation
strategy differs.**

This is the same relationship LSP has to language compilers: many
languages have native protocols (e.g. for completions); LSP gives
clients one shape to learn, with adapters bridging.

If JMAP wins more provider support in the future, MSP adapters
become thinner. If it doesn't, MSP adapters do the translation work
JMAP was meant to make unnecessary. Either way, the client wins.

**Other reference points:**

- DAP — wire envelope (we adopted LSP's vanilla JSON-RPC instead),
  capability soup (we namespace), three-step lifecycle (we kept).
- LSP — vanilla JSON-RPC framing (kept), namespaced capabilities
  (kept), dynamic registration (skipped — biggest interop bug).
- email-lib / pimalaya — per-op `Backend` trait, no orchestration
  layer. MSP fills the gap with the sync orchestration spec.
- melib — bundled with TUI, EUPL/GPL licensed. Not a portable
  protocol.

---

## Provisional design choices summary

Borrowing from the synthesis in Phase 1 of the spike:

1. **JSON-RPC 2.0 over Content-Length-framed stdio (and WebSocket).**
2. **Namespaced static capabilities + per-request `using`.** No
   dynamic registration.
3. **Opaque `AccountState` as the protocol's spine.** Server-issued,
   never parsed by clients.
4. **Bidirectional JSON-RPC with hard request/notification split.**
   Notifications carry only `(scope, hint)`.
5. **Three-step lifecycle:** `initialize` → `initialized` notification
   → `ready` signal.
6. **Single error shape with a namespaced `kind` enum.** Forward-
   compat: ignore unknown fields and enum variants.
7. **Client-side offline queue with adapter-side mutation idempotency.**
   `mutation_id` carries through.

---

## License

This draft is CC BY 4.0. Implementations are free to adopt the
protocol under any license they choose. Reference implementations,
where they exist, are MIT OR Apache-2.0.
