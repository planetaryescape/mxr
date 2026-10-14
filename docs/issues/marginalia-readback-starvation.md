# Failing Marginalia items can starve the background highlight read-back

**Status:** open, not fixed · **Found:** 2026-10-14 in review of PR #447 ·
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

## Fix to consider

Record the attempt separately from the success (for example
`highlights_attempted_at`, used for ordering and the half-hour gate), or
write `highlights_synced_at` on a non-stop failure too and keep the error
on the result. Add a test with 20 persistently failing rows ahead of one
healthy row.
