# Eleven hook dependency lists are silenced instead of correct

**Status:** open · **Severity:** low · **Found:** 2026-11-22 (delete-comment sweep, slice d-apps)

`react-hooks/exhaustive-deps` catches stale closures, so each suppression
hides a place where an effect or memo may read an old value. The comments that
explained each one were deleted in the sweep. These are the symbols:

- `apps/web/src/features/mail-actions/pendingMailOps.ts`, `useProjectedGroups` (lens is keyed by `lensKey`)
- `apps/web/src/features/messages/MessagesList.tsx`, the memo after `listRef`
- `apps/web/src/features/thread/ThreadReader.tsx`, five `useEffect` calls (focus, scroll and landing effects)
- `apps/web/src/lib/actions/hints.ts`, `useShortcutSections` and `useVisibleActions` (registry is rebuilt when `version` changes)
- `apps/web/src/state/landingStore.ts`, `useLanded`

Smallest fix: for each, move the value the effect reads into a ref or the
dependency list, or key the memo on the value it really depends on, then
delete the suppression. Where the dependency is a store version counter
(`hints.ts`), pass the counter to the memo as an explicit dependency.
