# 11 — Parity closure (W0–W8)

Closes every gap in [PARITY_MATRIX.md](./PARITY_MATRIX.md) and the "usable baseline" remainders in [STATUS.md](./STATUS.md). The architectural lever is a **shared web action registry** that replaces three independently-hardcoded lists (command palette, global keymap, help dialog).

Phases 3–8 stay "usable baseline" until their workstream PRs land, then flip to "complete". Phase 11 row tracks this overall effort.

## Why this exists

Most "Partial" rows in the parity matrix already have routes/UI — they just aren't palette-addressable. Web parity is non-negotiable per [README.md](./README.md): "Every TUI action must have a CLI equivalent and now a web equivalent."

Today three surfaces hold separate hardcoded lists:

- `apps/web/src/features/command-palette/CommandPalette.tsx:64` (`useMemo` block at lines 129–276)
- `apps/web/src/lib/keymap.ts:29` (chord literals at lines 43–86)
- `apps/web/src/lib/shortcutHints.ts:25` (per-route `shortcutSections()` arrays)

That's where the `g a` keymap collision comes from (keymap → archive, palette label → Analytics). One registry fixes the collision and unblocks ~20 palette gaps trivially.

## Workstreams

| # | Workstream | Depends on | LOC est | Bridge work needed |
|---|---|---|---|---|
| W0 | Shared action registry | — | 1350 (4 PRs) | none |
| W1 | Mailbox/reader command + workflow gaps | W0 | 900 (2 PRs) | apply-label, move, unsubscribe, read-and-archive |
| W2 | Compose closure (autocomplete, unsend, draft-assist UI) | W0 | 1150 (3 PRs) | contacts autocomplete, unsend window |
| W3 | Search + saved-search closure | W0 | 800 (2 PRs) | saved-search update/pin/color/reorder |
| W4 | Semantic controls UI | W0 | 400 (1 PR) | none |
| W5 | Account config + repair UI | W0 | 600 (2 PRs) | refresh-accounts, bug-report |
| W6 | Analytics closure (Wrapped story, drilldowns, share) | — | 600 (1 PR) | none |
| W7 | Rules closure (typed builder, apply-now coverage) | — | 550 (1 PR) | none |
| W8 | Triage + diagnostics polish | W0 | 350 (1 PR) | (see W5 bridge group) |

W0 is the gate. W6 / W7 can ship in parallel with W0 since they don't touch palette/keymap.

## Bridge routes to add first

Bridge work is **separate PRs** from web. Reason: Rust review is a different reviewer pool, OpenAPI must regenerate `apps/web/src/api/generated.ts` cleanly before the consuming web PR compiles. Bundling stalls merges.

Four bridge PRs gate web work:

1. **Bridge labels/move/unsubscribe/read-and-archive** (`crates/web/src/routes_v6.rs`, near `archive_messages` handler). Verify `mail/mutations/labels` and `mail/mutations/move` first — STATUS.md and exploration disagree on their existence. Implement only what's missing.
2. **Bridge contacts autocomplete + unsend window** (near scheduled-sends handlers around line 985). Unsend = "cancel within N seconds of send" semantics, not the same as cancel-scheduled.
3. **Bridge saved-search update/pin/color/reorder** (near `list_saved_searches` line 508 and `run_saved_search` line 534).
4. **Bridge refresh-accounts + bug-report** (near `repair_account_config` line 1909 and `list_events` line 107).

Already exist (no bridge work needed): account repair (`POST /accounts/repair`:1909), draft-assist (`POST /threads/draft-assist`:1346), semantic enable+backfill+profiles, scheduled-sends create/cancel, snoozed wake (`POST /snoozed/{id}/wake`:868), flag-reply-later (`POST /reply-later/{id}`:895), export-search (`POST /export-search`:1624).

Semantic disable: there is no separate route — `semantic_enable()` takes `{enabled: bool}` (line 717). Web "Disable" command calls enable with `false`.

## W0 — Action registry design

### Files to create

```
apps/web/src/lib/actions/
  types.ts          # Action, ActionContext, ActionRunner, ShortcutChord
  when.ts           # composable predicates: onRoute, onPane, withSelection,
                    #   withFocusedThread, firstAccountOnly, and(...)
  registry.ts       # defineAction, ActionRegistry class, getRegistry()
  context.ts        # useActionContext() — derives ctx from router/selection/accounts
  catalog.ts        # canonical Action[] (aggregates feature actions)
  index.ts          # barrel: useVisibleActions, useActionShortcuts,
                    #   useActionShortcutHints
  registry.test.ts
  when.test.tsx
  context.test.tsx

apps/web/src/features/<feature>/actions.ts    # added per-feature in later PRs
```

### Action shape (`types.ts`)

```ts
export type ActionContext = {
  path: string;
  activePane: MailPane;
  selectionCount: number;
  accountCount: number;
  hasFocusedThread: boolean;
  hasFocusedMessage: boolean;
  isFirstAccountOnly: boolean;
};

export type Action = {
  id: string;                  // stable kebab, e.g. "mail.archive"
  label: string;
  description?: string;
  group: ActionGroup;
  icon?: LucideIcon;
  shortcut?: ShortcutChord;    // tinykeys grammar
  paletteOnly?: boolean;
  when?: (ctx: ActionContext) => boolean;
  run: (ctx: ActionContext) => void | Promise<void>;
};
```

### Predicate helpers

`firstAccountOnly()` mirrors the TUI screener constraint (`crates/tui/src/app/mailbox_actions.rs:394` "Screener: open an inbox first so we know which account").

### Consumer wiring

- `CommandPalette.tsx:64` `CommandPaletteMount` reads from `useVisibleActions(ctx)`. Delete the `useMemo` block at lines 129–276 and `commandActionIds` at line 55.
- `keymap.ts:29` `buildGlobalKeymap(nav)` → `buildGlobalKeymap(nav, registry)`. Pulls every non-`paletteOnly` `action.shortcut`. Inline raw chords (alt-palette `Shift+Semicolon`) stay.
- `HelpDialog.tsx:26` reads from `useActionShortcutHints(ctx)`. Delete `shortcutHints.ts:25` `shortcutSections()` in the same PR.
- Runners use `getNavigateRef()` and `useModals.getState()` — same pattern as `keymap.ts:33`. No React hooks inside runners.

### `g a` collision resolution

`keymap.ts` sends `g a` to archive/all-mail; palette labels Analytics as `g a`. **Decision: Analytics moves to `g y`** ("graphs / y-axis"). Other letters considered: `g n` (numbers — ambiguous with "next"), `g v` (visualization — collides with Visual mode plans), removing the shortcut entirely (loses parity with TUI). `g y` is unused and mnemonic enough. Revisit-able in PR #3 review.

### Migration sequence (one consumer per PR)

1. PR #1 — registry + helpers + tests, **no consumers**.
2. PR #2 — migrate HelpDialog, delete `shortcutHints.ts`.
3. PR #3 — migrate keymap, fix `g a` collision.
4. PR #4 — migrate CommandPalette, delete `commandActionIds` + the `useMemo` block.

Each migration deletes the old hardcoded source in the same PR so dual-source drift cannot happen.

## TDD methodology

### Red-phase per workstream

Pattern follows `apps/web/src/features/command-palette/CommandPalette.test.tsx`: Vitest + RTL + jsdom + `vi.hoisted` mocks + `QueryClientProvider` wrapper + `waitFor` assertions.

Tests are **behavioral, not introspective**. Examples:

- W0: "palette in route `/m/inbox` with 0 selection does NOT show 'Apply label'" — asserts visibility *rule*, not registry membership.
- W1: "applying label `Receipts` to 3 selected messages updates optimistic cache and calls `applyLabel(['m1','m2','m3'], 'Receipts')`; on server error restores prior cache and shows toast." Fails because `MailAction` (`useOptimisticMailMutation.ts:17`) doesn't include `"label"`. Widen the type, add cache projection.
- W2: "typing 'al' in To shows top contacts from `/contacts/autocomplete`, ArrowDown selects, Enter commits as chip."
- W3: "clicking pin icon calls `pinSavedSearch(id)` and reorders optimistically."
- W5: render `AccountRow` in `auth_invalid` state → expect Repair button calling `repairAccount(id)`.
- W6: `WrappedStory` keyboard `j/k` advance/back; share button writes blob to clipboard.
- W7: `RuleBuilder` typed-row interaction asserts emitted DSL, not JSX shape.
- W8: with 2 accounts, Screener shows "first-account-only" empty state matching TUI.

### Mutation tests location

**Per-feature folder, colocated.** Example: `apps/web/src/features/mailbox/useOptimisticMailMutation.test.tsx`. Each PR owns one feature's mutations and its cache-projection logic.

### MSW vs Playwright matrix

- MSW (vitest, jsdom): optimistic projection, error rollback, palette filtering by `when`, autocomplete debounce, draft-assist streaming chunks, saved-search pin reorder.
- Playwright real-daemon (`apps/web/e2e/`, alongside existing `mutations.spec.ts`): label apply across reload, unsend within window, screener real-account flow, semantic enable round-trip. New specs: `e2e/labels.spec.ts`, `e2e/unsend.spec.ts`, `e2e/screener.spec.ts`, `e2e/saved-search-pin.spec.ts`.

### test-quality-rubric gate

Run the `test-quality-rubric` skill against the diff of `*.test.{ts,tsx}` on PRs **#1, #4, #6, #8, #11, #15, #16** — the test-LOC-heavy ones with the most temptation toward sycophantic / tautological / implementation-mirroring tests.

Disallowed test patterns:
- registry-shape snapshots (mirror implementation)
- "renders without crashing" (sycophantic)
- mutation tests that assert only the API call (must also assert optimistic UI change, rollback on rejection, and toast text — three or it's not real)

## PR sequence (17 PRs, ~6700 LOC of meaningful change)

| # | PR | Stack | LOC | Unblocks | Rubric gate |
|---|---|---|---|---|---|
| 0 | Docs: write `11-parity-closure.md`, update STATUS / README | docs | 200 | — | no |
| 1 | W0: registry types + helpers + tests, no consumers | web | 350 | 2/3/4 | **yes** |
| 2 | W0: migrate HelpDialog, delete `shortcutHints.ts` | web | 250 | — | no |
| 3 | W0: migrate keymap, `g a`→`g y` for analytics | web | 300 | — | no |
| 4 | W0: migrate CommandPalette, delete `commandActionIds` | web | 450 | W1/W3/W4/W8 palette items | **yes** |
| 5 | Bridge: labels + move + unsubscribe + read-and-archive | rust | 400 | W1 |  |
| 6 | W1: optimistic label/move/unsubscribe via registry | web | 500 | — | **yes** |
| 7 | Bridge: contacts autocomplete + unsend window | rust | 250 | W2 |  |
| 8 | W2: contact autocomplete + unsend toast | web | 500 | — | **yes** |
| 9 | W2: draft-assist UI panel | web | 400 | — | no |
| 10 | Bridge: saved-search update/pin/color/reorder | rust | 300 | W3 |  |
| 11 | W3: saved-search management UI + scope picker + j/k nav | web | 500 | — | **yes** |
| 12 | W4: semantic controls panel | web | 400 | — | no |
| 13 | Bridge: refresh-accounts + bug-report | rust | 200 | W5/W8 |  |
| 14 | W5: account repair + refresh + config edit form | web | 400 | — | no |
| 15 | W6: Wrapped story + drilldowns + share-as-image + stale window + contacts mode | web | 600 | — | **yes** |
| 16 | W7: typed RuleBuilder rows + apply-now for label/move/non-mail | web | 550 | — | **yes** |
| 17 | W8: screener first-account gating, sender route, diagnostics nav, keybindings page from registry | web | 350 | — | no |

Bridge PRs (5, 7, 10, 13) include OpenAPI regen step:

```bash
cargo run --example dump_openapi_spec -p mxr-web > spec.json
cd apps/web && npm run gen:types
```

The regenerated `apps/web/src/api/generated.ts` is committed.

## Reused utilities (do not reinvent)

- `useOptimisticMailMutation` (`apps/web/src/features/mailbox/useOptimisticMailMutation.ts:110`) — extend the `MailAction` union and the `mapMailboxRows` projection rather than building a parallel mutation hook for labels/move.
- `useModals` store and `getNavigateRef()` — the runner pattern in `keymap.ts:33`; reuse for action registry runners so we don't add a new dispatch primitive.
- `useKeybindings` (`apps/web/src/hooks/useKeybindings.ts:9`, wraps tinykeys) — feed the registry-derived map straight into it.
- `CommandPalette.test.tsx` mock pattern — every new test in W0–W8 follows the `vi.hoisted` + `QueryClientProvider` + `waitFor` shape from this file.
- Existing Playwright harness in `apps/web/e2e/` (real-daemon, fake-provider) — new specs sit beside `mutations.spec.ts`.

## Verification

End-to-end after each PR:

```bash
# from apps/web/
npm run typecheck && npm run lint && npm run test

# bridge PRs only
cargo check -p mxr-web
cargo test -p mxr-web
cargo run --example dump_openapi_spec -p mxr-web > /tmp/spec.json
cd apps/web && npm run gen:types  # commit the diff in apps/web/src/api/generated.ts

# real-daemon smoke (any web PR)
cargo build --features web-ui
./target/debug/mxr daemon --foreground &
mxr web --no-open
# manually drive the new feature in a browser per CLAUDE.md mandate
```

Per-PR Playwright (CI):

```bash
cd apps/web && npm run e2e -- --grep="<feature>"
```

Final acceptance for the whole effort:

1. `docs/web-app/PARITY_MATRIX.md` regenerated with every row marked **Covered** (or explicitly "out of scope" with reason). Run a final pass with `code-review` and `test-quality-rubric` skills before closing PR #17.
2. `docs/web-app/STATUS.md` Phase 3–8 rows flip to "complete" once their respective workstream PRs all land. Phase 11 row added and flipped to "complete" after PR #17.
3. Real-daemon smoke against `provider-fake`: every TUI action mentioned in PARITY_MATRIX.md works from web palette, keymap, OR direct UI. No "Partial" remaining.

## Risks

- **Optimistic invariants when adding label/move**: `mapMailboxRows` (`useOptimisticMailMutation.ts:29–53`) currently handles only star/read/destructive. Adding `"label"` requires a branch that mutates `row.labels` without removing the row; `"move"` removes from current view AND inserts into target if cached. **Mitigation**: treat move/label-as-folder as destructive in the *current* view, then `invalidateQueries` in `onSettled` (already happens line 156–159). Stale unread counts in shell are already invalidated via `shellKey` line 159. Safe.
- **Help-dialog migration window**: avoided by deleting `shortcutHints.ts` in the same PR that wires registry. No dual sources possible.
- **Bundle size**: catalog imports every feature's runners eagerly. ~80 actions × ~200 bytes ≈ 16 KB pre-gzip. Negligible vs current 53.86 kB gzipped main chunk (per STATUS.md). If this grows past 5 % of the chunk, lazy-load runners with dynamic `import()` keyed by `action.id` while keeping `Action` metadata eager so palette filtering stays sync.
- **Bridge / web type-drift between PRs**: bridge PR lands → typegen runs → web PR consumes. If a bridge PR merges before the next web PR rebases, `apps/web/src/api/generated.ts` may become inconsistent. **Mitigation**: web PR rebases on main before merge; CI runs `npm run gen:types` and fails on diff.
- **`g a` muscle memory**: existing users may have learned `g a` as Analytics from the palette. PR #3 includes a one-time toast on first `/m/archive` open: "Analytics moved to `g y`. Press `?` for shortcuts." Toast dismisses on click and is suppressed via `localStorage` flag.
- **TUI parity is a moving target**: TUI continues to add actions. After PR #17, treat any new `crates/tui/src/action.rs` enum addition as a defect against the registry. Capture as an issue, not a refactor.

## Out-of-scope explicitly

These are not parity work and are not addressed:

- Mobile (phone) build — locked decision in `00-overview.md`.
- Per-account UI prefs — locked decision (one global set).
- Native desktop wrapper — explicitly rejected per `docs/marketing/no-native-desktop-app.md`.
- Provider-direct calls from the SPA — architectural boundary per `00-overview.md`.
