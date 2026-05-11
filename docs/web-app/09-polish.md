# Phase 9 — Polish, Settings, Notifications, Diagnostics, Screener, e2e

Goal: ship the long tail. Screener triage page. Settings (incl. VIP allowlist). Diagnostics. Theme picker. Browser notifications. Bundle/perf budget. Playwright e2e against a real daemon. Accessibility pass. Docs.

## Deliverables

### Settings (`/settings/$section`)

Sections:
- `theme` — pick theme (midnight / light / eclipse / paper / system).
- `density` — compact / regular / comfortable.
- `keybindings` — view all keybindings; future: rebind.
- `notifications` — global toggle + VIP allowlist + "notify on all new mail" toggle.
- `compose` — editor preference (codemirror-vim / tiptap), default account, signature block.
- `snippets` — list / create / edit / delete (full CRUD).
- `token` — view / paste / regenerate bridge token.
- `about` — version, daemon version, build info.

### VIP allowlist (notifications setting)

- List of email addresses or domain patterns (`alice@example.com`, `@acme.com`).
- Stored on the daemon → syncs across clients.
- Bridge endpoint: probably `GET /api/v1/platform/vips` and `POST /api/v1/platform/vips/upsert` / `delete`. **If those endpoints don't exist yet, add them.** This requires:
  - New `Request::ListVips`, `UpsertVip`, `DeleteVip` variants in `crates/protocol/src/types.rs`.
  - Handlers in the daemon (`crates/daemon/src/handler/`).
  - Storage (a small table in `crates/store/`).
  - Bridge routes in `crates/web/src/lib.rs` or `routes_v6.rs`.
  - Regenerate OpenAPI types.

If the daemon already has a notion of "VIP" via labels or a config knob, layer on that instead. **Verify before adding new protocol surface.**

### Browser notifications

- Single global "Enable browser notifications" toggle in `/settings/notifications`.
- Permission prompt on enable.
- VIP-only mode: only notifies on new mail from VIPs.
- "All new mail" mode: notifies on every new envelope.
- WS `NewMessages` event → match against VIPs → fire `Notification(...)` if granted.
- Click notification → `window.focus()` + navigate to `/m/inbox/$threadId`.

### Screener (`/screener`)

- Page (not modal — recurring workflow).
- List of unknown senders waiting for triage with sample message preview.
- Per-sender disposition buttons: Allow / Deny / Feed / Paper-trail.
- Bulk select + bulk dispose (e.g. select 5 → "Deny all").
- Endpoint: bridge surface mirrors `mxr screener` CLI subcommands.

### Diagnostics (`/diagnostics`)

- Daemon status (uptime, version, accounts, queue depth)
- Recent logs (tail-like, follows by default)
- Doctor report (run on demand, displayed structured)
- Bug report generator (button → POST → download / copy)
- Sync status per account
- Semantic status (enabled, indexed count, last reindex)

### Theme picker

- Available everywhere via topbar / sidebar bottom slot / settings.
- Live preview on hover.

### Editor preference

- `compose.editor` in UI prefs settings → either `codemirror-vim` or `tiptap`.
- The setting is persisted to localStorage and (later) synced to the daemon if a `prefs` endpoint exists.

### Bundle / perf budget

- Run `npm run analyze` (uses rollup-plugin-visualizer) — confirm:
  - Initial route under 250 kB gzip
  - CodeMirror loads only on `/compose/*`
  - Tiptap loads only when explicitly opted-in
  - Recharts loads only on `/analytics/*`

### Playwright e2e suite

- Global setup: spawn `mxr daemon --foreground --no-bridge=false --bridge-port=$RANDOM` backed by `provider-fake`.
- Specs:
  1. `smoke.spec.ts` — open `/`, see app shell.
  2. `mailbox.spec.ts` — list renders, click row → reader opens.
  3. `archive-undo.spec.ts` — archive 3 → toast → undo restores.
  4. `compose-vim.spec.ts` — open compose, type in vim mode (`o` to open new line, `:w` save).
  5. `search.spec.ts` — type query, results render, save search → sidebar updates.
  6. `command-palette.spec.ts` — `Cmd-K` opens, type, select, navigate.
  7. `analytics.spec.ts` — open Storage dashboard, hover bar, drill down → side panel.
  8. `rules.spec.ts` — create rule, dry-run shows matches, save.
  9. `accounts.spec.ts` — onboarding flow with FakeProvider.
  10. `realtime.spec.ts` — WS event fires, mailbox updates without reload.
  11. `responsive.spec.ts` — at 768 px sidebar collapses.

### Accessibility pass

- Tab order logical from sidebar → topbar → main → right rail.
- All popovers/modals trap focus and return on close.
- Color contrast: every theme passes WCAG AA on body + interactive elements (verify with axe-core).
- Keyboard reachable: every action has a keybinding or visible button (no mouse-only).
- Screen reader: ARIA labels on icon-only buttons (especially in sidebar collapsed mode).
- `prefers-reduced-motion` respected — kill row-exit animations and Wrapped slide transitions.

### Docs

- `docs/blueprint/16-addendum.md` — append A009 documenting `mxr web` and the embedded SPA.
- `docs/guides/http-bridge.md` — extend with web-app distribution.
- `apps/web/README.md` — quickstart for contributors.
- Update root `README.md` quickstart to mention `mxr web`.

## Files

```
src/features/settings/
  SettingsLayout.tsx                # left tab list + right content
  SettingsRoute.tsx                 # /settings index
  ThemeSection.tsx
  DensitySection.tsx
  KeybindingsSection.tsx
  NotificationsSection.tsx          # global toggle + VIP allowlist
  ComposeSection.tsx                # editor preference + signature
  SnippetsSection.tsx
  TokenSection.tsx                  # view + paste-token fallback
  AboutSection.tsx
  VipAllowlist.tsx                  # the VIP list editor component
  useVips.ts
src/features/screener/
  ScreenerRoute.tsx
  ScreenerQueue.tsx
  ScreenerRow.tsx
  ScreenerDispositionButtons.tsx
  useScreenerQueue.ts
src/features/diagnostics/
  DiagnosticsRoute.tsx
  DaemonStatusCard.tsx
  LogsTail.tsx
  DoctorReportPanel.tsx
  BugReportGenerator.tsx
src/features/notifications/
  NotificationGrant.tsx             # permission prompt UI
  useNewMessageNotifier.ts          # WS subscription + Notification API
  matchVip.ts                       # email/domain matcher
e2e/
  smoke.spec.ts
  mailbox.spec.ts
  archive-undo.spec.ts
  compose-vim.spec.ts
  search.spec.ts
  command-palette.spec.ts
  analytics.spec.ts
  rules.spec.ts
  accounts.spec.ts
  realtime.spec.ts
  responsive.spec.ts
  helpers/
    daemon.ts                       # boot + tear down a daemon process
    fake-data.ts                    # seed with provider-fake
```

## Bridge endpoints used (new or first-touch)

- VIPs: probably new — see decision below.
- `GET /api/v1/mail/screener/queue?account_id=&limit=`
- `POST /api/v1/mail/screener/decisions`
- `GET /api/v1/mail/snippets`
- `POST /api/v1/mail/snippets`
- `DELETE /api/v1/mail/snippets/{name}`
- `GET /api/v1/admin/diagnostics/bug-report` (already exists)
- `GET /api/v1/admin/logs` (verify)
- `GET /api/v1/mail/sync/status?account_id=`

## Decisions

- 2026-05-10 — VIPs: if the daemon doesn't have a first-class VIP concept, ship VIPs as a UI-prefs list stored in the SPA's localStorage for v1, then promote to daemon storage in a follow-up. This avoids blocking shipping on a protocol change.
- 2026-05-10 — `prefers-reduced-motion` is a hard requirement. Wrapped story mode falls back to a still-image style with no animation under reduced-motion preference.
- 2026-05-10 — Token settings panel offers regenerate. Regenerate is destructive (other clients de-auth) — confirm modal.

## Verification

1. `/settings/notifications` → enable browser notifications → permission prompt → accept.
2. Add VIP `@acme.com`. Send test email from acme via fake provider → notification fires.
3. `/screener` → unknown senders listed. Pick one → "Deny" → row disappears, sender now auto-trashed on ingest.
4. `/settings/theme` → switch to "paper" → entire app re-themes live.
5. `/settings/compose` → switch editor to Tiptap → reload `/compose/new` → Tiptap loads.
6. `/diagnostics` → daemon status visible. "Generate bug report" → file downloads.
7. `/settings/snippets` → create snippet `;sig` with body → in compose, type `;sig<space>` → expands.
8. `npm run analyze` → bundle visualizer; confirm budgets met.
9. `npm run e2e` → all specs green.
10. axe-core scan on every route → no critical violations.
