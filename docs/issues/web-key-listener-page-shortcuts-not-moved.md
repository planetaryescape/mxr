# Two pages still bind keys outside the key definition

**Status:** open · **Severity:** low · **Found:** 2026-11-22 (delete-comment sweep, slice d-apps)

`apps/web/src/lib/keys/keyListeners.test.ts` allows direct key listeners in
`features/focus/FocusRoute.tsx` and `features/places/PlaceLayout.tsx` under the
rule "page shortcut not yet moved". The allowlist's doc comment called these
debt, not a rule, and said no new entry joins them.

Smallest fix: move both pages' shortcuts into the key definition, then remove
the rule and the two entries from `ALLOWED`.
