# mxr — Delight Plan

> The plan to take mxr from "well-built operator tool" to "the email client people are delighted to use."

## Context

mxr is a Rust daemon-backed terminal email client at v0.5.0. The architecture is sound (boundaries enforced, sync real, Tantivy wired, undo works). The TUI is functional but operator-grade. Recent investment has gone into the HTTP bridge (slices 1–8) and analytics polish — neither is what drives switching from aerc/notmuch/Superhuman.

A direct competitor (**Herald**) shipped Nov 2025 with the same thesis. The clock is real.

This plan implements 17 features across 4 phases that together answer:
- "Does it feel instant?" (Phase 1)
- "Does it scale to 200 emails/day?" (Phase 2)
- "Is there a reason to *tell my friends*?" (Phase 3 — sender-as-unit is the unique bet)
- "Can someone non-Rust install this without a 4-day yak-shave?" (Phase 4)

**Thesis to commit to**: *"Email that respects the keyboard, the network, and your data."* Local-first, instant on every keystroke, finishes setup before lunch, and treats the *sender* as a first-class unit.

Opposite test (positioning is real, not generic):
- Cloud-native sync-everywhere (Superhuman) — opposite is credible
- Mouse-first visual (Apple Mail) — opposite is credible
- Configuration-as-philosophy (notmuch/mbsync DIY) — opposite is credible

So this positioning differentiates.

## Methodology

### TDD (red → green → refactor, vertical slices)

Every behavior change follows the loop:

1. **RED**: Write ONE test for ONE behavior. Run the [5-question quality gate](#test-quality-gate) before any implementation.
2. **GREEN**: Minimal code to pass that test only. No anticipating future tests.
3. **REFACTOR** (optional): Only after green. Tests must continue to pass unchanged.

**Never write all tests first then all code (horizontal slicing).** Vertical tracer bullets only — one test, one impl, repeat.

### Test Quality Gate (mandatory per test)

Before writing any implementation for a new test, the test must answer YES to all five:

1. Would this test fail if I introduced a bug (flip `>` to `>=`, `&&` to `||`)?
2. Are expected values from the spec/requirement, NOT computed from implementation?
3. Does the test exercise something beyond the happy path (boundary, null, error)?
4. If I deleted the function body, would this test fail (not tautological)?
5. Would this test survive an implementation swap — same observable behavior, different internals — unchanged?

Tests that fail any of these are sycophantic and must be rewritten before proceeding.

### Test placement preferences (in order)

1. **Behavior tests through public APIs** — daemon CLI journeys (extend `crates/daemon/tests/cli_journey.rs`), TUI snapshot tests (`crates/tui/tests/snapshots.rs`), store integration tests with real SQLite, search tests with real Tantivy.
2. **Provider conformance tests** for adapter behavior (FakeProvider as ground truth).
3. **Unit tests with FakeProvider** only when integration is genuinely too slow.
4. **No tests of internal helper functions** unless they have non-trivial logic worth specifying.

Mocks are limited to the network boundary (provider HTTP, LLM HTTP). Never mock the store, search, or daemon-internal traits.

## Step 0 — Persist the plan in the repo

Once out of plan mode, copy this file into the project so it survives cross-session and is visible to other contributors and agents:

```
docs/vision/01-delight-plan.md     ← this plan, verbatim
docs/vision/README.md              ← short index pointing at the plan + status
```

Each phase below also gets a sibling task tracker:

```
docs/vision/phase-1-feel.md
docs/vision/phase-2-triage.md
docs/vision/phase-3-sender-as-unit.md
docs/vision/phase-4-onboarding.md
```

These get checkboxes ticked as features land. They're the canonical out-of-session memory.

---

## Build Sequence

The phases are ordered by user-perceived value, not engineering convenience. Within each phase, tasks have an explicit dependency arrow (`→`) where one blocks the next. Otherwise tasks are independent and can be parallelized.

```
PHASE 1 (feel)         PHASE 2 (triage)        PHASE 3 (sender-as-unit)   PHASE 4 (onboarding)
────────────────       ────────────────        ─────────────────────      ────────────────────
1.1 optimistic         2.1 reply-later         3.1 snippets               4.1 crash-safe drafts
    rollback                ↓                       (no deps)                  (extend existing)
       ↓                2.2 custom snooze       3.2 sender view            4.2 doctor 2.0
1.2 Cmd+K palette          (smallest)              (no LLM dep)               (extend existing)
       ↓                2.3 auto-reminders      3.3 LLM provider           4.3 setup wizard
1.3 inbox row              (deps schema)           trait ──────┐              (largest)
    richness            2.4 send-later                          ↓
       ‖                    (deps draft schema)  3.4 thread summarize
1.4 type-ahead          2.5 screener                            ↓
    search                  (deps sync hook)    3.5 draft assist
       ‖                2.6 bulk unsubscribe        (deps semantic + LLM)
1.5 saved-search            (deps unsub HTTP)
    tabs
```

Phases ship behind no feature flags. Each completed feature ships immediately.

---

## Phase 1 — Make it feel right

Goal: every keystroke responds in <50ms. The TUI feels like Linear/Superhuman, not like a database admin tool.

### 1.1 Optimistic mutation rollback

**Why**: The single highest-leverage UX investment. Stops every mutation from feeling like a network round-trip. Architecture already supports it (TUI is async, daemon is async, SQLite is fast); the TUI just has to honor it.

**Files**:
- Existing: `crates/tui/src/app/mutation_helpers.rs:387–413` (`queue_or_confirm_bulk_action` and `apply_local_mutation_effect` already exist for single-item mutations).
- Existing: `crates/tui/src/app/mod.rs:148–182` (App state, `pending_mutation_queue`).
- Existing: `crates/protocol` Response types (extend with reconciliation events).
- Modified: `crates/tui/src/app/mutation_helpers.rs` — add snapshot/rollback infrastructure.
- Modified: `crates/tui/src/app/mutation_actions.rs` — apply optimistic effect uniformly for bulk + single.
- New: `crates/tui/src/app/mutation_snapshot.rs` — bounded ring buffer of pre-mutation state deltas keyed by mutation ID.
- New: `DaemonEvent::MutationReconciliationFailed { mutation_id, error_kind }` in `crates/protocol/src/types.rs`.

**Behavior to implement**:
1. Every mutation (star, archive, label add/remove, mark read, snooze, trash, junk) applies its effect to local state and re-renders BEFORE the IPC request is sent.
2. The pre-state is snapshotted in a bounded queue (max 64 in-flight mutations).
3. On daemon `MutationCompleted` event with success → discard snapshot.
4. On `MutationReconciliationFailed` → replay snapshot, surface a toast in hint bar with undo guidance.
5. Snapshots are deltas (only the affected envelope fields), not full state clones — bounded memory.

**Tests** (TDD, in this order):

1. `apply_optimistic_star_updates_state_before_response` — call mutation, assert state shows starred = true *without* completing the IPC future. (Behavior: optimistic UI.)
2. `failed_reconciliation_rolls_back_to_pre_state` — drive a mutation with FakeProvider returning failure; assert state restored.
3. `snapshot_buffer_evicts_oldest_when_full` — enqueue 65 mutations, assert oldest snapshot is dropped (boundary case).
4. `concurrent_mutations_on_same_message_compose_correctly` — star then label; both reconcile; final state = both applied. (Equivalence class: out-of-order success.)
5. `concurrent_mutations_on_same_message_partial_failure` — star succeeds, label fails; state = starred but no label.

All tests use real SQLite + FakeProvider with controllable response delay. No mocking of store or app state.

### 1.2 Cmd+K command palette as primary discovery

**Why**: Power users learn shortcuts from the palette, not from docs. The hint bar is static and bounded; the palette is searchable and self-teaching.

**Files**:
- Existing: `crates/tui/src/ui/command_palette.rs` (basic palette already shipped).
- Existing: `crates/tui/src/app/state/command_palette.rs:136` (state).
- Existing: `crates/tui/src/keybindings.rs` (binding registry).
- Existing: `crates/tui/src/ui/hint_bar.rs:83–200` (current hint surface).
- Modified: `command_palette.rs` — promote to primary nav; show keybinding next to each command name.
- Modified: `keybindings.rs` — single source of truth: every action has `name`, `description`, `binding`, `context_visibility`.
- Modified: `hint_bar.rs` — pull from same registry; shows top 5 contextual bindings only; no duplication of palette content.

**Behavior to implement**:
1. `Ctrl+K` (or `:` in normal mode) opens palette regardless of current screen.
2. Every action that exists is in the palette. Compose, reply, archive, label, search, jump-to-saved-search-N, `mxr senders`, `mxr sender <addr>`, etc.
3. Each row shows: `<icon> <action label>           <binding>`
4. Fuzzy match on action label + description.
5. Recent commands surface to the top (last 8 used, persisted across sessions in `local_state.rs`).

**Tests**:
1. `palette_opens_on_ctrl_k_from_any_screen` — drive from inbox, message view, compose; palette visible in all.
2. `fuzzy_match_ranks_exact_prefix_above_substring` — search "rep" → "reply" before "report" (spec-driven ordering).
3. `every_keybinding_in_registry_is_searchable_in_palette` — for each binding in `keybindings.rs`, assert palette can fuzzy-find it.
4. `recent_command_surfaces_above_alphabetical` — execute "archive" twice, then open palette empty-query; archive is in top results.
5. `palette_dispatches_action_when_selected` — select "Star message"; assert star mutation queued.

### 1.3 Richer inbox rows

**Why**: The current row shows sender (22ch) + subject + date + flags. Reviewers compare this to Apple Mail's three-line preview. We need parity in the *single-line dense view* without losing the dense feel.

**Files**:
- Modified: `crates/tui/src/ui/mail_list.rs:109–199` (current `build_row`).
- Possibly modified: `crates/tui/src/app/state/mailbox.rs:9` (Envelope struct already has `snippet`, `has_attachments`, `size_bytes`, participation hints).
- Possibly: `crates/core` for any participation enrichment (if not already computed).

**Behavior to implement**:
1. Sender column: smart display name + first-name fallback + email when no display name. Truncate to 18ch but with overflow ellipsis.
2. Thread participation chip: when thread > 1 message, show `+N` next to subject (where N = other participants count).
3. Snippet preview: subtle dim style after subject, separated by `· `. Only when row has horizontal space.
4. Attachment chip: small `📎 45K` (no emoji if user disables emoji in config; fall back to `[A]`).
5. Relative time: `2m`, `3h`, `Tue`, `Mar 4` ladder.
6. Unread/starred/replied flags use intentional color, not just bold.

**Tests**:
1. `row_renders_smart_sender_when_display_name_present` — snapshot test on a known envelope.
2. `row_falls_back_to_local_part_when_no_display_name` — boundary.
3. `row_truncates_subject_with_ellipsis_at_terminal_width` — given width=80, assert no overflow; given width=200, assert no truncation.
4. `row_shows_thread_participation_chip_only_when_multi_message` — single-message thread = no chip.
5. `row_omits_snippet_when_terminal_too_narrow` — 60ch wide → snippet absent; 120ch → snippet present.
6. `row_relative_time_ladder_correct_at_each_threshold` — boundary cases at 1m/60m/24h/7d.

### 1.4 Type-ahead search

**Why**: Tantivy can serve queries in <50ms. Gating on Enter is leaving the speed feel on the table.

**Files**:
- Existing: `crates/tui/src/app/search_helpers.rs:265–326` (`trigger_live_search`, `queue_search_request`).
- Existing: `crates/tui/src/app/state/search.rs:8–17` (`pending_debounce` already a field — currently unused).
- Modified: `search_helpers.rs` — wire debounce timer.
- Modified: `crates/tui/src/input.rs` — search-input keypress triggers `Action::DebounceSearch`.

**Behavior to implement**:
1. On every keystroke in the search input, reset a 120ms debounce timer.
2. On expiry, call `queue_search_request` with current input.
3. Existing daemon search request is async and non-blocking; results stream into the result list as they arrive.
4. New keystroke during pending request cancels prior (`CancelInflight` request added; daemon already handles cancel for streaming search).
5. The result list is rendered as soon as first batch arrives, regardless of whether more pending.

**Tests**:
1. `search_fires_after_120ms_idle_not_per_keystroke` — five keystrokes within 50ms produce ONE daemon request (with the final string).
2. `new_keystroke_cancels_pending_search` — keystroke at t=100ms cancels the search queued at t=0; only the second result set is rendered.
3. `result_list_renders_first_batch_before_query_completes` — given a slow search response with two batches, assert the UI shows the first batch before the second arrives.
4. `empty_query_clears_results` — backspace to empty; assert results pane returns to default view.

### 1.5 Saved searches as top tabs

**Why**: Saved searches are first-class navigation in mu4e and Superhuman; mxr already has the primitive but buries it in the sidebar.

**Files**:
- Existing: `crates/tui/src/ui/sidebar.rs` (saved-search list).
- New: `crates/tui/src/ui/tab_strip.rs` (top tab bar).
- Modified: `crates/tui/src/app/state/mailbox.rs` (active-tab index).
- Modified: `crates/tui/src/input.rs` — `1`–`9` jump to tab N when in inbox normal mode.
- Modified: `crates/tui/src/ui/mod.rs` — layout adds tab strip above inbox.

**Behavior to implement**:
1. Top of the inbox view shows up to 9 saved-search tabs (configurable via existing saved-search ordering).
2. `1`–`9` jumps to that tab. `0` returns to "All Inbox".
3. Active tab visually distinct.
4. Tab unread counts shown when known.
5. Tabs respect the user's existing saved-search order (no auto-reordering).

**Tests**:
1. `tab_strip_renders_first_nine_saved_searches` — given 12 saved searches, assert exactly 9 in strip.
2. `digit_key_jumps_to_corresponding_tab` — press `3`, assert active query = saved searches[2].
3. `tab_unread_count_reflects_search_match_count` — after sync delivers 5 unread matching saved-search 1, tab 1 shows "5".
4. `zero_returns_to_default_inbox` — from any tab, `0` → default inbox query.

---

## Phase 2 — Triage that scales

Goal: a power user can clear 200 emails in 30 minutes without leaving keyboard or feeling rushed.

### 2.1 Reply-later stack + walk mode

**Why**: HEY's killer flow. Steals it cleanly into terminal-land.

**Files**:
- Schema: New migration `crates/store/migrations/013_message_flags.sql`:
  ```sql
  CREATE TABLE IF NOT EXISTS message_flags (
      message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
      reply_later INTEGER DEFAULT 0,
      reply_later_set_at INTEGER,
      reply_later_dismissed_at INTEGER
  );
  CREATE INDEX idx_message_flags_reply_later ON message_flags(reply_later, reply_later_set_at DESC);
  ```
- New IPC: `Request::SetReplyLater { message_ids, flag }`, `Request::ListReplyQueue { account_id }`, response `ResponseData::ReplyQueue { rows }`.
- New: `crates/daemon/src/handler/reply_later.rs`.
- Modified: `crates/store/src/wrapped.rs` to expose CRUD on `message_flags`.
- New CLI subcommand: `mxr replies` (list), `mxr replies walk` (interactive).
- TUI: New action `Action::ToggleReplyLater` bound to `r` in inbox; new sidebar item / tab for "Reply Later"; walk mode reuses compose flow (`crates/tui/src/compose_flow.rs`).
- Search operator: extend Tantivy query parser to support `is:reply-later`.

**Behavior**:
1. Press `r` on a message → flagged as reply-later (optimistic update via 1.1).
2. `is:reply-later` saved search shows the queue.
3. `mxr replies walk` (CLI) iterates the queue; for each, shows preview, then opens `$EDITOR` with reply context. On send, flag clears. On skip, advance.
4. TUI walk mode (in palette / `gr` keybinding): same flow inline.
5. Replying to a message via any path (CLI `reply`, TUI `r r`) auto-clears the flag.

**Tests**:
1. `set_reply_later_flag_persists_across_daemon_restart` — set flag, restart daemon, query, assert still flagged.
2. `replying_to_flagged_message_clears_flag` — flag, send reply, assert cleared.
3. `dismissing_flag_does_not_send_reply` — flag, dismiss, assert flag cleared but no message in sent folder.
4. `is_reply_later_search_returns_only_flagged_messages` — flag 3 of 10 messages, search, assert exactly 3.
5. `walk_mode_advances_after_send` — start walk on 3 flagged, send first, assert next is loaded.
6. `walk_mode_advances_after_skip` — skip first, assert next loaded, first still flagged.

### 2.2 Custom-time snooze

**Files**:
- Existing: `crates/tui/src/ui/snooze_modal.rs`, `crates/store/src/snooze.rs`.
- New utility: `crates/core/src/time_parse.rs` — parser for `"tomorrow 9am"`, `"in 2h"`, `"friday"`, RFC3339.
- Modified: snooze modal adds a "Custom..." entry → text input → parse → confirm.
- CLI: `mxr snooze --until "tomorrow 9am" <id>`.

**Behavior**:
1. User picks "Custom" in modal → text input.
2. Parser accepts: relative (`in 2h`, `in 5d`), named (`tomorrow 9am`, `monday 17:00`), absolute (RFC3339).
3. Parse failure shows inline error, keeps input.
4. On success, daemon stores wake_at as unix ts in `snooze` table (existing).

**Tests** (parser is pure, easy to test exhaustively):
1. `parses_in_n_hours` — "in 2h" → now + 2h.
2. `parses_named_day` — "monday" at Tuesday 14:00 → next Monday 09:00 (default time).
3. `parses_named_day_with_time` — "monday 17:00" → next Monday 17:00.
4. `parses_tomorrow_with_time` — "tomorrow 9am" → tomorrow 09:00.
5. `parses_rfc3339` — "2026-06-01T15:00:00Z" → exact timestamp.
6. `rejects_past_time` — "yesterday 9am" → error.
7. `rejects_garbage` — "asdf" → error.
8. `accepts_24h_format` — "monday 17:00" parses.
9. `accepts_12h_format` — "monday 5pm" parses.

### 2.3 Auto-reminders ("nudge if no reply")

**Files**:
- Schema: `crates/store/migrations/014_auto_reminders.sql`:
  ```sql
  CREATE TABLE IF NOT EXISTS auto_reminders (
      message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      sent_message_id TEXT NOT NULL,
      remind_at INTEGER NOT NULL,
      triggered_at INTEGER,
      cancelled_at INTEGER
  );
  CREATE INDEX idx_auto_reminders_pending ON auto_reminders(remind_at) WHERE triggered_at IS NULL AND cancelled_at IS NULL;
  ```
- New IPC: `Request::SetAutoReminder { sent_message_id, days_or_at }`, event `DaemonEvent::ReminderTriggered`.
- New background loop in `crates/daemon/src/loops.rs` (clone of snooze-loop pattern).
- TUI: at compose-send confirmation, optional `Remind in __ days` field.
- CLI: `mxr send --remind-after 5d`.

**Behavior**:
1. User sends a reply. Optionally specifies a remind-after window.
2. Reminder row created.
3. Background loop wakes every minute (cheap; existing pattern in snooze-loop).
4. For each pending reminder where `now > remind_at`: check `reply_pairs` table (existing). If sent message has a reply, mark `cancelled_at`. If no reply, mark `triggered_at` and emit `ReminderTriggered` event; surface as a synthetic "Reminder" entry in the reply-later queue.
5. User dismissing or replying clears the reminder (cancellation).

**Tests**:
1. `reminder_fires_when_no_reply_after_window` — set reminder for 1 minute, advance fake time, assert event emitted.
2. `reminder_cancels_when_reply_received` — set reminder; mock provider delivers reply; assert reminder cancelled, no event.
3. `reminder_persists_across_daemon_restart` — set, restart, advance time past window, assert still fires.
4. `cancelled_reminder_does_not_re_fire_on_restart` — reply received then daemon restart; reminder stays cancelled.
5. `multiple_reminders_each_fire_independently` — three reminders different windows; advance time; each fires at correct moment.

### 2.4 Send Later

**Files**:
- Schema: `crates/store/migrations/015_scheduled_sends.sql`:
  ```sql
  ALTER TABLE drafts ADD COLUMN send_at INTEGER;
  ALTER TABLE drafts ADD COLUMN scheduled_status TEXT;
  CREATE INDEX idx_drafts_pending_scheduled ON drafts(send_at) WHERE send_at IS NOT NULL AND scheduled_status IS NULL;
  ```
- IPC: `Request::ScheduleSend { draft, send_at }`.
- Background loop: `crates/daemon/src/loops.rs` — scheduled-send flusher (clone of snooze-loop).
- CLI: `mxr send --at "tomorrow 9am"` reuses `2.2` time parser.
- TUI: in compose-confirm, optional schedule prompt.

**Behavior**:
1. Scheduled send goes to drafts table with `send_at` set, status = `scheduled`.
2. Background loop scans every minute; fires due rows via existing send path.
3. On send success, status = `sent`. On failure, status = `failed`, error logged, optional retry via existing event log.
4. User can `mxr drafts cancel <id>` before fire to abort.
5. Daemon restart: loop re-reads on startup; idempotent because send writes to provider with a stable `Message-ID`.

**Tests**:
1. `scheduled_send_fires_at_send_at` — schedule for t+60s, advance time, assert sent.
2. `cancelled_schedule_does_not_send` — schedule, cancel, advance time, assert nothing sent.
3. `scheduled_send_survives_restart` — schedule, restart daemon, advance time, assert sent exactly once.
4. `failed_send_retains_draft_for_retry` — provider returns transient failure; draft remains with status=failed; user can retry.
5. `idempotent_under_double_fire` — race two flushers (simulated); sent exactly once.

### 2.5 Screener (consent-based first-touch)

**Files**:
- Schema: `crates/store/migrations/016_screener_decisions.sql`:
  ```sql
  CREATE TABLE IF NOT EXISTS screener_decisions (
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      sender_email TEXT NOT NULL COLLATE NOCASE,
      disposition TEXT NOT NULL CHECK (disposition IN ('allow','deny','feed','paper_trail','unknown')),
      route_label TEXT,
      decided_at INTEGER NOT NULL,
      PRIMARY KEY (account_id, sender_email)
  );
  ```
- IPC: `Request::ListScreenerQueue`, `Request::SetSenderPolicy`.
- Sync hook: in `crates/sync/`, when ingesting messages, classify sender against `screener_decisions`; if unknown, route to screener.
- TUI: new screen `Screen::Screener` reachable via sidebar entry; three-key disposition (`a`/`d`/`f` for allow/deny/feed).
- CLI: `mxr screener list`, `mxr screener allow <addr>`, `mxr screener deny <addr>`, etc.

**Behavior**:
1. On sync, every inbound message is checked against `screener_decisions` (account, sender_email).
2. If sender has `allow` → normal inbox. `deny` → trash + auto-mark-read. `feed` → feed view, skip inbox. `paper_trail` → paper-trail view, skip inbox. `unknown` (no row) → screener queue (via a synthetic system label `_screener`).
3. User dispositions a sender once; future messages auto-route.
4. **Decision**: Disposition is **local-only by default**. Screener is the user's *consent* metadata, not mail metadata — it's about whose mail you've agreed to read, not how mail is filed. The notmuch-vs-mu4e lesson is that *mail metadata* (read state, folders, labels) should sync; *local categorization* should not pollute the provider with `_mxr/*` labels users may not want.
5. Exception via per-disposition opt-in: `route_label` field on a screener decision (set via `mxr screener allow alice@x.com --label "VIP"`). When non-null, the **specified provider label** is applied via the existing label mutation pipeline; absent, behavior stays local. This satisfies users who want mobile/web Gmail to see categorization without forcing it on those who don't.

**Tests**:
1. `unknown_sender_routes_to_screener_queue` — sync delivers from new sender; assert in queue.
2. `allow_sender_bypasses_queue` — allow Alice; new Alice message → inbox, not queue.
3. `deny_sender_trashes_subsequent` — deny spam@x.com; new message → trash + read.
4. `feed_sender_routes_to_feed` — feed newsletters@x.com; future messages get feed label, no inbox.
5. `screener_decision_persists_across_restart` — disposition Alice; restart; new Alice message → bypass queue.
6. `disposition_change_applies_to_future_only` — Alice was allowed, now denied; existing inbox messages stay; new ones go to trash.
7. `bulk_disposition_updates_all_pending` — 5 unknown messages from Alice in queue; allow Alice → all 5 move to inbox.

### 2.6 Bulk sender triage + unsubscribe

**Files**:
- Existing: `crates/provider-gmail/src/parse.rs:596–629` (List-Unsubscribe parsing into `messages.unsubscribe_method`).
- New IPC: `Request::ListSenders { metric, limit }`, `Request::Unsubscribe { sender_email }`.
- New: `crates/daemon/src/handler/senders.rs`, `crates/daemon/src/handler/unsubscribe.rs`.
- Reuse: existing reqwest client in daemon state.
- CLI: `mxr senders --top 20 --metric volume|response-time|open-threads --since 90d`, `mxr unsubscribe <addr>`.

**Behavior**:
1. `mxr senders` aggregates from existing `contacts` materialized view + on-the-fly join with `messages`. Top-N by chosen metric.
2. `mxr unsubscribe <addr>` reads the `unsubscribe_method` from latest message.
3. For `OneClick` (RFC 8058): POST to URL with `List-Unsubscribe=One-Click` header.
4. For `Mailto`: queue an outbound message (uses existing send pipeline).
5. For `HttpLink`: open browser via system handler (existing pattern).
6. For `BodyLink` / `None`: report no automated path.
7. On success, log to event log + label sender as `_unsubscribed` locally.

**Tests**:
1. `senders_top_volume_returns_correct_order` — fixture with known counts, assert sorted desc by message count.
2. `senders_filtered_by_since_excludes_older` — `--since 30d` excludes 60d-old.
3. `unsubscribe_one_click_posts_correct_body` — assert reqwest POST with required header (use httpmock).
4. `unsubscribe_mailto_creates_outbound_draft` — assert draft created with correct To.
5. `unsubscribe_failed_request_does_not_label_sender` — POST fails 500; sender NOT labeled `_unsubscribed`.
6. `unsubscribe_idempotent` — second call when already unsubscribed → no-op success.

---

## Phase 3 — Sender-as-unit (the unique bet)

Goal: ship the feature reviewers tell their friends about. mxr is uniquely positioned because the relationship data is already in local SQLite.

### 3.1 Snippets with `;name` + `{var}` placeholders

**Decision**: Snippets live in SQLite (consistent with the rest of state). Daemon-managed CRUD via IPC. TUI editor for add/edit. CLI mirrors. No TOML side-channel — single source of truth.

**Files**:
- Schema: `crates/store/migrations/019_snippets.sql`:
  ```sql
  CREATE TABLE IF NOT EXISTS snippets (
      name TEXT PRIMARY KEY,
      body TEXT NOT NULL,
      vars TEXT NOT NULL DEFAULT '[]',  -- JSON array of declared {var} names
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
  );
  ```
- New: `crates/compose/src/snippets.rs` — load (from store), expand, missing-var detection.
- New IPC: `Request::ListSnippets`, `Request::SetSnippet { name, body }`, `Request::DeleteSnippet { name }`.
- Hook: in `crates/compose/src/lib.rs` between draft file write and editor launch — pre-expansion of any `;name` already present in template; in `crates/daemon/src/handler/runtime.rs` `SendDraft` handler — post-edit scan of body for residual `{var}`, warn but allow.
- CLI: `mxr snippets list`, `mxr snippets add <name>` (opens `$EDITOR` on a buffer; saves to DB on close), `mxr snippets edit <name>`, `mxr snippets remove <name>`.
- TUI: snippet manager modal accessible from palette — list, edit (in-modal text area), delete.

**Behavior**:
1. While composing, typing `;thanks` followed by space anywhere in the body triggers post-edit expansion.
2. Built-in vars auto-populate: `{first_name}`, `{full_name}`, `{my_name}`, `{date}`, `{thread_subject}`.
3. Custom vars must be filled in (or user gets a warning at send-time).
4. Missing vars = warning to stderr, but send proceeds (user override).

**Tests**:
1. `snippet_expands_when_keyword_followed_by_space` — body "...;thanks " → expanded.
2. `snippet_does_not_expand_mid_word` — "thanks;thanks" stays literal.
3. `builtin_var_first_name_populates_from_recipient` — recipient is "Alice Smith <alice@x.com>"; `{first_name}` → "Alice".
4. `unfilled_var_emits_warning_at_send` — body has unfilled `{deadline}`; warning emitted, send proceeds.
5. `multiple_snippets_in_body_all_expand` — `;thanks ... ;sig` both expand.
6. `unknown_snippet_keyword_left_as_literal` — `;notdefined` stays as-is.

### 3.2 Sender view (`mxr sender <addr>`)

**Why**: Nobody else has this. Local SQLite is the unfair advantage.

**Files**:
- Existing: `crates/store/src/contacts.rs` (already has `total_inbound`, `last_inbound_at`, `cadence_days_p50`, `is_list_sender`).
- New: `crates/store/src/sender_profile.rs` — `SenderProfile` aggregate. Joins:
  - `contacts` for cadence/volume.
  - `reply_pairs` for response-time stats.
  - `messages` filtered for unanswered questions (heuristic: last in thread is from sender, contains `?`, no outbound reply within `cadence_days_p50` * 2).
  - `auto_reminders` for active reminders.
  - `message_flags` for reply-later flags from this sender.
- IPC: `Request::GetSenderProfile { account_id, email }` → `ResponseData::SenderProfile { ... }`.
- New: `crates/daemon/src/handler/sender_view.rs`.
- CLI: `mxr sender alice@example.com` — JSON or table output.
- TUI: new screen `Screen::SenderProfile` reachable from `S` on a message, or via palette.

**Aggregates surfaced**:
- Volume: total in/out, last 30 / 90 / 365 days.
- Response time: p50/p90/p99 of *your* replies, of *their* replies. Latency histogram.
- Open commitments: list of unanswered questions (with thread links).
- Open threads: threads where last message is from sender, no reply.
- Active reminders.
- Flagged for reply-later.
- Most recent thread summary (if Phase 3.4 done).
- Subscribed/unsubscribed status.
- Trend chart: messages-per-week sparkline.

**Tests**:
1. `sender_profile_volume_matches_message_count` — fixture: 17 inbound from Alice, profile reports 17.
2. `sender_profile_response_time_p50_correct` — fixture with known reply latencies; profile p50 matches.
3. `unanswered_question_detected_when_no_reply_within_cadence` — message ends in `?`, no reply in cadence*2 days; surfaced.
4. `replied_question_not_surfaced_as_unanswered` — same but reply present; not surfaced.
5. `trend_sparkline_buckets_into_weekly` — given 30 messages over 4 weeks, 4-bucket sparkline matches.
6. `unknown_sender_returns_empty_profile_with_zero_aggregates` — query `nobody@example.com`; returns valid empty profile (no error).

### 3.3 LLM provider trait (foundation for 3.4 + 3.5)

**Why**: The codebase has fastembed (pure Rust embeddings) but no completion path. This is the missing abstraction.

**Decision**: Pure-Rust local inference is the default; cloud is an opt-in override.
- **Default**: `mistral.rs` for local inference (pure Rust, supports Qwen 2.5 4B Instruct, Llama 3.2, etc., quantized GGUF format, Metal + CUDA acceleration). Stays consistent with the local-first principle and avoids the "go install Ollama separately" yak-shave.
- **Recommended model**: Qwen 2.5 3B / 4B Instruct (Q4_K_M GGUF) — small enough to run on a laptop, capable enough for summarization and short drafts.
- **Optional cloud override**: when user supplies an API key in config, route to an OpenAI-compatible endpoint (works with OpenAI, Anthropic-via-proxy, Groq, OpenRouter, Mistral La Plateforme, etc.). Single backend implementation covers most cloud LLMs.

**Files**:
- New crate: `crates/llm/`. Public trait:
  ```rust
  #[async_trait]
  pub trait LlmProvider: Send + Sync {
      async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse>;
      fn capabilities(&self) -> LlmCapabilities;  // model name, context window, streaming support
  }
  pub struct CompletionRequest {
      pub system: Option<String>,
      pub messages: Vec<ChatMessage>,
      pub max_tokens: Option<u32>,
      pub temperature: Option<f32>,
  }
  ```
- Implementations:
  - `LocalRustProvider` (mistral.rs backend) — default. Loads a configured GGUF model on daemon startup; warm in memory.
  - `OpenAiCompatibleProvider` — single impl for any OpenAI-compatible endpoint (configurable base URL + API key + model). Selected when `[llm.cloud] api_key = "..."` set.
  - `NoopProvider` — when LLM disabled, all completions return `Err(LlmDisabled)`. Lets compose / summarize gracefully degrade.
- Config: extend `mxr_config`:
  ```toml
  [llm]
  enabled = false                                    # off by default
  backend = "local"                                  # "local" or "cloud"

  [llm.local]
  model_path = "~/.cache/mxr/models/qwen2.5-3b-instruct-q4_k_m.gguf"
  context_window = 8192
  device = "auto"                                    # "cpu", "metal", "cuda", "auto"

  [llm.cloud]
  base_url = "https://api.openai.com/v1"
  api_key_env = "MXR_LLM_API_KEY"                    # secret read from env, not file
  model = "gpt-4o-mini"
  ```
- Model bootstrap: `mxr llm install <model>` downloads the GGUF (HuggingFace API, resume-on-failure, content-hash verify) to `~/.cache/mxr/models/`. Default invocation downloads Qwen 2.5 3B Instruct.
- Wired via daemon state (`crates/daemon/src/state.rs`) — `Arc<dyn LlmProvider>` set at startup based on config; lazy-load on first call to amortize startup cost when LLM features unused.

**Tests**:
1. `noop_provider_returns_disabled_error` — call `.complete()`; assert `LlmDisabled`.
2. `local_provider_loads_gguf_and_completes` — fixture: tiny Qwen / TinyLlama GGUF in test fixtures; assert end-to-end completion returns text.
3. `local_provider_uses_metal_when_available_on_macos` — assert acceleration backend selection follows config + capability detection.
4. `cloud_provider_serializes_openai_chat_request` — httpmock asserts POST body matches OpenAI's `/chat/completions` schema.
5. `cloud_provider_redacts_api_key_from_logs_and_errors` — induce error; assert error message + tracing span do not contain the key.
6. `cloud_provider_propagates_rate_limit_error_kinded` — mock 429; error has actionable kind (`RateLimited` with retry-after).
7. `provider_times_out_within_configured_window` — mock slow; assert timeout error.
8. `streaming_response_chunks_reassemble_in_order` — mock streamed chunks; assert final text matches expected.
9. `lazy_load_does_not_load_model_when_llm_features_unused` — boot daemon with `[llm] enabled = true`; never call complete; assert model not loaded into memory.

### 3.4 Thread summarize on demand

**Files**:
- IPC: `Request::SummarizeThread { thread_id }` → `ResponseData::ThreadSummary { text, generated_at }`.
- New handler: `crates/daemon/src/handler/summarize.rs`.
- Reuses semantic chunks from `crates/semantic/`.
- CLI: `mxr summarize <thread-id>` (JSON: `{ summary, used_messages, model }`).
- TUI: `S` on thread view → modal with summary; `r` to regenerate.

**Behavior**:
1. Pull thread messages (existing query).
2. If thread length ≤ 3 messages: refuse (not worth it; just read it). Return `ThreadTooShort` error.
3. Build prompt: system = "Summarize this email thread in 2–3 sentences focused on what's actionable for the user." messages = thread messages flattened with role hints.
4. Call LLM; return summary.
5. Cache: summaries cached in a new `thread_summaries (thread_id, content_hash, summary, generated_at)` table; regenerate only when content_hash changes.

**Schema**: `crates/store/migrations/017_thread_summaries.sql`:
```sql
CREATE TABLE IF NOT EXISTS thread_summaries (
    thread_id TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    summary TEXT NOT NULL,
    generated_at INTEGER NOT NULL,
    model TEXT NOT NULL
);
```

**Tests**:
1. `summarize_returns_thread_too_short_for_3_or_fewer` — boundary.
2. `summarize_caches_by_content_hash` — call twice on unchanged thread, second is cached (assert via timing or cache hit counter).
3. `summarize_regenerates_when_new_message_arrives` — summarize, append message, summarize again; second is fresh (different `generated_at`).
4. `summarize_with_llm_disabled_returns_disabled_error` — config has `llm.enabled = false`; assert error.
5. `summarize_propagates_llm_timeout_error` — mock slow LLM; assert timeout surfaced.

### 3.5 Draft assist grounded on sent corpus

**Files**:
- IPC: `Request::DraftAssist { thread_id, instruction }` → `ResponseData::DraftSuggestion { body }`.
- New handler: `crates/daemon/src/handler/draft_assist.rs`.
- Uses semantic search filtered to `direction = outbound` to retrieve top-K of user's prior sent messages similar to the thread context.
- LLM prompt: system = "You write email replies in the user's voice based on examples of their previous sent mail. Be direct and concise. Match their tone."
- CLI: `mxr draft-assist --reply <thread-id> --instruct "decline politely, suggest next month"` opens `$EDITOR` with the draft pre-populated.
- TUI: from compose, `Ctrl+G` (Generate) prompts for instruction, returns suggestion that becomes draft body.

**Behavior**:
1. Retrieve thread context.
2. Embed thread context, query semantic index for top-5 similar sent messages by *user* (filter on direction).
3. Construct LLM prompt with: system instruction, retrieved examples (truncated), thread context, user instruction.
4. LLM returns body.
5. Caller decides what to do — write to draft file (CLI) or paste into compose buffer (TUI).
6. **Never auto-send.** Always opens for review.

**Semantic filter dependency**: extend `crates/semantic/` to filter by `direction` when retrieving. May require schema addition to `semantic_chunks` if direction not already stored.

**Tests**:
1. `draft_assist_grounds_on_users_sent_messages_only` — fixture: inbound + outbound; retrieved set excludes inbound.
2. `draft_assist_returns_disabled_when_llm_off` — graceful degradation.
3. `draft_assist_includes_thread_context_in_prompt` — capture LLM request; assert thread messages present.
4. `draft_assist_respects_temperature_setting` — config temp = 0.2; LLM call args reflect.
5. `draft_assist_truncates_examples_when_over_token_budget` — fixture: 50 long prior messages; prompt stays under configured token budget.
6. `draft_assist_never_auto_sends` — call API; assert no provider send call occurred (use FakeProvider to verify).

---

## Phase 4 — Onboarding & resilience

Goal: someone non-Rust can install and be reading email in 5 minutes. Existing users don't lose drafts.

### 4.1 Crash-safe drafts

**Files**:
- Existing: `crates/store/src/draft.rs` already SQLite-backed with status state machine.
- Modified: extend draft status to include `editing` (currently has draft start hook?); add `last_heartbeat_at` column.
- Modified: TUI / CLI compose flow updates `last_heartbeat_at` periodically while editor is open (every 10s).
- Modified: daemon startup scans `drafts` where status = `editing` and `last_heartbeat_at < now - 60s` → marks `recoverable`.
- New CLI: `mxr drafts recover` (lists recoverable), `mxr drafts open <id>` (resumes), `mxr drafts discard <id>`.

**Behavior**:
1. Compose flow heartbeats while editor open.
2. Daemon detects orphaned drafts on startup.
3. User sees recoverable drafts on next launch.

**Tests**:
1. `draft_marked_recoverable_after_orphan_window` — set heartbeat to 2 min ago, restart, assert status = recoverable.
2. `active_draft_not_marked_recoverable` — heartbeat fresh, restart; status stays editing.
3. `recover_command_lists_only_recoverable_drafts` — fixture mix; `mxr drafts recover` shows only recoverable subset.
4. `discarded_recoverable_drafts_purged` — discard, assert row deleted from DB.

### 4.2 Doctor 2.0

**Files**:
- Existing: `crates/daemon/src/handler/diagnostics/`, `status_helpers.rs`.
- Modified: `DoctorReport` struct gains `findings: Vec<Finding>` where `Finding` includes `category`, `severity`, `message`, `failure_class: Option<FailureClass>`, `remediation: Vec<RemediationStep>` where `RemediationStep` includes `command: String` (a shell-runnable suggestion) and `description: String`.
- Classification:
  - OAuth refresh failed → suggest `mxr accounts reauth <id>`.
  - Network unreachable → suggest checking connectivity, list DNS resolution test.
  - Rate-limit hit → suggest waiting + show retry timer.
  - Search index corrupted → suggest `mxr doctor --reindex`.
  - SQLite locked → suggest closing other clients.
- CLI: `mxr doctor --json` returns the findings array.

**Tests**:
1. `oauth_failure_classifies_and_suggests_reauth` — induce expired token; doctor finding has `RemediationStep` containing `mxr accounts reauth`.
2. `network_failure_classifies_distinctly_from_oauth` — different failure modes get different findings.
3. `successful_doctor_run_returns_no_findings` — happy path.
4. `unknown_failure_returns_generic_finding_not_panic` — unrecognized error; falls back to category=Generic, no panic.

### 4.3 `mxr setup` wizard

**Files**:
- New crate: none — extend `crates/daemon/src/cli/`.
- New: `crates/daemon/src/cli/setup.rs` — interactive prompts using `dialoguer`.
- Demo seeder: `crates/provider-fake/src/seed.rs` — synthetic populated mailbox (50 messages from 12 senders, varied threads, attachments, subscriptions).
- CLI flow: `mxr setup` → choose provider (Gmail / IMAP / Demo) → for Gmail: OAuth browser launch (existing) → for IMAP: hostname/port/credentials → for Demo: seed FakeProvider → run smoke sync → `mxr doctor` → success.

**Decision**: `dialoguer` interactive prompts (Select / Confirm / Input / Password). Best UX for the "5-minute setup" promise. CLI flags also accepted to short-circuit prompts (scriptable for CI / dotfiles automation).

**Behavior**:
1. Interactive prompts step the user through: provider → credentials/OAuth → name → default editor → optional LLM enable + model download.
2. Each step has a "back" / "skip" / "use defaults" affordance.
3. End: a `mxr doctor` run validates everything; failures are remediated inline (with prompts).
4. Demo mode: skips OAuth, seeds 50 synthetic messages, opens TUI immediately.
5. Flag overrides: `mxr setup --provider gmail --email me@x.com --skip-llm` runs non-interactively where flags supply answers.

**Tests** (this is harder to TDD because it's interactive; structure as command-builder + behavior-of-each-step rather than full flow):
1. `setup_demo_mode_seeds_expected_message_count` — run setup with `--demo` flag; assert 50 messages in fake account.
2. `setup_demo_mode_skips_oauth_step` — assert no auth flow attempted.
3. `setup_gmail_writes_account_config_correctly` — drive with mock OAuth; assert account file written.
4. `setup_imap_validates_credentials_before_writing_config` — bad credentials; assert no config written, error returned.
5. `setup_completion_runs_doctor_and_reports_status` — successful setup; doctor invoked; results displayed.

---

## Cross-cutting concerns

### IPC additions consolidated

CoreMail bucket:
- `SetReplyLater`, `SetAutoReminder`, `ScheduleSend`, `Snooze` (extend with custom time), `SummarizeThread`, `DraftAssist`.

MxrPlatform bucket:
- `ListReplyQueue`, `ListReminderCandidates`, `ListScreenerQueue`, `SetSenderPolicy`, `ListSenders`, `GetSenderProfile`, `ListSnippets`, `SetSnippet`.

AdminMaintenance bucket:
- `Doctor` (extend response with findings).

Events:
- `MutationReconciliationFailed`, `ReminderTriggered`, `ScheduledSendFlushed`, `ScreenerDecisionApplied`, `DraftRecoverable`.

### Schema migrations consolidated

| # | Migration | Phase |
|---|-----------|-------|
| 013 | message_flags (reply_later) | 2.1 |
| 014 | auto_reminders | 2.3 |
| 015 | scheduled_sends (alter drafts) | 2.4 |
| 016 | screener_decisions | 2.5 |
| 017 | thread_summaries | 3.4 |
| 018 | draft_heartbeat (alter drafts) | 4.1 |
| 019 | snippets | 3.1 |

(Numbering picks up from existing 012; verify before assigning.)

### CLI additions consolidated

```
mxr replies                 # list reply-later queue
mxr replies walk            # interactive walk
mxr snooze --until "..."    # custom time
mxr send --at "..." --remind-after 5d
mxr screener list|allow|deny|feed|paper-trail [<addr>]
mxr senders --top N --metric volume|response-time|open-threads --since 90d
mxr unsubscribe <addr>
mxr sender <addr>
mxr summarize <thread-id>
mxr draft-assist --reply <thread-id> --instruct "..."
mxr snippets list|add|edit|remove
mxr drafts recover|open|discard
mxr setup [--demo] [--provider <p>] [--skip-llm]
mxr doctor --json
mxr llm install [<model>]              # downloads default GGUF (Qwen 2.5 3B Instruct) or named model
```

Every new command has both:
- `--json` output (default for piping; structured per existing `output.rs::resolve_format`).
- Human-readable table (default for terminal-attached).

### Test infrastructure additions

- New: `crates/test_support/src/llm_mock.rs` — controllable `LlmProvider` impl for deterministic tests.
- New: `crates/test_support/src/clock.rs` — virtual clock for time-based tests (auto-reminders, scheduled sends, snooze).
- Extend: `crates/daemon/tests/cli_journey.rs` with end-to-end tests per new feature.
- Extend: `crates/tui/tests/snapshots.rs` with snapshots for new screens (sender view, screener, reply-later queue, command palette with rich entries).

### Telemetry / observability

Each new feature emits structured `tracing` events at key decision points (mutation queued, mutation reconciled, mutation rolled back, reminder fired, schedule flushed, LLM completion start/end, etc.). No analytics dashboards built yet — just the events. Doctor 2.0 surfaces recent error events from the event log.

---

## Verification plan (end-to-end)

After each phase, run:

```
cargo test --workspace                                       # all unit + integration
cargo test --workspace --features live-smoke                 # provider live tests (manual)
mxr doctor                                                   # health check on dev daemon
```

**Phase 1 acceptance**:
- Star/archive/label feel <50ms perceptually (manual TUI test).
- Cmd+K palette finds every action by fuzzy match.
- Type "from:alice" in search bar; results stream in without pressing Enter.
- Press `1`–`9`; jumps to corresponding saved-search tab.
- Inbox row shows snippet, attachment chip, thread participation.

**Phase 2 acceptance**:
- Press `r` on 5 messages; `mxr replies walk` walks them.
- `mxr snooze --until "tomorrow 9am" <id>` snoozes correctly.
- Send a reply with `--remind-after 1m`; wait 70s; reminder appears in queue.
- `mxr send --at "+30s"` queues; 60s later, sent.
- Sync delivers a message from a new sender; appears in `mxr screener list`.
- `mxr unsubscribe <addr>` actually fires List-Unsubscribe and labels the sender.

**Phase 3 acceptance**:
- Type `;thanks` in compose body; expanded with `{first_name}` filled.
- `mxr sender alice@example.com` shows volume, response-time histogram, open commitments.
- (LLM enabled) `mxr summarize <thread-id>` returns coherent summary.
- (LLM enabled) `mxr draft-assist --reply <id> --instruct "decline"` returns a draft in user's voice.
- (LLM disabled) Same commands return graceful `LlmDisabled` error.

**Phase 4 acceptance**:
- Kill the editor mid-compose; restart daemon; `mxr drafts recover` lists the orphaned draft.
- Cause an OAuth failure; `mxr doctor --json` returns finding with `mxr accounts reauth` remediation.
- `mxr setup --demo` finishes in <60s with seeded inbox visible in TUI.

---

## Critical files modified or added (summary)

**TUI** (Phase 1 mostly):
- `crates/tui/src/app/mutation_helpers.rs` — optimistic snapshots
- `crates/tui/src/app/mutation_snapshot.rs` (new)
- `crates/tui/src/ui/command_palette.rs` — promote to primary
- `crates/tui/src/ui/hint_bar.rs` — slim down
- `crates/tui/src/ui/mail_list.rs` — row richness
- `crates/tui/src/app/search_helpers.rs` — debounce wiring
- `crates/tui/src/ui/tab_strip.rs` (new)

**Daemon handlers** (mostly Phase 2-3):
- `crates/daemon/src/handler/reply_later.rs` (new)
- `crates/daemon/src/handler/screener.rs` (new)
- `crates/daemon/src/handler/sender_view.rs` (new)
- `crates/daemon/src/handler/senders.rs` (new)
- `crates/daemon/src/handler/unsubscribe.rs` (new)
- `crates/daemon/src/handler/summarize.rs` (new)
- `crates/daemon/src/handler/draft_assist.rs` (new)
- `crates/daemon/src/loops.rs` — add reminder + scheduled-send loops
- `crates/daemon/src/handler/diagnostics/` — extend with findings/remediation
- `crates/daemon/src/cli/setup.rs` (new)

**Store**:
- 6 new migrations (013–018)
- `crates/store/src/sender_profile.rs` (new)
- `crates/store/src/wrapped.rs` — CRUD for new tables

**Other**:
- `crates/llm/` (new crate)
- `crates/core/src/time_parse.rs` (new)
- `crates/compose/src/snippets.rs` (new)
- `crates/protocol/src/types.rs` — Request/Response/Event additions
- `crates/provider-fake/src/seed.rs` (new) — demo seed

---

## Decisions (resolved)

1. **LLM backend** → Pure-Rust local default via `mistral.rs` + Qwen 2.5 3B Instruct (Q4_K_M GGUF). Optional cloud override via OpenAI-compatible config + API key in env.
2. **Snippet storage** → SQLite `snippets` table. Daemon-managed CRUD via IPC. No TOML side-channel.
3. **Screener label sync** → Local-only by default. Per-disposition opt-in `route_label` for users who want mobile/web parity.
4. **Setup interactivity** → `dialoguer` interactive prompts; CLI flags accepted as overrides for scripts/CI.

## Remaining open questions

1. **Custom-time snooze parser**. Roll our own parser (no deps) vs use `chrono-english` / `humantime` (slightly heavier but battle-tested)? *Recommendation: chrono-english for the conversational forms ("tomorrow 9am"), humantime for ISO durations ("2h30m"). Two thin deps, full coverage.*
2. **Sender view in TUI**. Dedicated full-screen page (consistent with other detail pages) vs side panel pop-out triggered with `S` from a message? *Recommendation: full-screen — consistent with thread/compose pages, more space for charts.*
3. **Draft assist token budget**. Hard-cap per provider model (e.g., 6k tokens for the 8k-context Qwen) vs let the LLM provider decide and surface errors? *Recommendation: hard-cap with configurable margin; truncate retrieved examples first, thread context last.*
4. **Reminder window model**. Days-only (`--remind-after 5d`) vs arbitrary durations (`--remind-after 36h30m`)? *Recommendation: arbitrary durations via the same parser as custom-time snooze — one syntax to learn.*
5. **Default model size**. Qwen 2.5 3B Instruct (~2.0GB Q4_K_M) is the sweet spot for laptops; Qwen 2.5 7B Instruct (~4.4GB Q4_K_M) is meaningfully smarter but slower. *Recommendation: ship 3B as default download; document 7B as the "I have a beefy machine" upgrade.*
