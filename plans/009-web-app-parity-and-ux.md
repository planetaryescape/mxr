# Plan 009 — Web App: Bugs, CLI/TUI Parity, UX Overhaul

Generated 2026-07-12 from a four-agent review (web map, CLI/TUI inventory,
bug hunt, UX review) plus a live Playwright session against the running
v0.6.6 bridge (`mxr web`, port 42829). Source reviewed at `main` (1ccc3794).

Scope: `apps/web` (React 19 + TanStack Router/Query SPA) and `crates/web`
(axum bridge). Everything below is verified against source (file:line) or
observed live; live-only observations are marked **[live]**.

Statuses: TODO | IN PROGRESS | DONE | REJECTED.

---

## Workstream A — Correctness bugs

### A0-a. Duplicate email send during the undo-send window — P1, S
`features/compose/useComposeSession.ts:521` — `busy` excludes `pendingSends`,
so during the undo countdown Send/cmd+Enter stay enabled
(`ComposeActionBar.tsx:42`, `useComposeSession.ts:661-665`). A second press
starts a second timer and overwrites the single `pendingSendCancel`
(`:822-885`, `state/undoStore.ts`); both fire, and the bridge mints a fresh
`DraftId` per request (`crates/web/src/lib.rs:770-806`) so the daemon can't
dedupe → **email sent twice**. Pressing `z` after the second press cancels
only the newest timer and toasts "Send cancelled" while the first send still
fires. **Fix:** include pending sends in `busy`; make `dispatchSend`
idempotent per session (replace or refuse while a send is pending); cancel
must cancel all timers for the session.

### A0-b. Legacy 301 redirects shadow live SPA routes — P1, S
`crates/web/src/legacy.rs:16-98` + `router.rs:219` apply v0.4 path redirects
*outside* the router, before SPA fallback. `/search`, `/rules`, `/accounts`,
`/subscriptions`, `/diagnostics` are all SPA routes; reload or deep-link →
`301 → /api/v1/...` → `{"error":"unauthorized"}` JSON. `Redirect::permanent`
means browsers cache the break. **Fix:** drop the SPA-colliding entries (or
only redirect when the request has an `Accept: application/json` /
`X-Requested-With` marker); use 307/308 for whatever remains; add a
regression test per SPA route.

### A1. `?` opens Search instead of Help  — P1, S
`lib/actions/navigationActions.ts:44` aliases the search palette to
`"Slash"`, which tinykeys matches on `event.code` — shifted slash (`?`)
included. Help is bound to `"Shift+Slash"` (`shell.help`,
`navigationActions.ts:48`). **[live]** pressing `?` opens the search
overlay; the advertised Help never appears. Additionally two help dialogs
share `useModals().helpOpen` — `components/HelpDialog.tsx` (AppShell.tsx:103)
and `ShortcutHelpPanel` in `components/StatusBar.tsx:45,86-124` — so when
help *does* open, two stacked Radix dialogs appear.
**Fix:** bind search to `Slash` with an explicit shift-guard (or bind on
`event.key === "/"` vs `"?"`); delete the StatusBar `ShortcutHelpPanel`
(keep the searchable `HelpDialog`).

### A2. WebSocket never connects on first session; pill lies "no token" — P2, S
**[live]** on first load the footer showed `no token` + `bridge:
unauthorized` for minutes while the mailbox rendered fine. Root cause
(verified): `App.tsx:55-58` starts `daemonEvents` on mount, before the
loopback token handshake stores the token; `openSocket` bails to
`unauthorized` with no retry (`lib/ws.ts:90-95`), and `setToken`
(`lib/tokenStorage.ts`) has no listeners — reconnects only happen on
`online`/`focus` (`ws.ts:158-169`). A fresh session that never refocuses
gets **no live updates at all** (no new-mail invalidation, no sync
progress) despite every HTTP call succeeding. **Fix:** notify the WS client
when the token lands (subscribe in `tokenStorage.setToken` or await the
handshake before `daemonEvents.start()`); pill copy should distinguish
"live updates connecting" from "no token".

### A3. Compose shows validation errors before the user types — P2, S
**[live]** opening a fresh compose panel immediately renders three banners:
"No recipients" (red), "Subject is empty", "Message body is empty". Premature
validation; it also eats ~25% of the panel's vertical space.
**Fix:** suppress until first send attempt (or per-field blur); collapse to
a single compact line.

### A4. Default compose editor is Tiptap, not CodeMirror-vim — P2, XS
`state/uiPrefsStore.ts:48` sets `composeEditor: "tiptap"`. The recorded
product decision is vim-first: default `"codemirror-vim"`, Tiptap as the
opt-in rich mode. **[live]** fresh profile opens rich text.
**Fix:** flip the default to `"codemirror-vim"`.

### A5. Pluralization/count bugs in thread + screener — P3, XS
- **[live]** thread header renders "1 messages"; screener rows hardcode
  "N messages" (`features/screener/ScreenerRoute.tsx:175`).
- **[live]** thread header showed "0 unread" while the message chip showed
  `UNREAD` (opening the thread had already marked it read server-side but
  the chip renders from stale thread payload).
**Fix:** shared plural helper (`Intl.PluralRules` or trivial `n === 1`);
make the unread chip and header derive from the same field.

### A6. Analytics "Largest messages" shows sender "unknown" on every row — P2, S
**[live]** all rows render `unknown · N MB`. Either the daemon payload's
sender field is named differently than the SPA expects or the fallback
kicks in unconditionally (`features/analytics/AnalyticsDashboardRoute.tsx`,
largest-messages DataList). **Fix:** align the field mapping; add a test
against a recorded payload.

### A7. Recharts container has zero/negative size; axis labels clipped — P3, S
**[live]** console: `The width(-1) and height(-1) of chart should be greater
than 0` (vendor-recharts); the storage chart's top y-axis label renders cut
off. **Fix:** give the chart container a measured min-height/width; add
margins for axis labels.

### A8. Screener tab strip renders as "QueueDecisions" — P3, XS
**[live]** the two tabs render flush together with no gap/affordance.
**Fix:** proper Tabs component (shadcn `Tabs`) with spacing + active state.

### A9. Radix a11y warning: DialogContent without Description — P3, XS
**[live]** console warning on dialog open. Add `aria-describedby` /
`DialogDescription` to the offending dialogs.

### A10. Optimistic mutation layer is a complete no-op — P1(foundational), M
`useMailboxQuery.ts:77` stores mailbox data as `useInfiniteQuery`
(`{pages, pageParams}`), but `useOptimisticMailMutation.ts:85-87,115-124`
type-guards for a *plain* `MailboxResponse` — so every archive/trash/spam/
star/read optimistic update **and its rollback silently does nothing**
(rows linger until the `onSettled` refetch). The unit test seeds the wrong
cache shape, so it passes against a shape that never exists at runtime
(`useOptimisticMailMutation.test.tsx:56-118`). **Fix:** patch
`InfiniteData` pages (and `["thread"]`, see C5); rewrite the test to seed
the real infinite shape. This unblocks the "archive feels instant" promise
everywhere.

### A11. Mutations from Search never invalidate the search cache — P2, S
`useOptimisticMailMutation.ts:322-326` and `performUndo` (`:31-44`)
invalidate `["mailbox"]`/`["thread"]`/shell only. Search results
(`["search", …]`, `features/search/api.ts:60-62`) reuse the same list and
mutations (`SearchResultsRoute.tsx:444`) — trash 20 from search and all 20
stay listed. Note: the daemon emits no `MailUpdated`/`MailRemoved`/
`SyncProgress` events (`crates/protocol/src/types.rs:3313-3386`), so those
handler cases in `hooks/useDaemonEventInvalidation.ts:19-21,31` are dead
code and won't save you. Also stale: `["reply-queue"]`. **Fix:** invalidate
search + reply-queue keys in `onSettled`/undo; delete or implement the
dead event branches.

### A12. Saved-search lens: `j`/`k` hijacks navigation; Esc goes nowhere — P2, S
`MailboxRoute.tsx:15-18` treats the 3rd path segment as a thread id for
every `/m/*` except `label` — `/m/saved/<slug>` sets
`activeThreadId=<slug>` + `previewOnFocus`, so the first `j` navigates you
out to `/m/inbox/<threadId>` (`MailboxList.tsx:174-186,491-494`) and Esc
targets `/m/saved` which matches no route. **Fix:** exclude `saved` (and
future non-thread segments) or model lens routes explicitly.

### A13. Route-from-queue sends the slug, not the label name — P2, S
Sidebar ids are slugified (`crates/web/src/chrome.rs:281`;
`Sidebar.tsx:309`); `actions.ts:58-62` extracts that slug and
`RoutePicker.tsx:83-85` sends it as `from_queue_label`, which the daemon
matches by *name* — "Reply Later" → "reply-later" never matches, so the
queue label is never removed (message stays queued). Same mismatch makes
the picker offer the current queue label as a target (`RoutePicker.tsx:26`).
**Fix:** carry the real label name through sidebar item payloads.

### A14. Sidebar "Subscriptions" opens All Mail — P2, S
`Sidebar.tsx:310` links the subscriptions lens to `/m/label/subscriptions`;
`useMailboxQuery.ts:30-32,43-48` returns `undefined` for that item →
silent fallback to `{lens_kind:"all_mail"}`. Any unknown `/m/label/<x>`
does the same. **[live]** this also explains the two "Subscriptions"
sidebar entries confusion. **Fix:** resolve the lens properly; render
"unknown label" instead of silently showing All Mail.

### A15. Invite RSVP: silent failure, unmount-cancel, lying undo — P2, S
`features/thread/useInviteResponse.ts:43-58` has no `onError` and no one
reads `error` → failed RSVP looks sent. `:81` cancels the pending send on
unmount → Accept then navigate within 1s silently never sends. Undo toast
lives 1100ms but the send fires at 1000ms (`InviteCard.tsx:96-107`) →
last-100ms Undo "cancels" an already-sent response. **Fix:** onError toast;
flush (not cancel) on unmount; align window and timer.

### A16. "Accept/Decline with comment" creates an orphan draft, opens nothing — P2, S
`InviteCard.tsx:111-125`, `InvitesRoute.tsx:100-107` create a server-side
draft (`crates/web/src/lib.rs:1864-1905`), toast "Compose draft opened",
but never open the composer (response cast `{draftPath?}` is wrong too —
it's `{session:{…}}`). No RSVP sent; orphan drafts accumulate. **Fix:**
actually open ComposeHost with the returned session.

### A17. Closing the composer discards up to 3s of typing — P2, S
Autosave is a 3s trailing debounce (`useComposeSession.ts:440-448`) flushed
only on `visibilitychange` (`:452-462`); the close button (labelled
"draft is saved", `ComposeHost.tsx:106-113`) unmounts without flushing.
Same on switching reply targets. **Fix:** flush pending autosave on
unmount/close.

### A18. Thread rows duplicate across infinite-scroll pages — P2/P3, M
Bridge dedupes threads per page only with envelope-based offsets
(`crates/web/src/envelope_list.rs:189-221`, `lib.rs:180-181`); SPA merge
dedupes by message id (`useMailboxQuery.ts:110-119`) → busy threads appear
twice with inconsistent counts. **Fix:** dedupe by `thread_id` in the SPA
merge; longer-term, thread-aware pagination (pairs with B1 `ListThreads`).

### A19. P3 hygiene batch
- Every daemon error → 502, including user errors (invalid UUID, bad
  snooze time): `crates/web/src/auth.rs:15-27`; map to 4xx. P3, S.
- "Send failed" after successful send when post-send cleanup fails
  (`lib.rs:804-806`) — return success, log cleanup. P3, XS.
- Bulk snooze fans out `Promise.all` of singles; first rejection aborts
  with no per-message report (`features/mailbox/api.ts:376-378`,
  `SnoozeDialog.tsx:39-51`). P3, S.
- Palette "Mark read and archive" bypasses invalidation and undo
  (`features/mailbox/actions.ts:168-181`). P3, XS.
- Palette shows thread actions on `/m/label/*` pages
  (`actions.ts:35-39` regex). P3, XS.
- Unhandled rejections in compose cmd+shift+R / cmd+S
  (`useComposeSession.ts:634,652`). P3, XS.
- Bridge WS task never reads the client socket — dead browsers keep the
  daemon UnixStream alive until the next event send (`lib.rs:1709-1746`).
  P3, S.
- Search `account` param silently ignored — SPA sends it
  (`features/search/api.ts:80`) but bridge `SearchQuery` hardcodes
  `account_id: None` (`request_types.rs:76-98`, `lib.rs:391-399`). Fix in
  bridge; prerequisite for C4 account scoping. P2, S.
- Typed client 401 retry would throw on POSTs (`api/client.ts:42`,
  body-consumed Request reuse) — dormant until the typed client is
  adopted. P3, XS.

### A20. Remote images load by default — P2 (product/privacy), XS
`ThreadRoute.tsx:186` defaults `remoteImages` to `true` — tracking pixels
fire on open, in a privacy-first product whose TUI defaults remote images
OFF. **Fix:** default off + per-sender allow (TUI parity); keep the
existing toggle.

**Verified-solid (no action):** HTML sanitization (DOMPurify allowlist +
sandboxed `srcdoc` iframe, no `allow-scripts`), token scrubbing from URL,
bridge Host allowlist / loopback CORS / bearer enforcement, SPA CSP, and
compose autosave stale-write protection.

---

## Workstream B — Parity with CLI/TUI

Baseline: 185 IPC request variants (`crates/protocol/src/types.rs:462-1427`);
CLI exposes ~80 commands; the bridge (`crates/web/src/router.rs:88-201`,
`routes_v6.rs:2604-2763`) covers most, the SPA a subset.

### B1. Daemon capabilities with NO bridge route (Rust + SPA work)
| Capability | IPC request(s) | CLI/TUI surface | Priority |
|---|---|---|---|
| Owed-replies lens | `ListOwedReplies` | `mxr owed`, TUI Owed lens | P1 |
| Thread-grouped mailbox | `ListThreads` | `mxr threads`, TUI list-mode toggle | P1 |
| Async bulk mutations w/ progress | `StartMutationJob` | CLI/TUI large batches | P1 |
| Ask the archive | `ArchiveAsk` | `mxr ask` | P2 |
| Whois / explain entity | `ExplainEntity` | `mxr whois`, TUI whois modal | P2 |
| Decision log | `ListDecisionLog`, `GetDecision`, `RebuildDecisionLog` | `mxr decisions` | P2 |
| Cadence watchlist + drift | `WatchCadence`, `UnwatchCadence`, `ListCadenceWatch`, `ListCadenceDrift` | `mxr cadence`, TUI analytics Cadence Drift | P2 |
| Send-time recommendation | `SendTimeRecommendation` | `mxr send-time` | P3 |
| Saved-search unread badges | `ListSavedSearchUnreadCounts` | TUI saved-search tabs | P3 |
| Notification chimes | `Get/Update/PreviewNotificationChimes` | `mxr chimes` | P3 |
| Single invite detail / backfill | `GetInvite`, `BackfillCalendarInvites` | `mxr invite show`, `invites backfill` | P3 |
| Stored draft by id | `GetDraft` | `mxr drafts` | P3 |

Each: add bridge route (`crates/web`), OpenAPI entry, then SPA surface.
Keep handlers thin — daemon IPC already does the work.

### B2. Bridge routes that exist but have NO web UI
| Route | What's missing in the SPA | Priority |
|---|---|---|
| `DELETE /scheduled-sends/{draft_id}` | Can schedule a send but not cancel it — workflow trap | P1 |
| `POST /snoozed/{id}/wake`, `GET /snoozed` | No manual unsnooze in the snoozed lens | P2 |
| `POST /reminders`, `DELETE /reminders/{id}` | No set/cancel follow-up reminder (`remind-after`) from reply/compose | P2 |
| `GET /threads/{id}/export`, `POST /export-search` | No export from reader or search results | P2 |
| `POST /humanizer/score`, `/rewrite` | No humanizer gate in compose | P3 |
| `GET /drafts/orphaned`, `POST /drafts/{id}/reset-orphan`, send-stored, delete-stored | No orphaned/stored-draft recovery UI | P2 |
| `POST /labels/create` | No explicit label management (create; rename/delete exist in pickers only) | P3 |
| `GET /messages/{id}/headers`, `/body`, flags | No raw/headers view in reader (CLI `cat --view headers/raw`) | P3 |
| `POST /deliveries/scan`, `GET /deliveries/{id}` | No manual scan trigger / detail view | P3 |
| `GET /count` | Nothing shows match counts pre-action | P3 |
| `POST /relationship/rebuild` | Profile page shows but can't rebuild | P3 |
| `POST /saved-searches/run` | fine via lens — no action needed (verify) | — |

### B3. TUI interaction features missing on the web
- Pattern selection: TUI `PatternKind {All,None,Read,Unread,Starred,Thread}`
  (`crates/tui/src/action.rs:272,605`); web only has select-all/none
  (`*a`/`*n`). Add read/unread/starred/thread pattern selects to the bulk bar.
- Open-links modal (TUI `url_modal`, `L`): extract links from the current
  message and list them for open/copy.
- Visual line-mode selection (TUI `V`): stretch goal after A-keyboard work.
- Analytics parity: TUI has 8 dashboards; web lacks Cadence Drift (B1) and
  Search Aggregation (`SearchAggregation` request; bridge route exists —
  verify — else B1).
- Per-account mailbox scoping (TUI `SwitchAccount`): see C-IA account
  switcher item — web switcher must actually scope the mailbox.

### B4. Product-invariant violations — P1
Repo rule: "Mutations, destructive actions, and batch operations require a
dry-run or preview path" (CLAUDE.md; blueprint).
- `features/mailbox/api.ts:280,326` hardcode `dry_run: input.dryRun ?? false`
  and no UI ever passes `true` — unsubscribe-purge and route are destructive
  batch ops with zero preview.
- Bulk archive/trash/spam show a confirm count but no preview of *which*
  messages (dry-run selection must match real mutation path).
**Fix:** preview step in `BulkActionBar` confirm dialog + unsubscribe-purge
flow, powered by the existing dry-run params / `GET /count`.

---

## Workstream C — UX overhaul

### C1. Systemic foundations (do before polish)
1. **Design-token collision — the four themes don't work.** `styles/app.css:268-342`
   re-declares `:root`/theme blocks *after* importing `styles/tokens.css`,
   clobbering the designed palettes: midnight and eclipse both render the
   same generic magenta; paper can't render; `--color-star` maps to indigo
   (`app.css:61` → chart-4) instead of amber; radius collapses to 0
   (`app.css:82-88,292`) under components spelling `rounded-xl`; fonts split
   Inter (tokens.css:15) vs Roboto Variable (app.css:70). **Fix:** single
   token source (fold shadcn oklch blocks into tokens.css or delete them),
   consistent `--color-*` mapping, per-theme visual regression screenshots.
   P1, M.
2. **Two keyboard dispatch systems.** tinykeys registry (`lib/keymap.ts:46-72`)
   + raw `window.addEventListener("keydown")` in MailboxList
   (`features/mailbox/MailboxList.tsx:197-354`), ThreadRoute
   (`features/thread/ThreadRoute.tsx:357-437`), Sidebar, Screener. Double-fire
   risks: `g s` fires "Go to Starred" *and* stars the thread
   (`ThreadRoute.tsx:388-390`); `g a` vs reply-all; independent `g`-prefix
   timers. **Fix:** route page keys through the registry with scopes
   (`state/keyScopeStore.ts` exists); registry marks handled events. P1, M.

### C2. Reader/thread quality
- **No quoted-text collapsing** — nothing detects `gmail_quote` / `> ` runs /
  "On ... wrote:" (verified absent in `ThreadRoute.tsx`, `MessageBody.tsx`,
  `lib/sanitizeHtml.ts`). Table stakes for an email tool. P1, M.
- **No per-message collapse** — all messages render expanded
  (`ThreadRoute.tsx:736-746`); 30-message threads are a wall. Default:
  collapse read messages to header rows. P1, M.
- **Next/prev thread from reader** — only `[`/`]` archive-and-advance exists
  (`ThreadRoute.tsx:382-387`); add non-destructive next/prev using
  `siblingThreadIds()` (`:290-306`). P2, S.
- Reader loading is bare "Loading thread…" text (`ThreadRoute.tsx:140-146`);
  use skeleton to stop split-pane flash on j/k. P2, S.
- HTML-mail dark theme hardcodes a warm palette (`MessageBody.tsx:23,140`)
  that matches no app theme. P3, S.
- **[live]** three-pane layout clips reader content at ≲1200px wide
  (buttons/timestamps cut off); breadcrumb shows the raw thread UUID
  instead of subject (`components/Topbar.tsx:55-71`). P2, S.

### C3. State coverage (loading / empty / error)
- **Screener**: no loading state — fetching renders "Queue empty"
  (`features/screener/ScreenerRoute.tsx:150-157`), account-gate same
  (`:38-45`); `decide` mutation has no `onError` (`:101-114`). P1, S.
- **Analytics**: no loading skeletons anywhere; "No data yet. Run sync…"
  shows during fetch (`AnalyticsDashboardRoute.tsx:698-703`); `rebuild`
  has no onError/completion signal (`:66-69`). P2, M.
- **Mailbox empty state** ambiguous copy, no action
  (`MailboxList.tsx:386-394`); distinguish syncing vs empty (connection
  store has `syncProgress`), offer "Run sync". P2, S.
- **Onboarding**: OAuth step dumps raw state string
  (`OnboardingRoute.tsx:203-206`); step 4 shows full "Ready" bar before
  sync starts (`:314-326`). P2, S.
- Settings: unknown `/settings/$slug` renders blank pane
  (`SettingsRoute.tsx:49-61`). P3, XS.
- Silent-failure mutations missing `onError`: accounts makeDefault/disable
  (`AccountsListRoute.tsx:13-26`), analytics rebuild, screener decide. P2, S.

### C4. IA / navigation
- **Account switcher doesn't switch accounts** — items are raw `<a>` links
  to settings pages (`components/AccountSwitcher.tsx:64-88`); no way to
  scope the mailbox to an account (search accepts `account` param but no UI
  sets it). Make it set an account filter + use router `Link`. P1, M.
- **Search → open result → Escape dumps you in the inbox**, not back in
  results (`MailboxList.tsx:491-494` falls back to `"inbox"`;
  `ThreadRoute.tsx:375-381` returns to mailboxPath; analytics drill
  hardcodes inbox `AnalyticsDashboardRoute.tsx:224-229`). Preserve origin.
  P1, M.
- **Two compose entry flows**: topbar button → launcher dialog → full-page
  route (`components/Topbar.tsx:35`, `ComposeLauncher.tsx:57-66`) vs `c` →
  overlay host (`navigationActions.ts:59-66`). Consolidate on the overlay.
  P2, S.
- Sidebar: 10 workspace tools before the first mail lens
  (`components/Sidebar.tsx:46-57`) — lenses should lead; **[live]** the
  numeric badges (Mail 1 … Settings 0) read as counts next to real counts
  (INBOX 504); two different "Subscriptions" entries (workspace page +
  label) and duplicate icons (Sparkles for Subscriptions and Snoozed,
  `Sidebar.tsx:52,62`). Restructure + render shortcut hints as keycaps.
  P2, M.
- Numeric shortcut drift: sidebar advertises `2` = Search page while
  registry maps `2` to palette (`navigationActions.ts:44`). Audit 1–0. P3, XS.
- **[live]** no responsive adaptation: at ~760px the sidebar clips mid-word,
  dates/buttons overflow (`styles/app.css:201-214` collapses to 56px only
  below 768px; topbar fixed `w-[340px]` search `SearchInput.tsx:13`;
  BulkActionBar overflows). Half-screen tiling is a power-user norm. P2, M.

### C5. Feedback / optimistic UI
(The optimistic architecture — snapshot/rollback + 60s Undo toasts +
undo-send countdown — is well designed but currently **inert**: see A10.
Fix A10 first; the items below build on it.)
- Reader star/read not optimistic — patch `["thread"]` cache alongside the
  mailbox pages (part of the A10 rewrite). P2, S.
- Label edits: dialog spins then toast; no inline chip update
  (`ThreadRoute.tsx:322-335`). P3, S.
- Bulk ops: no link from toast to `/jobs` progress; add "View progress"
  action for large batches (pairs with B1 StartMutationJob). P2, S.
- Toasts render top-right while actions happen bottom (bulk bar/status
  bar, `components/ui/sonner.tsx:12`). Consider bottom-center. P3, XS.
- `row-exit` / `pending-pulse` animations defined but unused
  (`styles/app.css:134-161`) — wire into archive/trash or delete. P3, XS.

### C6. Accessibility
- j/k cursor is synthetic — `focusedId` ring only; DOM focus/AT never move
  (`MailboxList.tsx:63,464`, `MailboxRow.tsx:65-66`). Use
  listbox + `aria-activedescendant` (SearchPalette already does it right,
  `SearchPalette.tsx:133,148-155`). P2, M.
- Hover-only row controls: checkbox `opacity-0 group-hover:opacity-100`
  without `focus-visible:opacity-100` (`MailboxRow.tsx:96`); quick actions
  `hidden group-hover:flex` unreachable by keyboard (`MailboxRow.tsx:173`).
  P2, S.
- Analytics tabs: `role="tab"` on plain Links without tabs contract
  (`AnalyticsDashboardRoute.tsx:97-127`). P3, XS.
- 11px mid-gray meta text likely fails AA on some themes — audit after C1
  token fix. P3, S.
- Screener triage keys `a/d/f/p` exist (`ScreenerRoute.tsx:257-270`) but
  are undiscoverable — no hints on buttons, absent from
  `lib/pageKeyHints.ts:68-82`. P2, XS.
- Help dialog filters by fake context — hardcoded `selectionCount: 0`
  (`components/HelpDialog.tsx:37-51`). P3, XS.

### C7. Dead / half-finished surfaces
- Analytics drill-down rail renders raw JSON — no `analytics-drilldown`
  case in `components/RightRail.tsx:118`. P2, S.
- "Share as image" copies text, not an image
  (`AnalyticsDashboardRoute.tsx:608-638`) — rename or implement. P3, XS.
- ThemePicker "More in /settings/theme" is a disabled item, not a link
  (`ThemePicker.tsx:54-56`). P3, XS.
- `/dev` health card queries `/api/v1/admin/health`; bridge serves
  `/api/v1/health` (`routes/dev.tsx:24` vs `router.rs:194`) — dev-only 404.
  P3, XS.
- Raw `<a>` full-reload links in RightRail sender profile
  (`RightRail.tsx:294`) and AccountSwitcher. P3, XS.
- Date formatting drift across surfaces — centralize one formatter
  (`lib/utils.ts` has `formatRelativeAge`). P3, S.

---

## Execution phases

| Phase | Contents | Effort |
|---|---|---|
| 0. Ship-stoppers | A0-a duplicate send, A0-b legacy redirects, A10 optimistic no-op, A2 first-session WS | 4 fixes, S–M |
| 1. Correctness sprint | A1, A3–A9, A11–A18, A19 batch, A20 + C3 screener states + silent onError fixes | S each, 1 pass |
| 2. Foundations | C1 tokens+themes, C1 keyboard unification, C6 focus model | M each, sequential |
| 3. Reader parity | C2 quoted-text + message collapse + next/prev, C5 thread-cache optimism | M |
| 4. Parity: UI over existing routes | B2 table (cancel scheduled send first), B4 dry-run previews, B3 pattern select + links modal | S–M each |
| 5. Parity: new bridge routes | B1 table (owed + threads + mutation-jobs first) — Rust route + OpenAPI + SPA per row | M each |
| 6. IA & polish | C4 account scoping (needs A19 search-account bridge fix), search-return, sidebar restructure, responsive pass, C7 cleanup | M |

Ordering rationale: Phase 0 items either corrupt outbound mail, break the
app on refresh, or invalidate the UX the rest of the plan builds on.
Token/keyboard foundations change APIs phases 3–6 build on; new bridge
routes (5) are independent and can run parallel to 4/6 in a worktree.

## Verification (every phase)
- `scripts/cargo-test -p mxr-web --tests` for bridge changes; `cargo build -p mxr`.
- `apps/web`: `npm test` (vitest), `npm run build`, Playwright e2e (`apps/web/e2e`).
- Live: `pkill -f 'mxr daemon'` … `cargo run --bin mxr -- daemon --foreground`,
  `mxr web --print-url`, drive with Playwright; verify against CLI ground
  truth (`mxr search/threads/owed --format json`).
- Per repo rule: preview/dry-run selection must match the real mutation
  path — add a test asserting both call the same query builder.

## Unresolved questions
1. B1 scope: all 12 rows or top-5 (owed, threads, jobs, ask, whois)?
2. Account scoping (C4): per-lens filter param vs global account context?
3. Sidebar numeric hints: keep number-jump shortcuts at all, or drop for
   `g`-sequences only?
4. Thread-grouped mailbox (B1): default the web list to threads (TUI
   parity) or keep envelope rows with a toggle?
5. Compose entry: OK to kill the full-page `/compose/new` route and always
   use the overlay (deep-links redirect)?
6. Humanizer in compose (B2): inline gate like TUI or defer?
