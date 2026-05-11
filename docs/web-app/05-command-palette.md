# Phase 5 — Command Palette (Cmd-K)

Goal: a single overlay built on `cmdk` that fuzzy-matches across navigation, mutations, threads, labels, accounts, settings, saved searches. Context-aware — when a thread is open, thread actions surface first.

## Deliverables

1. `Cmd-K` (and `:`) opens the palette from anywhere.
2. **Scopes / tabs** (Tab cycles): Actions / Mail / Labels / Settings.
3. **Actions scope**: every page-level action with its keybinding chip and ID.
4. **Mail scope**: fuzzy match over recent threads + envelopes (last N pages of inbox + recent search results).
5. **Labels scope**: match over labels, opens `/m/label/$name`.
6. **Settings scope**: match over settings sections.
7. **Context-aware**: when a thread is open, prepend thread-specific actions (Reply, Forward, Archive this, Trash this, Snooze, Label, etc.) at the top.
8. `Esc` closes; arrow keys navigate; Enter executes.
9. Visible keybinding hints next to each action.
10. **Help cheat-sheet** (`?` popover) is a separate but related surface — lists keybindings only, no fuzzy match.

## Bridge endpoints used

Most palette items dispatch to existing endpoints (we don't add new ones). For "recent threads" backing the Mail scope:

- `GET /api/v1/mail/mailbox?label=inbox&limit=200` — primed once on palette open, cached for the session.

## Files

```
src/features/command-palette/
  CommandPaletteRoute.tsx          # mounted globally; not a TSR route
  CommandPalette.tsx               # cmdk wrapper
  CommandActions.tsx               # Actions scope contents
  CommandMail.tsx                  # Mail scope contents
  CommandLabels.tsx
  CommandSettings.tsx
  CommandThreadContext.tsx         # contextual actions when a thread is open
  CommandPaletteState.ts           # zustand: open, scope, query, last-thread-context
  actions.ts                       # the action registry — id, label, keybinding, run()
src/components/
  KeyChip.tsx                      # renders ⌘K-style chip
```

## Action registry

Single registry of actions, each with:
- `id` (stable)
- `label` (display)
- `description` (one line)
- `keybinding` (string for tinykeys + display)
- `scope` ("global" | "thread" | "mailbox" | "compose")
- `run(ctx: AppContext)` (async function)
- `available(ctx)` (predicate — hide entry when irrelevant)

The registry feeds:
1. The command palette.
2. The `tinykeys` map (auto-installed on mount).
3. The `?` help cheat-sheet (grouped by scope).

This is the single source of truth for keyboarded actions.

## Verification

1. `Cmd-K` from anywhere → palette opens. Type "arch" → "Archive (e)" surfaces.
2. With a thread open, `Cmd-K` shows thread actions at the top.
3. Tab → Mail scope. Type sender name → recent threads from that sender match.
4. Enter on a thread → palette closes, navigates to `/m/inbox/$threadId`.
5. Tab → Labels scope. Type label name → Enter → opens `/m/label/$name`.
6. Tab → Settings scope. Type "theme" → Enter → opens `/settings/theme`.
7. `?` opens cheat-sheet popover. Lists all keybindings grouped.

## Decisions

- 2026-05-10 — Use `cmdk` (vercel/cmdk) for the primitive. shadcn ships a pre-styled `Command` component built on it; copy that into `components/ui/command.tsx`.
- 2026-05-10 — Registry of actions is the canonical source. Don't duplicate keybindings in scattered components.
