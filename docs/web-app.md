# mxr web app

The mxr web app is a Vite + React 19 SPA in `apps/web/` that talks to the mxr
daemon through the HTTP and WebSocket bridge in `crates/web/`. Release builds
embed it in the `mxr` binary and `mxr web` opens it. The TUI is the reference
for behaviour and `DESIGN.md` for look; `docs/web-app-rubric.md` is the bar a
change is judged against.

This doc records why the app is built the way it is and what a maintainer must
not break. For what the code does, read the code.

## Architecture

```
Browser (apps/web SPA)  ──HTTP+WS──>  bridge (crates/web)  ──Unix socket──>  daemon
```

- The SPA never calls a mail provider. The bridge is its only server, and the
  bridge is a thin client of daemon IPC.
- Every capability lands daemon-first (IPC plus CLI JSON), then the bridge
  route, then the web surface. A web-only capability is incomplete.
- The bridge binds loopback only and checks the Host header against DNS
  rebinding.

## Locked decisions

| Concern | Choice | Why |
|---|---|---|
| Framework | React 19, TypeScript strict, Vite 7 | |
| Routing | TanStack Router, file routes, auto code splitting | Typed params; route components load lazily |
| Server state | TanStack Query v5 | Realtime invalidation from daemon events |
| UI state | zustand | Small, selector based, usable outside React (key runners) |
| Styling | Tailwind 4 with our own tokens (`styles/tokens.css`) | One token source; shadcn-style components read it |
| Keyboard | One dispatcher over the action registry (`lib/keys`) | See "Keyboard" |
| Compose editor | CodeMirror 6 + vim by default, Tiptap opt-in | Vim-first users; Tiptap lazy |
| HTML mail | DOMPurify, then a sandboxed `srcdoc` iframe | See "Reading a conversation" |
| Charts | No chart library; `BarList` in `features/analytics/analyticsParts.tsx` draws CSS bars | Long labels stay readable and every row is a keyboard drill-down |
| Distribution | `apps/web/dist` embedded with `include_dir!` behind the `web-ui` feature | One artifact |
| Responsive floor | 900 px for the desktop shell; under 640 px, five tabs (`components/MobileTabs.tsx`) | Blueprint 22 puts Now and the modes on a phone, inside the five-tab limit |
| UI prefs | One global set in `state/uiPrefsStore.ts` | No per-account prefs |

Rejected, do not propose: provider calls from the SPA, per-account UI prefs,
redirect-based OAuth in the SPA (device code is canonical). A desktop wrapper
was rejected on 2026-05-12 and reversed on 2026-10-16 (D142): the only
wrapper is the thin shell in `apps/desktop/`, below. The phone layout was rejected until blueprint 22 asked for Now and
the modes on a phone; it is five tabs over the same routes, not a separate
build.

## `mxr web` launch model

`crates/daemon/src/commands/web.rs`:

- Local: opens `http://mxr.localhost:42829`, reusing a healthy daemon-hosted
  bridge or spawning a detached one. `mxr web stop` stops a detached bridge;
  `--foreground` keeps it attached.
- Remote: `--remote-host <host>` opens
  `https://<host>/#token=<token>&remote=<host>` with the token from
  `bridge-tokens/<host>.token`. Nothing binds locally; TLS is the remote
  operator's job.
- Port conflicts fail unless `--auto-port`; the bound port is written to
  `<config_dir>/bridge-port` for the Vite proxy and scripts.

## Auth

1. The token lives at `<config_dir>/bridge-token` (0600), created on first use.
2. `GET /api/v1/auth/local-token` returns it to loopback peers when
   `[bridge].auto_local_token` is on (default). The SPA calls it on cold start
   and on 401, so local use never asks for a token.
3. Otherwise `#token=…` in the URL is stored and scrubbed from history, and
   `/settings/token` offers a paste field.
4. `#remote=<origin>` points the app at another bridge
   (`lib/tokenStorage.ts`). Any page can link there, so switching to a new
   origin asks for confirmation first, and the stored local token is cleared
   rather than sent to it: the remote link brings its own `#token=` or none.

The WebSocket authenticates with `Sec-WebSocket-Protocol: bearer, <token>`.
The socket starts before the handshake finishes on a first visit, so
`tokenStorage.onTokenChange` tells it to connect when the token lands.

## Desktop shell

`apps/desktop/` is a Tauri 2 shell for macOS. It is a window around this app,
not a second app: it bundles no frontend and no daemon, and it has no updater.
It is outside the root Cargo workspace (own `[workspace]`, `Cargo.lock` and
target dir) so workspace CI never builds tauri.

- Launch: runs `mxr web --print-url --no-open`, finds `mxr` on `PATH` plus
  `/opt/homebrew/bin`, `/usr/local/bin`, `~/.cargo/bin` and `~/.local/bin`,
  and loads the one URL it prints (never logged). No `mxr`, a failure or 15s
  without a URL shows `ui/down.html`, a single inline-CSS page with the
  command to start the daemon and a Try again link.
- Shortcut: Control+Option+Command+M (override with `MXR_DESKTOP_SHORTCUT`)
  focuses the window and navigates it to `/compose/new`, the deep link above,
  so it opens the same composer as `c`. Do not rename or break that route; the
  shell depends on it (`e2e/compose-send.spec.ts`).
- One instance (`tauri-plugin-single-instance`), window size and position
  restored (`tauri-plugin-window-state`). Closing hides the window; Command+Q
  quits.
- Trust boundary: no capabilities and no `invoke` handler, so no page gets
  Tauri IPC. `on_navigation` allows only the bridge origin it was given and
  the down page; other `http(s)` and `mailto:` links open in the default
  browser through the opener plugin, and `target=_blank` never opens a window.
  Try again is a navigation the shell intercepts, not a command.
- Build: `cd apps/desktop && npm ci --ignore-scripts && npx tauri build --bundles app`
  (unsigned). CI is `.github/workflows/desktop.yml`.

## Serving and deep links

With `--features web-ui` the bridge serves `apps/web/dist` at `/` with a strict
CSP (`img-src 'self' data: blob:`, no inline script) and falls back to
`index.html` for client routes. Without a built SPA it serves
`crates/web/src/spa_placeholder.html` from `crates/web/spa-empty-dist/`.

`crates/web/src/legacy.rs` still redirects v0.4 API paths, but only for API
clients. A browser navigation (`Accept: text/html` or `Sec-Fetch-Mode:
navigate`) always gets the SPA, so refreshing `/search`, `/rules` or `/drafts`
works. The redirect is a 308 so a POST keeps its method; browser navigations
never receive it, so a browser cannot cache it.

Dev: `cd apps/web && npm run dev` (Vite on 5173) proxies `/api` and the event
socket to the bridge found through `MXR_BRIDGE_URL` or the `bridge-port` file.

## Routes

URL is the state. The reader is a child route of the list it was opened from,
so the list stays mounted (scroll and cursor survive) and `Esc` returns to the
exact origin, query string included.

| URL | View |
|---|---|
| `/m/<mailbox>` and `/m/<mailbox>/<thread>` | System lens: inbox, starred, sent, archive (All Mail), spam, trash |
| `/m/label/<slug>/<thread?>` | Label lens (slug keeps letters in any script) |
| `/m/saved/<slug>/<thread?>` | Saved search |
| `/search?q=…&mode=…` and `/search/<thread>?…` | Search results with the reader |
| `/reply-queue/<thread?>`, `/owed/<thread?>`, `/snoozed/<thread?>` | Triage lists on the same list+reader component |
| `/drafts`, `/screener`, `/invites`, `/subscriptions`, `/deliveries` | Triage pages |
| `/now`, `/messages/<thread?>`, `/todo/<thread?>`, `/reading` and `/reading/<thread>` | The modes; `/reading/item/<key>` is an item's reader. `/find` is the phone's Find tab |
| `/updates` and `/updates/<thread>` | Updates: the digest as Needs a look, Changed and Routine, with since below; the reader opens beside it. `/paper-trail` redirects here, keeping the thread |
| `/analytics/<dashboard>`, `/rules`, `/accounts`, `/diagnostics`, `/activity`, `/jobs`, `/settings/<section>` | Tools |
| `/compose/new`, `/compose/<draft>` | Deep links that open the compose host |
| `/sender/<address>` | Sender profile page |
| `/archive` | Archive the mode: the answer box, the ledger of records by month and the record card. Not All Mail, which is `/m/archive` |

`features/mailbox/lenses.ts` turns the shell's sidebar items into typed lenses.
An unknown lens renders "No label by that name"; it never falls back to another
mailbox.

Triage lists pass a `rowAction` to `ListWithReader`, bound to `w`: **Wake now**
on `/snoozed` and **Done** on `/reply-queue`. `/snoozed` groups entries into one
row per conversation, so waking a row wakes every snoozed message in it.

## Layout

- Sidebar (248 px, icon rail below 1280 px): account switcher, mailboxes,
  triage, labels, saved searches (with unread counts), tools. Sections fold.
- Topbar: breadcrumb (lens, then subject), search, compose.
- Main: `features/mailbox/ListWithReader.tsx` for every mail list. With nothing
  open the list is full width; with a thread open it narrows beside the reader;
  below 1024 px or in full-width reader mode the reader replaces it.
- Right rail: context panels (sender, briefing, attachments, whois, ask the
  archive). Below 1024 px it overlays.
- Assistant: `features/assistant/`, a resizable pane beside the whole shell
  (`AssistantSplit`, parked while closed), closed until opened with `Mod+.` or
  the topbar button and kept across pages; an overlay drawer below 1024 px.
  Turns run in the daemon; the pane refetches on `AssistantUpdated` events.
  See [blueprint 25](blueprint/25-assistant.md).
- Status bar: connection, sync progress or a Sync button, live key hints.

Non-mail pages use `components/Page.tsx` (`Page`, `PageSection`, `PageTabs`).

## Install mxr as an app, and keep extensions out of its keys

### Install it

Chrome, Edge and Brave offer to install the web app when it is open. While
the browser can install it, the status bar shows **install app** and the
command palette has **Install as app**. Installed, mxr opens in its own
window. Hide the offer with its **×**; it stays hidden after that. Firefox
and Safari do not fire the install event, so the offer does not show there.

### Stop extensions reading mxr's keys

Extensions that run on every page can take mxr's single keys (`j`, `k`,
`g`, `/`) before the app sees them. Give mxr's address its own rules.

- **Chrome:** Open the extension's **Details** from **Manage extensions**.
  Under **Permissions**, either set site access to **When you select the
  extension**, or remove `mxr.localhost` from **Allowed sites** with
  **More** → **Remove**. Chrome's own help uses these names.
- **Firefox:** Open the extension from **Add-ons and themes** → **Extensions**
  and turn off its access to data on websites. Mozilla's docs do not show
  a per-site setting for this, so check your version's options for one.
- **Safari:** Open **Safari Settings** → **Extensions**, choose the
  extension, and set `mxr.localhost` to **Deny** under its websites.

Keyboard-shortcut extensions such as [Vimium](https://github.com/philc/vimium)
clash with single-key shortcuts. In Vimium, add an excluded URL rule with
the pattern `*mxr.localhost*`. Vimium patterns are JavaScript regular
expressions, and `*` matches any run of characters. Leave the excluded keys
empty to turn Vimium off on that address. Vimium's options page and its
toolbar popup both edit these rules.

## Keyboard

`lib/keys/dispatcher.ts` is the app's one key dispatcher. Every key is an action
in the registry (`lib/actions/`), including list and reader motion. Only two
capture listeners sit in front of it: the right rail's `Esc` (closes the rail
before the dispatcher closes the thread) and `lib/keys/typeAhead.ts`. The
latter holds printable keys typed straight after `⌘K`/`Ctrl+K`, `/` or `g l`
for up to 1.5 s and types them into the palette field once it takes focus, so
"⌘K settings" does not lose its first letters. The message iframe forwards
its keystrokes to the dispatcher.

- An action is global or bound to scopes: `sidebar`, `list`, `reader`,
  `screener`. A scoped action names a `command`; the mounted view registers a
  controller with `useScopeController(scope, {command: fn})` and pushes its
  scope with `useShortcutScope(scope, active)`. `e` archives the focused row in
  the list and the open thread in the reader through the same verb.
- Chords use `KeyboardEvent.key` tokens (`?`, `G`, `#`, `g i`, `Mod+k`), so `?`
  and `/` never collide. Off macOS a `Ctrl+` chord also matches `Mod+`, except
  browser essentials (Find, Print, Save, tabs).
- Keys are ignored in fields, dialogs and menus, except global modifier chords
  like `⌘K`. A dialog animating closed no longer owns keys. While a palette,
  help, a mail dialog or an overlay composer is open the dispatcher stands down.
- Bindings follow the TUI's live handler (not its help text, which drifted).
  Deliberate differences carry a `tuiNote`, shown in help and the docs.
- The palette, help (`?`), status hints, Settings → Keybindings and
  `site/.../reference/keybindings.md` all read the registry. A unit test fails
  if the published table drifts; `UPDATE_KEY_DOCS=1 npm test` rewrites it.

Adding a key: add an action to the feature's `actions.ts` (or `paneActions.ts`
for motion), implement the command in the view's controller, done.

## Mail actions

`features/mail-actions/` owns acting on mail.

- `performMailAction(action, messageIds, options)` is the only mutation path.
  It records a pending operation, sends the request through the serial
  mutation queue, toasts success with Undo, refreshes every mail query family
  (`invalidateMailQueries`), and only then retires the operation.
- Views render server rows with pending operations projected on top
  (`pendingMailOps.ts`). The projection is lens aware: archive removes a row
  from Inbox but not All Mail; label removal only from that label's lens;
  trash and spam everywhere but their own lens. A failed operation is removed,
  so it cannot undo another action's effect, and a refetch mid-flight cannot
  make a row flicker back.
- Thread rows carry `message_ids`, so an action covers the whole conversation
  (TUI thread mode). A row is starred when any message in its conversation
  is, including messages outside the lens (a starred reply in Sent); the row
  names those in `starred_message_ids`, so its unstar (`MailTarget.unstarIds`)
  reaches them too. Starring stars the listed messages.
- Batches of 200 or more run as daemon mutation jobs with a progress toast;
  Undo reverses every chunk. Snooze undoes by waking the messages.
- `u` (or `z`) undoes the newest action for 60 s. The daemon records undo for
  archive, trash, spam, read changes, stars, moves and label edits (moves and
  label edits restore the prior label set; a star restores each message's
  own prior star, so undoing a conversation star leaves an already-starred
  message starred). See `undoable_kind` in
  `crates/daemon/src/handler/mutations.rs`.
- Verbs (`mailVerbs.ts`) are written once and used by the list, the reader, the
  bulk bar and the palette. Trash or spam of several conversations, and any
  batch over 20 messages, confirm first with a list of what will change.
- Dialogs (snooze, labels, move/route, unsubscribe, links, confirm) live in
  `dialogs/` and open through one store, so every surface opens the same one.
- Unsubscribe-and-archive shows the count from a `dry_run` of the same daemon
  request before it runs.

## Updates

`features/updates/` renders the digest as a notification reel.

- A row is the source, its fact and at most one trailing value. `trailingItem`
  (`digestView.ts`) picks it in a fixed order: already in To do, the To do
  affordance, the computed change, the first amount the fact leaves out, then
  a routine count. Anything else waits for the peek.
- The row's keys and More menu replace the trailing text while the row is
  focused or hovered on a wide screen with a fine pointer. A narrow or touch
  screen has no hover, so they always show.
- `Space` opens the peek under one row at a time (`UpdateLineRow.tsx`): the
  first four lines of the latest email (`emailPreview.ts`, read through the
  reader's `["thread", id]` query only once the peek is open), why the row is
  here, the numbers the mail quoted, parcel items and references, and the
  link and the email. `Escape` closes it, then a hint.
- Routine is one line until `Enter` opens it, and stays open for that visit.
  "Let go of all" lives in the header at every width and only while the digest
  has something to let go of.
- Subject-only lines ask for a gist with `requestRowGists(..., { automated:
  true })`. The daemon writes it and drops it unless every number in it is in
  the mail (`crates/daemon/src/handler/gist_numbers.rs`); a dropped gist leaves
  the rule's fact on the row.

## Reading mode

- `features/reading/`: the edition (`ReadingRoute.tsx`: bands, You left off
  here, lead cards, digest links, Later via `?view=later`), the reader
  (`ReadingReader.tsx`: 66ch column, minutes left, progress line,
  Issue/Article, `R` original layout through the sanitized message view,
  engagement every 10 s and on leave, `h` highlights, and `HighlightToolbar.tsx`:
  a selection inside the article text offers Highlight and Highlight with note,
  and a selection equal to a saved highlight's words, which is what clicking a
  drawn highlight selects, offers its note and removal; Marginalia's are
  read-only. Notes and removals go over `/mail/reading/highlights/{id}/note`
  and `/remove`, with the daemon's undo) and the previews
  (`ReadingDialogs.tsx`: unsubscribe with evidence over the purge dry run,
  let go of all over the `SetModeDone` dry run, committing the same
  thread ids). Pure helpers live in `readingView.ts`.
- The reader renders the daemon's cleaned HTML (`ReadingItemDetailData.html`,
  or the article's) inline through `@planetaryescape/reader`, after
  `sanitizeReaderHtml` (`lib/sanitizeHtml.ts`): the email tag allowlist and
  tracker removal, remote images held until "Show images" or a trusted
  sender, and no `style`, `class`, `id`, relative links or relative images,
  so it can't restyle or navigate the app. With no HTML, or nothing left
  after sanitizing, it falls back to the daemon's paragraphs as text. The
  sender's layout (`R`) stays in the sandboxed iframe.
- Reader display (`ReaderSettingsMenu.tsx`, the **Aa** button): theme
  (Match app maps midnight, eclipse, light and paper to Ink, Night, Snow
  and Paper), font, size and line spacing, kept in `uiPrefsStore`
  (`readingDisplay`) as a client preference. Quote highlights are placed by
  matching their text in the rendered article, in the paragraph they were
  made in when the highlight has one (`paragraph` on the highlight, captured
  from the selection's start by `paragraphAnchor.ts`). The anchor is used
  only for mxr's own highlights shown in the view they were made in
  (`marksForView`), because each view and Marginalia number paragraphs
  differently; any other highlight marks its first match.
- The article is fetched only on `L` or the Article tab, or on `M` for a digest link whose article isn't saved (Marginalia needs it first), after the toast
  names the site; `b` on a link saves it and then fetches it so it reads
  offline.
- Scopes `reading` and `reading-reader` in `lib/actions`; swipe right is
  Later and left is let go on phones.

## Reading a conversation

- Read messages collapse to one line; the newest and every unread message start
  open. `J`/`K` move between messages, `o` toggles one, `X` all.
- Quoted history and signatures fold, in text (`textSegments.ts`) and in HTML
  (`htmlQuote.ts`, Gmail/Apple/Outlook/Yahoo markers). `Q` and `S` show them.
- Views: formatted HTML (`H`), reader text (`R`), plain.
- HTML goes through `lib/sanitizeHtml.ts` (DOMPurify, no style exfiltration,
  tracker pixels stripped) into a `srcdoc` iframe with
  `sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"` and
  a `script-src 'none'` CSP. No `allow-scripts`, ever. `allow-same-origin` is
  there to size the frame and forward keystrokes to the app, since the frame
  runs no script of its own.
- In Settings → Reader, set **HTML mail canvas** to Auto, Dark or Original. In a
  light scheme, Auto shows the sanitized original. In a dark scheme, it adapts
  simple mail and uses a paper canvas for designed mail. Dark uses a dark
  canvas; Original keeps the sanitized sender colors on a light canvas. New or
  missing preferences use Auto, while saved Dark and Original choices remain
  unchanged.
- Use **Appearance** above a formatted message to choose Auto, Dark or Original
  for that message. **Use sender/default** clears the message choice and returns
  to the sender preference or global setting. To keep Dark or Original for a
  sender, choose **Always use Dark** or **Always show Original**. Reset that
  sender in the same menu or under Settings → Reader → HTML mail preferences by
  sender. These preferences live in browser UI prefs, not daemon state.
- Remote images are blocked by default; `M` loads them for the thread and
  "Always from <sender>" remembers the sender in UI prefs.
- Inline `cid:` images load through `GET /mail/messages/{id}/inline-image?source=`,
  which serves only parts the daemon materialized for that message, and become
  `data:` URIs.
- Links (`L`), raw headers (`g H`), open original (`O`), export (`E`),
  summary (`y`, automatic for long threads only when an LLM is configured).

## Compose

- One host (`features/compose/ComposeHost.tsx`), inline under the thread, as an
  overlay, or fullscreen. `/compose/*` deep links open it.
- A new message focuses To; a reply focuses the body.
- Autosave flushes on close and on switching targets. One send lock per session
  covers the safety check and the undo window, so a double press sends once.
- Send confirmation shows recipients, From and the safety verdict; BLOCKED
  needs an explicit override. Suggested collaborators add as Cc.
- Send later goes through `POST /compose/session/schedule`, which parses the
  compose file like send does, so reply headers and invite answers survive.
  Scheduled sends are listed on the Drafts page and can be cancelled.
- Send and remind uses the `message_id` the send response returns.

## Search

The search palette (`/`) shows live suggestions; Enter opens `/search`. Results
use the same list and keys as a mailbox. Operators render as removable chips,
sender/list/category facets narrow the query, the account scope applies, and
saved searches can be created, renamed, re-queried, pinned and deleted.
`g f` filters the loaded list in place (TUI `Ctrl-f`).

## Realtime

`lib/ws.ts` holds one socket with backoff and a heartbeat (the bridge answers
`ping` and releases the daemon socket when the browser goes away).
`hooks/useDaemonEventInvalidation.ts` handles the daemon's real events
(`NewMessages`, `MessageUnsnoozed`, `LabelCountsUpdated`, `Sync*`,
`Operation*`, `ReminderTriggered`, `MutationReconciliationFailed`,
`EventsLagged`) and refreshes mail through `invalidateMailQueries`.

## Design system

- `styles/tokens.css` is the only place type, radii and density live. Colours
  are generated per theme: `styles/themes.ts` holds each theme's inputs (a
  base, an accent and a contrast level) and `npm run gen:themes` writes
  `styles/themes.generated.css`, which `tokens.css` imports and lint checks is
  current. Colours are complete values; Tailwind maps them once. The default theme is
  DESIGN.md's midnight (navy ink, cyan signal, yellow cue); light, eclipse and
  paper are alternates; `system` follows the OS live. `data-scheme` drives
  `dark:` utilities.
- Type sizes come from the scale in `tokens.css`: `text-2xs` (11 px) through
  `text-xl` (20 px) for the app, `text-2xl` to `text-4xl` above that. A literal
  size such as `text-[13px]` fails `npm run lint` (`scripts/check-type-scale.mjs`,
  with `check-motion.mjs` beside it). A file that needs one goes on the
  script's `ALLOWLIST` with its reason; keep that list short.
- Cyan (`--primary`) means focus, selection and the primary action; counts and
  unread marks use `--unread-marker`, which is the accent except in Paper
  (rust reads as error red there, so it is `--text-1`). It
  carries no other meaning, so a status, a progress bar or a decoration is
  never cyan. Amber (`--warning`) means attention, and green (`--success`)
  success.
- `components/Chip.tsx` is the one small label for a status or a membership
  ("In To do", "from a rule", a label name): neutral by default, with
  `attention` and `success` tones, one height, radius and size. Pass `asChild`
  to make a link or button look like one. `components/ui/badge.tsx` still
  serves Settings and Compose; new mode UI uses `Chip`.
- A mode page is a `ModeHeader` (title, a one-line subtitle, right-aligned
  actions) over a `ModeFrame` whose `width` is `list` (72 rem), `reader`
  (980 px) or `wide`. `wide` pages split into `ModeColumns` once the page is
  1280 px wide, inside a `mode-page` container. `SectionHeading` is the
  section label (mono caps, an optional count) and `KeyLine` the key hints at
  the foot of the page, on desktop only. `Page` shares `PageTitle` so every
  title has one size.
- `components/Avatar.tsx` draws a sender's initials on a hue taken from
  their name, so one sender wears one colour in every mode.
- Teaching hints (`AnchoredHint`) sit in the flow under the thing they explain,
  never over a row.
- Recursive is the only font; its MONO axis (`.font-mono`, `code`, `kbd`) is the
  monospace. Weight stays on `font-weight`.
- `styles/base.css` sits in the base layer. An unlayered reset there once beat
  every utility on buttons; keep it layered.
- Rows lay out by their list's width (container queries), not the window's.

## Accessibility

- The mail list is a `listbox` with `aria-activedescendant`; DOM focus follows
  the active pane. Row controls are mouse affordances; every action has a key.
- Dialogs return focus to the pane that opened them.
- Enter and Space on a focused button, link or checkbox keep their native
  meaning. `prefers-reduced-motion` removes animation.
- Axe runs in the Playwright suite on the main routes.

## Bundle

The entry chunk is about 63 kB gzipped. Compose (editors, sanitizer, address
parsing), the palettes, help, mail dialogs and the right rail load the first
time they open (`components/AppShell.tsx`). Route search params use small
validators (`lib/searchParams.ts`) instead of zod. Budget: 70 kB gzipped.

## Testing

- Vitest + Testing Library (`npm test`, about 350 tests). Tests assert
  what the user sees or what reaches the network, using real `QueryClient`
  cache shapes; they mock modules with `vi.mock` (no MSW).
- Playwright (`npm run e2e`) runs journeys against a real daemon with the
  FakeProvider demo dataset (`scripts/e2e-server.mjs`). CI runs the whole suite.
- Rust: `cargo test -p mxr-web`.

## Maintaining

- New bridge route: handler in `crates/web/src`, OpenAPI entry in
  `openapi.rs`, test, then `npm run gen:types` and commit
  `apps/web/src/api/generated.ts` as the generator writes it (do not format it).
- New mail verb: `mailVerbs.ts` plus an action in `verbActions.ts`. It then
  works in the list, reader, bulk bar and palette.
- New list page: `ListWithReader` plus a `<route>.$threadId.tsx` child route
  rendering `ThreadPane`.
- Never edit `routeTree.gen.ts`; the router plugin writes it.

## History

- 2026-05: first release in eleven phases (see git history).
- 2026-09: rebuilt to `docs/web-app-rubric.md`. One key dispatcher, projected
  mutations, list+reader routes, reader folding and remote-image blocking,
  thread-aware paging, one compose host, token system and responsive shell,
  plus bridge routes for owed replies, whois, cadence, saved-search counts,
  mutation jobs, scheduled-send listing, archive questions and inline images.
