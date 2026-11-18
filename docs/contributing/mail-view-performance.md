# Measure local mail views

Run the timing sweep to separate slow daemon views from slow browser loading. Use the installed binary on a normal mailbox. Use an isolated profile and a private database snapshot for restart, migration and invalidation tests.

## Run the installed sweep

Select the local runtime you want to measure, then run these commands from the repository root:

```sh
mxr version
python3 scripts/perf-mail-views.py --repeat 3 --budget-ms 250
python3 scripts/perf-mail-views.py --case all --repeat 3 --budget-ms 250
```

The script prints one JSON object per case, with `case`, `samples_ms`, `median_ms`, `p95_ms`, `within_budget` and `failures`. It exits nonzero if a request fails, returns invalid JSON or exceeds the budget. Each request times out after 30 seconds; `--timeout` changes that limit. `--binary` selects another executable. Do not point a development binary at your normal mailbox.

Each sample covers CLI startup, daemon work, reply transfer and JSON validation. The script prints timings and errors, never mail. The sweep changes no mail:

- Reading runs `reading edition --peek`, which records no visit.
- To do runs `todo list --state open`, which does not mark runway items seen. A plain `mxr todo list` does mark them seen.
- Updates runs `updates`, which does not mark the digest seen.
- Search adds a `search.run` entry to the local activity log.

With three samples, `p95_ms` is the largest sample. It is not a measured tail latency.

## Name the cache state

Record each layer separately when you compare optimized binaries on the same snapshot:

| Layer | What to record |
| --- | --- |
| Upgrade | Schema before and after, and the one-time projection backfill time |
| Startup | Whether normal maintenance has completed |
| Persisted facts | Whether the asks, recipient and thread projections already exist |
| Decoded facts | Whether startup has prepared in-memory thread history |
| Response | Whether the exact view or a reusable Attention response already exists |
| Client | Whether the browser kept its query data and mounted panes |

A first All response after maintenance can reuse decoded history facts even when no complete All response exists. Your turn and Unread can reuse a fresh Attention response. Neither case is a cold read. The script does not clear caches or restart a daemon.

To measure an independent first response, restart only an isolated daemon that you own. Wait for its normal maintenance to finish, then request All before any other view. Measure a projection miss separately. Keep tracing off for acceptance timings, name the build profile and avoid concurrent heavy work. The [release audit](../research/mail-view-performance-2026-10-26.md) shows how a passing installed sweep and slower cold reads can both be true.

## Verify freshness and output separately

Use synthetic mail in an isolated profile for mutation tests. Change source state through daemon operations, then read it back through the same public surface. Direct SQLite writes fire the projection triggers but bypass the daemon's response-cache change feed, so they cannot prove end-to-end freshness.

Compare complete normalized replies, not only row counts. The October audit excluded only `generated_at`, and the All replies otherwise matched at 844,337 bytes. Check these cases:

- account isolation
- newly eligible or previously excluded threads
- facet writes at the same source generation
- read and inbox edits
- future-message boundaries
- late builders

Keep detail evidence and attachments in the comparison, even though lists use compact facts.

Find the focused regression tests before you change a path:

```sh
rg -n 'history_cache|future|same_generation|late|returned' crates/daemon/src/handler/tests/messages.rs crates/store/src/conversation_summaries.rs
```

Do not send or change real mail to test performance. Mailbox snapshots, replies and traces are private data. Keep them out of committed documentation.

## Measure browser usability

In browser DevTools, record the time to first byte and total duration of the `/api/v1/mail/people` request. Also record the document navigation count, loading duration and layout-shift entries. Compare the filter element's identity, focus and bounding rectangle before and after a view change. Test desktop and phone layouts separately.

Delay the filter response in an isolated browser test and check that:

- The pane and controls stay mounted.
- Loading rows use the list's geometry.
- Ready data appears without a minimum skeleton hold.
- A superseded read does not replace the selected view when you switch again before the first reply arrives.
- `f` works in loading, empty, error and populated states.

For TUI changes, test that daemon events reach the UI while a request is held. Also test that abandoned queued reads do not delay the current selection. A mutation whose consumer has gone is not cancelled, so never assume that it did not apply.

See the [web maintainer notes](../web-app.md) for the interaction contract and [the cold-history issue](../issues/messages-cold-history-budget.md) for the open acceptance condition.
