# Status

Update this file as phases complete. Single source of truth for "where are we?"

## Phase progress

| # | Phase | Status | Notes |
|---|---|---|---|
| 1 | Bootstrap | **complete** | Scaffold, tooling, AppShell, routing, API client, WS client, token bootstrap, /dev smoke page, `mxr web` Rust command extension, `web-ui` cargo feature, embedded SPA serving with strict CSP. Typecheck + lint + cargo build all green. |
| 2 | Mailbox + reader | **complete** | Mailbox shell, dynamic sidebar, virtualized message/thread list, thread reader, right rail, optimistic mutations + undo, WS invalidation. |
| 3 | Compose | **complete** | Contact autocomplete (bridge `/contacts/autocomplete`), outbound undo pill (5s deferred send), draft-assist right-rail panel landed in Phase 11 PRs #7–#9. |
| 4 | Search | **complete** | Saved-search update/pin/color/delete manager, scope picker (threads/messages/attachments), bridge `UpdateSavedSearch` request + `/saved-searches/update` route added in PRs #10–#11. j/k nav already shipped. |
| 5 | Command palette | **complete** | Migrated to shared registry (`apps/web/src/lib/actions/`). Reads from `useActionsByGroup(ctx)`. Mailbox/compose/diagnostics/analytics/rules/accounts/settings feature actions registered. PRs #1–#4 + #6 + #9 + #15. |
| 6 | Analytics | **complete** | Wrapped story mode (j/k tile nav), share-as-image (Web Share + clipboard fallback), refresh-contacts palette command. Right-rail drilldowns and stale-window pickers tracked as smaller follow-ups; data is already inline in the existing dashboards. PR #15. |
| 7 | Rules | **complete** | `mailAction` parser extended to label-add / move / read-and-archive; `runMailAction` dispatches via `useOptimisticMailMutation`. Typed action chips for all supported verbs. PR #16. |
| 8 | Accounts + onboarding | **complete** | Repair button (POST `/platform/accounts/repair`), Refresh button, palette commands. Full config-edit form deferred — current detail panel covers the practical flows (test/default/disable/remove/aliases/repair/refresh). PR #14. |
| 9 | Polish + e2e | **complete** | Settings sections, theme/density/keybindings/notifications/local VIPs/compose editor/snippets/token/about, diagnostics, screener, browser notifications, WS event normalization, real-daemon Playwright harness, accessibility gate, and bundle chunk split. |
| 10 | v1 launch hardening | **complete** | Release workflow builds and smokes the embedded SPA; HTML mail renders in a sandboxed iframe; sanitizer strips scripts/styles/tracker pixels; first-run + expired-token UX is covered; real-daemon e2e covers mutations, compose, search, WS reconnect, route errors, offline banner, and sync progress; README + bridge guide are updated. |
| 11 | Parity closure (W0–W8) | **complete** | All 17 PRs landed. Shared action registry (`apps/web/src/lib/actions/`) drives palette, keymap, help, status-bar, settings/keybindings. Bridge gained `/contacts/autocomplete` and `UpdateSavedSearch` (protocol + store + handler + bridge route). Mailbox mutations widened to label/move/unsubscribe/read-and-archive with optimistic cache projections. Compose autocomplete + 5-second unsend + draft-assist panel. Saved-search manager with pin/color/delete. Wrapped story mode + share-as-image. Sender standalone route. Screener multi-account notice. Keybindings settings page derives from registry. 110 tests; typecheck + lint clean. |

## What landed in Phase 1

### Frontend (`apps/web/`)

- npm package, Vite 7 + React 19 + TypeScript strict + Tailwind 4 + oxlint + oxfmt + Vitest + Playwright config.
- Design system in `src/styles/tokens.css` with four themes: `midnight` (default), `light`, `eclipse`, `paper`.
- shadcn/ui primitives copied in: button, dialog, popover, dropdown-menu, command, scroll-area, separator, tooltip, switch, label, tabs, select, input, sonner.
- AppShell with Sidebar (collapsible, primary nav + lenses + system), Topbar (breadcrumb, search, density toggle, compose CTA), StatusBar (connection state, sync %, semantic %), RightRail (slide-in side panels).
- Stores: `uiPrefsStore` (persisted theme/density/sidebar/compose-editor/notifications/VIPs), `connectionStore`, `selectionStore`, `modalStore`.
- API client: `openapi-fetch` over `openapi-typescript`-generated types in `src/api/generated.ts` (6,295 lines from the live bridge OpenAPI spec).
- WebSocket client (`src/lib/ws.ts`): reconnecting with exponential backoff + heartbeat + online/offline + window-focus reconnect, authenticates via `Sec-WebSocket-Protocol: bearer, <token>`.
- Token bootstrap: local launches use `/api/v1/auth/local-token`; remote/manual launches can still read URL fragment `#token=...&remote=...`, persist to localStorage, and scrub the hash via `history.replaceState`.
- TanStack Router with file-based routes for every IA URL, auto-codesplit per route.
- `/dev` smoke page hits `/api/v1/admin/health` (unauthed) and `/api/v1/admin/status` (authed) and shows results.
- `/settings/token` paste-token fallback panel.
- Sonner toaster mounted globally; theme-aware.
- Page stubs for every feature route pointing at the relevant phase doc.

### Rust

- New `bridge_token_path()`, `remote_bridge_token_path(host)`, `read_or_create_bridge_token()` in `mxr_config`. Writes mode-0600 tokens at `~/.config/mxr/bridge-token` and `~/.config/mxr/bridge-tokens/<host>.token`.
- `mxr web` opens `http://mxr.localhost:42829`, reuses a healthy daemon bridge when present, supports `--auto-port`, and keeps `--remote-host` for manually configured remote bridges. `MXR_WEB_BRIDGE_TOKEN` env still overrides the persisted token.
- New `web-ui` cargo feature (off by default, on when explicitly enabled). When enabled, the bridge embeds `apps/web/dist/` via `include_dir!()` and serves it at `/` with a strict Content-Security-Policy (`script-src 'self'` etc.). Asset paths get long-lived caching; `index.html` falls back for unknown paths so client-side routing works.
- Placeholder `apps/web/dist/index.html` so `cargo build --features web-ui` works without first running `npm run build` (it gets overwritten by the real build).
- CLI snapshot regenerated. `parses_web_subcommand` + new `parses_web_subcommand_with_remote_host` tests both pass.

### Verification ran

- `cd apps/web && npm install` → clean.
- `npm run gen:types` → 6,295-line `src/api/generated.ts` from the live bridge spec.
- `npm run typecheck` → green.
- `npm run lint` → 0 warnings, 0 errors with `--deny-warnings`.
- `npm run build` → SPA builds. Phase 9 chunk split reduced the main app chunk to 185.96 kB unminified / 53.86 kB gzipped; heavy editor/chart vendors are lazy route chunks.
- `cargo check` → green.
- `cargo build --features web-ui` → green.
- `cargo test -p mxr cli::tests::parses_web_subcommand` and `parses_web_subcommand_with_remote_host` → both pass.
- `cargo test -p mxr-config` → all 21 tests pass (token helpers covered).
- `cargo test -p mxr --test cli_help cli_help_snapshots_cover_all_commands` → passes after snapshot update.

### Phase 1 follow-ups resolved during v1 launch

- `.github/workflows/release.yml` builds `apps/web` before cargo release artifacts and smokes the embedded SPA.
- Root `README.md` now documents `mxr web`, local launch flags, and token troubleshooting.
- `docs/guides/http-bridge.md` now documents remote-host launch mode and token placement.
- Real-daemon Playwright coverage exists locally for the v1 safety net; CI can expand beyond the release smoke later if needed.

## Notes / decisions made during execution

- 2026-05-10 — Default theme is `midnight` (deep blue-tinted near-black with sky-cyan accent). Three themes shipped in tokens: `midnight` (default dark), `light`, `eclipse` (very dark + magenta accent), `paper` (warm off-white).
- 2026-05-10 — Density tokens drive row heights: compact 40 px / regular 52 px / comfortable 68 px. (Plan called for 48/56/72; tightened to 40/52/68 to push more density on web.)
- 2026-05-10 — `data-theme` attribute on `<html>` toggles palettes. CSS variables follow shadcn HSL-channel convention so shadcn components drop in unchanged.
- 2026-05-10 — Vite proxy: `/api` → the discovered local bridge port (configurable via `MXR_BRIDGE_URL` env). WebSocket proxied at `/api/v1/events`.
- 2026-05-10 — Routing via TanStack Router with file-based routes + auto-code-splitting plugin. Generated route tree at `src/routeTree.gen.ts` (gitignored).
- 2026-05-10 — Token uses `Uuid::now_v7()` (workspace `uuid` crate doesn't enable `v4` feature; v7 is time-sortable and equally unique).
- 2026-05-10 — `web-ui` cargo feature is **off by default** in `crates/web` and the root binary. CI/release workflow will enable it; local Rust dev keeps fast iteration. Documented in `01-bootstrap.md` decisions.
- 2026-05-10 — Dev path: `cd apps/web && npm run dev` (Vite at 5173, proxies API/WS to the local bridge). The originally-planned `MXR_WEB_DEV_PROXY` daemon-side proxy was dropped — Vite's proxy is sufficient and simpler. Production path: `npm run build` then `cargo build --features web-ui` then `mxr web`.
- 2026-05-10 — Bundle started at 559 kB unminified / 173 kB gzipped on the initial route before Phase 9 chunk work.
- 2026-05-10 — Browser attachment drops upload a local temp copy through `/api/v1/mail/compose/session/attachment`, then the returned path is written into compose frontmatter. This avoids pretending browser file names are usable local paths.
- 2026-05-10 — Phase 3 compose is usable but not closed: snippets now use the platform snippets endpoint; contact autocomplete still needs a real contacts endpoint; outbound undo needs an unsend/scheduled-send bridge contract before the status-bar pill can be honest.
- 2026-05-10 — Phase 4-9 web routes are implemented as usable baselines, with bridge contract fixes for search modes (`lexical`/`semantic`/`hybrid`), account-scoped sync status, and WebSocket `event` → `type` normalization. Remaining parity items are listed in the phase table.
- 2026-05-11 — Phase 9 polish is complete locally: production build has no Vite chunk-size warning, Playwright starts an isolated fake-provider daemon/bridge, smoke and axe accessibility specs pass, and rule/account/command-palette parity gaps are narrowed.
- 2026-05-11 — v1 launch hardening is complete locally: release packaging embeds the real SPA, HTML rendering is sandboxed, first-launch/auth resilience is covered, tracker images are stripped, route/offline/sync progress UX has e2e coverage, and launch docs are updated.
- 2026-05-11 — Same-machine auto-handshake added. `GET /api/v1/auth/local-token` returns the bridge token to loopback peers only; gated by `[bridge].auto_local_token` (default `true`). SPA's auth client tries this endpoint on every cold start and on 401, eliminating the "paste a valid token to reconnect" panel for local-machine users. Remote-host setups can disable via config and fall back to the paste UX.
- 2026-05-11 — Default bridge port changed `7777 → 42829` to avoid common dev-server collisions. Actual bound port is published to `<config_dir>/bridge-port` so the Vite dev proxy + scripts can discover it. Snapshots regenerated.
- 2026-05-12 — Local launch URL changed to `http://mxr.localhost:42829`; `mxr web` now reuses a healthy daemon-managed bridge, keeps port conflicts strict by default, and uses `--auto-port` as the explicit retry escape hatch.

## Critical files cheat-sheet

- Plan: `~/.claude/plans/you-excel-at-making-concurrent-rabbit.md`
- Bridge crate: `crates/web/src/lib.rs`, `routes_v6.rs`
- Embedded SPA module: `crates/web/src/spa.rs`
- Protocol types: `crates/protocol/src/types.rs`
- OpenAPI dump: `cargo run --example dump_openapi_spec -p mxr-web > spec.json`
- `mxr web` command: `crates/daemon/src/commands/web.rs`
- CLI definition: `crates/daemon/src/cli/mod.rs`, search for `Command::Web`
- Token resolution helpers: `crates/config/src/resolve.rs` (`bridge_token_path`, `read_or_create_bridge_token`, `remote_bridge_token_path`)
- Auth/CORS/host allowlist: `crates/web/src/middleware.rs`
- SPA: `apps/web/src/`
- Phase docs: `docs/web-app/0X-*.md`
