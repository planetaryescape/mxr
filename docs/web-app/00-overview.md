# 00 — Overview

Always read this file first when picking up work on `apps/web/`.

## What we're building

A Vite + React 19 SPA at `apps/web/` that talks to the mxr daemon's existing HTTP + WebSocket bridge. Full feature parity with the TUI and CLI. Fresh design, web-native interaction model — not a TUI port. The SPA is embedded into the daemon binary and launched via a new `mxr web` CLI subcommand.

## Why now

The bridge has been stable since v0.5; the daemon is the system, clients are additive. The user already has an Electron desktop app at `apps/desktop/` but explicitly does **not** want to copy or share UI from it. The web app is its own surface.

## Locked decisions (do not re-debate)

- **Path**: `apps/web/` (sibling to `apps/desktop/` and `site/`).
- **Stack**: React 19 + TypeScript (strict) + Vite 7 + Tailwind 4 (`@tailwindcss/vite`).
- **Components**: shadcn/ui (Radix + Tailwind, copy-paste into the project) plus shadcn-recommended deps (cmdk, sonner).
- **Routing**: TanStack Router (typed, file-based, auto-code-splitting).
- **Server cache**: TanStack Query v5.
- **UI state**: Zustand.
- **API client**: `openapi-fetch` over types from `openapi-typescript`.
- **Compose default**: CodeMirror 6 + `@replit/codemirror-vim` (Markdown source, full Vim surface).
- **Compose alt**: Tiptap (rich-text WYSIWYG), code-split lazy.
- **Virtualization**: TanStack Virtual.
- **Keybindings (page-level)**: `tinykeys`. Compose vim is handled by the editor itself.
- **Toasts**: Sonner.
- **Charts**: Recharts.
- **Forms**: react-hook-form + zod.
- **Sanitizer**: DOMPurify (inbound HTML mail).
- **Lint/format**: oxlint + oxfmt.
- **Testing**: Vitest + Testing Library + jsdom; MSW for bridge mocking; Playwright for e2e against a real daemon backed by `provider-fake`.
- **Distribution**: SPA `dist/` embedded into the daemon binary via `include_dir!` (Rust crate). Bridge serves it at `/`. `MXR_WEB_DEV_PROXY=http://localhost:5173` falls through to Vite during dev.
- **Launch command**: `mxr web` opens browser to `http://127.0.0.1:42829`. Flags: `--port`, `--no-open`, `--print-url`, `--strict-port`, `--remote-host`. Token persisted at `~/.config/mxr/bridge-token` (mode 0600). Default port retries on `EADDRINUSE` up to 32 attempts (opt-out with `--strict-port`); the bound port lands in `<config_dir>/bridge-port`.
- **Auth flow**: on the same machine the SPA self-authenticates by calling `GET /api/v1/auth/local-token` — bridge returns the token only to loopback peers, gated by `[bridge].auto_local_token`. On 401 the SPA retries the handshake automatically; only when both that and any cached `localStorage` token fail does it fall back to `/settings/token`. URL fragment `#token=...` still works for remote-host launches.
- **WebSocket auth**: `Sec-WebSocket-Protocol: bearer, <token>` (already supported by the bridge).
- **Strict CSP** on the embedded SPA HTML response.
- **Responsive**: desktop + tablet (≥768 px). Sidebar collapses to icons at tablet. No phone build.
- **Browser support**: evergreens — Chrome/Edge/Firefox last 2, Safari 17+. Vite target `es2022`.
- **UI prefs scope**: one global set across all accounts (theme, density, keybindings, notifications). No per-account override.
- **Notifications**: global toggle + first-class VIP allowlist (email or domain pattern), stored on the daemon so they sync across clients.

## Architecture

```
Browser (apps/web SPA)  ──HTTP+WS──>  bridge (crates/web)  ──Unix socket──>  daemon
       ▲
       └ embedded via include_dir!() into the daemon binary
```

The SPA never calls providers directly. It speaks only to the bridge. The bridge is loopback-only by default with a host-header allowlist defending against DNS rebinding. All mutations are optimistic with rollback on error and a 60-second undo affordance for destructive ones.

## Information architecture

URL is canonical state wherever possible — every TUI screen is deep-linkable.

| URL | Purpose |
|---|---|
| `/` | Redirect to `/m/inbox` |
| `/m/$mailbox` | System mailbox: `inbox`, `starred`, `snoozed`, `drafts`, `sent`, `archive`, `spam`, `trash`, `reply-later` |
| `/m/label/$name` | Label view |
| `/m/saved/$slug` | Saved-search lens |
| `/m/$mailbox/$threadId` | Thread reading pane |
| `/search` | Search results page (`?q=`, `?mode=lex|sem|hybrid`, `?account=`, `?sort=`) |
| `/compose/new`, `/compose/$draftId` | Compose (full page) — Vim editor by default |
| `/screener` | Screener triage page |
| `/analytics`, `/analytics/{storage,stale,contacts,response-time,subscriptions,wrapped}` | Analytics dashboards |
| `/rules`, `/rules/new`, `/rules/$id` | Rules editor with first-class dry-run |
| `/accounts`, `/accounts/new`, `/accounts/$key` | Account management + OAuth flow |
| `/onboarding` | First-run wizard |
| `/settings/{theme,density,keybindings,notifications,snippets,token,about}` | Settings sub-pages |
| `/diagnostics` | Daemon health, logs, sync status, doctor |
| `/dev` | Component sandbox (dev build only) |

## Layout shell

- **Left sidebar (264 px, collapsible to 56 px on tablet)**: account avatar/switcher, primary nav (Mail / Search / Analytics / Rules / Accounts / Screener / Diagnostics), then **Lenses** section pinning system mailboxes + saved searches with live unread counts driven by the WebSocket. Bottom slot: storage gauge + sync/connection pill + theme picker.
- **Top action bar (44 px)**: breadcrumb, central search input that morphs into the command palette on `Cmd-K` or `:`, density toggle, compose CTA.
- **Main pane**: primary content. Density tokens drive row height (40 / 52 / 68 px).
- **Right contextual rail (380 px, slide-in, ESC closes)**: Sender Profile, Thread Summary, Attachments, URL list, Snippets browser, Saved-Search form, analytics drill-downs.
- **Status bar (28 px, bottom)**: sync %, semantic reindex %, daemon connection state, pending undo countdown, queued outbound mail.

## Web-native reclassification (TUI → web)

| TUI surface | Web treatment |
|---|---|
| Help, Snippets browser, Snooze, Label Picker, Compose Picker, More-actions | **Popover** |
| Bulk Confirm (>20 messages), Send Confirm, Unsubscribe, Delete Account | **Modal** (blocking) |
| Sender Profile, Thread Summary, URL list, Attachments, Reply Queue, Saved-Search form | **Right-rail side panel** |
| Analytics filters | **Inline sticky header** |
| Rules editor, Onboarding, Screener, Account setup | **Page** |
| Command palette | **`Cmd-K` overlay** with cmdk |
| Errors | **Toast** (Sonner) + optional details popover |

## Folder layout

```
apps/web/
  src/
    api/
      client.ts                 # openapi-fetch instance + auth middleware
      generated.ts              # openapi-typescript output (committed)
      events.ts                 # DaemonEvent type re-exports
    routes/                     # TanStack Router file-based routes
    features/
      mailbox/                  # list, row, virtualizer, selection, bulk bar
      thread/                   # reader, message renderer, sanitizer, attachments
      compose/
        codemirror/             # CodeMirror 6 + vim editor (default)
        tiptap/                 # Tiptap rich-text editor (alt, lazy)
      search/                   # input, results, syntax chips, saved-search form
      analytics/                # shared chrome + per-dashboard pages
      rules/                    # builder, dry-run pane, history
      accounts/                 # list, detail, OAuth flow
      onboarding/
      screener/
      settings/
      command-palette/
      diagnostics/
    components/
      ui/                       # shadcn-generated (button, dialog, dropdown, …)
      AppShell.tsx, Sidebar.tsx, Topbar.tsx, RightRail.tsx, StatusBar.tsx, …
    hooks/
      useDaemonEvents.ts, useOptimisticMailMutation.ts, useKeybindings.ts, useConnectionStatus.ts, useBridgeToken.ts
    lib/
      queryClient.ts, ws.ts, keymap.ts, sanitizeHtml.ts, tokenStorage.ts, utils.ts
    state/
      selectionStore.ts, modalStore.ts, connectionStore.ts, uiPrefsStore.ts
    styles/
      tokens.css                # @theme — fresh palette
      base.css
      app.css
    test/
      msw/                      # handlers, server
      setup.ts
  e2e/                          # Playwright specs + global setup
  scripts/
    gen-bridge-types.mjs
  index.html
  vite.config.ts
  tsconfig.json
  package.json
```

## Existing repo touchpoints

- `crates/daemon/src/cli/mod.rs:408-416` — `Command::Web` (needs `--no-open`, `--remote-host` added).
- `crates/daemon/src/commands/web.rs` — current implementation; rewrite to persist token, open browser, support remote-host.
- `crates/web/src/lib.rs` — bridge router; add SPA serving + CSP behind `web-ui` cargo feature.
- `crates/web/Cargo.toml` — add optional `include_dir` dep behind `web-ui` feature.
- `Cargo.toml` (root) — surface `web-ui` feature so `mxr` can build with it.
- `crates/config/src/lib.rs` — add `bridge_token_path()` helper.
- `crates/daemon/tests/snapshots/cli_help__cli_help_*.snap` — accept new help output.
- `.github/workflows/release.yml` — build the SPA before the Rust binary so `include_dir!` sees `apps/web/dist/`.
- `docs/blueprint/16-addendum.md` and `docs/guides/http-bridge.md` — document `mxr web` and the embedded SPA.
- `README.md` — quickstart mentions `mxr web`.

## Conventions

- Strict TypeScript. No `any`. `unknown` at boundaries.
- Server data → TanStack Query. UI state → Zustand. URL state → TanStack Router search params (zod-validated).
- Imports: `@/...` alias maps to `apps/web/src/...`.
- Keep components small and dumb. Containers wire data.
- Optimistic mutations all funnel through `useOptimisticMailMutation`.
- WS events all funnel through `<DaemonEventsProvider>` (a single mount, dispatches to query cache).
- HTML mail bodies always pass through `sanitizeHtml.ts` (DOMPurify wrapper) before render. Inline images proxied via the daemon. No remote-content load until the user opts in per-thread.

## Design system summary

- Default theme `midnight` (data-theme="dark" or unset).
- Type scale 11/12/13/14/15/17/20/24/32/48 px.
- Body default 13 px, meta 11 px mono with tabular nums.
- Density modes: compact 40 px / regular 52 px / comfortable 68 px.
- Radii 4/6/10/14 px.
- Motion: 120/180/280 ms with `cubic-bezier(0.2, 0.8, 0.2, 1)` ease.
- Shadcn HSL-channel CSS variable convention so any shadcn component drops in.

## Verification mantra

After every phase, run:

```bash
# from apps/web/
npm run typecheck && npm run lint && npm run test

# from repo root
cargo check -p mxr-web
cargo build --features web-ui   # once Phase 1 lands
```

End-to-end smoke: `cargo build --release && ./target/release/mxr daemon --foreground` in one terminal, `mxr web` in another. Check the relevant feature actually works in the browser against a real daemon. Per CLAUDE.md: "tests passing means nothing if the real system is broken."
