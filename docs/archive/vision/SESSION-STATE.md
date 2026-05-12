# mxr Delight Plan — Session State

> Snapshot of where the parity work stands. Updated as the parity push lands.
> Read [HANDOFF-NOTES.md](./HANDOFF-NOTES.md) first for context.

## Parity matrix

For each delight-plan feature, a ✅/⏳/◯ in each surface column. ✅ = fully wired; ⏳ = partial; ◯ = not started; n/a = doesn't apply to this surface.

| Feature | Daemon/Store | CLI | TUI | HTTP bridge | Desktop |
|---------|:------------:|:---:|:---:|:-----------:|:-------:|
| 1.1 Optimistic mutation rollback | ✅ | n/a | ✅ | n/a | ◯ |
| 1.2 Cmd+K palette ranking + recents | n/a | n/a | ✅ | n/a | ✅ (palette wired) |
| 1.3 Inbox row formatters | n/a | n/a | ✅ | n/a | ✅ (smart sender + chip) |
| 1.4 Type-ahead search debounce | ✅ | n/a | ✅ | n/a | ◯ |
| 1.5 Saved-search keyboard nav | n/a | ✅ | ✅ | n/a | ◯ |
| 2.1 Reply-later | ✅ | ✅ | ✅ (b mark + queue modal) | ✅ | ✅ (queue dialog) |
| 2.2 Custom-time snooze | ✅ | ✅ | ✅ (Custom… modal entry) | n/a (uses snooze) | ◯ |
| 2.3 Auto-reminders | ✅ | ✅ | n/a | ✅ | ◯ |
| 2.4 Send Later | ✅ | ✅ | n/a | ✅ | ◯ |
| 2.5 Screener | ✅ | ✅ | ✅ (queue modal + a/d/f/p) | ✅ | ✅ (queue + dispose) |
| 2.6 Bulk unsubscribe | ✅ | ✅ (existing) | ✅ (existing) | ✅ (existing) | n/a |
| 3.1 Snippets | ✅ | ✅ | ✅ (browser modal, read-only) | ✅ | ✅ (browser dialog) |
| 3.2 Sender view | ✅ | ✅ | ✅ (profile modal) | ✅ | ✅ (profile dialog) |
| 3.3 LLM provider trait | ✅ | n/a | n/a | n/a | n/a |
| 3.4 Thread summarize | ✅ | ✅ | ✅ (summary modal) | ✅ | ✅ (summary dialog) |
| 3.5 Draft assist | ✅ | ✅ | n/a | ✅ | ✅ (assist dialog) |
| 4.1 Crash-safe drafts | ✅ + heartbeat | ✅ (recover/resume/discard) | ◯ (CLI-led) | ✅ (ListOrphaned + Reset) | ◯ (CLI-led) |
| 4.2 Doctor 2.0 findings | ✅ | ✅ | ✅ (Status pane) | ✅ (in DoctorReport) | ◯ |
| 4.3 Setup wizard demo | ✅ | ✅ | ✅ (welcome modal w/ d/g/i shortcuts) | n/a | ◯ |

**Legend update**: TUI entries marked ✅ on 2026-05-08 use the modal-browser pattern (Snippets, Sender View, Screener Queue, Reply Queue) — read-only viewers that surface daemon data with the same key conventions across modals (Esc close, j/k navigate). Editing/state changes flow through the CLI; this satisfies the discoverability ask without rebuilding text editors in-TUI. Full-screen page rebuilds (Screen::SenderProfile etc.) are deferred as a separate polish pass when richer rendering (charts, sparklines) is wanted.

## Non-feature outstanding items

- **cli_journey flake**: 5 daemon-startup tests (`cli_journey_archive_then_undo_restores_inbox` and friends). Failure mode: `Starting daemon... failed.` despite the daemon log showing `Daemon listening`. Investigation path in HANDOFF-NOTES.md.
- ~~**Live draft heartbeat**: `touch_draft_heartbeat` exists but isn't called from the live send pipeline.~~ ✅ Wired in `send_stored_draft` after CAS-to-Sending (2026-05-08).
- ~~**Recent-actions persistence**: command palette recents are in-memory.~~ ✅ Persisted via `recent_action_labels` in `tui-state.json`; `restore_recents_from_labels` re-hydrates on startup; `pending_local_state_save` flag triggers async save on confirm (2026-05-08).
- ~~**hint_bar slim**: still shows everything.~~ ✅ Capped at `HINT_BAR_MAX_HINTS = 5`; per-context lists pruned to top-5 by user-task primacy (2026-05-08).
- ~~**Render integration: subject snippet + attachment chip in `build_row`.**~~ ✅ Subject cell uses `format_subject_line`; attach cell uses `format_attachment_chip`; attachment column widened to 8 chars (2026-05-08).
- ~~**Doctor findings rendering.**~~ ✅ Status pane renders "Findings: N issue(s)" + per-finding glyph/category/message + indented remediation (2026-05-08).
- ~~**TUI Snippets modal.**~~ ✅ Read-only browser at `Action::OpenSnippets` (2026-05-08).
- ~~**TUI Sender View modal.**~~ ✅ Aggregates modal at `Action::OpenSenderView` (2026-05-08).
- ~~**TUI Screener Queue modal.**~~ ✅ Triage modal with a/d/f/p disposition keys (2026-05-08).
- ~~**TUI Reply Queue modal.**~~ ✅ Browser modal at `Action::OpenReplyQueue` (2026-05-08).
- **OpenAPI route audit**: confirm whether the bridge auto-generates routes from IPC types or each route is hand-written. Determines whether parity is "regenerate" or "write 15 routes."
- ~~**Desktop UI surfaces** for the 13 new bridge routes.~~ ✅ Built `apps/desktop/src/renderer/dialogs/BrowserDialogs.tsx` — Snippets / Reply Queue / Sender View / Screener (with `a`/`d`/`f`/`p` disposition) / Thread Summary / Draft Assist (with copy-to-clipboard). Wired through stable palette action keys. Each dialog fetches via `/api/v1/mail/...` with empty/error/loading states (2026-05-08).
- ~~**TUI custom-time snooze.**~~ ✅ Snooze modal now has a `Custom…` row that opens a text prompt parsed by the same `parse_relative_time` grammar as `mxr snooze --until` (2026-05-08).
- ~~**TUI summarize-thread invocation.**~~ ✅ `Action::SummarizeCurrentThread` opens a real `ThreadSummary` modal that fetches via `Request::SummarizeThread`. Loading / error / disabled states all surface inline (2026-05-08).
- ~~**Desktop summarize + draft-assist surfaces.**~~ ✅ Both are now self-contained dialogs in `BrowserDialogs.tsx`. Summary fetches on open; Draft Assist takes a typed instruction, generates, and exposes a copy button. Never auto-sends (2026-05-08).
- ~~**Crash-safe drafts CLI hooks.**~~ ✅ `mxr drafts recover` lists orphans, `mxr drafts resume <id>` force-resets to `'draft'`, `mxr drafts discard <id>` deletes. Backed by `Request::ListOrphanedDrafts` + `Request::ResetOrphanedDraft` (2026-05-08).
- ~~**Desktop inbox row formatters.**~~ ✅ Smart sender display (display-name → email local-part → email → placeholder) + attachment chip with size readout matching the TUI's `format_attachment_chip`. Helpers in `MailRow.formatters.ts` with isolated unit tests (2026-05-08).
- ~~**TUI setup-wizard onboarding.**~~ ✅ Welcome modal now lists three setup paths with shortcuts: `d` (`mxr setup --demo`), `g` (Gmail form), `i` (IMAP form). `Enter` opens the new-account form, `Esc` dismisses (2026-05-08).
- ~~**Docs site coverage**~~ ✅ `cli.md` covers `drafts recover/resume/discard`, custom-snooze TUI entry, and TUI/desktop access for summarize+draft-assist. `keybindings.md` documents all the new modal key conventions. `tui.md` lists the new modal/overlay surfaces. New `guides/crash-safe-drafts.md` and `guides/desktop-app.md` registered in `astro.config.mjs`. Site builds clean (37 pages) (2026-05-08).

## Outstanding work plan (this session)

1. **Investigate + fix cli_journey flake.** Likely fix in `run_startup_maintenance` — make it spawn rather than block.
2. **Update docs site (`site/src/content/docs/`)** with new CLI commands + new reference pages.
3. **HTTP bridge parity** — audit how routes are defined; add the missing IPC routes.
4. **TUI parity** — add Action variants + handlers + keybindings + palette entries for the new features. Don't rebuild every UI; use the command palette as the primary surface.
5. **Desktop app TS regeneration** — confirm the codegen path; regen if straightforward.

Each item has its own section below as I work on it.

## 1. cli_journey flake — investigation + fix

(Filled in as the work progresses.)

## 2. Docs site updates

CLI reference (`site/src/content/docs/reference/cli.md`) needs new sections:

- `mxr replies` — manage reply-later queue
- `mxr remind <id>` — set/cancel auto-reminders
- `mxr send <id> --at` and `mxr unsend <id>` — Send Later
- `mxr snippets` — manage compose snippets
- `mxr sender <addr>` — relationship aggregates
- `mxr screener` — sender consent triage
- `mxr setup --demo` — first-run helper
- `mxr summarize <id>` and `mxr draft-assist <id>` — LLM features

New guide pages:

- `guides/triage-flow.md` — reply-later, screener, custom snooze
- `guides/automated-followups.md` — auto-reminders, send-later
- `guides/llm-features.md` — config + Ollama / LM Studio / OpenAI examples + summarize / draft-assist

Existing pages to update:

- `guides/compose.md` — note snippets
- `guides/why-mxr.md` — refresh value props
- `reference/keybindings.md` — `b` for reply-later, `g 0..9` for saved-search nav
- `reference/config.md` — `[llm]` section docs
- `reference/cli.md` — add the 9 new commands

## 3. HTTP bridge parity

(Filled in as we audit `crates/web/src/routes/`.)

## 4. TUI parity

(Filled in. Strategy: minimal-viable Action+keybinding+palette per feature; full-screen pages where the data warrants.)

## 5. Desktop app

(Filled in after surveying `apps/desktop/`.)
