---
title: Desktop app
description: How the Electron desktop app surfaces the daemon, what's wired today, and where the CLI is still canonical.
---

The mxr desktop app is an Electron front-end that talks to the same
daemon as the TUI and CLI. Everything goes through the HTTP bridge —
no provider credentials live in the renderer, no SQLite handles in
the main process. The daemon is the source of truth.

## What's wired today

The renderer runs the same command palette as the TUI (`Cmd+K`), with
shortcuts auto-generated from the TUI's desktop manifest. The
following surfaces have first-class dialogs:

- **Mailbox** — list view with smart sender display, snippet preview,
  attachment chip showing size, and a relative-date column.
- **Compose** — full editor dialog with frontmatter parsing, contact
  autocomplete, attachment picker, send / save / discard, draft
  resume.
- **Search** — the same lexical / hybrid / semantic backends as the
  TUI, with explain output.
- **Reply Queue** — read-only browser of messages flagged for
  reply-later. Open from the palette: `Cmd+K → Reply Queue`.
- **Snippets** — read-only list of compose snippets with body
  preview. CRUD is still through `mxr snippets` for now.
- **Sender View** — per-sender aggregates (volume, response cadence,
  open commitments) for the focused message's sender.
- **Screener Queue** — interactive triage with `a` / `d` / `f` / `p`
  buttons (allow / deny / feed / paper-trail). Decisions fire
  `Request::SetScreenerDecision` and the queue refreshes after each
  disposition.
- **Thread Summary** — LLM-generated Markdown summary and next steps for
  the focused thread. Renders the model name in the title.
- **Draft Assist** — text-input prompt, streamed result, and a "Copy
  to clipboard" button so the suggestion lands in compose for review.
  Never auto-sends.
- **Diagnostics** — surface for daemon health, sync status, recent
  events / logs, doctor findings (with copy-pasteable remediation
  commands).
- **Rules** and **Accounts** — full CRUD dialogs.

## What's still CLI-canonical

- Crash-safe draft recovery: `mxr drafts recover / resume / discard`.
- Bulk unsubscribe sweeps: `mxr subscriptions --rank --format json`,
  then `mxr unsubscribe --search ... --dry-run`.
- Initial setup: `mxr setup --demo` / `mxr accounts add gmail` /
  `mxr accounts add imap`.

The desktop dialogs deliberately stop short of "rebuild the CLI in
React." If a workflow has a stable JSON output and lives long enough
to be scripted, the CLI is the right surface — the desktop's job is
discoverability and click-through navigation, not to replace
automation.

## How the bridge connects

The renderer reads a base URL + auth token from the bundled `mxr`
binary at startup (or an external one via the bridge-mismatch
fallback). All dialogs hit `/api/v1/mail/...` directly — the same
routes documented in the [HTTP bridge OpenAPI
spec](/api/v1/openapi.json) and exercised by the daemon's web tests.

The TypeScript types in `apps/desktop/src/shared/api.generated.ts`
are produced by `pnpm gen:types` from that OpenAPI spec, so any new
IPC route lands in the desktop with type-safe payloads automatically.

## Keyboard model

The desktop app inherits the TUI's keybinding manifest. That means:

- `Cmd+K` opens the command palette.
- `?` opens the help overlay.
- Provider-agnostic shortcuts (compose / reply / archive / star /
  snooze / etc.) match the TUI.
- Browser dialog shortcuts use platform-native conventions (`Esc` to
  close, click to select); the in-dialog disposition keys (e.g. the
  Screener `a` / `d` / `f` / `p`) are exposed as buttons rather than
  raw key bindings to keep them keyboard-and-mouse safe.

## Logging and crash recovery

The renderer logs to the standard Electron user data directory.
Crashes don't lose draft state — the same `drafts` SQLite table that
backs the TUI compose flow is used here, so a renderer crash with an
unsaved compose still leaves the draft recoverable via the daemon's
startup orphan-recovery loop or via `mxr drafts recover`.
