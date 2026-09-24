# Plan 021: Preserve the mail journey and honor keyboard focus

> Execute step by step, checking every gate. This is planned work; validation commands have not been run for an implementation. The coordinator maintains the index during this batch.

## Status

- **Priority:** P1
- **Effort:** L
- **Risk:** MED — routing, focus restoration, and shared action context
- **Depends on:** `011-safe-verification-and-ci.md`, `020-web-mutation-consistency.md`
- **Category:** bug
- **Planned at:** fetched `origin/main` `9160b1a12ef9dc5d3fb5513b45c68fe57183074f`, 2026-09-04
- **Followed by:** `022-shared-mutation-previews.md`; Plan 025 consumes the browser journeys

## Why this matters

Opening mail from a search, label, or saved search currently falls back to an Inbox reader route, losing the user's working context. Separately, Enter/Space on nested row controls bubbles into “open this message.” Complete the existing find → read → act → next → return workflow, with native keyboard controls that do the action they advertise.

## Current state

`apps/web/src/features/mailbox/MailboxList.tsx:146` opens every row through:

```tsx
void navigate({
  to: "/m/$mailbox/$threadId",
  params: {
    mailbox: mailboxSegment(mailboxPath),
    threadId: row.thread_id,
  },
});
```

The helper at line 491 discards label/saved/search identity:

```ts
function mailboxSegment(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[1] && parts[1] !== "label" && parts[1] !== "saved" ? parts[1] : "inbox";
}
```

`features/search/SearchResultsRoute.tsx:448` passes `/search` without its q/mode/account/sort/scope/verdict/groupBy parameters. `features/mailbox/MailboxRoute.tsx:16` treats a three-part path as an open thread unless the second part is `label`; `/m/saved/<slug>` is therefore misclassified. `features/thread/ThreadRoute.tsx:114` derives the return path by dropping the last path segment and builds subsequent paths with string concatenation at line 315.

The same loose interpretation appears in `lib/actions/context.ts:18` (`/^\/m\/[^/]+\/[^/]+/`) and `features/mailbox/actions.ts:37`. A label/saved lens can be classified as a focused thread. `activeRouteQueueLabel` at actions line 58 decodes a route slug as if it were the actual label; resolve label identity from the existing shell lens instead.

`MailboxRow.tsx:70` intercepts events from all descendants:

```tsx
onKeyDown={(event) => {
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    onOpen();
  }
}}
```

Nested Select and Star buttons at lines 91/103 stop click propagation, but the row still sees their keydown. `MailboxList.tsx:197` listens on window without honoring `event.defaultPrevented` or native button targets. Row focus only sets the active pane (`MailboxRow.tsx:69`), not the logical focused row. Quick actions are hover-only at line 173, and the selection button is opacity zero until hover/selected at line 96.

Existing patterns to preserve: TanStack typed routes; Zod search validation in `routes/search.tsx`; page-local navigation in `lib/pageKeyHints.ts`; shared durable actions in `lib/actions`; the current `mailboxPaneStore` and selection scope. `docs/web-app.md` states, “URL is canonical state wherever possible.” Terminal-only motions do not need copied global browser actions.

## Lessons carried forward

- **Keyboard Parity Is a Product Contract:** test discovery, reaching the action, performing it, and recovering with cancel/Undo/focus return. Having a shortcut does not prove Tab/Enter works.
- **Same-Code-Path Preview:** preserving navigation must not silently change a selection from explicit message IDs to whole threads or all query matches.
- **Parser Parity Is Not Execution Parity:** a well-typed route that opens the wrong lens still fails the user journey; test actual router/rendered state.

## Scope

Modify only:

- `apps/web/src/features/mailbox/MailboxList.tsx`, `MailboxList.test.tsx`, `MailboxRow.tsx`, `MailboxRoute.tsx`, `useMailboxQuery.ts`, `useMailboxQuery.test.tsx`
- `apps/web/src/features/thread/ThreadRoute.tsx`, `ThreadRoute.test.tsx`
- `apps/web/src/features/search/SearchResultsRoute.tsx`, `SearchResultsRoute.test.tsx`
- `apps/web/src/routes/search.tsx`, `routes/m.$mailbox.$threadId.tsx`, `routes/m.$mailbox.tsx`, `routes/m.label.$name.tsx`, `routes/m.saved.$slug.tsx`
- New `apps/web/src/features/mailbox/navigationContext.ts` and `.test.ts`
- `apps/web/src/lib/actions/context.ts`, `context.test.tsx`, `features/mailbox/actions.ts`, `actions.test.ts`; `lib/pageKeyHints.ts` only for changed discovery hints
- `apps/web/src/state/mailboxPaneStore.ts`, `selectionStore.ts` only for scoped focus/selection restoration
- `apps/web/src/routeTree.gen.ts` through the route generator only
- `apps/web/e2e/keyboard-navigation.spec.ts`, `accessibility.spec.ts`; new `navigation-context.spec.ts`

Out of scope: new workspaces, global UI redesign, changing search semantics, bulk confirmation policy (Plan 022), changing which messages a row action mutates, touch/mobile support, or introducing a second router/state framework. Styles may change only inline class names for touched controls' visible focus/hover parity. Do not mark the rest of legacy Plan 009 complete.

## Commands, environment, and Git

Work in an isolated `codex/021-web-navigation-and-keyboard` branch. Fetch `origin main`, record fetched/working SHAs, then inspect drift from the baseline:

```sh
git diff --stat 9160b1a12ef9dc5d3fb5513b45c68fe57183074f..HEAD -- apps/web/src/features/mailbox apps/web/src/features/thread apps/web/src/features/search apps/web/src/routes apps/web/src/lib/actions apps/web/src/state apps/web/e2e
git status --short
```

Plan 020 changes are expected: reread its production query/mutation interfaces instead of reverting them. Never switch/reset/clean the user's active checkout.

Use Node 24 (`node --version` must report `v24.*`) and `npm ci --prefix apps/web` for locked dependencies. Use repaired verification from Plan 011; the old cargo wrapper can kill unrelated processes and is prohibited. All commands below are future gates:

| Purpose | Command | Success |
|---|---|---|
| Navigation unit tests | `npm --prefix apps/web test -- src/features/mailbox/navigationContext.test.ts src/features/mailbox/MailboxList.test.tsx src/features/thread/ThreadRoute.test.tsx src/features/search/SearchResultsRoute.test.tsx src/lib/actions/context.test.tsx src/features/mailbox/actions.test.ts` | Nonzero relevant tests all pass |
| Typecheck | `npm --prefix apps/web run typecheck` | Exit 0 |
| Lint | `npm --prefix apps/web run lint` | Exit 0 |
| Web build/route generation | `npm --prefix apps/web run build` | Exit 0; generated route diff reviewed |
| Daemon build | `cargo build -p mxr` | Exit 0 |
| Browser | From `apps/web`: `npm run e2e -- e2e/navigation-context.spec.ts e2e/keyboard-navigation.spec.ts e2e/accessibility.spec.ts` | All pass |

The Playwright harness starts an isolated FakeProvider daemon and real bridge with `.playwright` state, default ports 5173/17777/17778, and `target-cli/debug/mxr`. Read `apps/web/scripts/e2e-server.mjs` and check ports are free; never kill their owners. Install the locked runner's Chromium if needed. No real mailbox access. Commit convention: `fix: preserve mail navigation context and keyboard focus`; no attribution footers or publishing without instruction.

## Steps

### 1. Capture the broken user journeys

Extend unit tests with the real TanStack router/memory history for navigation behavior rather than asserting only `navigate` mock calls. Render a saved lens without a reader and assert it is not a focused thread. Open results from a search with all supported parameters and from a named label/saved lens; assert return context survives. Include label names whose display text differs from slug and names with spaces/non-ASCII characters.

Add row-control tests: focus Select/Star, press Enter/Space, assert exactly one intended mutation/selection and zero navigation. Add Tab focus on the second row then a row shortcut; assert it operates on that row. Keep existing reader/mailbox j/k ownership tests.

**Verify:** navigation unit command reports the named regression failures on pre-fix behavior. New helper tests may be introduced with the additive context skeleton, but at least one real existing-journey regression must fail before the implementation.

### 2. Carry a typed origin through the existing reader route

Define one discriminated `MailNavigationContext` for system mailbox, label lens, saved lens, and search. For search, retain q/mode/account/sort/scope/verdict/groupBy. For labels, retain route identity separately from actual label ID/name. Include explicit reader thread ID; do not infer it from a path segment count. Derive context from route IDs/validated parameters and the existing shell data.

Keep existing reader deep links working. Add an optional validated `origin` search parameter to the existing `/m/$mailbox/$threadId` route to carry the source context when it cannot be inferred from that route. Encode/decode through TanStack/Zod, not manually assembled query strings. Accept only known internal lens/search contexts, not arbitrary external URLs. Old reader links without origin continue to return to their system mailbox.

Replace the affected split/regex readers with this shared resolver: list open, `MailboxRoute`, reader return/next/previous, action-context `hasFocusedThread`, and registry focused-target/queue-label resolution. Render the originating list/query next to the reader; do not fetch Inbox because the canonical reader path used its compatibility mailbox segment. Invalid/deleted origin should show an explicit unavailable-context recovery to an existing mailbox, not silently present a different lens as the original one.

Do not change IDs selected for mutations. Opening a representative thread message is still navigation; selecting a row must retain the existing message-ID semantics. Do not enable an action on an unhydrated/fake focused thread.

**Verify:** real-router navigation tests and action-context tests pass. Typecheck and build/route generation pass. Search parameters and label IDs round-trip exactly; old reader deep links still work.

### 3. Restore position and make focus ownership explicit

Keep focused row ID and virtualizer offset scoped by the canonical origin. Opening/returning within the same origin restores focus to the initiating row and its scroll position. If that row was removed by an action, choose the nearest remaining row deterministically; do not select a different message for a still-open confirmation. A genuine lens/query change clears selection using its own scope; reader navigation within a lens does not masquerade as a lens change.

Update row activation so Enter/Space opens only when the row itself is the target (`event.target === event.currentTarget`). Native buttons/checkboxes retain browser Enter/Space behavior. Window handlers must return for already-handled events, editable elements, and interactive descendants that own the key; allow existing explicit page shortcuts from the appropriate pane. Row DOM focus must update the logical focused ID as well as active pane.

Use the existing checkbox component or native checkbox semantics for selection; expose checked state and visible focus. Add focus-within/focus-visible visibility to touched hover actions. Preserve one logical tab/focus model: do not leave an invisible focused action or make virtualized offscreen rows steal focus. Update page hints only where behavior changed.

**Verify:** row/reader unit tests pass, including bubbled events and focus restoration after mutation. Typecheck/lint pass. Search-filter changes clear stale selection; same-origin reader navigation preserves the existing selection policy.

### 4. Exercise real keyboard-only journeys

Add Playwright cases using the actual router, bridge, and FakeProvider. Create label/saved fixtures through existing bridge calls. For each origin, navigate using keyboard, open a message, switch pane, perform a reversible action, Undo, return, and verify original query/lens/position. Use actual Tab and keyboard Enter/Space for native controls; `fireEvent.keyDown(window)` alone cannot prove focus behavior. Check focus before and after closing dialogs/palette/reader.

**Verify:** build and browser gates pass, including existing accessibility tests. Test reports must show no new serious/critical axe violations in the touched journey. `git diff --check` passes; only approved source/test/generated files changed. Hand Plan 022 the shared context/target resolution entry points.

## Test matrix and done criteria

| Origin or interaction | Required result |
|---|---|
| Inbox old deep link | Existing reader URL works; returns to Inbox |
| Label with encoded name | Actual label identity retained; route action uses actual label |
| Saved lens without reader | Not mistaken for focused thread; no preview navigation on j/k |
| Search with every parameter | Open/next/return preserves full query context |
| Second/later virtual page | Return restores visible row/position without jumping to first page |
| Removed opening row | Deterministic neighbor focus; no selection widening |
| Select/Star Enter and Space | Exactly one intended control action, no row open |
| Tab-focused row plus shortcut | Logical row and DOM focus agree |
| Close reader/palette/dialog | Focus returns to reachable initiating control |
| Unknown/deleted origin | Clear recovery; no mislabeled Inbox substitution |

Done requires all gates to pass with real browser keyboard interaction and stable source context. Record command output and test names. This supersedes the affected navigation/accessibility portions of legacy Plan 009 A12/A13 and C6 only after validation; it does not complete other legacy findings.

## STOP conditions

Stop if the context fix requires changing selection meaning, broadening a search, adding unrelated routes/workspaces, duplicating backend query execution, or changing the action safety policy. Stop after two repeated verification failures, an unexplained fetched-code mismatch, missing test collection, or necessary edits beyond the scope. If a current route already carries origin metadata after prerequisites, reuse it rather than introducing a parallel origin format.
