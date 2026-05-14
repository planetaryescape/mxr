# Phase 8 — Power Features

Goal: turn the activity log from "reachable" into "delightful". Saved filters, fuzzy-time recall, narrative replay, in-screen stats. Build on Phase 7 — privacy invariants must already be locked.

## Deliverables

1. Saved activity filters (CRUD across CLI / TUI / web).
2. Activity stats dashboard in TUI + web.
3. `mxr activity recall "before lunch"` — real fuzzy-time parser.
4. `mxr activity replay --since 1h` — real narrative generator (replaces Phase 4 stub).
5. "Resume what I was reading" affordance — finds the most recent `thread.open` before a timestamp.
6. Top-targets surface: most-archived senders, most-searched terms, most-snoozed threads — in stats.

## Out of scope

- Activity-as-undo (deferred indefinitely; the existing undo infra owns that).
- LLM-summarized activity narratives (later, opt-in).

## 8.1 — Saved activity filters

Mirrors existing `saved_searches` pattern.

### Storage

New table:

```sql
CREATE TABLE IF NOT EXISTS saved_activity_filters (
    slug         TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    filter_json  TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    last_used_at INTEGER
);
```

### IPC verbs

Under `MxrPlatform` (saved filters are platform-level, like saved searches):

- `ListSavedActivityFilters`
- `GetSavedActivityFilter { slug }`
- `SaveActivityFilter { slug, name, filter }`
- `DeleteSavedActivityFilter { slug }`

### CLI

```
mxr activity saved list
mxr activity saved save <slug> --name "Yesterday's mail" [filter args]
mxr activity saved delete <slug>
mxr activity saved open <slug>          # alias for `list --using <slug>`
mxr activity list --using <slug>        # apply a saved filter inline
```

### TUI

- `s` on activity screen → save current filter (prompts for name + slug).
- Filter modal has a "Saved filters" picker.

### Web

- Filter sidebar has a `Saved` section at the top with quick-load.
- Save-current-filter button below the filter form.

## 8.2 — Stats dashboard

### Surfaces

- **TUI**: activity screen sidebar (right rail) with a compact summary:

  ```
  Last 7 days:
    Total: 1,243
    Top action: mail.archive (412)
    Top sender:  alice@... (87)
    Most active hour: 09:00 (142)
    [view full →]
  ```
  Press `Tab` → expand to full dashboard.

- **Web**: dedicated `/activity/stats` route with three charts:
  - Daily bar chart (count per day, last 30 days).
  - Hour-of-day histogram (rollup).
  - Top actions / top targets tables.

### Backend

Reuse `Request::ActivityStats { since, until, group_by }` — already in Phase 3. Add new group_by variants if needed:

- `TargetId` — frequency of specific targets (top senders, top threads). Cap result set to top 100.
- `Day` (already), `Hour` (already), `Action` (already), `Source` (already).

For "top sender", `target_id` is the thread id; the daemon joins against `threads` to dereference into the sender field. Add a dedicated handler `ActivityTopSenders { since, until, limit }` returning `[{ sender_email, sender_name, count }]` — cheaper than client-side joins.

## 8.3 — Fuzzy-time recall

`mxr activity recall "<phrase>"` resolves natural language to a time range and runs `list` for that range.

### Phrase grammar (curated)

- Absolute days: `today`, `yesterday`, `tomorrow` (last two days only; `tomorrow` returns empty).
- Times of day: `morning` (06:00-12:00), `afternoon` (12:00-18:00), `evening` (18:00-23:00), `lunch` (12:00-13:30), `breakfast` (06:00-09:00), `night` (22:00-04:00 next day).
- Relative: `last hour`, `last 5 minutes`, `last week`, `past 2 days`.
- Anchored: `before lunch`, `after lunch`, `since this morning`, `until yesterday evening`.
- Day-of-week: `last monday`, `monday`, `wednesday afternoon`.

### Implementation

Use the `chrono` crate (already in tree). Don't pull in `chrono-english` — keep the grammar curated and predictable. A small recursive-descent parser handles:

```
phrase     := period | anchor period
anchor     := "before" | "after" | "since" | "until"
period     := relative | named | dow
relative   := "last" duration | "past" duration
named      := "today" | "yesterday" | "tomorrow"
                | "morning" | "afternoon" | ...
dow        := ["last"] DAY_OF_WEEK [time_of_day]
```

Anything else → return an error: `"could not parse '<phrase>'. Try: 'yesterday afternoon', 'last hour', 'before lunch'"`.

### CLI

```
mxr activity recall "yesterday afternoon" [--limit 50] [--json]
mxr activity recall "since this morning" --action mail.archive
```

Maps to `list` with computed `--since` / `--until`.

## 8.4 — Replay narrative

Replaces Phase 4 stub. Aggregates rows into a prose summary.

### Grouping rules

1. Sort rows ascending by `ts`.
2. Walk and group consecutive rows where:
   - `action == previous.action`, OR
   - `action_prefix == previous.action_prefix` AND time delta < 5 min.
3. Per-group template (table-driven, one per action prefix).

### Example output

```
Last 1h on tui:
  09:42  Read 5 threads from inbox (Alice, Bob, GitHub)
  09:43  Searched "invoice 2026" → 12 results; opened 2
  09:44  Archived 12 threads (bulk)
  09:46  Composed reply to bob@example.com; sent at 09:51
  10:03  Snoozed 3 threads until tomorrow morning
```

### Templates

```rust
// crates/daemon/src/cli/replay_templates.rs
pub fn template_for(action: &str) -> Option<&'static str> {
    match action {
        "mail.read"    => Some("Read {count} thread{s}{from}"),
        "mail.archive" => Some("Archived {count} thread{s}{bulk}"),
        "search.run"   => Some("Searched \"{query}\" → {result_count} results"),
        "mail.send"    => Some("Sent to {recipient}"),
        "mail.snooze"  => Some("Snoozed {count} thread{s} until {until}"),
        "draft.create" => Some("Started draft to {recipient}"),
        "view.open_screen" => None,        // skip noisy view-open events in replay
        _ => None,
    }
}
```

`{from}` joins distinct sender names from grouped `mail.read` rows. `{bulk}` is empty for single rows; `(bulk: N)` for multi.

### CLI

```
mxr activity replay [--since 1h] [--limit 200] [--json]
mxr activity replay --since 24h --source tui
```

Web version: `/activity/replay` route, same data, rendered as a vertical timeline.

## 8.5 — "Resume what I was reading"

Convenience command for context-switching recovery.

```
mxr activity resume-reading [--around HH:MM] [--source tui|cli|web]
```

Finds the latest `thread.open` action before `--around` (defaults to "now" or "last app-stop") and either:
- Prints the target thread id (default).
- Opens it: `mxr activity resume-reading --open` (requires TUI/web client to consume the response).

In the TUI: palette entry `Resume reading` and keybind `g r` (g-prefix "go-recent").

## 8.6 — Top-targets surface

For each target kind, surface the top-N targets within a time range.

```
mxr activity top-targets --kind thread --since 30d --limit 20
mxr activity top-targets --kind search --since 30d
mxr activity top-targets --kind sender --since 30d
```

`sender` is a synthesized kind that resolves through the `threads` table.

Output:

```
RANK  TARGET                          COUNT  LAST_SEEN
1     alice@example.com (Alice)         87   2026-05-13 09:42
2     bob@example.com (Bob)             54   2026-05-13 08:11
...
```

## Files

```
crates/store/migrations/0NN_saved_activity_filters.sql
crates/store/src/saved_activity_filters.rs
crates/daemon/src/cli/activity_saved.rs
crates/daemon/src/cli/activity_recall.rs
crates/daemon/src/cli/activity_replay.rs
crates/daemon/src/cli/replay_templates.rs
crates/daemon/src/handler/activity_saved.rs
crates/tui/src/screens/activity/saved.rs
crates/tui/src/screens/activity/stats_rail.rs
apps/web/src/routes/activity.stats.tsx
apps/web/src/components/activity/SavedFilters.tsx
```

## Tests

- Saved filters: CRUD round-trip via IPC. Slug uniqueness. Loading by slug.
- Recall parser: table-driven test, 50+ phrases.
- Recall errors: unknown phrase → error with hint.
- Replay grouping: synthetic rows → expected narrative.
- Top-senders: seeds threads + activity, asserts ranking and tie-breaks.

## Acceptance criteria

- All saved filter operations work across CLI/TUI/web.
- Recall accepts every grammar in [#phrase-grammar-curated](#phrase-grammar-curated).
- Replay narrative reads cleanly for typical days (eyeball test in PR).
- Stats dashboard renders in <100 ms for 30d windows.

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Recall grammar drifts toward NLP rabbit hole | Strict curated list. Reject unknown phrases. |
| Replay templates feel robotic | Iterate after dogfooding. Templates are isolated in one file. |
| Top-senders is expensive | Cap join to top-100; index `target_id` already (Phase 1). |

## Exit criteria

Phase 8 is done when:
- A user can `mxr activity recall "before lunch"` and get useful output.
- A user can save and reload filters from all three clients.
- Replay narrative ships in CLI and web.
- `STATUS.md` Phase 8 boxes ticked.
