# TUI

Paths are relative to the repository root.

### An "Always for this sender?" prompt ends at the next key that is not its answer
**Chosen**: the prompt stays armed only until a key other than its answer arrives, then clears.
**Rejected**: keeping it armed across other keys, so a later `K` could move a sender by surprise.
**Where**: crates/tui/src/app/actions.rs, `App::apply`.

### Opening Analytics cancels the pending mailbox auto-mark-read timer
**Chosen**: `mailbox.pending_preview_read` is cleared when Analytics opens. Opening remote mail clears it as well, so no implicit flag write is queued.
**Rejected**: letting the preview timer fire later. It queued a SetRead nobody asked for and showed a "Mutation Failed" modal when the daemon pool was busy.
**Where**: crates/tui/src/app/analytics_actions.rs (screen entry), crates/tui/src/app/mailbox_helpers.rs (open remote mail).

### Analytics drill-downs jump to a search instead of opening the envelope
**Chosen**: sender, counterparty and heaviest-message drill-downs call `jump_to_search("from:...")`. Tab cycling refetches only when the destination view has no data or a stale cache; `r` and filter changes always refresh.
**Rejected**: opening the envelope directly, which left the mailbox list out of sync with the preview pane; refetching on every tab change, which is not instant.
**Where**: crates/tui/src/app/analytics_actions.rs, `handle_analytics_drill_down` and the tab-cycle handler.

### Analytics unsubscribe reuses the mailbox unsubscribe-confirm path
**Chosen**: `u` on a Subscriptions row fills the existing confirm modal, so `ConfirmUnsubscribeOnly` and `ConfirmUnsubscribeAndArchiveSender` and their IPC are shared. Drill-down targets stay read-only navigations; mutations have their own keys.
**Where**: crates/tui/src/app/analytics_actions.rs, `handle_analytics_unsubscribe`.

### The Wrapped window request mirrors the CLI's window logic
**Chosen**: `wrapped_window_to_request` copies `crates/daemon/src/commands/wrapped.rs` so the TUI request and label match the CLI's exactly.
**Where**: crates/tui/src/app/analytics_actions.rs, `wrapped_window_to_request`.

### Analytics views show shares and headlines, not duplicate bar charts
**Chosen**: `Share` and `Open %` columns replace top-by-volume bars; the Reply view puts percentile callouts in its title; an empty tile renders a stat-shape that mirrors the populated one; histograms appear only for real distributions (decay buckets). The business-hours line is omitted until the Slice-14 reconciler fills it.
**Rejected**: a bar of `total_inbound` beside `total_inbound` (duplication, and a bar scaled to its own max is always full); an age histogram under a filter that collapses every row into one bucket; a "(business-hours pending)" filler; a 0% "Top mime share" gauge with no label, which reads as broken.
**Where**: crates/tui/src/ui/analytics_page.rs, the per-view draw functions (`draw_table`, Storage, Contacts, Subscriptions, Reply, Decay).

### Analytics cold load replaces the pane; a refresh keeps painting cached data
**Chosen**: only "no cached data and a request in flight" shows "Computing analytics..."; a refresh with cached data keeps painting it; an error replaces the table.
**Where**: crates/tui/src/ui/analytics_page.rs, the dispatching `draw`.

### Lazy thread summaries are debounced and skip short or automated threads
**Chosen**: a missing cached summary is requested only after the cursor rests (about 250 ms). `AUTO_SUMMARY_MIN_WORDS` is 200 (about a one-minute read) and `AUTO_SENDER_PATTERNS` is a conservative list; `y` forces a summary. When `body_word_count` is 0 (rows synced before that column existed) the cached body is counted instead.
**Rejected**: a request per row passed while holding the down arrow; summarising sub-minute reads; gating out every old thread on a zero word count.
**Where**: crates/tui/src/app/mailbox_helpers.rs (`auto_summary_eligible`, thread-open handler), crates/tui/src/runner.rs (summary debounce drain).

### Archive hides the row optimistically only when viewing INBOX
**Chosen**: `removes_from_view = active_label_matches("INBOX")`.
**Rejected**: always removing the row. In Starred, All Mail or custom labels the message still belongs in the view, so the row would flicker until the next sync restored it.
**Where**: crates/tui/src/app/mutation_actions.rs, archive handling.

### Mutations act on `selected_set` only in the flat list and search results
**Chosen**: `pattern_select_applies()` gates pattern selection. Visual-mode anchors are row indices into the thread-collapsed list; list mutations and rollbacks drop the anchor and keep `selected_set`, which is keyed by `MessageId`.
**Rejected**: acting on a selection elsewhere, where the count shown would be ignored by the next action; indexing the anchor into the flat envelope list, which silently mis-targets messages.
**Where**: crates/tui/src/app/mutation_actions.rs (pattern select), crates/tui/src/app/mutation_helpers.rs (visual-mode reset, selection by rows).

### Modals that need an account take it from a visible envelope
**Chosen**: read `account_id` from any visible envelope.
**Rejected**: a "default account" surface; the CLI is the tool for cross-account sweeps.
**Where**: crates/tui/src/app/mailbox_actions.rs, the handler that reads `account_id` from visible envelopes.

### Reply-context prewarm runs on its own daemon connection
**Chosen**: the prewarm opens a short-lived socket. The daemon's `PrepareReply` is a cheap memo hit on the second call for the same message, so the user's `r`/`a` wins.
**Rejected**: the shared IPC worker, which handles one request at a time and would queue the prewarm ahead of the keypress.
**Where**: crates/tui/src/runner.rs (prewarm block in the main loop), crates/tui/src/compose_flow.rs (`fetch_reply_context`).

### Saved-search edits queue [Delete, Create] and refresh on the next dispatcher tick
**Chosen**: queued requests dispatch in order; the sidebar refresh runs on the next tick, not inline, so pipelined edits show the final state; a refresh already in flight clears its flag so the response handler can set it again for the next mutation.
**Where**: crates/tui/src/runner.rs, the saved-search drain and refresh blocks.

### Daemon responses for a view the user left are dropped, and optimistic state masks refreshes
**Chosen**: analytics, sender-profile and label-envelope responses are dropped when the view or sender is no longer shown. A refresh response cannot undo an optimistic remove or flag change before its mutation is acked. A selection made in the old list during a fetch does not follow into the new list.
**Rejected**: applying late responses (the table flashes the previous view's data).
**Where**: crates/tui/src/runner.rs, analytics response arm, label-envelope result handling and `MutationResult` handling; crates/tui/src/app/search_helpers.rs.

### Held keys and key-up events never act twice
**Chosen**: a prompt ignores answers shortly after it appears, because most terminals send a held key as repeated presses; `j` in Focus & reply is rate-limited; only key-down acts (Windows consoles also report key-up); a prompt's answer cannot be the same press that opened it.
**Rejected**: acting on every event, which runs each key twice on Windows and lets a held `y` accept a prompt.
**Where**: crates/tui/src/app/promises.rs, crates/tui/src/app/focus_actions.rs, crates/tui/src/app/input.rs (key-kind filter).

### Retired chords and remaps never fall back to legacy defaults
**Chosen**: an explicit remap or retired default does not reach hardcoded fallbacks; a remapped suffix cannot steal an unrelated default chord (retiring `a` keeps `ga` and `ia`); a second key that completes nothing cancels the prefix and runs nothing; legacy branches that ignore modifiers cannot resurrect a retired destructive shortcut; ambiguous user prefixes are rejected, not chosen by HashMap order.
**Rejected**: replaying unmatched keys into the legacy resolver (a mistyped `g U` could mark mail unread).
**Where**: crates/tui/src/app/input.rs (chord handling), crates/tui/src/keybindings.rs, crates/tui/src/app/key_dispatch.rs.

### The mailbox filter (Ctrl-f) never pages
**Chosen**: it keeps its own limit and ignores the config page size.
**Rejected**: paging by config size, which would hide matches.
**Where**: crates/tui/src/runtime.rs, the mailbox filter request.

### Saved drafts and destructive prompts open on the safe choice, and Esc never destroys
**Chosen**: delete and discard confirms open on Keep; only `y` or Enter on the option that names the discard throws a draft away; Esc goes back to the draft. A stored draft is sent exactly as saved and its review cannot be rewritten by "Draft for me".
**Rejected**: Enter-to-discard defaults; rewriting a stored draft's text, which would move the revision the schedule names.
**Where**: crates/tui/src/app/input.rs (send review and discard handlers), crates/tui/src/ui/send_confirm_modal.rs, crates/tui/src/ui/drafts_modal.rs.

### A failed safety check or a send-From check does not block the send modal
**Chosen**: a daemon error or dropped IPC worker during CheckDraftSafety leaves `safety_report = None` and a visible reason ("Safety check unavailable"); an unexpected successful response is surfaced as a protocol mismatch. Only a genuine IPC failure is non-fatal. The effective From is validated through the daemon's choke point (`ResolveSendFrom`) so an unowned From is rejected at the confirm step.
**Rejected**: silently proceeding on an unexpected response; looking like a clean run when the check never ran.
**Where**: crates/tui/src/compose_flow.rs, the pre-send check and `resolve_send_from` functions.

### The editor-failure path keeps the temp file and shows its path
**Chosen**: on editor failure the file stays and its path is shown, because there is no in-app retry.
**Rejected**: deleting it, which loses the user's edits.
**Where**: crates/tui/src/compose_flow.rs, editor-failure handling.

### Export writes use `create_new`, and a partial file is removed
**Chosen**: without `overwrite_ok` the write uses `create_new`, so a file that appeared after the user's last look fails; a file this call created and only half wrote is removed.
**Rejected**: leaving the partial file, which would make the retry ask to overwrite a file the user never saw.
**Where**: crates/tui/src/export_output.rs, the file-writing function.

### IPC requests have a timeout and a timed-out connection is dropped
**Chosen**: the worker bounds every request, drops the connection after a timeout and reconnects, and leaves other in-flight reads cancellable. A mutation is never discarded, because cancellation cannot show whether it applied.
**Rejected**: no bound (a daemon that accepts and never replies parks the worker); keeping the old connection (a late reply would be attributed to the next request).
**Where**: crates/tui/src/ipc.rs, the request loop; crates/tui/src/client.rs, `map_request_error` (an unexpected frame is an error, not skipped).

### Mail-controlled text is sanitized at the render boundary
**Chosen**: C0/DEL/C1 controls and bidi-override characters are stripped from senders, subjects and bodies just before they become spans; `\n` and `\t` stay.
**Where**: crates/tui/src/ui/sanitize.rs, `sanitize_for_display`; callers in message_view.rs, mail_list.rs, desk_lens.rs, focus_lens.rs, bulk_preview.rs.

### Mail-list columns give way in a fixed order
**Chosen**: in a narrow list the sender column shrinks first; the subject is allocated before the snippet; the sender column reserves room for the thread-count badge; a snippet needs at least four characters.
**Rejected**: a full-width sender column, which left the subject four characters.
**Where**: crates/tui/src/ui/mail_list.rs, the row layout and `format_subject_line`.

### Reading and Paper trail counts are this week's mail, not unread counts
**Chosen**: those lens counts show this week's mail and say so.
**Where**: crates/tui/src/ui/desk_lens.rs, count labels.

### Time handling in the TUI uses local time and a fixed clock in tests
**Chosen**: the open email shows a local arrival time, never UTC and never a day alone; subscription days are stored at midday UTC so UTC reads the day meant.
**Where**: crates/tui/src/ui/received_label.rs, crates/tui/src/ui/subscriptions_lens.rs.

### Messages: the selection is a person, and "done here" waits for the next page
**Chosen**: the Messages selection follows a person, not a row position, so a refetch that reorders the bands keeps it on them and a refetch that drops them moves it to a neighbour. The page asked for last is the only one shown; `e` waits until the page it moved to is on screen, so it cannot act on the page left behind. A Got it counts down only while the lens still shows its conversation, and opening the filter picker cancels it. A second `e` before the next page arrives cannot send the same thread twice.
**Rejected**: acting on a late answer for an earlier topic; sending a Got it after the user left its conversation.
**Where**: crates/tui/src/app/messages_actions.rs (`messages_done`, `show_messages_ack`, `messages_ack_still_wanted`, `guard_messages_ack`, `reselect_messages_row`); crates/tui/src/app/state/messages.rs.

### On the desk and in a place, verbs act on the row under the cursor
**Chosen**: on the desk and in a place, a verb acts on that row's account and whole thread (a place row is one message). Archive on the desk is Done. In Updates and Reading `e` is done in that mode, and the provider archive happens only when no other mode holds the thread. Verbs that need more than a row ask to open it first.
**Rejected**: acting on whatever the mailbox or reader showed last, which would mutate unrelated mail.
**Where**: crates/tui/src/app/mutation_actions.rs; crates/tui/src/app/mailbox_helpers.rs (`done_on_desk_row`, `selected_desk_row_message_ids`).

### Star toggles by what the row shows
**Chosen**: one row toggles what it shows; a multi-selection always stars. A desk row toggles its thread. A conversation row shows its newest message's star, so a starred row unstars every listed message.
**Where**: crates/tui/src/app/mutation_actions.rs, the star handler.

### Refresh masking, label switches and optimistic state
**Chosen**: label and reply-later overrides need no refresh masking, because the daemon writes them before it responds and the rollback path covers failure. Flag and removal overrides are masked until the ack. `active_label` changes only on a user-initiated switch (`pending_active_label` set); refresh-only fetches keep the current label. After an ack the optimistic change is authoritative on both sides, so masking stops for that mutation.
**Rejected**: masking every refresh response.
**Where**: crates/tui/src/app/pending_optimistic.rs; crates/tui/src/runner.rs, label envelope and mutation-result handling.

### The connection watchdog times the whole outage and refreshes by refetch
**Chosen**: the IPC worker re-emits `Reconnecting` with a fresh `since` on every failed retry; the TUI keeps the first so the watchdog times the whole outage, and `Connected` clears it. Five seconds before the error modal matches `START_DAEMON_TIMEOUT`. No replay exists for events missed while disconnected, so the authoritative views are refetched without replacing private compose ownership. A background retry never takes the keyboard; a sync the user asked for gets one status line naming the account. Errors never stack modals; a second error goes to the log.
**Rejected**: restarting the outage clock on each retry (the modal would never open during a long outage); replaying missed events (none exist).
**Where**: crates/tui/src/app/mod.rs (`set_connection_state`, `report_error`, connection constants); crates/tui/src/daemon_events.rs.

### The IPC client has no timeout of its own; the worker owns timeout and reconnect
**Chosen**: `client.rs` stays a plain awaited exchange. The worker bounds every request (`IPC_REQUEST_TIMEOUT`), drops the connection after a timeout and reconnects. Initial connect retries forever, reports state transitions and answers queued requests with an error so the mutation queue does not stall. An unexpected frame id is an error, not skipped. Cancelled filter and person reads never block the current selection, but a mutation is never discarded, because cancellation cannot show whether it applied. Slow LLM work uses dedicated connections, so the bound can be generous.
**Rejected**: a timeout in the client (two owners of reconnect policy); skipping rogue frames (the worker would park); exiting the worker on a failed first connect (the UI hung at "connecting").
**Where**: crates/tui/src/client.rs; crates/tui/src/ipc.rs (worker loop, `IPC_REQUEST_TIMEOUT`, `ipc_call_dedicated`).

### Sync requests coalesce until the daemon answers, and background syncs are acked early
**Chosen**: a second SyncAll click coalesces only while the first has not been answered; the daemon acks as soon as the sync starts, so a backfill longer than the client's 60-second IPC timeout does not time out the request. Progress arrives as `OperationProgress` events and the list refreshes on `SyncCompleted`. Events that race the acknowledgement are deferred until the daemon names the accounts of this request.
**Where**: crates/tui/src/app/screen_actions.rs; crates/tui/src/app/mod.rs (batch accounts and deferred events).

### The status bar always says how fresh the store is
**Chosen**: local mail is served from the store regardless of sync state, so an in-flight delta or a failing provider never reads as "wait for me"; the bar shows when the newest mail arrived and the worst account's sync state, from the most recent success across accounts. A degraded status snapshot carries no reading, so the bar shows no count instead of a wrong one.
**Where**: crates/tui/src/app/body_helpers.rs, status words; crates/tui/src/ui/status_bar.rs.

### Compose: From, drafts and the send review
**Chosen**: From is seeded from the daemon-computed default (the delivered-to owned address), not the account primary, for parity with the CLI. An RSVP with a comment sends From the same owned alias it answers as. An HTML draft bails before the account round trip because it has no compose-file form. A stored draft is sent exactly as saved. Send is suppressed while the verdict is Blocked; `O` is the advertised override and Ctrl-O stays as a silent alias; the override consumes the single-use token minted by the preceding `CheckDraftSafety`. The safety check passes `now` so timing checks fire for immediate sends. Reopening a retained send is an explicit review even without edits. A reply left unsent on a Messages row is continued, not started again.
**Rejected**: the account primary as default From; a "default From" the daemon would reject at send time.
**Where**: crates/tui/src/compose_flow.rs; crates/tui/src/app/input.rs (send review handlers); crates/tui/src/app/compose_actions.rs.

### Key routing order and single-key shortcuts
**Chosen**: `q` closes the innermost layer and quits only at the root; Ctrl-z suspends from anywhere, text fields included. Remaps count only in mail panes, after every modal and text owner; the second key of a chord completes it before any single-key binding. A saved-search delete confirm is checked before the form; the save-attachment modal before the attachment panel; the Deliveries screen passes unclaimed keys to the shared handler so the user is never trapped. A one-key answer to a question cannot take the suffix of a global chord (`g m`). With single-key shortcuts off (WCAG 2.1.4), a letter typed on its own does nothing outside a text field. In the reader `j`/`k` scroll and `J`/`K` move between messages (message focus used to be on `j`/`k`, which left a long body unscrollable). `g` plus a capital opens a tool; `g C` opens Accounts.
**Where**: crates/tui/src/app/input.rs; crates/tui/src/app/key_dispatch.rs; crates/tui/src/app/now_actions.rs; crates/tui/src/keybindings.rs.

### Teaching state lives in the daemon; TUI view state stays local
**Chosen**: dismissed hints are daemon state so every client shares them; TUI-owned view state, such as recent palette commands, stays in `local_state`. Recent palette commands persist by label, because the `Action` enum changes shape across versions. Now is the front page with no tour; each mode teaches itself with a hint at the element, the first time it is needed.
**Where**: crates/tui/src/local_state.rs; crates/tui/src/app/hints.rs; crates/tui/src/runner.rs.

### Theme, palette and key tables are built once
**Chosen**: the theme and the default command palette are built once because eight distinct 256-colour picks and a full walk of the key definition were most of the cost of `App::new` in debug builds. The default key tables are built once because each lookup re-parsed every key string. The palette is rebuilt on each open so rows show the keys that work where it opened, with the person's remaps. Palette order is match score, then recency, then registration order, so a strong match beats a recent weak one.
**Where**: crates/tui/src/theme.rs; crates/tui/src/ui/command_palette.rs; crates/tui/src/keybindings.rs.

### Reader scroll is clamped at render time
**Chosen**: callers advance the offset blind (`j`, Ctrl-d, `G` asks for `u16::MAX`); the renderer clamps to the wrapped height and returns the value for the caller to store.
**Rejected**: trusting the caller's offset; past the end `render_blocks` skipped every block and painted a blank pane.
**Where**: crates/tui/src/ui/message_view.rs, the reader draw function.

### Assistant output is untrusted and remote content never loads
**Chosen**: an image in an assistant reply is only its description; an `mxr:` link shows its words, numbered when the reply's links open by number. Only the latest reply with links numbers them, so a number means one thing. The card takes at most half the pane, and the warning for a change that cannot be undone is drawn outside that room, so a long preview cannot push it off while `y` still approves.
**Where**: crates/tui/src/ui/assistant_describe.rs; crates/tui/src/ui/assistant_pane.rs.

### Send review shows the exact From and sizes to content
**Chosen**: the modal shows the daemon-resolved From, with the account chip first, because a wrong account is the costly mistake; it falls back to the raw `from:` only when the resolve could not run. The popup sizes to wrapped content so the action row is never clipped. Sidebar counts are plain digits because separators cost width the column lacks.
**Where**: crates/tui/src/ui/send_confirm_modal.rs; crates/tui/src/ui/sidebar.rs.

### Visual-mode range and mail-list selection are row based
**Chosen**: `visual_anchor` and the cursor are row indices into the thread-collapsed list; a row range expands to its member message ids, mirroring `mutation_target_ids`. The row-to-envelope source slice must be the list the rows were built from.
**Rejected**: slicing the flat envelope list, which silently mis-targets messages in Threads mode.
**Where**: crates/tui/src/app/mutation_helpers.rs, visual selection update.
