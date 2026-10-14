# A closing menu swallows the next key

Status: open. Found while adding the Updates row menu (feat/updates-reel).

## What happens

Press Escape to close an open menu, then press a shortcut straight away
(`u` to undo). The shortcut does nothing. Waiting until the menu has left
the DOM makes the same shortcut work.

## Evidence

- `apps/web/e2e/verbs.spec.ts`, the Updates `pin` journey. Without a wait
  after Escape, `u` did not run the undo: the failure was the undo toast
  `/^(Snooze u|U)ndone$/` never appearing (error context at
  `test-results/verbs-pin-…/error-context.md`). The page snapshot showed the
  "Pinned: a sweep leaves it here" toast still up and the row's More trigger
  focused. With `await expect(page.getByRole("menu")).toHaveCount(0)` after
  Escape, the same journey passes (run on 2026-10-14).
- Observed runs: the full `verbs.spec.ts` run without the wait failed the
  `pin` journey once; the run with the wait passed it. One failing run
  without the wait is the whole sample, so the failure rate is not known.

## Suspected cause (not yet confirmed)

`apps/web/src/lib/keys/dispatcher.ts` `ownsKeyboard` (L226-240) treats any
`[role="menu"]` ancestor as text-entry-like, so the dispatcher ignores the
key. The closing-state exception (L230-233) covers only
`[role="dialog"]` and `[role="alertdialog"]`, not menus. Radix keeps the menu
mounted with `role="menu"` through its exit animation, which would explain
the window. Confirm by logging `event.target.closest('[role=menu]')` in the
keydown handler during the exit, then fix in `ownsKeyboard` by treating a
`[data-state="closed"][role="menu"]` like a closing dialog.

## Why it matters

Keys pressed right after closing a menu are dropped. A user who opens a row
menu, picks nothing, and presses `u` or `e` within the exit animation loses
the keystroke.

## Scope

Shared seam (dispatcher.ts). Not changed in feat/updates-reel; the Updates
journey test waits for the menu to detach instead.
