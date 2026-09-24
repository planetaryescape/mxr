# Plan 002: Invalidate visual-mode anchor when mailbox lists mutate

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat a57a53f5..HEAD -- crates/tui/src/app/mutation_helpers.rs crates/tui/src/runner/tests/mutations_and_bulk.rs`
> On any drift, compare the "Current state" excerpts against the live code;
> on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `a57a53f5`, 2026-07-05

## Why this matters

Visual line mode anchors a selection range by **list index**
(`mailbox.visual_anchor: Option<usize>`). When envelopes are removed from the
lists (a mutation, or a rollback restoring rows and re-sorting), the indices
shift but the anchor is not invalidated. The next cursor move calls
`update_visual_selection()`, which rebuilds `selected_set` from the stale
anchor — silently selecting the wrong range. The user's next bulk action
(archive/trash/etc.) then targets messages they never selected. This is a
data-integrity bug in the mutation path, worse than a crash because it is
silent.

## Current state

All in `crates/tui/src/app/mutation_helpers.rs` (impl block on `App`):

- `apply_removed_message_ids` (lines 586-658): removes ids from
  `mailbox.envelopes`, `mailbox.all_envelopes`, `search.page.results`,
  `mailbox.viewed_thread_messages`, and `mailbox.selected_set`; clamps
  `selected_index` (lines 616-624). **Never touches `visual_anchor` /
  `visual_mode`.**
- `restore_removed_from_lists` (lines 682-717): rollback path; merges
  envelopes back and re-sorts (`sort_unstable_by_key`, line 694), clamps
  indices. **Never touches `visual_anchor` / `visual_mode`.**
- `update_visual_selection` (lines 841-857) — consumer of the stale anchor:

  ```rust
  pub(super) fn update_visual_selection(&mut self) {
      if self.mailbox.visual_mode {
          if let Some(anchor) = self.mailbox.visual_anchor {
              let (cursor, source) = if self.screen == Screen::Search {
                  (self.search.page.selected_index, &self.search.page.results)
              } else {
                  (self.mailbox.selected_index, &self.mailbox.envelopes)
              };
              let start = anchor.min(cursor);
              let end = anchor.max(cursor);
              self.mailbox.selected_set.clear();
              for env in source.iter().skip(start).take(end - start + 1) {
                  self.mailbox.selected_set.insert(env.id.clone());
              }
          }
      }
  }
  ```

- `clear_selection` (lines 797-801) shows the invalidation idiom already used
  elsewhere:

  ```rust
  pub(super) fn clear_selection(&mut self) {
      self.mailbox.selected_set.clear();
      self.mailbox.visual_mode = false;
      self.mailbox.visual_anchor = None;
  }
  ```

- Anchor lifecycle: set in `crates/tui/src/app/mutation_actions.rs:532-562`
  (entering/toggling visual mode); state lives in
  `crates/tui/src/app/state/mailbox.rs:299`.

Design note: do NOT call `clear_selection()` from the two mutation helpers —
`selected_set` is keyed by `MessageId` and stays valid across list changes;
only the index-based anchor goes stale. Exit visual mode and drop the anchor,
keep `selected_set` as-is (`apply_removed_message_ids` already retains it
correctly at lines 612-614).

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `scripts/cargo-test -p mxr-tui --tests` | all pass, exit 0 |
| Build | `cargo build -p mxr` | exit 0 |

## Scope

**In scope**:
- `crates/tui/src/app/mutation_helpers.rs`
- `crates/tui/src/runner/tests/mutations_and_bulk.rs` (add tests)

**Out of scope**:
- `mutation_actions.rs` visual-mode entry logic — works correctly.
- Re-anchoring by MessageId (tracking the anchor as an id instead of an
  index) — a larger design change; explicitly deferred.

## Git workflow

- Branch: `advisor/002-visual-anchor-invalidation`
- Conventional commit, e.g. `fix: drop stale visual anchor when mail lists mutate`
- No AI attribution lines. Do not push/PR unless instructed.

## Steps

### Step 1: Invalidate the anchor in both mutation helpers

At the top of the mutating section of `apply_removed_message_ids` (after the
early return for empty `ids`, before/after the `retain` calls — end of
function is also fine) and at the end of `restore_removed_from_lists`, add:

```rust
self.mailbox.visual_mode = false;
self.mailbox.visual_anchor = None;
```

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → existing tests pass
(if an existing test asserts visual mode survives a mutation, STOP — see STOP
conditions).

### Step 2: Add regression tests

In `crates/tui/src/runner/tests/mutations_and_bulk.rs`, following the file's
existing test style (construct app state via the helpers already used there):

1. Enter visual mode over a known range, apply
   `apply_removed_message_ids` removing rows **before** the anchor, move the
   cursor, and assert `selected_set` does not contain messages outside the
   visually intended range — concretely: assert `visual_mode` is false and
   `visual_anchor` is `None` after the removal.
2. Same assertion after a rollback via the code path that calls
   `restore_removed_from_lists` (grep its caller — the optimistic-undo path in
   `mutation_helpers.rs`).

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → all pass, including 2 new tests.

## Test plan

As Step 2. Model after the existing mutation tests in
`crates/tui/src/runner/tests/mutations_and_bulk.rs` (1329 lines of prior art —
pick a test that drives `apply_removed_message_ids` or bulk actions and copy
its setup).

## Done criteria

- [ ] `scripts/cargo-test -p mxr-tui --tests` exits 0 with the 2 new tests
- [ ] `cargo build -p mxr` exits 0
- [ ] Both helpers contain the anchor invalidation (grep `visual_anchor = None` in `mutation_helpers.rs` → ≥3 hits: clear_selection + 2 new)
- [ ] No files outside scope modified
- [ ] `plans/README.md` status row updated

## STOP conditions

- An existing test asserts visual mode/anchor persists across list mutations —
  the current behavior may be intentional; report with the test name.
- Excerpts don't match live code (drift).
- A verification fails twice.

## Maintenance notes

- Anyone adding a new list-mutating helper (e.g. for a new lens) must
  invalidate `visual_anchor` the same way; reviewers should check for it.
- Deferred: representing the anchor as a `MessageId` so visual mode survives
  refreshes. Worth considering if users complain about visual mode dropping
  during background syncs.
