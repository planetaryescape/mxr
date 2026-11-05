# `mxr undo --dry-run` says "Would undo" for any id

**Status:** open · **Found:** 2026-11-04, verifier round 5 on
`feat/bulk-search-all` at `b762866ac` (the same on `origin/main`) ·
**Severity:** low, the preview is wrong but nothing changes

`mxr undo MUTATION_ID --dry-run` prints `Would undo mutation MUTATION_ID` and
exits 0 without asking the daemon whether that id has an undo entry
(`undo` in `crates/daemon/src/commands/mutations/mod.rs`). The real undo then
fails with `undo: mutation … not found (expired or already undone)` or
`undo: window expired …`. A typo, an expired id, or an id already undone all
preview as undoable, so the dry run does not match the path it previews.

`feat/bulk-search-all` added one real check to the dry run: it asks for the id
as a job (`GetJob`) and refuses an `--all` job with the real refusal. That
check cannot see undo entries, so unknown and expired mutation ids keep
main's behaviour.

Fixing it needs a read-only daemon request that reports an undo entry's
state (present, expired, missing) without claiming or restoring it, used by
the dry run, with the same not-found and expired messages as `UndoMutation`.
