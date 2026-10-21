# Mode reads starved the reader pool; three gaps remain after the index fix

**Status:** root cause fixed on `fix/reader-pool-starvation` (migration 092) ·
**Found:** 2026-10-21 on v0.6.117 against a 110,839-message store ·
**Severity:** high while it lasted: Now, Messages, Updates, Reading, Records and
Arrivals timed out after 90 s, and so did background loops (snooze, scheduled
sends, reply-pair reconcile)

## What happened

The daemon log went from a handful of `pool timed out while waiting for an open
connection` errors a week to 57 on 2026-10-20 and 55 by midday 2026-10-21,
starting about 90 minutes after v0.6.115 was installed. A stack sample of the
live daemon showed every reader connection inside `sqlite3_step`, walking an
index and looking each row up.

The per-message evidence columns in `PLACE_COLUMNS` (`places.rs`) and
`desk_messages_sql` (`desk.rs`) look up the row's own thread three times. With
no `sqlite_stat1`, the planner chose `idx_messages_account` /
`idx_messages_account_date` for those subqueries, so every inbox message walked
the whole account. On the real store the inbox candidate read took 280 s for
854 messages; with `idx_messages_account_thread_date` it takes 0.01 s and
returns the same rows. Plan tests in both files now fail if any thread lookup
searches by account alone.

Measured on a copy of the same store with the v0.6.117 release binary: with the
index every mode returned in 0.1–4.8 s; with the index dropped, `reading` timed
out at 120 s.

## Still open

1. **Abandoned requests keep running.** When a client times out or
   disconnects, the handler keeps its reader connection until the query ends
   (`dropping client connection: response send failed`). Retrying clients
   stack copies of the same slow query and turn one slow read into pool
   starvation. Handlers should be cancelled when the client goes away.
2. **Now, Messages, Updates, Reading and the rail still take 1–5 s** on this
   store with the release build (measured while a lexical reindex ran, so
   somewhat pessimistic). A stack sample showed regex compilation
   (`regex_syntax`) on the request path. Fine for the TUI; too slow for a
   launcher client such as Raycast.
3. **No query-planner statistics.** The store never runs `ANALYZE` /
   `PRAGMA optimize`, so the planner guesses from schema alone. That guess is
   what picked the account index here. Running `PRAGMA optimize` on close or
   periodically would make this class of regression less likely; the plan
   tests stay the guard either way.
