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
- [x] Daemon-side `DaemonEvent::MutationReconciliationFailed`; TUI replays optimistic snapshots when it arrives
- [x] `Action::MoveToLabel` applies optimistically (`MutationEffect::RemoveFromList`, see `mutation_actions.rs`)
- [ ] Manual: star/archive/label feel <50ms perceptually in real TUI

### 1.2 Cmd+p command palette as primary discovery

Note: palette opens with **Ctrl+p** (`keybindings.rs`); macOS Cmd+K is not bound in-tree.

- [x] Mail-list shortcut labels synced from registry (`primary_mail_list_key_display` → `apply_registered_mail_list_shortcuts`); exhaustive binding→palette coverage still guarded by RED below
- [x] RED: `palette_opens_from_any_screen`
- [x] GREEN: open binding works from Mailbox/Search/Rules/Diagnostics/Accounts/Analytics
- [x] RED: `prefix_match_ranks_above_substring_match`
- [x] GREEN: ranking pass — score tiers (exact > prefix > word-prefix > substring > shortcut/category)
- [ ] RED: `every_keybinding_in_registry_is_searchable_in_palette`
- [x] GREEN: shortcut text registry-driven (`command_palette.rs`); command list remains explicit registration order
- [x] RED: `recently_used_commands_surface_to_top_with_empty_query`
- [x] RED: `most_recent_command_ranks_first_when_multiple_are_recent`
- [x] GREEN: in-memory recent-commands list (cap 8, FIFO eviction, deduped)
- [x] Persist recent palette commands via `local_state.rs` (`recent_action_labels` in `tui-state.json`, saved from `local_io.rs`)
- [x] RED: `confirm_returns_selected_command_action`
- [x] RED: `empty_query_lists_all_commands_in_registration_order`
- [x] GREEN: confirm path closes palette and yields the action
- [x] Slim `hint_bar.rs` — `HINT_BAR_MAX_HINTS = 5` (`hint_bar.rs`)

### 1.3 Richer inbox rows

- [x] RED: `sender_uses_display_name_when_present`, `sender_falls_back_to_email_when_display_name_absent`, `sender_falls_back_to_email_when_display_name_empty`
- [x] GREEN: `format_sender(address, max_width)` — display-name → email fallback with ellipsis truncation
- [x] RED: `sender_truncates_long_display_name_with_ellipsis`, `sender_passes_through_text_at_or_below_max_width`
- [x] GREEN: integrated into `sender_parts` (reserves 18ch when thread badge present, 22ch otherwise)
- [x] RED: `subject_line_includes_snippet_when_room_available`, `subject_line_omits_snippet_when_row_too_narrow`, `subject_line_truncates_long_snippet_with_ellipsis`, `subject_line_truncates_subject_when_subject_alone_overflows`, `subject_line_omits_snippet_when_snippet_is_blank`
- [x] GREEN: `format_subject_line(subject, snippet, max_width)` returning `(subject, Option<snippet>)`
- [x] Wire `format_subject_line` into `build_row` (`mail_list.rs`)
- [x] RED: `row_shows_thread_participation_chip_only_when_multi_message` (`mail_list.rs` fixture tests)
- [x] GREEN: `other_participant_count` + `+N` participation chip (`thread_participation_chip`)
- [x] RED: relative-time ladder boundary tests (10 cases)
- [x] GREEN: `format_date_relative` (now/Xm/Xh/weekday/Mon D/MM/DD/YY ladder; auto-applied via `format_date` delegation)
- [x] RED: `attachment_chip_*` (6 boundary cases: bytes/KiB/MiB thresholds)
- [x] GREEN: `format_attachment_chip(has_attachments, size_bytes)` with B/K/M ladder
- [x] Wire `format_attachment_chip` into row (`mail_list.rs`, 8ch attachment column)

### 1.4 Type-ahead search

- [x] Existing `pending_debounce` field + `process_pending_search_debounce` already wired through `tick()`. Debounce delay lives in `SEARCH_DEBOUNCE_DELAY` (`search.rs`; plan target 120ms).
- [x] RED: `pending_debounce_does_not_fire_before_due_time`
- [x] GREEN: existing debounce check in `process_pending_search_debounce`
- [x] RED: `expired_debounce_fires_pending_search_on_tick`
- [x] GREEN: existing flush logic
- [x] RED: `new_debounce_replaces_an_unfired_one`
- [x] GREEN: `schedule_search_page_search` overwrites prior `pending_debounce`
- [ ] RED: `result_list_renders_first_batch_before_query_completes` (search page streams via `search_ipc` / `run_streamed_search_page_initial`; adversarial widget-level RED still open)
- [x] GREEN: streamed search segments on search page (`run_streamed_search_page_initial`)
- [x] GREEN: empty query clears/resets workspace (`schedule_search_page_search` → `reset_search_page_workspace`)
- [ ] RED: `empty_query_clears_results` (behavior shipped; regression test still open)

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
- [x] Visual saved-search strip: `crates/tui/src/ui/saved_search_tabs.rs` + `draw_saved_search_tabs` in `draw.rs` (plan named `tab_strip.rs`)
- [x] Layout includes strip above inbox (`draw.rs`)
- [ ] RED: `tab_strip_renders_first_nine_saved_searches`
- [ ] RED: `tab_unread_count_reflects_search_match_count`

## Phase 1 acceptance

Product checks (mostly manual):

- [ ] Star/archive/label feel <50ms perceptually (still validate on real inbox)
- [x] Ctrl+p palette: fuzzy ranking + persisted recents + registry-synced shortcuts; exhaustive `every_keybinding_in_registry_*` RED test still open (`phase-1-feel §1.2`)
- [x] Live search UX: debounce + cancellation; segmented loading on search page (first visible batch before remainder may still merit a dedicated RED)
- [x] Saved-search chord `g` + `0..9`; visual tab strip above inbox
- [x] Row: snippet, attachment chip, relative date, thread `+N` when applicable
