# Phase 2 — Triage that scales

> Goal: a power user can clear 200 emails in 30 minutes without leaving keyboard or feeling rushed.

See [01-delight-plan.md §Phase 2](./01-delight-plan.md#phase-2--triage-that-scales) for full specs.

## Tracker

### 2.1 Reply-later stack + walk mode

**Store layer ✅**
- [x] Migration `013_message_flags.sql` (table + partial index on flagged set)
- [x] Wired into `pool.rs` MIGRATIONS array (version 13)
- [x] `crates/store/src/message_flags.rs` with `set_reply_later`, `clear_reply_later`, `is_reply_later`, `list_reply_later`
- [x] `cargo sqlx prepare` regenerated `.sqlx` cache
- [x] RED+GREEN: `is_reply_later_returns_false_for_unflagged_message`
- [x] RED+GREEN: `set_reply_later_persists_flag`
- [x] RED+GREEN: `clear_reply_later_unsets_flag`
- [x] RED+GREEN: `list_reply_later_is_empty_for_fresh_store`
- [x] RED+GREEN: `list_reply_later_returns_only_flagged_messages`
- [x] RED+GREEN: `list_reply_later_orders_by_set_at_descending`
- [x] RED+GREEN: `re_setting_reply_later_refreshes_set_at`

**Daemon layer ✅**
- [x] IPC types: `Request::SetReplyLater { message_id, flag }`, `Request::ListReplyQueue`, `ResponseData::ReplyQueue { messages }`
- [x] Handler `crates/daemon/src/handler/reply_later.rs` (`set_reply_later`, `list_reply_queue`)
- [x] Wired into dispatch + safety policy + request_kind table
- [x] RED+GREEN: `dispatch_set_reply_later_persists_flag_visible_in_queue` (set, list, clear, list — full IPC round-trip via `handle_request` against a real in-memory daemon)
- [x] Auto-clear reply-later flag when a reply-send completes (`handler/mutations.rs` → `clear_reply_later_for_reply_parent`; daemon RED `dispatch_send_draft_preserves_parent_thread_for_synthetic_sent`)
- [x] Tantivy parser + index field: `is:reply-later` (`FilterKind::ReplyLater`; `search` E2E `e2e_search_reply_later_filter`)

**Client surfaces ✅**
- [x] CLI: `mxr replies` / `mxr replies list` / `mxr replies add <id>` / `mxr replies remove <id>` (`crates/daemon/src/commands/replies.rs`)
- [x] CLI help snapshot updated to include the new subcommand
- [x] CLI: `mxr replies walk` (interactive walker; `commands/replies.rs`)
- [x] TUI: `b` bound to `Action::FlagReplyLater` ("bookmark for reply later"); status message on confirm
- [x] TUI: optimistic `reply_later` row marker (`selection_helpers.rs` overlays `mailbox.reply_later_message_ids`; `mutation_helpers.rs` applies on flag clear/set)
- [x] TUI: reply queue modal walk — reply advances via normal compose/send flow (`ReplyQueueModalReply` → compose)

**Automated acceptance / gaps**
- [ ] `set_reply_later_flag_persists_across_daemon_restart` (integration via `cli_journey` when flake fixed)
- [x] Clear-on-send IPC path covered (`dispatch_send_draft_preserves_parent_thread_for_synthetic_sent` in daemon `handler/mod.rs`)
- [ ] `dismissing_flag_does_not_send_reply`
- [x] `is_reply_later_search_returns_only_flagged_messages` (`crates/search` `e2e_search_reply_later_filter`)
- [ ] `walk_mode_advances_after_send` / `walk_mode_advances_after_skip` (CLI `mxr replies walk`; automated RED deferred)

### 2.2 Custom-time snooze

- [x] New `crates/core/src/time_parse.rs` with `parse_relative_time(input, now) -> Result<DateTime, TimeParseError>`
- [x] Forms accepted: `in N{m|h|d|w}`, `tomorrow [time]`, `today <time>`, `<weekday> [time]`, RFC3339
- [x] Time formats: 12h (`9am`, `5pm`, `12am=00:00`, `12pm=12:00`), 24h (`17:00`)
- [x] Wired into `mxr snooze --until ...` (config presets first, then conversational fallback)
- [x] CLI help text updated to advertise the richer forms
- [x] RED+GREEN: 23 boundary tests covering minutes/hours/days/weeks, named weekdays (full + 3-letter), tomorrow/today, RFC3339, in-past rejection, garbage rejection, empty-string rejection, case-insensitive, whitespace tolerance, 12am/12pm semantics, invalid 24h hour, zero/negative durations
- [x] TUI: snooze modal "Custom..." entry (`snooze_modal.rs` + `parse_relative_time`)

### 2.3 Auto-reminders ("nudge if no reply")

**Store layer ✅**
- [x] Migration `014_auto_reminders.sql` (table + partial pending-only index)
- [x] Wired into `pool.rs` MIGRATIONS array (version 14)
- [x] `crates/store/src/auto_reminders.rs` with `set_auto_reminder`, `cancel_auto_reminder`, `mark_auto_reminder_triggered`, `get_due_auto_reminders`, `get_auto_reminder`
- [x] `cargo sqlx prepare` regenerated `.sqlx` cache
- [x] RED+GREEN: `set_auto_reminder_persists_and_round_trips`
- [x] RED+GREEN: `re_setting_clears_triggered_and_cancelled_state`
- [x] RED+GREEN: `get_due_excludes_future_reminders`
- [x] RED+GREEN: `get_due_includes_past_pending_reminders`
- [x] RED+GREEN: `get_due_excludes_triggered_reminders`
- [x] RED+GREEN: `get_due_excludes_cancelled_reminders`
- [x] RED+GREEN: `get_due_orders_by_remind_at_ascending`

**Daemon layer ✅**
- [x] IPC types: `Request::SetAutoReminder`, `Request::CancelAutoReminder`, `DaemonEvent::ReminderTriggered`
- [x] Handler `crates/daemon/src/handler/reply_later.rs` extended with reminder methods
- [x] Background loop `auto_reminders_loop` + extracted testable `process_due_reminders(state, now) -> Result<u32>` function
- [x] Wired into `server.rs` startup; loop handle registered on `RuntimeTasks::auto_reminders_loop`
- [x] RED+GREEN: `dispatch_set_auto_reminder_persists_and_loop_fires_when_due` (full IPC + loop + event round-trip)
- [x] RED+GREEN: `dispatch_cancel_auto_reminder_prevents_firing`

**CLI surface ✅**
- [x] `mxr remind <message-id> --when "in 5d"` (uses time_parse from 2.2)
- [x] `mxr remind <message-id> --cancel`
- [x] CLI help snapshots updated and passing

**Still TBD**
- [ ] TUI surface for setting/cancelling reminders (CLI remains canonical)
- [ ] Reminder integration with reply-later queue UI (“nudge follows” surfacing beyond daemon events)

**Cancelled-on-reply**
- [x] Auto-cancel pending reminder when inbound reply_pairs links to parent (`reply_pairs.rs` → `cancel_auto_reminder_for_parent_id`; store RED `inbound_reply_pair_cancels_pending_auto_reminder_on_sent_parent`)

### 2.4 Send Later

**Store layer ✅**
- [x] Migration `015_scheduled_sends.sql` — adds `send_at` column to drafts + partial pending-only index
- [x] `crates/store/src/scheduled_sends.rs` with `schedule_send`, `cancel_scheduled_send`, `get_scheduled_send`, `get_due_scheduled_drafts`
- [x] RED+GREEN: `schedule_send_persists_and_round_trips`
- [x] RED+GREEN: `cancel_scheduled_send_clears_send_at`
- [x] RED+GREEN: `get_due_excludes_future_scheduled_drafts`
- [x] RED+GREEN: `get_due_includes_past_scheduled_drafts`
- [x] RED+GREEN: `get_due_excludes_already_sending_drafts`
- [x] RED+GREEN: `get_due_orders_by_send_at_ascending`

**Daemon layer ✅**
- [x] IPC types: `Request::ScheduleSend { draft_id, send_at }`, `Request::CancelScheduledSend`
- [x] Handlers `mutations::schedule_send` / `mutations::cancel_scheduled_send`
- [x] Background flusher `scheduled_sends_loop` + extracted `process_due_scheduled_sends(state, now) -> Result<u32>`
- [x] Reuses `send_stored_draft` for the actual send (re-exported as `pub(crate)`)
- [x] Idempotent: clears `send_at` before invoking send so a crashed prior attempt won't re-fire
- [x] Wired into server.rs startup; loop handle on RuntimeTasks
- [x] RED+GREEN: `dispatch_schedule_send_persists_and_loop_flushes_when_due`
- [x] RED+GREEN: `dispatch_cancel_scheduled_send_prevents_flush`

**CLI surface ✅**
- [x] `mxr send <draft-id> --at "in 1h"` schedules instead of sending
- [x] `mxr unsend <draft-id>` cancels a scheduled send
- [x] CLI help snapshots updated and passing

**Still TBD**
- [ ] Cross-restart integration test (existing flake on `cli_journey_*` blocks adding to that suite)

**TUI ✅**
- [x] Compose send-confirm `Send at:` prompt parses `mxr`-style relative time and calls `ScheduleSend` (`send_confirm_modal.rs`; `parse_relative_time`)

### 2.5 Screener (consent-based first-touch)

**Shipped (store + daemon + CLI + TUI)** — plan migration number drifted.

- [x] Migration `018_screener_decisions.sql` (not `016`; sequence picked up after snippets)
- [x] IPC: `ListScreenerQueue`, `SetScreenerDecision`, `ClearScreenerDecision` (plan text said `SetSenderPolicy`)
- [x] Sync-time ingest: `sync::engine::apply_screener_decision` evaluates decisions as messages land (`Unknown` enqueue / deny routes / feed+papertrail labels)
- [x] CLI: `mxr screener …` (`commands/screener.rs`)
- [x] TUI: screener modal / disposition flow (`screener_modal.rs`)
- [ ] RED suite breadth from delight plan (spot tests exist under `provider-gmail`/daemon; fuller matrix still optional)

### 2.6 Bulk sender triage + unsubscribe

**Shipped**
- [x] IPC: `Request::ListSenders` + `commands/senders.rs` (`mxr senders --top N`)
- [x] IPC + CLI: `Request::Unsubscribe` by message id + batch CLI (`commands/mutations/mod.rs::unsubscribe`)
- [ ] CLI: positional **email-address** shorthand for unsubscribe (today: message ids and `--search`)
- [x] Daemon helper `crates/daemon/src/unsubscribe.rs` (RFC 8058 one-click POST; mailto UX)

**Deferred RED / QA**
- [ ] RED: `senders_top_volume_returns_correct_order`
- [ ] RED: `senders_filtered_by_since_excludes_older`
- [x] RED: `unsubscribe_one_click_posts_correct_body` (`unsubscribe::tests::one_click_posts_rfc_8058_form_body` / wiremock)
- [ ] RED: `unsubscribe_mailto_creates_outbound_draft`
- [ ] RED: `unsubscribe_failed_request_does_not_label_sender`
- [ ] RED: `unsubscribe_idempotent`

## Phase 2 acceptance

- [ ] Press `r` on 5 messages; `mxr replies walk` walks them
- [ ] `mxr snooze --until "tomorrow 9am" <id>` snoozes correctly
- [ ] Send a reply with `--remind-after 1m`; wait 70s; reminder appears in queue
- [ ] `mxr send --at "+30s"` queues; 60s later, sent
- [ ] Sync delivers a message from a new sender; appears in `mxr screener list`
- [ ] `mxr unsubscribe <addr>` actually fires List-Unsubscribe and labels the sender
