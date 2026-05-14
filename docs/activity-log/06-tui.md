# Phase 5 — TUI

Goal: a first-class activity screen in the TUI, reachable from anywhere with a single chord, with filter + detail + redact + jump-to-target.

## Deliverables

1. `Action::OpenActivityScreen` (+ subsidiary actions: `ActivitySearch`, `ActivityFilter`, `ActivityRedact`, `ActivityClear`, `ActivityJumpToTarget`, `ActivityPause`, `ActivityResume`, `ActivityExport`).
2. Keybinding `g a` (g-prefix for "go to") + palette entry `View activity`.
3. Screen module `crates/tui/src/screens/activity.rs`.
4. Filter bar (date range, source, action prefix, full-text).
5. Reverse-chron table with paged scrolling.
6. Detail drawer (`Enter` expands a row).
7. Jump-to-target (`o` opens the referenced thread / draft / search).
8. Redact-with-confirm (`r`).
9. Help overlay (`?`).
10. Integration test that the screen renders with seeded data.

## Out of scope

- Saved filters (Phase 8).
- Charts / dashboards (Phase 8 mostly; one summary line is fine here).

## Screen layout

```
┌ activity ─────────────────────────────────────────────────────────────┐
│ Filter: since=24h  source=any  prefix=  query=                        │
│ Tier: any   Account: any   Redacted: hidden                            │
├───────────────────────────────────────────────────────────────────────┤
│ TIMESTAMP            SRC ACTION              TARGET           CONTEXT │
│▶09:42:11 today       tui mail.read           thr_abc          Alice — │
│ 09:43:08             tui search.run          "invoice 2026"   12 res  │
│ 09:44:02             tui mail.archive (×8)   thr_def +7       bulk    │
│ 09:46:30             cli mail.send           draft_ghi        to: bob │
│ ...                                                                    │
│                                                                        │
│ 142 rows · cursor: 1715592090123,4321 · paused: no                    │
├───────────────────────────────────────────────────────────────────────┤
│ j/k navigate · Enter detail · o open target · / search · f filter      │
│ r redact · e export · p pause · q close · ? help                       │
└───────────────────────────────────────────────────────────────────────┘
```

Filter bar is the top region; row table is the body; status bar shows count + cursor + paused flag.

## Keybindings (screen-local)

| Key | Action | Notes |
|---|---|---|
| `j` / `↓` | move down | wraps at end, page-scrolls if at last visible |
| `k` / `↑` | move up | |
| `g g` | top | |
| `G` | bottom | |
| `Ctrl-d` / `Ctrl-u` | half-page jump | match existing TUI conventions |
| `Enter` | open detail drawer | renders full `context` JSON, pretty |
| `o` | open referenced target | thread/draft/search → opens that screen for the target |
| `/` | enter search mode | edits `filter.query` (FTS5) |
| `f` | filter modal | full filter form |
| `s` | source filter | quick toggle source |
| `a` | action prefix filter | quick input for prefix |
| `t` | tier filter | toggle ephemeral/standard/important |
| `r` | redact row | confirmation modal |
| `R` | redact filtered | redact all rows matching current filter (confirmation) |
| `e` | export current filter | format prompt → file path prompt |
| `p` | pause toggle | if paused, resume; if not, pause indefinitely |
| `C` | clear menu | sub-menu for "last 1h / 1d / 7d / all" |
| `n` / `N` | next / prev match | when in `/` search mode |
| `q` / `Esc` | close screen | returns to previous screen |
| `?` | help overlay | |

Global (from any screen):

| Chord | Action |
|---|---|
| `g a` | open activity screen |
| Palette: `View activity` | open activity screen |
| Palette: `Activity: clear last hour` | quick clear |
| Palette: `Activity: pause` | toggle pause |

## State + IPC

The screen state is a Zustand-like struct:

```rust
pub struct ActivityState {
    pub filter: ActivityFilter,
    pub rows: Vec<ActivityEntry>,
    pub selected: usize,
    pub cursor: Option<ActivityCursor>,
    pub fetching: bool,
    pub paused: bool,
    pub paused_until: Option<i64>,
    pub detail_open: bool,
    pub mode: ActivityMode,           // List | Search | FilterModal | RedactConfirm | ClearMenu | ExportPrompt
    pub search_input: String,
    pub error: Option<String>,
}
```

IPC calls:
- On screen open: `ListActivity { filter: default, limit: 50, cursor: None }`. Default filter is `since = now - 24h`.
- On scroll-past-end: re-issue `ListActivity` with last cursor, append to `rows`.
- On filter change: reset rows, refetch.
- On `r`: `RedactActivity { ids: [selected.id], dry_run: false }`. Optimistically mark redacted in local state.
- On `R`: `RedactActivity { filter: current.filter, dry_run: false }` (preview count with dry-run modal first).
- On `p`: `PauseActivity { until_ts: None }` / `ResumeActivity`.
- On `e`: `ExportActivity { filter, format, path }`.
- On `o`: navigate based on `target_kind`:
  - `thread` → open thread reader for `target_id`
  - `draft` → open compose with `target_id`
  - `search` → re-run the saved search (`context.query` if present)
  - `label` → open label view
  - others → noop with a transient toast.

## Detail drawer

When `Enter` is pressed:
- Right-side modal panel (40% width).
- Renders:
  - All scalar columns.
  - `context_json` pretty-printed.
  - "Open target" button (if applicable).
  - "Redact this row" button.
- `Esc` closes.

## Filter modal

When `f` is pressed:
- Modal with form fields for each filter:
  - Since / Until (date input)
  - Source (multi-checkbox: tui/cli/web/daemon)
  - Action / Prefix (text inputs)
  - Target kind / id
  - Tiers (multi-checkbox)
  - Query (FTS text)
  - Include redacted (checkbox)
- `Tab` to navigate, `Enter` to apply, `Esc` cancels.

## Files

```
crates/tui/src/
  action.rs                              # extend Action enum
  keybindings.rs                         # add `g a` chord + palette entry
  app.rs                                 # route Action::OpenActivityScreen
  screens/
    mod.rs                               # register `activity` screen
    activity/
      mod.rs                             # screen entry, layout + dispatch
      state.rs                           # ActivityState + reducers
      list.rs                            # table renderer
      filter.rs                          # filter bar + modal
      detail.rs                          # detail drawer
      ipc.rs                             # async fetch helpers
      keymap.rs                          # screen-local key handling
```

## Action enum additions

```rust
// crates/tui/src/action.rs
pub enum Action {
    // ... existing ...
    OpenActivityScreen,
    ActivityFetchPage,
    ActivityNext,
    ActivityPrev,
    ActivityJumpTop,
    ActivityJumpBottom,
    ActivityOpenDetail,
    ActivityCloseDetail,
    ActivityOpenTarget,
    ActivityEnterSearch,
    ActivityApplySearch(String),
    ActivityOpenFilterModal,
    ActivityApplyFilter(ActivityFilter),
    ActivityToggleTier(Tier),
    ActivityToggleSource(ClientKind),
    ActivityRedactRow,
    ActivityRedactFiltered,
    ActivityClear(Duration),
    ActivityExport(ActivityExportFormat, Option<String>),
    ActivityPauseToggle,
    ActivityClose,
}
```

## Visual conventions

- Source color: `tui` cyan, `cli` magenta, `web` blue, `daemon` dim.
- Tier indicator: optional `[E]`/`[S]`/`[I]` prefix when `--show-tier` toggle is on (default off; saves space).
- Redacted rows render dimmed with strikethrough action text and `(redacted)` instead of context.
- Paused state: status bar `paused: yes (until 10:30)` in yellow.

## Tests

- Unit: action reducers in `state.rs`.
- Render: snapshot of the screen with seeded rows (use `ratatui::TestBackend`).
- Integration: spawn daemon, open TUI, navigate to activity screen, press `j`, press `Enter`, assert detail drawer state, press `r`, confirm, assert row redacted.

## Acceptance criteria

- `g a` from any screen opens activity in <100 ms (data fetch async, screen renders empty + loading state).
- Filter changes feel instant (request <50 ms p99 against seeded test data).
- `o` from a `mail.read` row opens the thread reader for that thread (real IPC round-trip).
- Redaction confirms before mutating.

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Filter modal complex to render | Reuse existing modal pattern from rules editor / saved search editor in the TUI. Don't invent a new layout. |
| Open-target races (target deleted since the activity was recorded) | Graceful: show a toast "target no longer exists" and stay on the activity screen. |
| Pause UI causes the user to think mxr is broken | Persistent status-bar indicator; first row after pause should be `activity.paused`. |

## Exit criteria

Phase 5 is done when:
- TUI screen ships and `g a` works from every other screen.
- Redact + jump-to-target + export all wired through real IPC.
- `STATUS.md` Phase 5 boxes ticked.
