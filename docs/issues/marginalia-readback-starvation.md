# Failing Marginalia items can starve the background highlight read-back

**Status:** resolved 2026-10-14 on `fix/marginalia-readback-starvation` · **Found:** 2026-10-14 in review of PR #447 ·
**Severity:** low; highlights stop arriving on their own, and `mxr reading
marginalia sync` and opening the item still work

## What happens

After each mail sync the daemon reads back highlights for items sent to
Marginalia in the last 60 days (`after_sync` in
`crates/daemon/src/handler/reading/marginalia.rs`), in batches of
`BACKGROUND_BATCH` (20). `reading_marginalia_due`
(`crates/store/src/reading.rs`) picks them with
`ORDER BY COALESCE(highlights_synced_at, 0), sent_at LIMIT ?`, least
recently read first.

A read that fails with an error other than "stop" (a 5xx, a timeout, an
article that answers oddly) never writes `highlights_synced_at`. Its row
keeps sorting first. With 20 or more such rows, every batch is those same
20 and the healthy items behind them are never read in the background.

## Fix

`after_sync` now remembers, in memory, which items failed their background
read-back and when. Its batch is the due items minus those that failed in
the last half hour, so the same failing items can't fill every batch. A
success clears the item; entries older than half an hour are dropped. The
memory goes with a restart, so after one each failing item is tried once
more before it waits again. That cost is one extra request per failing
item per restart, and it avoids a migration and a backup schema bump.
`highlights_synced_at` still means "last read successfully", so the time
clients show stays true.

`background_tests::failing_items_dont_starve_the_ones_behind_them` covers
20 failing items ahead of one healthy item.
