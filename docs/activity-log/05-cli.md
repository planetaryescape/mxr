# Phase 4 — CLI

Goal: every IPC verb from Phase 3 has a usable `mxr activity` subcommand, with sensible defaults, JSON output for piping, and `--help` text that documents filter semantics in one place.

## Deliverables

1. `mxr activity` subcommand tree under `crates/daemon/src/cli/activity.rs`.
2. Alias `mxr act`.
3. Subcommands: `list`, `tail`, `stats`, `top`, `export`, `prune`, `redact`, `clear`, `pause`, `resume`, `replay`, `recall`.
4. `--json` flag on every read subcommand for scripting.
5. Snapshot tests for help text at `crates/daemon/tests/snapshots/cli_help__cli_help_activity*.snap`.
6. Tab completion verified (re-run `mxr completions zsh`, eyeball output).
7. End-to-end test using a real daemon + provider-fake.

## Out of scope

- TUI screen (Phase 5).
- Web routes (Phase 6).
- Saved activity filters (Phase 8).
- `recall` heuristics — stub it now, ship in Phase 8.

## Subcommand tree

```
mxr activity (alias: act) [SUBCOMMAND]
├── list              — paginated reverse-chron list
├── tail              — last N or follow (-f)
├── stats             — aggregates over a time window
├── top               — most-frequent actions in a window
├── export            — write CSV / JSON / NDJSON
├── prune             — hard delete by time / tier
├── redact            — tombstone by id / filter
├── clear             — convenience: tombstone recent activity
├── pause             — stop recording temporarily
├── resume            — resume recording
├── replay            — narrative prose of recent activity
└── recall            — fuzzy time lookup (Phase 8; stub now)
```

## Common filter flags (shared by `list`, `tail`, `stats`, `top`, `export`, `redact`)

| Flag | Type | Default | Description |
|---|---|---|---|
| `--since` | duration / ISO date | `24h` for `list`/`tail`, `7d` for `stats`/`top` | inclusive lower bound (e.g. `1h`, `3d`, `2026-05-01`, `2026-05-01T09:00`) |
| `--until` | duration / ISO date | `now` | exclusive upper bound |
| `--source` | repeatable | any | `tui`, `cli`, `web`, `daemon` |
| `--action` | repeatable | any | exact action token (e.g. `mail.archive`) |
| `--prefix` | string | none | matches all actions starting with this prefix (e.g. `mail.`) |
| `--target-kind` | string | none | `thread`, `message`, `draft`, `search`, etc. |
| `--target-id` | string | none | exact match |
| `--tier` | repeatable | any | `ephemeral`, `standard`, `important` |
| `--account` | string | any | account id |
| `--query` | string | none | FTS5 expression against context_json |
| `--include-redacted` | flag | false | include tombstoned rows |

### Duration parsing

Parse with the existing time-format crate already used elsewhere in the CLI (search the codebase for `humantime` or a local helper). Accept: `30s`, `5m`, `2h`, `3d`, `2w`, plus ISO 8601 dates and datetimes. Reject ambiguity with a clear error.

## Subcommand specs

### `list`

```
mxr activity list [filters] [--limit N] [--cursor TS,ID] [--json]
```

Default human output (TTY):

```
TIMESTAMP                SRC ACTION             TARGET                CONTEXT
2026-05-13T09:42:11Z     tui mail.read          thr_abc123            Alice — Q2 plan
2026-05-13T09:43:08Z     tui search.run         search:"invoice 2026" → 12 results
2026-05-13T09:44:02Z     tui mail.archive       thr_def456 (+7)       bulk: 8 threads
2026-05-13T09:46:30Z     cli mail.send          draft_ghi789          to: bob@…
```

- Pretty-prints `ts` to local timezone.
- Truncates context to fit the column; full context visible with `--json` or `--wide`.
- When cursor returned, prints `Next: --cursor 1715592090123,4321` at the bottom.

With `--json`:

```json
{
  "entries": [ { "id": ..., "ts": ..., ... } ],
  "next_cursor": { "ts": 1715592090123, "id": 4321 }
}
```

### `tail`

```
mxr activity tail [-n N | --lines N] [-f | --follow] [filters]
```

- `-n` defaults to `20`.
- `-f` polls the daemon at 1 s intervals and prints new rows as they arrive.
- Polling uses `ListActivity` with `since = last_seen_ts + 1`. No new IPC verb needed.
- `^C` cleanly closes.

### `stats`

```
mxr activity stats [--since 7d] [--until now] [--group-by action|day|source|target-kind|hour] [--json]
```

Default `--group-by action`. Human output:

```
ACTION             COUNT
mail.read            142
mail.archive          88
search.run            34
view.open_screen      29
...
```

For `--group-by day`:

```
DAY            COUNT
2026-05-13     312
2026-05-12     298
...
```

For `--group-by hour` (rollup, 0-23 in local time):

```
HOUR  COUNT  HISTOGRAM
00       3   ▁
...
09      87   █████████
10     124   █████████████
```

### `top`

```
mxr activity top [--since 7d] [--limit 20]
```

Convenience over `stats --group-by action` with desc sort + limit. Default limit 20.

### `export`

```
mxr activity export --format csv|json|ndjson [--out PATH] [filters]
```

- `--out -` (or omitted) writes to stdout.
- `--out PATH` writes to file (atomic — write to `PATH.tmp`, rename).
- Reports `Wrote N rows (M bytes) to PATH` on success.
- Emits `activity.exported` marker (Phase 3).

### `prune`

```
mxr activity prune --before 90d|YYYY-MM-DD [--tier ephemeral|standard|important] [--dry-run]
```

- Requires `--before`. No default — operator must explicitly state the cutoff.
- Without `--tier`, applies to all tiers.
- `--dry-run` prints "would delete N rows" and exits 0.
- Confirms interactively unless `--yes` is passed.

### `redact`

```
mxr activity redact (--ids ID1,ID2,... | [filters]) [--dry-run] [--yes]
```

- Either explicit ids OR a filter. Not both.
- Tombstones (sets `redacted=1`, clears `context_json`).
- Confirms interactively unless `--yes`.

### `clear` (browser-history pattern)

```
mxr activity clear --last 1h|1d|7d|30d|all [--dry-run] [--yes]
```

- Convenience over `redact` with a relative time window.
- `--last all` redacts every row, regardless of redaction state (subject to confirm).

### `pause` / `resume`

```
mxr activity pause [--for 1h] [--quiet]
mxr activity resume
```

- `--for` is optional; without it, pause is indefinite.
- `--quiet` suppresses stdout (useful for scripts).
- `mxr activity status` (free helper) reports paused state and `paused_until` if any.

### `replay`

```
mxr activity replay [--since 1h] [--limit 50]
```

Prints a prose narrative of recent activity:

```
In the last 1h on tui:
- Read 5 threads from inbox (Alice, Bob, GitHub)
- Searched "invoice 2026" → opened 2 results
- Archived 12 threads (bulk)
- Composed 1 reply to bob@example.com
- Snoozed 3 threads until tomorrow morning
```

Implementation: client-side aggregation. Group consecutive same-action rows; render with templates per action group. This is convenience over `stats` + per-row inspection.

### `recall` (stub now, real impl in Phase 8)

```
mxr activity recall "before lunch"
mxr activity recall "this morning"
mxr activity recall "yesterday afternoon"
```

Phase 4 implementation: parses a curated allowlist of phrases (`yesterday`, `this morning`, `last hour`) → time range → `list` output. Free-form phrases return an error pointing at Phase 8.

## Files

```
crates/daemon/src/cli/activity.rs       # new
crates/daemon/src/cli/mod.rs            # register subcommand; alias `act`
crates/daemon/src/cli/format.rs         # extend with activity-row formatter
crates/daemon/tests/cli_help_activity.rs # snapshot tests
crates/daemon/tests/snapshots/cli_help__cli_help_activity*.snap
```

## Output formatting

Reuse the existing CLI table formatter (look for `tabled`, `comfy_table`, or a local helper in `crates/daemon/src/cli/format.rs`). Pattern after `mxr logs` output for consistency.

Column widths:
- TIMESTAMP: 20 chars (ISO 8601 local, truncated to seconds)
- SRC: 4 chars (`tui`/`cli`/`web`/`daem`)
- ACTION: 18 chars (truncated with `…` if longer)
- TARGET: 30 chars (truncated)
- CONTEXT: remainder of terminal width (or 60 default)

Wide mode (`--wide`) doesn't truncate.

## Snapshot tests

`crates/daemon/tests/snapshots/cli_help__cli_help_activity.snap` — top-level help.
Per-subcommand `cli_help__cli_help_activity_list.snap` etc.

Test runner reads `--help` output through clap and snaps with `insta` (already in use; check `Cargo.toml`).

## End-to-end test

`crates/daemon/tests/cli_activity_e2e.rs`:

1. Spin up daemon with `provider-fake`.
2. Run `mxr activity list --json --since 1h`. Assert exit 0, parses as JSON, has `entries: []` initially.
3. Through a separate daemon IPC, fire a `MarkRead` to produce one activity row.
4. Re-run `mxr activity list --json --since 1h`. Assert one entry with expected fields.
5. `mxr activity stats --since 1h --json` returns expected bucket.
6. `mxr activity export --format ndjson --out -` prints one line containing the entry.
7. `mxr activity redact --ids <id> --yes` returns affected count 1.
8. `mxr activity list --since 1h --include-redacted --json` shows the redacted row with `redacted: true` and `context: null`.

## Acceptance criteria

- `mxr activity --help` lists all subcommands.
- Every read command accepts `--json` and emits valid, schema-stable JSON.
- Destructive commands (`prune`, `redact`, `clear`) prompt unless `--yes`.
- Help snapshots committed and stable.
- E2E test green.

## Risks & mitigations

| Risk | Mitigation |
|---|---|
| Filter-flag combinatorial complexity | Reuse one filter-building helper for `list`, `tail`, `stats`, `top`, `export`, `redact`. |
| `tail -f` polling load | 1 s poll; daemon caps `ListActivity` server-side. |
| `replay` group heuristic too rigid | Keep the template set small at v1; expand based on feedback. |

## Exit criteria

Phase 4 is done when:
- A new user can install mxr, run `mxr activity list`, and see their activity from a daemon that's been running for a day.
- `STATUS.md` Phase 4 boxes ticked.
- All snapshot tests committed.
