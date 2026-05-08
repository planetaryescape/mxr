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
- [ ] Auto-clear on reply-send (hook in send pipeline)
- [ ] Tantivy parser: `is:reply-later` operator

**Client surfaces — CLI ✅, TUI ⏳**
- [x] CLI: `mxr replies` / `mxr replies list` / `mxr replies add <id>` / `mxr replies remove <id>` (`crates/daemon/src/commands/replies.rs`)
- [x] CLI help snapshot updated to include the new subcommand
- [ ] CLI: `mxr replies walk` (interactive walker)
- [x] TUI: `b` bound to `Action::FlagReplyLater` ("bookmark for reply later"); status message on confirm
- [ ] TUI: optimistic visual indicator on flagged rows (currently only a status message)
- [ ] TUI: walk mode reusing compose flow

**Acceptance ⏳**
- [ ] `set_reply_later_flag_persists_across_daemon_restart` (integration via cli_journey when daemon-flake fixed)
- [ ] `replying_to_flagged_message_clears_flag`
- [ ] `dismissing_flag_does_not_send_reply`
- [ ] `is_reply_later_search_returns_only_flagged_messages` (Tantivy)
- [ ] `walk_mode_advances_after_send`
- [ ] `walk_mode_advances_after_skip`

### 2.2 Custom-time snooze

- [x] New `crates/core/src/time_parse.rs` with `parse_relative_time(input, now) -> Result<DateTime, TimeParseError>`
- [x] Forms accepted: `in N{m|h|d|w}`, `tomorrow [time]`, `today <time>`, `<weekday> [time]`, RFC3339
- [x] Time formats: 12h (`9am`, `5pm`, `12am=00:00`, `12pm=12:00`), 24h (`17:00`)
- [x] Wired into `mxr snooze --until ...` (config presets first, then conversational fallback)
- [x] CLI help text updated to advertise the richer forms
- [x] RED+GREEN: 23 boundary tests covering minutes/hours/days/weeks, named weekdays (full + 3-letter), tomorrow/today, RFC3339, in-past rejection, garbage rejection, empty-string rejection, case-insensitive, whitespace tolerance, 12am/12pm semantics, invalid 24h hour, zero/negative durations
- [ ] TUI: snooze modal "Custom..." entry calling the parser

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
- [ ] Auto-cancellation when a reply arrives (sync hook → cancel reminder for the parent's sent_message_id)
- [ ] TUI surface for setting/cancelling reminders (currently CLI-only)
- [ ] Reminder integration with reply-later queue UI (so triggered reminders surface as "reply later" follow-ups)

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
- [ ] TUI: schedule prompt in compose-confirm (currently CLI-only)
- [ ] Cross-restart integration test (existing flake on `cli_journey_*` blocks adding to that suite)

### 2.5 Screener (consent-based first-touch)

- [ ] Migration `016_screener_decisions.sql`
- [ ] IPC: `ListScreenerQueue`, `SetSenderPolicy`
- [ ] Sync hook to classify unknown senders
- [ ] CLI: `mxr screener list|allow|deny|feed|paper-trail`
- [ ] TUI: Screener screen + 3-key dispositions
- [ ] RED: `unknown_sender_routes_to_screener_queue`
- [ ] RED: `allow_sender_bypasses_queue`
- [ ] RED: `deny_sender_trashes_subsequent`
- [ ] RED: `feed_sender_routes_to_feed`
- [ ] RED: `screener_decision_persists_across_restart`
- [ ] RED: `disposition_change_applies_to_future_only`
- [ ] RED: `bulk_disposition_updates_all_pending`

### 2.6 Bulk sender triage + unsubscribe

- [ ] IPC: `ListSenders`, `Unsubscribe`
- [ ] Handler `senders.rs`, `unsubscribe.rs`
- [ ] CLI: `mxr senders --top N`, `mxr unsubscribe <addr>`
- [ ] RED: `senders_top_volume_returns_correct_order`
- [ ] RED: `senders_filtered_by_since_excludes_older`
- [ ] RED: `unsubscribe_one_click_posts_correct_body`
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
