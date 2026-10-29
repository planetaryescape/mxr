-- Migration 107: who is restoring an undo entry.
--
-- A chunk undo (`UndoMutation`) and a whole-job undo (`UndoJob`) can reach
-- the same entry. Each takes it with one conditional UPDATE that only
-- succeeds while `claimed_by` is NULL, so exactly one of them restores it.
-- NULL means free. The claim is cleared when the restore ends, by the undo
-- job when it ends, and for every row at daemon startup (a claim held by a
-- process that died).
ALTER TABLE mutation_undo_log ADD COLUMN claimed_by TEXT;
