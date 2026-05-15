# Activity Log — Maintainer's Guide

> Internal doc. The user-facing version lives at [`site/src/content/docs/guides/activity-log.md`](../site/src/content/docs/guides/activity-log.md). The CLI reference at [`site/src/content/docs/reference/cli/activity.md`](../site/src/content/docs/reference/cli/activity.md) is auto-generated from `--help` snapshots — don't hand-edit it.
>
> This file is the institutional knowledge: **why we built it the way we did, where the seams are, what to watch out for when changing it.** Code-derivable details are intentionally absent — read the code for those. What's here is the part that doesn't survive in `git blame`.

## What we were solving

Email clients don't ship a user-visible record of what the user did. Gmail exposes IP-level audit only for Workspace admins. Superhuman, Hey, Fastmail, Proton, Spark, Mimestream, Thunderbird — none of them give the end user a browseable "what did I do yesterday?" surface. That whitespace is real and stable: nobody competes on it because nobody else's architecture rewards it. Ours does. Mxr is local-first and daemon-mediated, so we have a clean seam to capture every IPC request and zero pressure to phone it home.

The product framing locked early and shouldn't drift:

> **"Email diary, not surveillance. Local-only, append-only, queryable like `git reflog` for your inbox, shreddable on demand."**

The differentiation trade-off test passes: a competitor *could* credibly claim the opposite ("we don't keep that data") and we *do* keep it (so the user can use it). That's how we know it's real positioning and not generic fluff. We surface this in the activity-log guide and the privacy guide; if a future change weakens any of the four invariants below, the framing breaks and the feature isn't worth shipping.

## Locked decisions (do not re-debate without updating this doc)

1. **Naming**: table `user_activity`; CLI surface `mxr activity` (alias `mxr act`); IPC verbs `ListActivity / CountActivity / ActivityStats / ExportActivity / RedactActivity / PruneActivity / PauseActivity / ResumeActivity` plus the saved-filter set, all under the `AdminMaintenance` IPC bucket. Saved-filter management lives under `mxr activity saved` to keep the namespace flat.
2. **Storage shape**: a new dedicated table. We deliberately did *not* extend `event_log` (system diagnostics, different schema, different consumer) or `message_events` (per-message state transitions, keyed by message_id, not by user intent). Mixing surfaces would have collapsed three different audit needs into one undifferentiated firehose.
3. **Tombstone, not delete**: redaction sets `redacted=1` and clears `context_json`. The audit-trail columns (id, ts, source, action, target) survive. Retention prune is the only path that *hard* deletes. This means a user can prove they cleared something at time T without revealing what.
4. **Action tiers** — `ephemeral | standard | important`. Default retention 30 / 90 / 365 days. Configurable per-tier. Tier is assigned by the mapper from the action token; we never let callers pass tier in directly because that's a soft-rule we'd inevitably regret.
5. **Single capture seam**: the daemon's IPC dispatcher (`crates/daemon/src/handler/mod.rs::handle_request`). One seam means one place to instrument, one place to test, one place to reason about pause / redact / source-tagging. No deep TUI-local hooks in v1 — we explicitly do not capture cursor moves, scroll, pane focus, or palette opens-then-cancelled.
6. **Source tagging on the wire**: every `IpcMessage` carries `source: ClientKind` (`Tui | Cli | Web | Daemon`). The client sets it at request-build time. Legacy clients (pre-source-field) decode as `Cli` via `#[serde(default)]` — the most realistic guess for scripts hand-rolled against the socket.
7. **Link-click capture is opt-in**. `activity.track_link_clicks` defaults to `false`. URL history reveals a lot; making it explicit means users who *do* enable it know what they're trading.
8. **Search query bodies are stored verbatim** with a per-row redact path. We accepted the privacy trade-off because the alternative — auto-redacting every query — destroyed the recall feature ("what did I search for last week?"). Users with sensitive query habits can disable `track_search_queries` or clear with `mxr activity clear`.
9. **Export formats**: CSV, JSON, NDJSON. All three required. NDJSON is for piping into shell tools (`jq`, `awk`); without it the CLI ergonomics drop a lot of value.
10. **Cross-device sync is strictly forbidden** — the activity table is never replicated, never exported automatically, never sent to a server. Hard invariant. Codified in `AGENTS.md` and enforced by the structural single-writer test.
11. **Encryption at rest is out of scope for v1**. Relies on user's filesystem encryption (FileVault, LUKS). Documented as a known limitation. Revisit when account-key infra exists.
12. **Pause is observable**. `mxr activity pause` writes one `activity.paused` marker *before* the pause flag flips, so the diary itself shows the gap. Auto-resume writes a synthesized `activity.resumed` marker so the user can see when recording came back. We considered silent pause; rejected it because hidden pauses undermine the entire "diary" framing.
13. **Browser-history-style clear**: `mxr activity clear --last 1h|1d|7d|30d|all`. Tombstones matching rows. Doesn't hard-delete — retention prune still runs deterministically and explicitly.
14. **Async write path**: activity writes are fire-and-forget through a bounded `tokio::sync::mpsc`. Failures are `tracing::warn!` only — never propagated to the user-facing IPC response. This is the absolute load-bearing invariant: activity is observability, not correctness. If a write fails, the user's `mail.archive` still archives.
15. **Cursor pagination**: monotonic `(ts DESC, id DESC)`. Resumes safely under concurrent inserts. Don't switch to offset/limit — that's incorrect under concurrent writes (rows shift between pages).
16. **What activity does NOT capture** (codified in mapper): heartbeats, internal getters used as plumbing (`GetThread` called by render path is the deliberate exception), poll loops, sync ticks, FTS index rebuilds, doctor self-checks, reconciler passes. Capture only what a user *intends to do*. The mapper is the canonical list.
17. **Failure isolation**: if the recorder errors, the underlying user action still succeeds. Activity is observability, not correctness. (Restated because it's that important.)
18. **PII surface**: `context_json` may contain subjects, recipient handles, search queries, snippet text, draft prefixes (first 80 chars), URLs (opt-in only). Never bodies. Never attachments. Never credentials. The PII audit test gates against drift.

## Architecture at a glance

```
                       ┌──────────────────────────────────────┐
                       │  IpcMessage { id, source, payload }  │
                       └─────────────────┬────────────────────┘
                                         ▼
   ┌────────┐   ┌────────┐   ┌────────┐  │
   │  TUI   │   │  CLI   │   │  Web   │──┤
   └────────┘   └────────┘   └────────┘  │
                                         ▼
                ┌────────────────────────────────────────┐
                │  daemon dispatcher (handler/mod.rs)    │
                │  ─────────────────────────────────────  │
                │  1. tracing span                       │
                │  2. handle request → response          │
                │  3. activity::Recorder::record(...)    │  ◄── seam
                │     (mpsc::try_send, never blocks)     │
                │  4. return response                    │
                └─────────────────┬──────────────────────┘
                                  ▼
                         ┌──────────────────┐
                         │  SQLite          │
                         │  user_activity   │
                         │  + FTS5 mirror   │
                         └──────────────────┘
                                  ▼
                         ┌──────────────────┐
                         │  daily prune     │  (per-tier retention)
                         └──────────────────┘
```

## Where everything lives

| Concern | Path |
|---|---|
| Migrations | `crates/store/migrations/035_user_activity.sql`, `036_user_activity_fts.sql`, `037_saved_activity_filters.sql` |
| Storage repo | `crates/store/src/user_activity.rs` |
| Tier / ClientKind types | `mxr_store::Tier`, `mxr_protocol::ClientKind` |
| Recorder (mpsc worker) | `crates/daemon/src/activity/mod.rs` |
| Mapper (Request → entry) | `crates/daemon/src/activity/mapper.rs` |
| Tier classifier | `crates/daemon/src/activity/tier.rs` |
| Dispatcher seam | `crates/daemon/src/handler/mod.rs::handle_request` |
| IPC handlers | `crates/daemon/src/handler/activity.rs` |
| Daily prune loop | `crates/daemon/src/loops.rs::activity_prune_loop` (spawned from `server.rs`) |
| CLI subcommand tree | `crates/daemon/src/cli/mod.rs` + `commands/activity.rs` |
| TUI modal state | `crates/tui/src/app/state/modals.rs::ActivityModalState` |
| TUI modal renderer | `crates/tui/src/ui/activity_modal.rs` |
| TUI diagnostics integration | `crates/tui/src/app/state/diagnostics.rs` (`Activity` pane) |
| Web bridge routes | `crates/web/src/routes_v6.rs` under `extend_admin` (`/api/v1/admin/activity/*`) |
| Web React surface | `apps/web/src/features/activity/{ActivityRoute.tsx,api.ts}` + `apps/web/src/routes/activity.tsx` |
| Web diagnostics integration | `apps/web/src/features/diagnostics/{DiagnosticsRoute.tsx,EventsPanel.tsx,LogsPanel.tsx}` |
| Config | `crates/config/src/types.rs::ActivityConfig` + `ActivityRetentionConfig` |
| AGENTS.md invariants | `AGENTS.md` "Activity Log Invariants" section |
| PII audit + structural test | `crates/daemon/tests/activity_invariants.rs` |
| Bench harness | `benches/user_activity.rs` |

## Capture seam — why one place, what flows through it

We argued about this for a while during design. Three options were on the table:

1. **Per-handler emit** — each mutation handler explicitly logs activity.
2. **Macro-wrapped handlers** — `#[capture_activity]` proc-macro on each handler fn.
3. **Single dispatcher wrap** — instrument `handle_request` once; capture from the request shape.

We picked (3) for these reasons:

- **One place to enforce pause**. The recorder checks pause state once; per-handler emit would require every handler to remember.
- **One place to enforce source tagging**. The `source: ClientKind` comes off the envelope, not from in-band data the handler can forget to plumb.
- **One place to enforce failure isolation**. A handler can't accidentally `?` the activity write and break the user action.
- **One place to enforce the PII budget**. The mapper is the only thing that touches request internals to build `context_json`. Auditing one file beats auditing every mutation handler.

The trade-off: the mapper has to *know* the shape of every request it cares about. We accept that. The exhaustive-match version (compile error on every new variant) was the original plan; the current implementation uses explicit arms plus `_ => None` because the protocol churns enough that the compile-error friction was paying for itself in maintenance overhead, not safety. **If a new request variant adds user-intent capture, add it to the mapper. There is no automated way to remind you.** This is the price we pay for the protocol staying ergonomic.

The seam is *after* `handle_request` returns, not before. That's deliberate: we record `ok = response.is_ok()` so failed actions don't pollute the log. Failures live in `event_log`, not `user_activity`.

## Recorder — what's load-bearing

The recorder is the most subtle component. It runs as a single `tokio::spawn`ed worker draining a bounded `mpsc::channel`. Three properties are load-bearing:

1. **`try_send`, never `send`.** If the channel is full, drop the entry with `tracing::warn!` and continue. We absolutely cannot have backpressure into the dispatcher — the dispatcher must return the response immediately. Channel capacity is 1024; at realistic IPC rates that's ~1 second of buffering, plenty.
2. **Pause-respect loop.** The worker checks the pause flag on every record. Pause is per-process state; auto-resume fires when `paused_until` elapses. We considered persisting pause across daemon restarts and decided against it: pause is a deliberate, user-visible action and getting it stuck on across a restart would be worse than the alternative. The user can re-`pause` after restart if they want.
3. **Force-record path.** Pause/resume markers go through `record_forced` which bypasses the pause check. Without this, pausing would hide the act of pausing, which destroys the diary framing.

Compaction lives inside the worker. The cache is a small per-process `HashMap<(account_id, action, target_id), CompactionEntry>` with LRU-ish eviction (drop oldest `ts` when over 32 entries). This is sized for *rapid-fire bursts*, not for general dedup — important-tier rows are never coalesced (audit fidelity). If you raise the window from 250ms, raise the cache size proportionally. If you ever make compaction tier-blind, you've broken the audit log; please don't.

## Mapper — adding a new IPC verb

Whenever someone adds a new `Request` variant, you have two options:

1. **It's user-intent worth logging.** Add an explicit arm in `crates/daemon/src/activity/mapper.rs`. Pick the action token from the canonical taxonomy (`<noun>.<verb>`, snake_case, present tense, paired verbs for inverses). Pick `target_kind`/`target_id` if applicable. Build `context_json` honoring the size and PII rules below. Then add the action to `tier.rs` so it doesn't fall into the `Standard` default if you wanted a different tier.
2. **It's a getter / poll / internal.** Do nothing. The catch-all `_ => None` arm logs a `tracing::debug!` and skips. If you want a compile-time signal, add a unit test in `mapper.rs` that asserts `map_request(&Request::YourNewVerb { … }, …)` returns `None`.

A new action token also needs:

- An entry in the canonical action catalog (now collapsed into the user-facing guide at `site/.../guides/activity-log.md`; if it's important enough to surface to users, document it there).
- A formatter in `apps/web/src/features/activity/ActivityRoute.tsx` if the default JSON.stringify isn't readable.
- A replay template in `crates/daemon/src/commands/activity.rs::describe_group` if it belongs in narrative output.

### Context-JSON discipline (codified in recorder)

| Field | Limit | Behavior |
|---|---|---|
| Subject | 200 chars | Truncate with `…` |
| Draft body prefix | 80 chars | Truncate with `…` |
| Search query | 500 chars | Reject (too-long search is a UI bug) |
| Recipient list | 20 entries | Truncate; encode `truncated_count: N` |
| URL (`link.click`) | 2000 chars | Truncate with `…` |
| Filename | 255 chars | Truncate |
| Total `context_json` after serialization | 4 KiB | Truncate `target_ids` first, then fall through |

**Forbidden keys at any nesting depth** — codified by the PII audit test:
`password`, `password_hash`, `token`, `access_token`, `refresh_token`, `secret`, `api_key`, `client_secret`, `private_key`, `oauth_token`, `id_token`, `cookie`, `session_id`.

Never store: OAuth tokens, refresh tokens, password hashes, attachment bytes, full mail body text. The audit test scans column content and fails the build if any forbidden key appears.

## Tier policy — why the three buckets are not negotiable

| Tier | What goes here | Default retention | Why |
|---|---|---|---|
| `ephemeral` | view changes, palette opens, navigation | 30 days | High volume, low retrospective value beyond a month. |
| `standard` | searches, snippet inserts, thread reads, attachment views | 90 days | Mid-value. Matches `event_retention_days` default for symmetry. |
| `important` | mail mutations, sends, account changes, rule edits, redactions, retention prunes | 365 days | Permanent record of state-changing actions. |

Three knobs is the right number. Two collapses ephemeral and standard into noise; four is decision paralysis. The retention windows aren't arbitrary — they map to common "how far back would I look?" intervals (a month, a quarter, a year). If you want different defaults, change `crates/config/src/types.rs::ActivityRetentionConfig::default` and the docs that mention the defaults (search for "30 / 90 / 365" across `site/`).

If you add a new top-level action prefix that doesn't fit existing buckets, decide its tier *before* shipping. The default for unknown actions is `Standard`. That's safe for most cases but wrong for, say, a `bulk_*` family — those usually belong in `important`.

## IPC contract — choices to remember

- **`ClientKind` is on the envelope, not in the payload.** Putting it in the payload would mean every request type duplicates the field. The envelope is the right place — it's metadata about the *transmission*, not the *content*.
- **`#[serde(default)]` on `IpcMessage.source`.** Legacy clients (pre-Phase-2) decode as `Cli`. We picked `Cli` because hand-rolled scripts against the socket are the realistic legacy.
- **Pagination is cursor-based.** `(ts DESC, id DESC)`. Offset/limit is wrong under concurrent inserts; cursors stay stable.
- **`Count` already existed in `ResponseData`** with `count: u32`. We added `ActivityCount { count: i64 }` rather than overload. Different scale (activity can grow large), different consumer (UI badges). Don't merge them.
- **Saved-filter slugs are user-chosen.** Validated only for non-empty. We deliberately don't enforce a slug format; users want `mail-week` and `my favorite filter (test)` to both work.
- **Stats `group_by` is a closed enum** (`Action / Day / Source / TargetKind / Hour`). Open-ended grouping would require an SQL-injection-safe column-name validator; we didn't want that surface.

## TUI integration — where the seams are

The activity surface in the TUI lives in two places:

1. **Modal** (`g a` chord from any screen): `crates/tui/src/ui/activity_modal.rs` + `ActivityModalState`. Read-only browser of the last 24h. Pause toggle, but no redact — destructive ops flow through the CLI in v1.
2. **Diagnostics pane**: `DiagnosticsPaneKind::Activity` in the existing diagnostics six-pane layout. Same data, different framing — diagnostics is for "something's broken, what was I doing?".

Both consume the same `ListActivity` IPC. Refetch is opt-in via `r` in the diagnostics page; the modal refetches on open and on pause toggle. Don't add an auto-poll — the TUI is keyboard-driven and pollers fight with vim-style navigation.

The `/` key in the diagnostics page enters live-search mode for the focused pane (Events / Logs / Activity). The state lives at `DiagnosticsPageState.{events,logs,activity}_search` and the input cursor at `search_input: Option<DiagnosticsPaneKind>`. Filter is applied at render time — no IPC round-trip. If you ever wire a daemon-side filter, keep the local fallback for offline / paused-IPC cases.

## Web integration — surface notes

The web app has two activity surfaces:

1. **`/activity`** — the dedicated browser route. `apps/web/src/routes/activity.tsx` → `features/activity/ActivityRoute.tsx`. `ActivityBrowser` is the reusable inner component; `ActivityRoute` is the page wrapper. We extracted the browser so the Diagnostics tab can embed it without duplicating filter logic.
2. **`/diagnostics`** Activity tab — tabbed layout: Overview / Logs / Events / Activity. Same `ActivityBrowser` embedded with `embedded` prop to suppress the outer header.

The Diagnostics page got a similar treatment for logs and events: `LogsPanel` and `EventsPanel` are rich filtering panels with category dropdowns, free-text search, level filters, paging, and (logs only) live-tail with pause. They consume new bridge routes `/events/count` and `/events/categories` for pagination and dropdown population respectively.

The bridge dispatch is a thin proxy — no transformation in Rust. The OpenAPI snapshot at `crates/web/src/snapshots/mxr_web__tests__openapi_spec_summary.snap` is generated; if you add types to `ResponseData`, the snapshot will drift and the test will tell you. Accept the new snapshot.

TanStack Router uses file-based routing. `apps/web/src/routes/activity.tsx` is the route file; the `routeTree.gen.ts` is generated by `vite build`. Don't hand-edit `routeTree.gen.ts`.

## Privacy — three control layers and one trap

From most permanent to most temporary:

1. **Hard kill** — `MXR_ACTIVITY=off` env var at daemon startup. Recorder is spawned but every `record()` call is a no-op. Honored for the lifetime of the daemon.
2. **Soft pause** — `mxr activity pause [--for DURATION]`. Pause/resume markers land via `record_forced` so the gap is visible in the log.
3. **Tombstone** — `mxr activity clear --last DURATION|all` or `mxr activity redact --ids ... | --filter ...`. Irreversible. Audit columns survive; `context_json` is cleared. Retention prune still runs.

The trap: **don't add a "history" mode** that recovers redacted context. We explicitly chose tombstone-without-recovery so users can trust the redact button. If you ship a recovery path, the trust property dies.

`activity.track_link_clicks` is off by default. URL history reveals a lot. If you ever want to default-on a new context field, ask: does this reveal more than the action token itself? If yes, opt-in.

## Performance — what we measured, what we didn't

The bench harness at `benches/user_activity.rs` is a Criterion suite with four scenarios (insert serial, list unfiltered over 10k, list by action_prefix over 10k, stats by action over 10k). It's a guardrail, not a CI gate — bench environments vary too much for absolute thresholds. **Run it locally before any change to the storage layer**, capture the baseline, then re-run after.

Targets the design assumed (informative, not enforced):

| Bench | Target | Notes |
|---|---|---|
| Insert serial | < 0.5 ms p99 | warm db, WAL, single writer |
| Insert concurrent (10 producers, 1 writer-pool) | < 1.5 ms p99 | producer wait included |
| List 50 rows over 100k-row DB | < 25 ms p99 | indexed `(ts DESC)` |
| List 50 rows with `action_prefix='mail.'`, 100k rows | < 50 ms p99 | LIKE prefix on `idx_action_ts` |
| FTS query, 100k rows | < 250 ms p99 | unicode61 tokenizer |
| Stats by action, 100k rows | < 100 ms p99 | covered by `idx_action_ts` |

If a target degrades, add an index — don't change the schema or the writer pattern. Schema migrations are expensive to deploy through users' on-disk DBs.

### Compaction details

The 250ms window and 32-slot cache size were calibrated for typical TUI burst rates (holding a shortcut, bulk-archive sweeps). If you bump either, you raise memory ceiling and you might start coalescing genuinely separate user intents. If you ever consider tier-blind compaction: don't. Important-tier rows (sends, redactions, account changes) need full audit fidelity.

The compaction cache is per-process. Daemon restart drops it. That's fine — the worst case is one extra row right after restart.

## Tests — what's load-bearing vs. what's regression bait

| Test surface | Why it exists | Don't delete |
|---|---|---|
| `mxr_store::user_activity` unit tests (14) | Lock cursor pagination, redact semantics, FTS-after-redact, prune-by-tier behavior, saved-filter upsert/mark/delete | Yes — every one represents a contract |
| `mxr::activity::mapper` tests (11) | Lock the action tokens emitted per Request shape (so a refactor of the protocol can't silently change what gets logged) | Yes |
| `mxr::activity` recorder tests (6) | Pause drop, force-record, auto-resume, coalesce, important-tier-no-coalesce | Yes — these test the load-bearing invariants |
| `mxr::activity::tier` tests (6) | Tier classification table-driven | Yes |
| `mxr::handler::activity` tests (6) | IPC shape + dry-run-no-mutate | Yes — dry-run safety is on the path |
| `mxr::commands::activity` tests (11) | Duration parsing, recall grammar, replay grouping | Yes |
| `mxr_tui::activity_modal` state tests (5) | Pure state machine — open, select, clamp, close | Yes |
| `cli_help` snapshots (15) | Snapshot the user-facing help text. Drift means user docs are now wrong | Accept the snapshot when you change a flag intentionally; do not delete the test |
| `activity_invariants` integration (2) | **PII audit** (no forbidden keys ever appear) and **structural single-writer** (no code path outside `crates/daemon/src/activity/` calls `record_activity` or writes to `user_activity`) | Absolutely — these are the privacy gates |

The structural test reads the workspace as text. If you split the activity module, update the allow-list in the test. If a legitimate new path needs to write to `user_activity` (it shouldn't, but if), add it explicitly with a comment explaining why.

## Cross-phase invariants we ran every merge against

These appear in `STATUS.md`-style checklists but the actual gates are tests and code-review discipline:

1. Activity write failures never propagate to user-facing actions. Tested by recorder unit tests; enforced by `Recorder::record` swallowing errors.
2. Source enum is set correctly on every IPC request. Verified at the four client construction sites: `crates/tui/src/client.rs` → `Tui`, `crates/web/src/ipc.rs` → `Web`, `crates/daemon/src/ipc_client.rs` → `Cli`, daemon-synthesized markers → `Daemon`.
3. Heartbeats / polls / internal getters are not logged. Tested by `ping_produces_no_activity` and `list_envelopes_is_a_getter_and_does_not_log` in the mapper tests.
4. No credential material ever lands in `context_json`. Tested by the PII audit integration test.
5. Only `crates/daemon/src/activity/` writes to `user_activity`. Tested by the structural single-writer integration test.

If you have to break #5 for a feature that genuinely belongs outside `activity/`, that's the signal to revisit the module boundary, not to relax the test.

## Out of scope (intentional)

- **Cross-device sync of activity.** Hard invariant. The activity table is the user's diary; replicating it would convert "diary" into "telemetry" framing-wise.
- **Encryption at rest.** Relies on FS-level encryption. Revisit when account-key infra lands.
- **Activity-driven recommendations / ML.** Out of v1. Would need a separate consent flow.
- **Per-account separate activity stores.** Single table with `account_id` column. Splitting would multiply migration cost.
- **Activity-replay-as-undo.** The regular undo path covers this. Activity tells you what happened; undo lets you reverse it. Two features, two stores.

## How to enhance this safely (a future-maintainer checklist)

When adding new behavior:

- [ ] If a new `Request` variant captures user intent, add an arm to the mapper *and* a test asserting the resulting `OwnedEntry`.
- [ ] If a new action prefix is added, set its tier explicitly in `tier.rs`. Don't rely on the `Standard` default unless that's genuinely correct.
- [ ] If `context_json` gains a new field, update the size table above and the user-facing docs at `site/.../guides/activity-log.md`.
- [ ] If a new context field is sensitive (URLs, body excerpts, attachment names with PII), make it opt-in via a new `activity.track_*` config key. Default to `false`.
- [ ] If a destructive operation is added (a new `prune`/`redact`/`clear` variant), pair it with a `--dry-run` flag and an interactive confirm (skippable with `--yes`). Same discipline as the existing mutating verbs.
- [ ] If the schema changes, mind the 4 KiB context-json cap and the WAL behavior. Add a migration; don't try to evolve existing rows in place.
- [ ] If a new IPC verb queries the table, make sure it can't bypass the `redacted` filter unless `include_redacted=true` is explicit. We accidentally exposed redacted rows in the first export-handler draft; the test in `redact_excluded_from_default_list` (in storage tests) catches that.
- [ ] If the TUI gains a new pane, register it in `DiagnosticsPaneKind::next()/prev()`, `all_panes()`, `pane_label()`, scroll offset accessors, and `summary_lines` / `pane_lines`. Then re-snapshot the diagnostics-page test.
- [ ] If the web gains a new bridge route, add it to `extend_admin` (or the appropriate router), regenerate the OpenAPI snapshot, and add a typed client in `apps/web/src/features/activity/api.ts`.
- [ ] If the CLI gains a new subcommand, regenerate `crates/daemon/tests/snapshots/cli_help__*.snap` and the published reference page (`cd site && npm run generate`).

When changing existing behavior:

- [ ] Don't break the `(ts DESC, id DESC)` cursor invariant. Anything that changes ordering breaks pagination under concurrent writes.
- [ ] Don't change the tombstone semantics. Redacted rows must keep audit columns and drop `context_json`. Anything else breaks the "scrub but provable" property.
- [ ] Don't add automatic recovery for redacted rows. See the "Privacy" section's trap warning.
- [ ] Don't move writes outside `crates/daemon/src/activity/`. The single-writer test will catch it; the structural property is what users trust.
- [ ] Don't make pause/resume markers themselves redactable in the same `clear`/`redact` call. They land *after* the redact completes for a reason — the user needs to be able to see what was redacted, even if not what was inside it.

## Test verification commands

```bash
# Unit + module tests
cargo test --workspace user_activity
cargo test -p mxr activity::
cargo test -p mxr commands::activity::
cargo test -p mxr handler::activity::
cargo test -p mxr-tui activity_modal_tests
cargo test -p mxr-tui --test snapshots diagnostics_page_snapshot

# Integration tests
cargo test -p mxr --test activity_invariants
cargo test -p mxr --test cli_help

# End-to-end against a real daemon (or `mxr demo`):
mxr activity --help
mxr activity list --since 1h
mxr activity stats --group-by action --since 7d
mxr activity recall "yesterday afternoon"
mxr activity replay --since 1h
mxr activity saved list
mxr activity pause --for 1h
mxr activity status

# Surfaces:
# TUI:   press `g a` from any screen, or open Diagnostics and tab to Activity.
# Web:   /activity route, or /diagnostics → Activity tab.

# Inspect the schema:
sqlite3 ~/.local/share/mxr/mxr.db \
  "SELECT version, name FROM schema_migrations ORDER BY version DESC LIMIT 5;"
# Expect 37 (saved_activity_filters), 36 (user_activity_fts), 35 (user_activity) + earlier.

# Bench harness (optional, local-only):
cargo bench --bench user_activity
```

## Background — why this took ten phases

We sequenced phases to ship something at the end of every one: Phase 1 is testable in isolation against the store; Phase 2 gets you data flowing through the dispatcher; Phase 3 makes it queryable; Phase 4 is the user-facing CLI. The TUI and web surfaces (Phases 5 and 6) layer on the same IPC contract — they consume what Phase 3 provides. Phase 7 (privacy) is cross-cutting; we didn't break it out earlier because the controls don't make sense until they have something to control. Phase 8 power features (saved filters / recall / replay) are deliberate dessert — they're the affordances that make the feature *fun* rather than just *useful*. Phase 9 polish (compaction, benches, exhaustiveness sketch, docs) is where the rough edges get sanded.

If you're tempted to short-cut a future phase ("we don't need a recorder, just synchronous insert in the dispatcher"), re-read the "Recorder — what's load-bearing" section above. The fire-and-forget pattern is the single thing that makes activity actually safe to leave on by default. Without it, a slow disk turns into a slow inbox.

## Pointer index

- User-facing guide: [`site/src/content/docs/guides/activity-log.md`](../site/src/content/docs/guides/activity-log.md)
- CLI reference (auto-generated): [`site/src/content/docs/reference/cli/activity.md`](../site/src/content/docs/reference/cli/activity.md)
- Config reference: [`site/src/content/docs/reference/config.md`](../site/src/content/docs/reference/config.md) (`[activity]` section)
- Diagnostics integration: [`site/src/content/docs/guides/observability.md`](../site/src/content/docs/guides/observability.md)
- Architecture sketch: [`ARCHITECTURE.md`](../ARCHITECTURE.md) (Activity log section)
- Invariants in agent context: [`AGENTS.md`](../AGENTS.md) (Activity Log Invariants section)
