# Phase 2 — Capture (Dispatcher Instrumentation)

Goal: every user-intent IPC request that reaches the daemon produces (where appropriate) one `user_activity` row, tagged with the originating client. Single seam, fire-and-forget, never blocks the response.

Phase 1 must be merged before this lands — the storage layer is the prerequisite.

## Deliverables

1. Protocol change: `IpcMessage` carries `source: ClientKind`.
2. Recorder module `crates/daemon/src/activity/mod.rs`.
3. Mapper `crates/daemon/src/activity/mapper.rs` — request → `ActivityEntry`.
4. Tier table `crates/daemon/src/activity/tier.rs` — action → `Tier`.
5. Dispatcher wrap at `crates/daemon/src/handler/mod.rs:263-287` — spawns recorder after handle.
6. Source tagging in TUI, CLI, web clients.
7. Synthesized `activity.paused` / `activity.resumed` / `activity.pruned` markers emitted by the daemon itself.
8. Retention prune extended to cover `user_activity` (separately per tier).
9. Integration smoke: press a key in the TUI, see a row via the storage repo from a unit-driver test.

## Out of scope this phase

- IPC query verbs (Phase 3).
- CLI / TUI / web surfaces (Phases 4-6).
- Pause/clear/redact user commands (Phase 7 — though the recorder must respect the paused flag now).

## Protocol change

### `IpcMessage`

Add `source: ClientKind` to the wire envelope. Backwards-compat path:

```rust
// crates/protocol/src/types.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IpcMessage {
    pub id: u64,
    #[serde(default = "ClientKind::default_for_legacy")]
    pub source: ClientKind,
    pub payload: RequestPayload,
}

impl ClientKind {
    /// Legacy clients (pre-source-field) get treated as `Cli` — most realistic guess for scripts
    /// hand-rolled against the socket.
    pub fn default_for_legacy() -> Self { Self::Cli }
}
```

Newer clients always set `source` explicitly. The default exists only so a partially-upgraded surface doesn't crash decoding.

### Codec

`crates/protocol/src/codec.rs` already does length-delimited JSON. JSON is forward-compatible with new fields by default. No codec change needed; just the type addition.

## Client source plumbing

### TUI
- `crates/tui` builds requests through an IPC client helper. Find the single helper and have it set `source: ClientKind::Tui` on every outgoing message.
- Grep for the function that constructs `IpcMessage` in TUI code; usually one shared `send_request(...)` site.
- Smoke-check: add a `tracing::trace!("ipc out: source={:?}", source)` temporarily during dev.

### CLI
- `crates/daemon/src/cli/` builds requests in the subcommand handlers via a client helper. Same single-site treatment: set `source: ClientKind::Cli`.

### Web
- `crates/web/src/routes_v6.rs` converts HTTP → IPC. Set `source: ClientKind::Web` at the construction site.
- The web bridge is loopback-only and authenticated; we can trust the `Web` tag.

### Daemon-internal
- When the daemon emits its own activity (`activity.pruned`, scheduled jobs, etc.) it bypasses the dispatcher and writes directly via the recorder with `source: ClientKind::Daemon`.

## Recorder module

```rust
// crates/daemon/src/activity/mod.rs

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::atomic::AtomicI64;
use tokio::sync::mpsc;
use mxr_store::user_activity::{ActivityInsert, Tier};
use mxr_protocol::ClientKind;

pub struct Recorder {
    store: Arc<mxr_store::Store>,
    paused: Arc<AtomicBool>,
    paused_until: Arc<AtomicI64>,        // unix ms; 0 means indefinite when paused=true
    tx: mpsc::Sender<RecorderMessage>,
}

enum RecorderMessage {
    Record(OwnedEntry),
    Pause { until: Option<i64> },
    Resume,
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct OwnedEntry {
    pub ts: i64,
    pub account_id: Option<String>,
    pub source: ClientKind,
    pub action: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub tier: Tier,
    pub context: Option<serde_json::Value>,
}

impl Recorder {
    pub fn spawn(store: Arc<mxr_store::Store>) -> Self {
        let (tx, mut rx) = mpsc::channel::<RecorderMessage>(1024);
        let paused = Arc::new(AtomicBool::new(false));
        let paused_until = Arc::new(AtomicI64::new(0));

        // worker
        let pstore = store.clone();
        let ppaused = paused.clone();
        let ppaused_until = paused_until.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    RecorderMessage::Record(e) => {
                        // observe pause window
                        let now = current_unix_ms();
                        let until = ppaused_until.load(Ordering::SeqCst);
                        if ppaused.load(Ordering::SeqCst) && (until == 0 || now < until) {
                            continue;
                        }
                        // auto-resume if window expired
                        if ppaused.load(Ordering::SeqCst) && until != 0 && now >= until {
                            ppaused.store(false, Ordering::SeqCst);
                            // synthesize a 'activity.resumed' on auto-resume — see below
                        }
                        // best-effort insert
                        let insert = ActivityInsert {
                            ts: e.ts,
                            account_id: e.account_id.as_deref(),
                            source: e.source.as_str(),
                            action: &e.action,
                            target_kind: e.target_kind.as_deref(),
                            target_id: e.target_id.as_deref(),
                            tier: e.tier,
                            context: e.context.as_ref(),
                        };
                        if let Err(err) = pstore.record_activity(insert).await {
                            tracing::warn!(error=%err, action=%e.action, "activity write failed");
                        }
                    }
                    RecorderMessage::Pause { until } => {
                        ppaused.store(true, Ordering::SeqCst);
                        ppaused_until.store(until.unwrap_or(0), Ordering::SeqCst);
                    }
                    RecorderMessage::Resume => {
                        ppaused.store(false, Ordering::SeqCst);
                        ppaused_until.store(0, Ordering::SeqCst);
                    }
                    RecorderMessage::Shutdown => break,
                }
            }
        });

        Self { store, paused, paused_until, tx }
    }

    /// Non-blocking submit. Drops on backpressure (channel full) with a warn log.
    pub fn record(&self, entry: OwnedEntry) {
        if let Err(err) = self.tx.try_send(RecorderMessage::Record(entry)) {
            tracing::warn!(error=%err, "activity recorder backpressure; dropping entry");
        }
    }

    pub fn pause(&self, until: Option<i64>) { let _ = self.tx.try_send(RecorderMessage::Pause { until }); }
    pub fn resume(&self) { let _ = self.tx.try_send(RecorderMessage::Resume); }
}

fn current_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
```

Design notes:
- A bounded channel with `try_send` provides backpressure protection without slowing the dispatcher. Dropped entries are logged so the daemon can detect overload.
- Synchronous insert from the worker is fine; we already serialize writes in `Store`'s single writer.
- Pause/resume go through the same channel so ordering is preserved.

## Mapper

The mapper is the **closed list** that decides which IPC requests produce activity. It's an exhaustive `match` on the request enum so adding a new IPC verb forces a mapping decision at compile time.

```rust
// crates/daemon/src/activity/mapper.rs

use mxr_protocol::Request;
use mxr_protocol::ClientKind;
use crate::activity::{OwnedEntry, tier::tier_for};

/// Build an activity entry from a request + response context.
///
/// Returns `None` for requests that intentionally don't produce activity
/// (queries, getters, polls, internal plumbing).
pub fn map_request(
    req: &Request,
    source: ClientKind,
    account_id: Option<&str>,
    response_ok: bool,
) -> Option<OwnedEntry> {
    use Request::*;

    if !response_ok { return None; }    // don't record failed actions; let event_log capture errors instead

    let now = current_unix_ms();

    let (action, target_kind, target_id, context) = match req {
        // ---- mail mutations ----
        MarkRead { thread_id, .. } => ("mail.read", Some("thread"), Some(thread_id.clone()), None),
        MarkUnread { thread_id, .. } => ("mail.unread", Some("thread"), Some(thread_id.clone()), None),
        Archive { thread_id, .. } => ("mail.archive", Some("thread"), Some(thread_id.clone()), None),
        Trash { thread_id, .. } => ("mail.trash", Some("thread"), Some(thread_id.clone()), None),
        Star { thread_id, .. } => ("mail.star", Some("thread"), Some(thread_id.clone()), None),
        Snooze { thread_id, until, .. } => (
            "mail.snooze",
            Some("thread"),
            Some(thread_id.clone()),
            Some(serde_json::json!({ "until": until })),
        ),
        Move { thread_id, label, .. } => (
            "mail.move",
            Some("thread"),
            Some(thread_id.clone()),
            Some(serde_json::json!({ "to": label })),
        ),
        Send { draft_id, .. } => ("mail.send", Some("draft"), Some(draft_id.clone()), None),
        Reply { thread_id, draft_id, .. } => (
            "mail.reply",
            Some("thread"),
            Some(thread_id.clone()),
            Some(serde_json::json!({ "draft_id": draft_id })),
        ),
        // ... etc — every Request variant covered explicitly

        // ---- searches ----
        Search { query, .. } => (
            "search.run",
            Some("search"),
            None,
            Some(serde_json::json!({ "query": query })),
        ),
        SaveSearch { name, query, .. } => (
            "search.save",
            Some("search"),
            None,
            Some(serde_json::json!({ "name": name, "query": query })),
        ),

        // ---- view / navigation ----
        OpenScreen { screen, .. } => (
            "view.open_screen",
            None,
            None,
            Some(serde_json::json!({ "screen": screen })),
        ),
        // ...

        // ---- queries / getters / polls — NO activity ----
        GetThread { .. }
        | GetMessage { .. }
        | ListLabels { .. }
        | ListAccounts { .. }
        | Status { .. }
        | Ping { .. }
        | ListEvents { .. }
        | ListActivity { .. }            // activity surface itself doesn't self-log queries
        | CountActivity { .. }
        | ActivityStats { .. }
        => return None,

        // ---- catch-all: explicit None so compiler forces a decision ----
        // (NO `_ =>` arm; we want exhaustiveness.)
    };

    Some(OwnedEntry {
        ts: now,
        account_id: account_id.map(str::to_owned),
        source,
        action: action.to_owned(),
        target_kind: target_kind.map(str::to_owned),
        target_id,
        tier: tier_for(action),
        context,
    })
}

fn current_unix_ms() -> i64 { /* same as recorder */ }
```

### Why an exhaustive `match`?

Mxr's `Request` enum has dozens of variants and grows. If we use `_ =>` we silently miss new verbs as they're added. Exhaustive match means **adding a new IPC verb fails compilation until someone decides "log this as X" or explicitly "skip"**. That's the right ergonomic for a long-lived feature.

## Tier classification

```rust
// crates/daemon/src/activity/tier.rs

use mxr_store::user_activity::Tier;

pub fn tier_for(action: &str) -> Tier {
    // Order: most-specific first.
    match action {
        // important — state-changing mutations
        a if a.starts_with("mail.")            => Tier::Important,
        a if a.starts_with("draft.")           => Tier::Important,
        a if a.starts_with("account.")         => Tier::Important,
        a if a.starts_with("rule.")            => Tier::Important,
        a if a.starts_with("screener.")        => Tier::Important,
        a if a.starts_with("reminder.")        => Tier::Important,
        a if a.starts_with("activity.")        => Tier::Important,
        "thread.flag_reply_later"
        | "thread.unflag_reply_later"           => Tier::Important,

        // standard — searches, snippets, navigations with retrospective value
        a if a.starts_with("search.")          => Tier::Standard,
        a if a.starts_with("saved.")           => Tier::Standard,
        a if a.starts_with("snippet.")         => Tier::Standard,
        a if a.starts_with("link.")            => Tier::Standard,
        a if a.starts_with("attachment.")      => Tier::Standard,
        "thread.open" | "thread.close" | "thread.summarize" => Tier::Standard,

        // ephemeral — views, app lifecycle, palette opens
        a if a.starts_with("view.")            => Tier::Ephemeral,
        "app.start" | "app.stop"               => Tier::Ephemeral,

        _ => Tier::Standard,                    // safe default for unmapped action tokens
    }
}
```

## Dispatcher wrap

The current dispatcher is at `crates/daemon/src/handler/mod.rs:263-287` (rough range — verify at impl time). The wrap:

```rust
// pseudo-diff inside the dispatcher
let activity = self.activity.clone();      // Arc<Recorder>
let source = ipc_msg.source;
let req = ipc_msg.payload.clone();         // cheap clone or Arc

let span = tracing::info_span!("ipc_request", kind = ?req.kind(), account = ?account_id_hint);
let response = span.in_scope(|| async {
    self.handle_request(req.clone(), account_id_hint).await
}).await;

let ok = response.is_ok();
let account_id = account_id_from_response_or_request(&response, &req);

// fire-and-forget activity record
if let Some(entry) = crate::activity::mapper::map_request(&req, source, account_id.as_deref(), ok) {
    activity.record(entry);
}

response
```

Notes:
- Use `request.clone()` (or wrap in `Arc`) since the mapper needs to inspect the request after the handler ran. Existing request types should already derive `Clone`; check + add if not.
- `account_id` resolution: for requests that carry an explicit account, take it from the request. For responses that return a new account (e.g. account add), pull from the response. The recorder is tolerant of `None`.

## Synthesized markers

Daemon-side activity emissions (no IPC request triggers these):

| When | Action | Notes |
|---|---|---|
| Recorder starts paused via CLI | `activity.paused` | Written before pause flag flips. `context_json: { until: <ts or null> }`. |
| Recorder resumes | `activity.resumed` | Written after flag flips. |
| Daily prune runs | `activity.pruned` | `context_json: { tier, before_ts, deleted }`. |
| `mxr activity clear` redacts a range | `activity.cleared` | Written as the final action so it's not itself redacted. `context_json: { range, affected }`. |
| Export to file | `activity.exported` | `context_json: { path, format, filter_summary, count }`. |

These bypass the mapper and call `Recorder::record` directly from the responsible code path.

## Retention pruner

Extend the existing prune path:

- `crates/daemon/src/commands/logs.rs` already has `prune_events()` running a scheduled sweep.
- Add a sibling `prune_activity()` that consults config and calls `store.prune_activity_before(cutoff, Some(tier))` for each tier.
- Schedule from the same background loop.
- Emit one `activity.pruned` marker per tier per sweep.

Config additions in `crates/config/src/types.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivityRetentionConfig {
    pub ephemeral_days: u32,   // default 30
    pub standard_days: u32,    // default 90
    pub important_days: u32,   // default 365
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivityConfig {
    pub retention: ActivityRetentionConfig,
    pub track_link_clicks: bool,  // default false
    pub paused: bool,             // default false; managed by daemon, not user-editable directly
}
```

Defaults live in `crates/config/src/defaults.rs`.

## What does NOT produce activity (codify in mapper)

- Heartbeats, `Ping`, `Status`, daemon-status polls.
- All read-side getters: `Get*`, `List*` (with the deliberate exception of search runs and saved-search opens).
- Sync ticks, reconciler passes, doctor self-checks (these emit to `event_log`, not `user_activity`).
- FTS / index rebuilds.
- Activity-query verbs themselves (`ListActivity`, `CountActivity`, `ActivityStats`, `ExportActivity`) — these don't self-log. Mutating verbs (`PruneActivity`, `RedactActivity`, `PauseActivity`, `ResumeActivity`) **do** log themselves via synthesized markers (see [#synthesized-markers](#synthesized-markers)).

## Tests

### Unit
- `mapper::map_request` for each request kind → assert action / target / tier / context.
- Tier table — table-driven test for representative actions.
- Recorder pause/resume: send 10 records during pause, 10 after resume — assert only the 10-after-resume reach the store.
- Recorder auto-resume on expired `paused_until`.

### Integration
- Boot a daemon with `provider-fake`. Connect via real socket from a test client that sets `source = ClientKind::Tui`.
- Send a `MarkRead`. Assert one row appears in `user_activity` with `source='tui'`, `action='mail.read'`, expected `target_id`.
- Send a `Ping`. Assert **no** row appears.
- Pause via direct recorder API. Send a `MarkRead`. Assert **no** row. Resume. Send another. Assert one row.
- Bench: 10,000 dispatched requests; assert per-request overhead p99 < 100 µs (recorder is async).

## Acceptance criteria

- All deliverables above ship.
- Adding a new `Request` variant fails compilation in the mapper until handled (verify by adding a stub variant temporarily and observing the error).
- TUI / CLI / web all stamp `source` correctly; integration test asserts the right values land in the table.
- Dispatcher overhead bench passes.
- Retention prune sweep runs on schedule (verify with a `tracing` log line, since no IPC surface yet — that's Phase 3).

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Mapper churn slows IPC PRs | Acceptable — a one-line addition per new verb. The compile error is the point. |
| Recorder channel saturates | Bounded channel + warn log + Phase 9 bench validates capacity. Channel buffer 1024 ≈ ~1s burst at unrealistic 1k req/s. |
| `IpcMessage` change breaks legacy in-flight clients | `#[serde(default)]` handles it. Add a one-line test that decodes a legacy payload (no `source`) and gets `ClientKind::Cli`. |
| Account ID resolution miss | Recorder tolerates `None`; mapper falls back to `None` if not derivable. Document the gap and accept it. |

## Exit criteria

Phase 2 is done when:
- Pressing `j`/`k` in the TUI does not produce activity (no IPC for cursor moves).
- Archiving a thread in any of {TUI, CLI, web} produces one row with the correct `source`.
- The integration test suite at `crates/daemon/tests/activity_capture.rs` is green.
- `STATUS.md` Phase 2 boxes ticked.
