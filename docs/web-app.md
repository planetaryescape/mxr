# mxr web app

The mxr web app is a Vite + React 19 SPA at `apps/web/` that talks to the mxr daemon's HTTP + WebSocket bridge. It's distributed as part of the daemon binary (embedded via `include_dir!`) and launched via the `mxr web` CLI subcommand. Feature parity with the TUI and CLI is non-negotiable.

This doc captures the institutional knowledge behind the implementation — the **why** behind decisions, gotchas the implementers ran into, and constraints future maintainers should know about. For "what" the code does, read the code.

## Why this exists

The bridge has been stable since v0.5; the daemon is the system, clients are additive. The web app is the installable GUI surface for users who want a graphical front-end without leaving the mxr ecosystem. We explicitly do **not** ship a native desktop wrapper (see `docs/marketing/no-native-desktop-app.md`) — the embedded SPA served by the daemon is the GUI story.

## Architecture

```
Browser (apps/web SPA)  ──HTTP+WS──>  bridge (crates/web)  ──Unix socket──>  daemon
       ▲
       └ embedded via include_dir!() into the daemon binary
```

- The SPA never calls providers directly. It speaks only to the bridge.
- The bridge is loopback-only by default with a host-header allowlist defending against DNS rebinding.
- Mutations are optimistic with rollback on error and a 60-second undo affordance for destructive ones.
- Every TUI action has a CLI equivalent and now a web equivalent. Web parity is enforced by the shared action registry (see below).

## Locked decisions (do not relitigate)

These were debated once and locked. Stack:

| Concern | Choice | Rationale |
|---|---|---|
| Path | `apps/web/` (sibling to `site/`) | Keeps web app outside Rust crate tree |
| Framework | React 19 + TypeScript strict + Vite 7 | Industry-standard; Vite for fast dev |
| Styling | Tailwind 4 (`@tailwindcss/vite`) | Aligns with shadcn |
| Components | shadcn/ui (Radix + Tailwind, copy-paste) | Owned components, theming via CSS vars |
| Routing | TanStack Router (typed, file-based, auto-codesplit) | Per-route chunks for free |
| Server cache | TanStack Query v5 | Realtime invalidation via WS events |
| UI state | Zustand | Tiny, selector-based |
| API client | `openapi-fetch` over `openapi-typescript` | Types regenerated from live bridge spec |
| Compose default | CodeMirror 6 + `@replit/codemirror-vim` | Full vim surface, Markdown source |
| Compose alt | Tiptap (rich-text WYSIWYG, lazy) | Opt-in; not in default chunk |
| Virtualization | TanStack Virtual | Mailbox lists ~100 DOM nodes max |
| Page-level keybindings | `tinykeys` | Compose vim is handled by the editor itself |
| Toasts | Sonner | Theme-aware |
| Charts | Recharts | Familiar, themeable |
| Forms | react-hook-form + zod | Discriminated unions for rule schema |
| HTML sanitizer | DOMPurify | + custom tracker-pixel stripping |
| Lint/format | oxlint + oxfmt | Fast, project-wide standard |
| Testing | Vitest + Testing Library + jsdom; MSW; Playwright (real daemon, FakeProvider) | Per CLAUDE.md mandate: don't trust unit tests alone |
| Distribution | `dist/` embedded in daemon binary via `include_dir!()` | One artifact to ship |
| Responsive | Desktop + tablet (≥768 px) | No phone build — explicit |
| Browser support | Evergreens (Chrome/Edge/Firefox last 2, Safari 17+); Vite target `es2022` | |
| UI prefs scope | One global set across all accounts | No per-account override |

Architectural rejections (do not propose these):

- Provider-direct calls from the SPA — the bridge is the only surface.
- Per-account UI prefs — single global set is the contract.
- Mobile / phone responsive — tablet (≥768 px) is the floor.
- Native desktop wrapper — explicitly rejected.
- Redirect-based OAuth in the SPA — too much cookie/CSRF surface; device-code is canonical.

## `mxr web` launch model

The CLI command lives at `crates/daemon/src/commands/web.rs`. Behavior:

- **Local launch**: opens browser to `http://mxr.localhost:42829`. Reuses a daemon-managed bridge when one is healthy; otherwise spawns a detached bridge. `mxr web stop` shuts a detached bridge down. `--foreground` keeps it attached for debugging.
- **Remote launch**: `--remote-host mxr.example.com` opens the browser at `https://<host>/#token=<token>` reading the per-host token from `~/.config/mxr/bridge-tokens/<host>.token`. The local CLI **does not bind anything** in this mode — TLS is the remote operator's job.
- **Port handling**: conflicts fail by default. `--auto-port` retries up to 32 attempts; the bound port is published to `<config_dir>/bridge-port` so the Vite dev proxy and scripts can discover it.
- Other flags: `--port`, `--no-open`, `--print-url`, `--strict-port`.

Default port `42829` was chosen after `7777` collided with other dev servers. Snapshots regenerate when the help text changes; expect to re-record `crates/daemon/tests/snapshots/cli_help__cli_help_*.snap` whenever `Command::Web` is touched.

## Auth model — three layers

This is the most subtle part of the system. Implementation lives across `crates/web/src/middleware.rs`, `crates/config/src/resolve.rs`, and `apps/web/src/lib/tokenStorage.ts` / `useBridgeToken.ts`.

1. **Token at rest**: `~/.config/mxr/bridge-token` (mode 0600). Per-host remote tokens at `~/.config/mxr/bridge-tokens/<host>.token`. Generated lazily via `mxr_config::read_or_create_bridge_token()`. UUID v7 (time-sortable; the workspace `uuid` crate doesn't enable the `v4` feature).
2. **Local same-machine auto-handshake**: `GET /api/v1/auth/local-token` returns the bridge token only to loopback peers, gated by `[bridge].auto_local_token` (default `true`). The SPA calls this on every cold start and on 401. This eliminates the "paste a token" panel for normal local use.
3. **Remote / fallback**: URL fragment `#token=...&remote=...` is read once at boot, persisted to localStorage, and the hash is scrubbed via `history.replaceState`. If both auto-handshake and localStorage fail, the SPA routes to `/settings/token` with a `role="alert"` banner where the user can paste a token.

WebSocket auth uses `Sec-WebSocket-Protocol: bearer, <token>` (already supported by the bridge) — chosen because browsers don't allow setting custom WS headers.

## Embedded SPA serving

When the `web-ui` cargo feature is enabled (default in the root binary), the bridge embeds `apps/web/dist/` via `include_dir!()` and serves it at `/`. Behavior:

- Strict CSP: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ws: wss:; frame-ancestors 'none'; base-uri 'none'`.
- Asset paths get long-lived caching; `index.html` falls back for unknown paths so client-side routing works.
- A placeholder `apps/web/dist/index.html` exists in-repo so `cargo build --features web-ui` works without first running `npm run build`. The placeholder ships a `spa-not-built` marker that the CI smoke step in `.github/workflows/release.yml` greps for to fail loud if the SPA build was skipped.
- We serve files manually from `include_dir!()` (returning `Bytes` with mime via `mime_guess`) — `tower-http`'s `ServeDir` doesn't work with embedded fs.
- The bridge already serves Swagger UI at `/api/v1/docs` and OpenAPI JSON at `/api/v1/openapi.json`. SPA fallback routes must not shadow these.
- Dev path: `cd apps/web && npm run dev` (Vite at 5173, proxies `/api` and `/api/v1/events` to the discovered local bridge port via `MXR_BRIDGE_URL` env). The originally planned daemon-side `MXR_WEB_DEV_PROXY` was dropped — Vite's proxy is sufficient and simpler.

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

- **Left sidebar (264 px, collapses to 56 px on tablet)**: account switcher, primary nav (Mail / Search / Analytics / Rules / Accounts / Screener / Diagnostics), then **Lenses** section pinning system mailboxes + saved searches with live unread counts driven by WebSocket. Bottom slot: storage gauge + sync/connection pill + theme picker.
- **Top action bar (44 px)**: breadcrumb, central search input that morphs into command palette on `Cmd-K` or `:`, density toggle, compose CTA.
- **Main pane**: primary content. Density tokens drive row height (40 / 52 / 68 px — tighter than the original 48/56/72 plan to push more density on web).
- **Right contextual rail (380 px, slide-in, ESC closes)**: Sender Profile, Thread Summary, Attachments, URL list, Snippets browser, Saved-Search form, analytics drill-downs.
- **Status bar (28 px, bottom)**: sync %, semantic reindex %, daemon connection state, pending undo countdown, queued outbound mail.

## TUI → web reclassification

| TUI surface | Web treatment |
|---|---|
| Help, Snippets browser, Snooze, Label Picker, Compose Picker, More-actions | **Popover** |
| Bulk Confirm (>20 messages), Send Confirm, Unsubscribe, Delete Account | **Modal** (blocking) |
| Sender Profile, Thread Summary, URL list, Attachments, Reply Queue, Saved-Search form | **Right-rail side panel** |
| Analytics filters | **Inline sticky header** |
| Rules editor, Onboarding, Screener, Account setup | **Page** |
| Command palette | **`Cmd-K` overlay** (cmdk) |
| Errors | **Toast** (Sonner) + optional details popover |

## Conventions

- Strict TypeScript. No `any`. `unknown` at boundaries.
- Server data → TanStack Query. UI state → Zustand. URL state → TanStack Router search params (zod-validated).
- Imports: `@/...` alias maps to `apps/web/src/...`.
- Components small and dumb; containers wire data.
- Optimistic mutations all funnel through `useOptimisticMailMutation`.
- WS events all funnel through `<DaemonEventsProvider>` (single mount, dispatches to query cache).
- HTML mail bodies always pass through `sanitizeHtml.ts` (DOMPurify wrapper) before render. Inline images are proxied via the daemon. No remote-content load until the user opts in **per-thread**.

## The shared action registry (`apps/web/src/lib/actions/`)

This is the architectural lever that makes parity sustainable. Before it existed, three surfaces held independently-hardcoded lists:

- `CommandPalette.tsx` — palette items
- `keymap.ts` — global keybindings
- `shortcutHints.ts` — help dialog

Drift between them caused real bugs (e.g. `g a` collision: keymap routed it to archive/all-mail while the palette labeled it Analytics).

The registry replaced all three. Every Action has:

```ts
type Action = {
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

Consumed by:

- `CommandPalette.tsx` via `useActionsByGroup(ctx)`
- `useKeybindings.ts` via the registry-derived chord map (fed straight into `tinykeys`)
- `HelpDialog.tsx` via `useActionShortcutHints(ctx)`
- `/settings/keybindings` page (derived live from registry — no hardcoded list)

**Predicate helpers** (`when.ts`) compose: `onRoute`, `onPane`, `withSelection`, `withFocusedThread`, `firstAccountOnly`, `and(...)`. `firstAccountOnly` mirrors the TUI screener constraint.

**Runners** use `getNavigateRef()` and `useModals.getState()` — same pattern as the original `keymap.ts`. **No React hooks inside runners.** This is what lets a runner fire from `tinykeys` (outside React), the palette (inside React), or programmatically.

**`g a` resolution**: Analytics is now `g y` ("graphs / y-axis"). `g a` belongs to archive/all-mail (TUI parity). When this changed, a one-time toast on first `/m/archive` open notified users; suppressed via localStorage flag.

**Bundle impact**: catalog imports every feature's runners eagerly. ~80 actions × ~200 bytes ≈ 16 KB pre-gzip. If this grows past 5% of the main chunk, lazy-load runners with dynamic `import()` keyed by `action.id` while keeping `Action` metadata eager so palette filtering stays sync.

**TUI parity is a moving target**: any new `crates/tui/src/action.rs` enum addition is a defect against the registry. Capture as an issue, not a refactor.

## Optimistic mutations (`useOptimisticMailMutation`)

The heart of every mailbox mutation. All mutations funnel through this hook (`apps/web/src/features/mailbox/useOptimisticMailMutation.ts`).

Pattern:

1. `onMutate`: cancel inflight queries, snapshot envelope caches, mutate optimistically.
2. `onError`: restore from snapshot, toast error.
3. `onSuccess`: if response includes `mutation_id` and action is undoable, show 60s Undo toast.
4. `onSettled`: invalidate `["envelopes"]` and `["labels"]`.

Supports: archive, trash, spam, star, mark-read, mark-unread, label add/remove, move, snooze, unsubscribe, read-and-archive.

**Cache projection rules** (`mapMailboxRows`):

- Star / read: mutate row in place.
- Destructive (archive/trash/spam/snooze): remove row from current view.
- Label as folder / move: treat as destructive in the *current* view, then `invalidateQueries` in `onSettled` repopulates if appropriate. Stale unread counts in the shell are invalidated via `shellKey`.

**Undo**: bridge response includes `mutation_id`. `POST /api/v1/mail/undo { mutation_id }` reverses. The Undo toast lasts 60s.

## HTML mail rendering — sandboxed iframe

`MessageBody.tsx` renders sanitized HTML inside `<iframe srcDoc sandbox="allow-popups allow-popups-to-escape-sandbox">`. **No `allow-scripts`. No `allow-same-origin`.** This was a security hardening cycle — earlier code injected raw HTML into the document.

Sanitizer (`sanitizeHtml.ts`):

- DOMPurify with `style` attribute stripped entirely (CSS exfiltration vector).
- `<script>` tags removed.
- `target="_blank" rel="noopener"` forced on links.
- Remote `<img>` src stripped unless remote content is enabled per-thread.
- Tracker pixel stripping in `afterSanitizeAttributes`:
  - Remove `<img>` if `width ≤ 2` OR `height ≤ 2`.
  - Remove `<img>` if `src` host matches a known tracker (`mailtrack.io`, `track.customer.io`, `email.mg.*`, `sendgrid.net/wf/open`, `mandrillapp.com/track`). **Keep this list tight; new entries via PR review.**
- Inline images: `cid:` references replaced at render time with proxied URLs through the daemon's `/api/v1/mail/attachments/open`.

Iframe height is auto-sized via `ResizeObserver` on `contentDocument.body`.

## Compose

Two editors, lazy-loaded. **CodeMirror 6 + `@replit/codemirror-vim` is the default.** Tiptap is loaded only on user opt-in.

CodeMirror notes:

- Vim ex commands wired: `:w` saves, `:wq` saves and navigates to drafts, `:q!` discards (with confirm if dirty), `:send` triggers send confirm modal.
- `@codemirror/lang-markdown` for highlighting.
- Custom theme matching our tokens (don't use `oneDark` — palette mismatch).
- Snippet expansion: `;name<space>` looks up snippet by name and replaces the prefix. Snippet definitions come from `/api/v1/platform/snippets`.
- **`Esc` is reserved for vim mode** — page-level keyboard handlers MUST skip when the editor is focused.

Send flow:

1. `Cmd-Enter` opens the SendConfirm modal — does NOT send immediately.
2. Modal: "Send 1 message to N recipients via $account?" with Cancel / Send.
3. On Send: POST `/compose/session/send` → bridge schedules send → returns draft-id + outbound undo handle.
4. Status bar `OutboundUndoPill` mounts with **5-second** unsend countdown (locked decision: undo in status bar as pill, not toast — toasts can stack and be dismissed; this needs to be unmissable). Original design said 30s; reduced to 5s after bridge unsend semantics were defined.

Attachments: drag overlay covers the entire compose form (not just body). Browser file drops upload via `/api/v1/mail/compose/session/attachment`, then the returned local temp path is written into compose frontmatter — we don't pretend browser file names are usable local paths.

Auto-save: debounced 3s after idle. Status strip shows "Saved 3s ago" / "Saving…" / "Unsaved changes".

Reply / Reply-All / Forward: `/compose/new?reply=$messageId&mode=single|all|forward`. Bridge prepares the prefill (subject prefix, recipients, quoted body). Trust the bridge — don't re-prefix locally.

## Search

- Top-bar input morphs into command palette on `Cmd-K` / `:`.
- `/` focuses the input.
- Live debounced dropdown (120ms): top 5 messages, top 3 threads, top 2 contacts.
- Results page at `/search?q=&mode=lexical|semantic|hybrid&account=&sort=`.
- Token chips parsed locally with a ~50-line regex parser. Operators: `from:`, `to:`, `cc:`, `subject:`, `label:`, `has:attachment`, `is:unread`, `is:starred`, `older_than:7d`, `newer_than:1d`, `before:`, `after:`. **The bridge is authoritative on the result set** — local parsing is for visual feedback only, edge-case divergence is acceptable.
- Saved searches with `pin: true` show in sidebar Lenses with live counts (60s poll + WS invalidation on `LabelCountsUpdated` / `MailUpdated`).
- Live dropdown shares state with command palette via Zustand `searchStore`. Both `/` and `Cmd-K` populate this store; the visible UI is whichever is mounted.

## Realtime (WebSocket)

`<DaemonEventsProvider>` is mounted once inside `__root.tsx` (children of `QueryClientProvider`). All WS events funnel through it.

WS client (`apps/web/src/lib/ws.ts`):

- Reconnects with exponential backoff (250ms → 8s, jittered).
- 25s heartbeat ping.
- Online/offline + window-focus reconnect triggers.
- Authenticates via `Sec-WebSocket-Protocol: bearer, <token>`.

Event handlers invalidate queries:

| Event | Effect |
|---|---|
| `NewMessages`, `MailUpdated`, `MailRemoved` | `invalidateQueries({ queryKey: ["envelopes"] })` |
| `LabelCountsUpdated` | `setQueryData(["labels"], merge)` |
| `SyncProgress` | Update `connectionStore.syncProgress` |
| `OperationProgress` (sync) | Drives manual sync progress |
| `SyncCompleted` | Clears sync progress, refreshes mail |
| `SyncError` | Shows error state |

Connection states: `connecting | connected | reconnecting | offline`. After 30s offline, the sticky `<OfflineBanner>` shows in `AppShell`.

## Notifications

Browser Notification API. Single global toggle in `/settings/notifications`. Two modes: VIP-only or all-new-mail.

VIP allowlist: email addresses or domain patterns (`alice@example.com`, `@acme.com`). **Currently stored in localStorage** as a UI-prefs list — daemon-side VIP storage was deferred to a follow-up to avoid blocking shipping on a protocol change. When promoted to daemon storage, add `ListVips`/`UpsertVip`/`DeleteVip` request variants + handler + storage table + bridge routes; the SPA already abstracts via `useVips()` so the swap is local.

## Settings

Sections: `theme`, `density`, `keybindings` (derived from action registry), `notifications`, `compose` (editor preference + signature), `snippets`, `token` (view + paste-token fallback + regenerate), `about`.

Token regenerate is destructive (other clients de-auth) — confirm modal required.

## Design system

- Default theme `midnight` (deep blue-tinted near-black with sky-cyan accent).
- Themes shipped: `midnight` (default dark), `light`, `eclipse` (very dark + magenta accent), `paper` (warm off-white).
- `data-theme` attribute on `<html>` toggles palettes. CSS variables follow shadcn HSL-channel convention so shadcn components drop in unchanged.
- Type scale: 11/12/13/14/15/17/20/24/32/48 px. Body default 13 px, meta 11 px mono with tabular nums.
- Density modes: compact 40 px / regular 52 px / comfortable 68 px.
- Radii: 4/6/10/14 px.
- Motion: 120/180/280 ms with `cubic-bezier(0.2, 0.8, 0.2, 1)` ease.
- `prefers-reduced-motion` is a hard requirement. Wrapped story mode falls back to a still-image style with no animation under it.

## Accessibility

- Tab order: sidebar → topbar → main → right rail.
- Popovers/modals trap focus and return on close.
- Every theme passes WCAG AA on body + interactive elements (axe-core gate in CI).
- Every action reachable via keybinding or visible button (no mouse-only).
- Every icon-only button has `aria-label`.
- Sidebar in collapsed mode keeps full names in `aria-label`.
- `prefers-reduced-motion` kills row-exit animations and Wrapped slide transitions.

## Distribution and CI

`.github/workflows/release.yml`:

1. `actions/setup-node@v6`
2. `cd apps/web && npm ci && npm run build`
3. `cargo build --features web-ui --release` (every cargo build invocation in the matrix gets `--features web-ui`)
4. **CI smoke**: launch the binary, hit the SPA URL, fail if the response contains the `spa-not-built` placeholder marker or lacks Vite-hashed asset paths (`/assets/index-`).

The `web-ui` cargo feature is **default-on** in the root binary. Packagers who want a smaller binary can use `cargo build --no-default-features --features semantic-local`.

## Bundle budget

After Phase 9 chunk work:

- Main app chunk: 185.96 kB unminified / 53.86 kB gzipped.
- CodeMirror loads only on `/compose/*`.
- Tiptap loads only when explicitly opted-in.
- Recharts loads only on `/analytics/*`.

`npm run analyze` runs rollup-plugin-visualizer.

## Testing strategy

Per CLAUDE.md mandate: "tests passing means nothing if the real system is broken." Three layers:

1. **Vitest unit** (`*.test.{ts,tsx}` colocated): MSW for bridge mocking. Used for sanitizer, optimistic projection, error rollback, palette filtering by `when`, autocomplete debounce, draft-assist streaming chunks, saved-search pin reorder.
2. **Playwright real-daemon** (`apps/web/e2e/`): isolated fake-provider daemon/bridge spawned in global setup. Specs cover mutations, compose, search, WS reconnect, route errors, offline banner, sync progress, label apply across reload, unsend within window, screener real-account flow, semantic enable round-trip.
3. **Real-daemon manual smoke**: per CLAUDE.md, every implementer must drive new features in a browser against a real running daemon.

**Disallowed test patterns** (enforced by `test-quality-rubric` skill review on test-LOC-heavy PRs):

- Registry-shape snapshots (mirror implementation)
- "Renders without crashing" (sycophantic)
- Mutation tests asserting only the API call — must also assert optimistic UI change, rollback on rejection, and toast text.

## Critical files cheat-sheet

| Concern | File |
|---|---|
| `mxr web` command | `crates/daemon/src/commands/web.rs` |
| CLI definition | `crates/daemon/src/cli/mod.rs` (search `Command::Web`) |
| Token resolution helpers | `crates/config/src/resolve.rs` (`bridge_token_path`, `read_or_create_bridge_token`, `remote_bridge_token_path`) |
| Bridge crate | `crates/web/src/lib.rs`, `routes_v6.rs` |
| Embedded SPA module | `crates/web/src/spa.rs` |
| Auth/CORS/host allowlist | `crates/web/src/middleware.rs` |
| Protocol types | `crates/protocol/src/types.rs` |
| OpenAPI dump | `cargo run --example dump_openapi_spec -p mxr-web > spec.json` |
| Generated TS types | `apps/web/src/api/generated.ts` (regenerate via `npm run gen:types`) |
| Action registry | `apps/web/src/lib/actions/` |
| Optimistic mutations | `apps/web/src/features/mailbox/useOptimisticMailMutation.ts` |
| HTML sanitizer | `apps/web/src/lib/sanitizeHtml.ts` |
| WS client | `apps/web/src/lib/ws.ts` |
| Token storage | `apps/web/src/lib/tokenStorage.ts`, `apps/web/src/hooks/useBridgeToken.ts` |
| App shell | `apps/web/src/components/AppShell.tsx` |
| Route tree | `apps/web/src/routeTree.gen.ts` (auto-generated by TSR plugin — **never edit by hand**) |

## Verification mantra

After any non-trivial change:

```bash
# from apps/web/
npm run typecheck && npm run lint && npm run test

# from repo root
cargo check -p mxr-web
cargo build --features web-ui

# end-to-end smoke (per CLAUDE.md)
./target/release/mxr daemon --foreground   # one terminal
mxr web                                    # another terminal
# drive the feature in a browser against a real daemon
```

For bridge-touching changes:

```bash
cargo run --example dump_openapi_spec -p mxr-web > spec.json
cd apps/web && npm run gen:types  # commit the diff in apps/web/src/api/generated.ts
```

## Common pitfalls

- **Never edit `apps/web/src/routeTree.gen.ts` by hand.** TSR's auto-codegen plugin watches `routes/` and regenerates on every save.
- The bridge serves Swagger UI at `/api/v1/docs` and OpenAPI JSON at `/api/v1/openapi.json` — SPA fallback routes must not shadow these.
- The bridge auto-rejects non-loopback binds without TLS. For `--remote-host`, the local CLI does **not** bind anything; it just opens the browser to the remote URL. The remote operator handles TLS.
- shadcn components depend on `lib/utils.ts:cn()`. Don't break it.
- CLI snapshot tests will fail when help text changes — regenerate via `cargo test -p mxr cli_help`.
- Bridge PRs that change protocol must regenerate `apps/web/src/api/generated.ts` and commit the diff. Web PRs consuming new endpoints must rebase on main after the bridge PR lands.

## Maintenance & enhancement guidance

When adding a new TUI action:

1. Add a new `Action` to the appropriate feature's `actions.ts` in `apps/web/src/features/<feature>/`.
2. The palette, keymap, help dialog, and `/settings/keybindings` page pick it up automatically.
3. If the action mutates mail, route through `useOptimisticMailMutation` — extend the `MailAction` union and `mapMailboxRows` projection rather than building a parallel hook.
4. If the action needs a new bridge endpoint, **ship the bridge PR first** (separate Rust review pool), regenerate types, then ship the web PR. Bundling stalls merges.

When adding a new bridge route:

1. Add to `crates/web/src/routes_v6.rs` (or `lib.rs` for top-level).
2. Add request/response types to `crates/protocol/src/types.rs`.
3. Implement handler in `crates/daemon/src/handler/`.
4. Regenerate OpenAPI: `cargo run --example dump_openapi_spec -p mxr-web > spec.json`
5. Regenerate TS types: `cd apps/web && npm run gen:types` and commit the diff.

When adding a new theme:

1. Add CSS variables to `apps/web/src/styles/tokens.css` under a `[data-theme="<name>"]` block.
2. Use shadcn HSL-channel convention (e.g. `--background: 222 47% 11%;`).
3. Add the theme to the `/settings/theme` picker.
4. Run axe-core to confirm WCAG AA contrast on body + interactive elements.

## Out of scope (do not implement)

- Mobile / phone responsive build.
- Per-account UI prefs.
- Native desktop wrapper.
- Provider-direct calls from the SPA.
- Web Push notifications (post-v1; Notification API only for now).
- Account-scoped query invalidation (refetch papers over it adequately).
- Notification deduplication on reconnect (rare, mild duplication acceptable).

## Implementation history

The web app shipped across 11 phases between 2026-05-10 and 2026-05-12:

1. Bootstrap — Vite/React/TS scaffold, AppShell, routing, API client, WS client, token bootstrap, `mxr web` command, embedded SPA serving.
2. Mailbox + reader — virtualized list, thread reader, optimistic mutations + undo, WS invalidation.
3. Compose — CodeMirror+vim default, Tiptap alt, contact autocomplete, 5s unsend, draft-assist panel.
4. Search — top-bar + results + token chips + lex/sem/hybrid + saved searches.
5. Command palette — cmdk overlay, scoped fuzzy match.
6. Analytics — six dashboards including Wrapped story+dashboard modes.
7. Rules — builder + always-visible dry-run + history + apply-now.
8. Accounts + onboarding — first-run wizard, OAuth device flow, account management.
9. Polish — settings, screener, browser notifications, e2e suite, accessibility gate, bundle chunk split.
10. v1 launch hardening — release packaging embeds real SPA, HTML sandboxed iframe, sanitizer hardening, first-launch + 401 UX, real-daemon e2e safety net.
11. Parity closure (W0–W8, 17 PRs) — shared action registry replacing three hardcoded lists, bridge gained `/contacts/autocomplete` and `UpdateSavedSearch`, mailbox mutations widened to label/move/unsubscribe/read-and-archive, compose autocomplete + 5s unsend + draft-assist, saved-search manager, Wrapped story + share-as-image, sender standalone route, screener multi-account notice, keybindings page derived from registry.

Total: 110 tests at parity-closure complete; typecheck + lint clean; main chunk 53.86 kB gzipped.
