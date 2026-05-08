# Phase 1 — Make it feel right

> Goal: every keystroke responds in <50ms. The TUI feels like Linear/Superhuman, not like a database admin tool.

See [01-delight-plan.md §Phase 1](./01-delight-plan.md#phase-1--make-it-feel-right) for full specs.

## Tracker

### 1.1 Optimistic mutation rollback

- [x] Create `crates/tui/src/app/mutation_snapshot.rs` (bounded ring buffer of pre-state deltas)
- [x] RED: `apply_optimistic_star_updates_state_before_response` (baseline; existing single-item path)
- [x] GREEN: optimistic apply path in `mutation_helpers.rs`
- [x] RED: `failed_reconciliation_rolls_back_to_pre_state`
- [x] GREEN: rollback on failure event (snapshot replay in `handle_mutation_reconciliation_failed`)
- [x] Wire IPC: queue carries `MutationId`, `AsyncResult::MutationResult { id, outcome }` threads it through
- [x] RED: `snapshot_buffer_evicts_oldest_when_full`
- [x] GREEN: bounded eviction (FIFO at `MUTATION_SNAPSHOT_CAPACITY = 64`)
- [x] RED: `concurrent_mutations_on_same_message_compose_under_out_of_order_success`
- [x] GREEN: composition holds for orthogonal effects (Star + ApplyLabel)
- [x] RED: `concurrent_mutations_on_same_message_partial_failure`
- [x] GREEN: per-mutation isolation (snapshot keyed by id, replays only the affected fields)
- [x] RED: `bulk_star_reverts_all_messages_when_reconciliation_fails`
- [x] GREEN: bulk-confirm path captures snapshot before applying optimistic effect
- [x] `Action::ApplyLabel` now applies optimistically (was `None`)
- [ ] Add `MutationReconciliationFailed` daemon-side event for proactive rollback (currently triggered from local IPC error path)
- [ ] `Action::MoveToLabel` still passes `None` for optimistic_effect — track in follow-up RED test
- [ ] Manual: star/archive/label feel <50ms perceptually in real TUI

### 1.2 Cmd+K command palette as primary discovery

- [ ] Audit existing `keybindings.rs`; ensure every action exposed as `PaletteCommand`
- [x] RED: `palette_opens_from_any_screen`
- [x] GREEN: open binding works from Mailbox/Search/Rules/Diagnostics/Accounts/Analytics
- [x] RED: `prefix_match_ranks_above_substring_match`
- [x] GREEN: ranking pass — score tiers (exact > prefix > word-prefix > substring > shortcut/category)
- [ ] RED: `every_keybinding_in_registry_is_searchable_in_palette`
- [ ] GREEN: registry-driven population
- [x] RED: `recently_used_commands_surface_to_top_with_empty_query`
- [x] RED: `most_recent_command_ranks_first_when_multiple_are_recent`
- [x] GREEN: in-memory recent-commands list (cap 8, FIFO eviction, deduped)
- [ ] Persist `recent_actions` across sessions via `local_state.rs`
- [x] RED: `confirm_returns_selected_command_action`
- [x] RED: `empty_query_lists_all_commands_in_registration_order`
- [x] GREEN: confirm path closes palette and yields the action
- [ ] Slim `hint_bar.rs` to top-5 contextual bindings

### 1.3 Richer inbox rows

- [x] RED: `sender_uses_display_name_when_present`, `sender_falls_back_to_email_when_display_name_absent`, `sender_falls_back_to_email_when_display_name_empty`
- [x] GREEN: `format_sender(address, max_width)` — display-name → email fallback with ellipsis truncation
- [x] RED: `sender_truncates_long_display_name_with_ellipsis`, `sender_passes_through_text_at_or_below_max_width`
- [x] GREEN: integrated into `sender_parts` (reserves 18ch when thread badge present, 22ch otherwise)
- [x] RED: `subject_line_includes_snippet_when_room_available`, `subject_line_omits_snippet_when_row_too_narrow`, `subject_line_truncates_long_snippet_with_ellipsis`, `subject_line_truncates_subject_when_subject_alone_overflows`, `subject_line_omits_snippet_when_snippet_is_blank`
- [x] GREEN: `format_subject_line(subject, snippet, max_width)` returning `(subject, Option<snippet>)`
- [ ] Wire `format_subject_line` into `build_row` rendering (subject Cell becomes Line with two Spans)
- [ ] RED: `row_shows_thread_participation_chip_only_when_multi_message` (requires participants count on `MailListRow`)
- [ ] GREEN: participation chip
- [x] RED: relative-time ladder boundary tests (10 cases)
- [x] GREEN: `format_date_relative` (now/Xm/Xh/weekday/Mon D/MM/DD/YY ladder; auto-applied via `format_date` delegation)
- [x] RED: `attachment_chip_*` (6 boundary cases: bytes/KiB/MiB thresholds)
- [x] GREEN: `format_attachment_chip(has_attachments, size_bytes)` with B/K/M ladder
- [ ] Wire `format_attachment_chip` into row (requires column-width adjustment from 2ch to ~7ch)

### 1.4 Type-ahead search

- [x] Existing `pending_debounce` field + `process_pending_search_debounce` already wired through `tick()`. SEARCH_DEBOUNCE_DELAY = 250ms (vs. plan's 120ms — adopt as default).
- [x] RED: `pending_debounce_does_not_fire_before_due_time`
- [x] GREEN: existing debounce check in `process_pending_search_debounce`
- [x] RED: `expired_debounce_fires_pending_search_on_tick`
- [x] GREEN: existing flush logic
- [x] RED: `new_debounce_replaces_an_unfired_one`
- [x] GREEN: `schedule_search_page_search` overwrites prior `pending_debounce`
- [ ] RED: `result_list_renders_first_batch_before_query_completes` (requires streaming IPC; defer)
- [ ] RED: `empty_query_clears_results` (existing branch in `schedule_search_page_search`; add regression test)

### 1.5 Saved searches as top tabs

- [x] Action: `Action::OpenSavedSearchByIndex(usize)` (1-indexed; 0 = clear filter)
- [x] Handler: index → `SelectSavedSearch` lookup; 0 → `ClearFilter`; out-of-range → no-op
- [x] RED+GREEN: `open_saved_search_by_index_1_targets_first_saved_search`
- [x] RED+GREEN: `open_saved_search_by_index_2_targets_second_saved_search`
- [x] RED+GREEN: `open_saved_search_by_index_zero_clears_active_filter`
- [x] RED+GREEN: `open_saved_search_by_out_of_range_index_is_noop`
- [x] RED+GREEN: `open_saved_search_with_empty_registry_is_noop`
- [x] Input: `g 0..9` chord bound to the action (vim-style; avoids conflict with existing `1`-`6` screen tabs)
- [x] RED+GREEN: `chord_g_then_digit_one_through_nine_jumps_to_saved_search`
- [x] RED+GREEN: `chord_g_then_zero_returns_to_default_inbox`
- [x] RED+GREEN: `bare_digit_still_opens_screen_tab_not_saved_search`
- [ ] Create `crates/tui/src/ui/tab_strip.rs` (visual tab bar)
- [ ] Modify `ui/mod.rs` layout to include strip above inbox
- [ ] RED: `tab_strip_renders_first_nine_saved_searches`
- [ ] RED: `tab_unread_count_reflects_search_match_count`

## Phase 1 acceptance

- [ ] Star/archive/label feel <50ms perceptually (manual TUI test)
- [ ] Cmd+K palette finds every action by fuzzy match
- [ ] Type "from:alice" in search bar; results stream in without pressing Enter
- [ ] Press `1`–`9`; jumps to corresponding saved-search tab
- [ ] Inbox row shows snippet, attachment chip, thread participation
