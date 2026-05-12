# mxr Delight Plan — Handoff Notes

> **Read this first when picking up this work in a new session.**
>
> Comprehensive context for the multi-session "delight plan" implementation. Captures what's done, what's pending, where the bodies are buried, conventions, and gotchas. Update as you go.

## Plan provenance

The canonical plan lives at `docs/vision/01-delight-plan.md`. Per-phase trackers are `docs/vision/phase-{1..4}-*.md`. Read those for design intent. This file captures **operational state**: how to build, test, and extend.

## Current implementation state (2026-05-12)

**Workspace builds clean. ~908 lib tests pass.**

### Done end-to-end (CLI surface, daemon dispatch, tests)

| Phase | Status | Surface |
|-------|--------|---------|
| 1.1 Optimistic mutation rollback | ✅ TUI core | Star/Label/Move optimistic; bulk-confirm rollback; bounded snapshot eviction (cap 64) |
| 1.2 Cmd+K palette ranking + recents | ✅ TUI core | 5-tier match (exact > prefix > word-prefix > substring > shortcut/category) + FIFO recents |
| 1.3 Inbox row formatters | ✅ formatters; render integration partial | `format_date_relative`, `format_sender`, `format_attachment_chip`, `format_subject_line` |
| 1.4 Type-ahead search | ✅ verified | 250ms debounce, cancellation-on-keystroke |
| 1.5 Saved-search keyboard nav | ✅ keyboard + visual strip | `g 0..9` chord; TUI saved-search tab strip; desktop `g`+digit parity |
| 2.1 Reply-later | ✅ store + daemon + CLI + TUI `b` | `mxr replies`, `mxr replies walk`, `Action::FlagReplyLater` |
| 2.2 Custom-time snooze | ✅ wired | `mxr snooze --until "in 2h" / "tomorrow 9am" / "monday 17:00" / RFC3339` |
| 2.3 Auto-reminders | ✅ store + daemon + loop + CLI + desktop | `mxr remind <id> --when ... / --cancel`; desktop selected-message reminder UX; `auto_reminders_loop` |
| 2.4 Send Later | ✅ store + daemon + flusher + CLI + desktop | `mxr send <id> --at ...`; `mxr unsend <id>`; desktop compose send-later UX; `scheduled_sends_loop` |
| 2.5 Screener | ✅ store + daemon + CLI | `mxr screener queue/list/allow/deny/feed/paper-trail/clear` |
| 2.6 Bulk unsubscribe | ✅ pre-existing | `crate::unsubscribe::execute_unsubscribe` already wired |
| 3.1 Snippets | ✅ store + IPC + CLI + compose expansion | `mxr snippets list/set/remove`; compose expands known `;name` snippets before save/send |
| 3.2 Sender view | ✅ store + IPC + CLI | `mxr sender <addr>` |
| 3.3 LLM provider trait | ✅ via OpenAI-compatible HTTP | Covers Ollama, LM Studio, OpenAI, Groq, OpenRouter |
| 3.4 Thread summarize | ✅ end-to-end | `mxr summarize <thread-id>` |
| 3.5 Draft assist | ✅ basic (no semantic retrieval yet) | `mxr draft-assist <thread-id> "<instruction>"` |
| 4.1 Crash-safe drafts | ✅ store + daemon-startup recovery | Heartbeat column + auto-reset orphaned `'sending'` drafts on startup |
| 4.2 Doctor 2.0 | ✅ structured findings + CLI/TUI/desktop render | DoctorFinding with category/severity/remediation classifier |
| 4.3 Setup wizard | ✅ demo + quick-start + desktop affordance | `mxr setup --demo` writes Fake account; quick-start guidance; desktop palette/onboarding points users to demo setup |

### Pending parity work (this is the current ask)

For each feature, parity = surfaced in TUI, CLI, HTTP-bridge, and desktop app where applicable.

**TUI parity gaps**

- ~~Reply-later walk mode (CLI walk also TBD)~~ ✅ `mxr replies walk` implemented (2026-05-12)
- Sender view full-screen page (`Screen::SenderProfile`) — intentionally deferred; current modal stays
- Screener queue page + 3-key disposition
- Snippet manager modal
- Summarize/draft-assist invocation from thread view
- Setup wizard TUI screen
- Custom-snooze modal "Custom..." entry
- ~~Visual saved-search tab strip~~ ✅ TUI saved-search tab strip added (2026-05-12)
- Render integration: `format_subject_line` / `format_attachment_chip` in `build_row`
- Live heartbeat plumbing in compose flow
- ~~Optimistic visual indicator on flagged rows (currently just a status message)~~ ✅ pending mutation row marker added (2026-05-12)
- Doctor findings rendering in TUI diagnostics page

**HTTP bridge gaps**

- New routes for ~15 IPC types added in this arc:
  - `SetReplyLater`, `ListReplyQueue`
  - `SetAutoReminder`, `CancelAutoReminder`
  - `ScheduleSend`, `CancelScheduledSend`
  - `ListSnippets`, `SetSnippet`, `DeleteSnippet`
  - `GetSenderProfile`
  - `ListScreenerQueue`, `ListScreenerDecisions`, `SetScreenerDecision`, `ClearScreenerDecision`
  - `SummarizeThread`, `DraftAssist`
- New event variant: `ReminderTriggered`
- New response types: `ReplyQueue`, `Snippets`, `SnippetData`, `SenderProfile`, `ScreenerQueue`, `ScreenerDecisions`, `ThreadSummary`, `DraftSuggestion`
- See `crates/web/src/` for the route table

**Desktop app gaps**

- ~~Doctor findings rendering~~ ✅ structured findings render in Diagnostics overview + details report (2026-05-12)
- ~~Auto-reminder UX~~ ✅ selected-message set/cancel reminder commands use existing bridge routes (2026-05-12)
- ~~Send-later UX~~ ✅ compose dialog can schedule saved drafts through existing bridge routes (2026-05-12)
- ~~Saved-search keyboard parity~~ ✅ desktop `g`+digit opens inbox/saved searches (2026-05-12)
- ~~Search pacing parity~~ ✅ explicit desktop debounce plus request cancellation (2026-05-12)
- ~~Setup/demo onboarding affordance~~ ✅ desktop command/palette path points to `mxr setup --demo` (2026-05-12)
- TypeScript types for new IPC need regeneration (`pnpm gen:types` or whatever the codegen step is) — route-specific types are now reflected in desktop local types where used

**Persistence/UX**

- Persist `recent_actions` for the command palette across daemon restarts (currently in-memory in TUI)
- Hint bar slim down (currently shows everything; should be top-5 contextual)

## How the build works

### Workspace layout

The root crate (`mxr`) is unusual: it includes `crates/daemon/src/lib.rs` directly (no `crates/daemon/Cargo.toml`). Tests at `crates/daemon/tests/*.rs` are wired via `[[test]]` blocks in the **root** `Cargo.toml`. Run them as `cargo test -p mxr --test cli_journey` (NOT `cargo test -p mxr-daemon`).

### sqlx prepared queries (CRITICAL)

All store queries use `sqlx::query!` macros checked at compile time against the `.sqlx/` cache. **After adding any new query**, regenerate the cache:

```bash
# One-time setup
rm -f /tmp/mxr-prep.db
DATABASE_URL="sqlite:///tmp/mxr-prep.db?mode=rwc" sqlx database create

# After adding migrations:
DATABASE_URL="sqlite:///tmp/mxr-prep.db?mode=rwc" sqlx migrate run --source crates/store/migrations

# After adding query!() calls:
DATABASE_URL="sqlite:///tmp/mxr-prep.db?mode=rwc" cargo sqlx prepare --workspace --
```

### Stack overflow workaround (CRITICAL)

The CLI has 60+ subcommand variants. clap-derive generates large stack frames in debug builds; the default 2MB test thread stack overflows on `Command::parse` in `cli/tests`. Fix is in `.cargo/config.toml`:

```toml
[env]
SQLX_OFFLINE = "true"
RUST_MIN_STACK = "16777216"
```

If that file gets reverted, daemon CLI tests will overflow. Always check `.cargo/config.toml` first when test failures look like stack overflows.

### Migration registration

Migrations live in `crates/store/migrations/NNN_name.sql` AND are registered in `crates/store/src/pool.rs::MIGRATIONS`. Both are required — the SQL file is for `sqlx-cli`, the array entry is for the daemon's runtime migration loop. Use `MigrationKind::Sql(include_str!(...))` for plain SQL or `MigrationKind::AddColumn { ... }` for add-column-only steps (which need special handling for SQLite's lack of `ADD COLUMN IF NOT EXISTS`).

Migration numbering as of 2026-05-08:

```
001_initial.sql                013_message_flags.sql
002 (body_metadata, in pool)   014_auto_reminders.sql
003_sync_runtime_status.sql    015_scheduled_sends.sql (composite)
004_semantic_search.sql        016_snippets.sql
005_inline_attachment_metadata 017_draft_heartbeat.sql (add column only)
006_message_events.sql         018_screener_decisions.sql
007_message_analytics_columns
008_account_addresses.sql
009_reply_pairs.sql
010_contacts.sql
011_draft_status.sql
012_mutation_undo_log.sql
```

Next available: 019.

## Patterns used throughout the new code

### Store module pattern

```rust
// crates/store/src/foo.rs
use crate::{decode_id, decode_timestamp, trace_query};

impl super::Store {
    pub async fn list_foos(&self, ...) -> Result<Vec<Foo>, sqlx::Error> {
        let started_at = std::time::Instant::now();
        let rows = sqlx::query!(
            r#"SELECT col as "col!" FROM foos WHERE ..."#,
            ...
        )
        .fetch_all(self.reader())
        .await?;
        trace_query("foo.list", started_at, rows.len());
        rows.into_iter().map(|r| {
            // build Foo from r
        }).collect()
    }
}
```

Use `self.writer()` for `INSERT`/`UPDATE`/`DELETE`, `self.reader()` for `SELECT`. The writer pool is single-connection (serializes writes); the reader pool is multi.

### Daemon handler pattern

```rust
// crates/daemon/src/handler/foo.rs
use super::HandlerResult;
use crate::state::AppState;
use mxr_protocol::ResponseData;

pub(super) async fn foo(state: &AppState, ...) -> HandlerResult {
    state.store.do_thing(...).await.map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}
```

`HandlerResult = Result<ResponseData, String>`. The dispatch happens in `handler/mod.rs::handle_request`. Add a `mod foo;` declaration plus a match arm in `handle_request`. Also add to `request_kind()` for tracing labels and the safety policy `is_read_only_request()` if applicable.

### IPC type additions (3 places)

When adding a new `Request` variant, you must update **all three** of these in `crates/protocol/src/types.rs`:

1. The `Request` enum (the variant itself)
2. `impl Request::category(&self)` — categorize as CoreMail / MxrPlatform / AdminMaintenance
3. `request_kind()` in `crates/daemon/src/handler/mod.rs` — tracing label

For a new `ResponseData` variant: enum + `category(&self)`.

For a new `DaemonEvent` variant: enum + `category(&self)`.

### Background loop pattern

See `crates/daemon/src/loops.rs::auto_reminders_loop` and `scheduled_sends_loop`. Each has:

1. A pure async function `process_due_X(state, now) -> Result<u32>` that's testable.
2. The loop wrapper that spawns a 60s ticker + shutdown signal + calls the pure function.
3. Registration on `RuntimeTasks` in `state.rs` (field + setter + register method + `take_all` entry).
4. Spawn at startup in `server.rs` after the existing snooze_loop spawn.

### CLI subcommand pattern

```
crates/daemon/src/cli/mod.rs        # Add to Command enum + define subcommand action enum
crates/daemon/src/commands/foo.rs   # Implementation
crates/daemon/src/commands/mod.rs   # Add `pub mod foo;`
crates/daemon/src/lib.rs            # Add dispatch arm in match Some(Command::Foo { .. }) => ...
crates/daemon/tests/cli_help.rs     # Add a case to the snapshot list + bump cases.len() count
```

Then accept the new snapshot:

```bash
INSTA_FORCE_PASS=1 cargo test -p mxr --test cli_help
cd crates/daemon/tests/snapshots && for f in *.snap.new; do mv "$f" "${f%.new}"; done
cargo test -p mxr --test cli_help  # should pass cleanly now
```

## Pre-existing issue: cli_journey flake

5 `cli_journey_*` tests fail on a clean state with timeouts during daemon startup. Confirmed independent of my work via stash round-trip earlier in the arc. Investigation notes:

- Failure: `Starting daemon... failed.` despite the daemon log showing `Daemon listening on .../mxr.sock`.
- Suggests a startup race between socket-listen and the test client's status check.
- Likely related to the user's WIP `POOL_ACQUIRE_TIMEOUT = 90s` change in `crates/store/src/pool.rs` — startup tasks that hold the writer for a long time could block the daemon's first status response.
- May also involve the `ensure_daemon_running` helper's retry timing in `crates/daemon/src/server.rs`.
- The daemon-startup recovery I added (`run_startup_maintenance` orphan-draft scan) does additional store work at startup. May need to make it a fire-and-forget background task rather than blocking before socket-accept.

**Next-session fix path** for the flake:

1. `grep -rn "Starting daemon\.\.\." crates/daemon/src/` to find the user-facing message.
2. Look at the daemon's main loop for the order of operations: `bind_socket → spawn_tasks → first status check`.
3. Verify `run_startup_maintenance` is async-spawned, not awaited inline.
4. The `mxr status` client retries 5 times with 100ms backoff (find this in `commands::status` or `server::ensure_daemon_running`); bump to longer or smarter backoff.

## Conventions

- **Test boundaries**: prefer behavior tests through public APIs. Daemon dispatch tests use `AppState::in_memory_with_fake()` + `handle_request(&state, &msg)`.
- **No mocks of store, search, daemon-internal traits**. Network boundary only (provider HTTP, LLM HTTP).
- **No emojis in code unless explicit**. Doctor CLI does use ✗/!/· for severity glyphs (intentional).
- **No `Co-Authored-By: Claude` / `🤖 Generated with` lines in commits.** User has been clear about this.

## Critical files to touch when adding a feature

A typical end-to-end feature touches:

```
crates/store/migrations/NNN_*.sql
crates/store/src/foo.rs                              (new)
crates/store/src/lib.rs                              (mod + pub use)
crates/store/src/pool.rs                             (Migration entry)
.sqlx/*                                              (regenerated)
crates/protocol/src/types.rs                         (Request, ResponseData, optional DaemonEvent + Data structs)
crates/daemon/src/handler/foo.rs                     (new)
crates/daemon/src/handler/mod.rs                     (mod + dispatch + request_kind + safety policy)
crates/daemon/src/commands/foo.rs                    (new CLI handler)
crates/daemon/src/commands/mod.rs                    (mod)
crates/daemon/src/cli/mod.rs                         (Command enum + Action enum)
crates/daemon/src/lib.rs                             (CLI dispatch arm)
crates/daemon/tests/cli_help.rs                      (test list entry)
crates/daemon/tests/snapshots/cli_help__cli_help_foo.snap   (new)
docs/vision/phase-N-*.md                             (tracker update)
crates/web/src/...                                   (HTTP route, if exposing to bridge)
```

## TUI integration pattern

For each new daemon feature that should be exposed in the TUI:

1. **Action enum** (`crates/tui/src/action.rs`): add a new variant. If parameterless, place near related actions; if parameterized, add appropriate doc comment.
2. **Dispatch route** (`crates/tui/src/app/actions.rs`): add to the appropriate `apply_*` group's match arm.
3. **Action handler** (`crates/tui/src/app/{mailbox_actions,mutation_actions,...}.rs`): implement `Action::Foo => { ... }`. For a request, use `self.queue_mutation(Request::Foo(...), MutationEffect::StatusOnly("..."), "Doing..."::into())`. For complex modal flows, set `self.modals.foo_panel.visible = true` and handle confirm in a separate action.
4. **Keybinding** (`crates/tui/src/input.rs`): add a single-key or chord binding. Avoid conflicts (`r`/`R` already taken by Reply / Reading View). Free single keys: `b` (used for bookmark), `\\`, `;`, `&`. Free `g`-chord targets: most digits + most letters not yet used.
5. **Command palette** (`crates/tui/src/ui/command_palette.rs::default_commands`): add a `PaletteCommand` so the action is discoverable via Cmd+K.
6. **Desktop manifest** (`crates/tui/src/desktop_manifest.rs`): add the action variant to the `match` so the desktop manifest exporter doesn't fail compilation.
7. **Tests** (`crates/tui/tests/*.rs`): add behavior tests for the dispatch, optimistic effect, etc.

## HTTP bridge integration pattern

The bridge lives in `crates/web/`. For each IPC type that should be exposed:

1. **Route definition** (`crates/web/src/routes/`): add an Axum handler that parses the path/body, builds a `Request`, calls the IPC, returns the response shape.
2. **OpenAPI doc** (the route fn likely has an `#[utoipa::path(...)]` macro): document parameters and response schemas.
3. **Snapshot test**: the OpenAPI snapshot in `crates/web/src/snapshots/` will need regeneration via `cargo insta accept`.

## Desktop app integration pattern

The desktop app at `apps/desktop/` (Electron) consumes the bridge via TypeScript types generated by `pnpm gen:types` (from the OpenAPI spec). After adding new bridge routes, regenerate types and add UI surfaces.

## Stash safety net

`stash@{0}: phase-1-wip-temp-for-test-isolation` from earlier session. All work has been re-applied; the stash is a duplicate. Safe to drop:

```
git stash drop stash@{0}
```

## Quick health check (run at session start)

```bash
cargo check --workspace                                # 30-90s after fresh build
cargo test --workspace --lib 2>&1 | grep "test result" | tail -20
cargo test -p mxr --test cli_help                      # CLI surface
```

If any fail, check:

1. `.cargo/config.toml` for `RUST_MIN_STACK`
2. `.sqlx/` cache freshness (regenerate if recent migration changes)
3. `crates/store/src/pool.rs::MIGRATIONS` matches the files in `crates/store/migrations/`

## Background loops running in the daemon

Verified spawned in `crates/daemon/src/server.rs`:

| Loop | Cadence | Purpose |
|------|--------:|---------|
| `sync_loop_for_account` | per-account, configurable | Mail sync from providers |
| `snooze_loop` | 60s | Wake snoozed messages |
| `auto_reminders_loop` (NEW) | 60s | Fire `ReminderTriggered` events |
| `scheduled_sends_loop` (NEW) | 60s | Flush due scheduled drafts via `send_stored_draft` |
| `reply_pair_reconciler_loop` | 60s | Backfill reply_pairs for analytics |
| `contacts_refresher_loop` | 5min | Materialize `contacts` table |
| `bridge_loop` (HTTP server) | continuous | Axum bridge if `[bridge] enabled = true` |

Each registers a JoinHandle on `RuntimeTasks` so shutdown can drain them cleanly.

## What "compact the session" means here

The user asked me to compact the session before working. The harness auto-compacts; I make sure these notes survive by writing them to disk (in this file). The next session starts by reading this file + `01-delight-plan.md` + the relevant phase tracker.
