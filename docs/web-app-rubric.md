# Web app quality rubric

What "great" means for the mxr web app, written so each line can be checked
against the running app rather than argued about. Use it to score a change, to
decide what to fix next, and to review PRs that touch `apps/web` or
`crates/web`.

The bar is the best keyboard-first mail clients (Superhuman, Mimestream,
Fastmail, mutt/aerc for the power user), adjusted for what mxr is: local-first,
daemon-backed, and read by people who live in a terminal. The TUI is the
reference implementation for behaviour; `DESIGN.md` is the reference for look.

## Scoring

Each criterion scores 0 to 3.

| Score | Meaning |
|---|---|
| 0 | Absent, or broken enough that a user would stop trusting it |
| 1 | Present but flawed: works on the happy path, fails an edge the user will hit |
| 2 | Solid: correct, complete, verified in a browser against a real daemon |
| 3 | Best in class: a keyboard user would notice and prefer it |

**Pass bar:** every criterion at 2 or better, and every criterion in sections
1 and 2 (the triage loop and reading) at 3. A criterion only scores 2 or more
when a Playwright journey or a recorded browser session against the
FakeProvider daemon shows it working. Unit tests alone cap a score at 1.

## 1. The triage loop

The loop a user runs a hundred times a day: see mail, decide, act, move on.

| # | Criterion | How to check |
|---|---|---|
| 1.1 | **Instant list.** Inbox paints rows from cache on revisit, virtualizes long lists, holds its scroll position, and never shifts layout as data lands. | Open inbox, open a thread, return: same row focused, same scroll offset. 5k rows scroll without dropped frames. |
| 1.2 | **Actions feel instant and are honest.** Archive, trash, spam, star, read/unread, label, move and snooze update the view before the network returns; failures roll back only their own change and say why. | Delay the bridge response: the row leaves immediately. Fail it: the row returns with an error toast. A second action during the first survives the first's rollback. |
| 1.3 | **Every destructive action is undoable** for the daemon's undo window, from the toast and from `u`/`z`, and Undo refreshes every view that showed the change (mailbox, search, thread). | Archive from search results, undo, the row is back in search and inbox without a reload. |
| 1.4 | **Threads or messages.** The list can group by conversation (with message count and participants) or show single messages, like the TUI's thread/message toggle. | Toggle the list mode; a thread row acts on every message in the thread, and bulk confirmation says so. |
| 1.5 | **Selection at speed.** `x` toggle, `V` range (visual mode), select all/none/read/unread/starred, shift-click range, and a bulk bar that states the count and scope. | Select a range with `V j j j`, archive, undo. |
| 1.6 | **Previews before destruction.** Batch and destructive actions (bulk trash/spam, unsubscribe-and-purge, route, rule apply) show exactly what will change, computed by the same path as the real mutation. | Unsubscribe-purge shows the message count from a `dry_run` before confirming. |
| 1.7 | **Flow between items.** From the reader: next/previous thread without acting, archive-and-advance, and return to the exact origin (label, saved search, search query with all its parameters). | Search, open result 3, archive-and-advance, `Esc`: back in the same search at result 4. |

## 2. Reading

| # | Criterion | How to check |
|---|---|---|
| 2.1 | **Readable threads.** Read messages collapse to a one-line header; the newest unread (or last) message is expanded; `J`/`K` move between messages; expand/collapse all. | Open a 5-message thread: only the relevant message is expanded. |
| 2.2 | **Quoted text and signatures collapse** behind an explicit "show quoted text" / "show signature" control, in plain and HTML views. | A reply with a 40-line quote shows one line with a count. |
| 2.3 | **Safe rendering by default.** Sanitized HTML in a sandboxed iframe, remote images blocked by default with per-message allow, tracker pixels stripped, no script. Plain text and reader mode are one key away. | New thread from an unknown sender: no remote request fires until the user allows it. |
| 2.4 | **Body views.** Reader (cleaned text), original HTML and plain text, plus raw headers, matching the TUI's `R`/`H`/`M` and the CLI's `cat --view headers`. | Each view renders for an HTML message and a plain-text message. |
| 2.5 | **Links and attachments are first-class.** A link list for the message (TUI `L`) with open and copy, an attachment list with open and download, and inline `cid:` images. | `L` lists the message's links; copy puts the URL on the clipboard. |
| 2.6 | **Typography for reading.** Measure 60 to 80 characters, clear header hierarchy (sender, recipients, date), consistent date formatting, correct plurals. | "1 message", not "1 messages". Long lines wrap at a readable measure in a wide window. |

## 3. Writing

| # | Criterion | How to check |
|---|---|---|
| 3.1 | **Compose opens fast and in place** from anywhere (`c`, `r`, `a`, `f`) with the reply context prefilled by the daemon. One compose surface, not two competing flows. | `r` in the reader opens an inline reply with quoted body and correct recipients. |
| 3.2 | **Never loses text.** Autosave, and closing or navigating away flushes the pending save. | Type, close within one second, reopen the draft: the text is there. |
| 3.3 | **Send is safe.** Confirmation states recipients and account, the safety verdict (SAFE/WARN/BLOCKED) is shown, undo-send works, and a second press cannot send twice. | Double-press send during the undo window: exactly one message is sent. |
| 3.4 | **Send options.** Send later with cancel, send and remind (follow-up reminder), send and archive, From address selection, signatures, snippets, attachments by drag and drop. | Schedule a send, see it listed, cancel it. |
| 3.5 | **Validation helps, never nags.** Problems show when they matter (on send or on blur), in one compact place. | A fresh compose shows no errors. Sending with no recipient explains what is missing. |
| 3.6 | **Drafts are recoverable.** Drafts list with edit, delete and send; orphaned drafts can be reset or sent. | Stored draft opens, edits and sends. |

## 4. Finding

| # | Criterion | How to check |
|---|---|---|
| 4.1 | **Search as you type** with lexical, semantic and hybrid modes, operator hints, and results that act exactly like a mailbox (same keys, same actions). | `/` then type: suggestions within 150 ms of the debounce; Enter opens full results. |
| 4.2 | **Saved searches** can be created, edited, pinned and deleted, show unread counts, and open as a lens with keyboard jumps (TUI `g1` to `g9`). | Save a query, jump to it with its shortcut, see its unread count update after archiving from it. |
| 4.3 | **Scope and truth.** Search can be scoped to an account; counts are real; empty results suggest what to try. | Account filter changes results; a zero-result query explains itself. |

## 5. Workflows beyond the inbox

Parity with the TUI's lenses and pages. Each must be reachable by keyboard and
handle its own loading, empty and error states.

| # | Criterion | How to check |
|---|---|---|
| 5.1 | **Snooze** with presets and natural-language time, a snoozed lens, and manual unsnooze. | Snooze, find it under Snoozed, wake it. |
| 5.2 | **Reply later and owed replies.** Flag reply-later (`b`), a reply queue, and the owed-replies lens. | `b` flags; the queue shows it; owed lens lists threads waiting on the user. |
| 5.3 | **Screener** with allow/deny/feed/paper-trail and visible key hints. | Each decision moves the sender out of the queue. |
| 5.4 | **Labels** can be applied, removed, created, renamed and deleted; moves and routes use real label names. | Create a label from the picker, apply it, rename it, delete it. |
| 5.5 | **Unsubscribe** single and purge-with-preview; subscriptions list. | Purge previews the count before acting. |
| 5.6 | **Calendar invites** RSVP with undo and with comment (opens compose), and an invites lens. | Accept, undo within the window; accept with comment opens a reply. |
| 5.7 | **Rules** list, edit, enable/disable, dry run, history, delete with confirmation. | Dry run shows matches before save. |
| 5.8 | **Accounts** list, add (OAuth device flow, IMAP), test, repair, default, disable, remove; the account switcher scopes the mailbox. | Switch account: the list shows only that account's mail. |
| 5.9 | **Intelligence.** Thread summary, thread briefing, sender profile, whois/explain, draft assist with tone and length, find expert, commitments. Each degrades clearly when no LLM is configured. | With no LLM configured, each says so and links to setup. |
| 5.10 | **Analytics** covering the TUI's dashboards (storage, stale, contacts, cadence drift, response time, subscriptions, search groups, wrapped), each drilling into a search. | Every TUI dashboard has a web page; drill-down opens the matching search. |
| 5.11 | **Operations.** Diagnostics (status, doctor, sync health, events, logs, jobs, bug report), activity log, deliveries, manual sync with progress. | Trigger sync; progress shows and completes. |

## 6. System behaviour and trust

| # | Criterion | How to check |
|---|---|---|
| 6.1 | **Live.** New mail, label counts and sync progress arrive over the WebSocket on first load, without a refocus; reconnect is automatic and visible. | Fresh profile: sync progress appears without clicking the window. |
| 6.2 | **Every view has all its states.** Loading (skeleton, not a blank or a false "empty"), empty (with a next action), error (with retry and the daemon's message). | Throttle the bridge: every route shows a skeleton, never "No data". |
| 6.3 | **Deep links work.** Every route survives a hard refresh in the embedded build; the URL carries the state (lens, query, open thread). | Refresh `/search?q=x`, `/rules`, `/drafts` on the embedded bridge. |
| 6.4 | **Errors are useful.** User errors come back as 4xx with a message the UI shows; a daemon or network failure is distinguished from a bad input. | Bad snooze time shows "couldn't parse ...", not "502". |

## 7. Keyboard

| # | Criterion | How to check |
|---|---|---|
| 7.1 | **One dispatcher.** Global and page keys go through the action registry with scopes, so a key never fires two actions and help always matches behaviour. | `g s` in the reader goes to Starred and does not star the thread. |
| 7.2 | **TUI muscle memory works.** The TUI's live bindings (section 1 of the TUI inventory) do the same thing on the web unless a browser convention forbids it, and deviations are listed in help. | Walk the TUI key table; each key either matches or is listed as a documented deviation. |
| 7.3 | **Discoverable.** `?` opens searchable help for the current context; the command palette lists every action with its shortcut; hints on buttons. | `?` opens help (not search). Every palette item shows its chord. |
| 7.4 | **Focus is real.** Keyboard focus moves the DOM focus (or `aria-activedescendant`), is always visible, returns to the trigger when a dialog closes, and nested controls own their keys. | Tab to a row's star button, press Space: it stars; the thread does not open. |

## 8. Craft

| # | Criterion | How to check |
|---|---|---|
| 8.1 | **One design system.** A single token source for color, type, radius and spacing; every theme renders as designed; the default theme matches the `DESIGN.md` brand (navy ink, cyan signal, Recursive). | Switch each theme; screenshots differ as designed and pass contrast. |
| 8.2 | **Hierarchy and density.** Unread vs read is obvious at a glance; the sender, subject and date columns stay legible in split view; density modes change row height without breaking alignment. | Split view at 1280 px shows at least 40 characters of subject. |
| 8.3 | **Responsive down to half a laptop screen.** At 900 px the app switches to a single-pane flow (list or reader) with no clipped content; the sidebar collapses to icons. | Screenshots at 1440, 1100 and 900 px show no overflow. |
| 8.4 | **Motion with purpose.** Row exit on archive, pane transitions and toasts animate briefly (120 to 280 ms) and respect `prefers-reduced-motion`. | Reduced-motion emulation removes the animations. |
| 8.5 | **Accessible.** No serious or critical axe violations on the main routes, AA contrast in every theme, icon buttons named, lists exposed with list or grid semantics. | Axe run across the main routes. |
| 8.6 | **No debris.** No dev tools, dead routes, broken links, duplicate nav entries or placeholder text in the production build. | The production build shows no devtools bubble; every sidebar entry opens a working page. |

## 9. Engineering health

| # | Criterion | How to check |
|---|---|---|
| 9.1 | **Tests cover journeys**, not implementation: Playwright specs for the triage loop, reader, compose and search against the FakeProvider daemon, and CI runs them. | CI runs the e2e suite, not only `smoke`. |
| 9.2 | **Bundle budget** kept: main chunk under 70 kB gzipped; editors and charts lazy. | `npm run build` output. |
| 9.3 | **Files do one job.** No route component over roughly 500 lines; shared logic lives once. | `wc -l` on route components. |

## Baseline: 2026-09-27, `origin/main` `fec4aefc` (v0.6.32)

Scored from source reading, both client inventories, and a live session
against the FakeProvider demo daemon at 1440 and 900 px.

| # | Score | Evidence |
|---|---|---|
| 1.1 | 1 | Virtualized, but thread rows duplicate across pages (dedupe by message id); return from reader loses origin and position. |
| 1.2 | 0 | Optimistic layer guards for a flat `{mailbox}` shape; the cache is `InfiniteData`, so nothing updates until refetch (`useOptimisticMailMutation.ts:85,119`). |
| 1.3 | 1 | Undo exists; search and thread caches are not invalidated, so undo from search leaves stale rows. |
| 1.4 | 0 | No thread-grouped list; bridge has no `ListThreads` route. |
| 1.5 | 1 | `x`, shift-x, select all/none; no visual mode, no read/unread/starred patterns. |
| 1.6 | 0 | `dry_run` hard-coded false for purge and route (`features/mailbox/api.ts`). |
| 1.7 | 1 | `[`/`]` archive-and-advance only; opening from search, label or saved search returns to Inbox. |
| 2.1 | 0 | Every message renders expanded. |
| 2.2 | 0 | No quote or signature detection. |
| 2.3 | 1 | Sanitizer and sandbox are solid; remote images default **on** (`ThreadRoute.tsx:186`). |
| 2.4 | 1 | HTML/Reader/Plain toggle exists (crushed layout); no raw headers view. |
| 2.5 | 1 | Attachments in a rail; no link list. |
| 2.6 | 1 | "1 messages"; raw thread UUID in the breadcrumb; mixed date formats. |
| 3.1 | 1 | Two competing compose flows (launcher → page vs overlay). |
| 3.2 | 1 | Closing compose drops up to 3 s of typing. |
| 3.3 | 1 | Undo-send works; double send possible during the undo window. |
| 3.4 | 2 | Send later, signatures, snippets, attachments, From picker present; no send-and-remind; scheduled sends are not listed. |
| 3.5 | 0 | Three error banners on a fresh compose. |
| 3.6 | 1 | Drafts list works; orphan recovery not surfaced. |
| 4.1 | 2 | Palette with live suggestions and modes; results page is dense and unpolished. |
| 4.2 | 1 | CRUD and pins exist; no unread counts, no `g1`-`g9`. |
| 4.3 | 1 | `account` param sent but ignored by the bridge. |
| 5.1 | 1 | Snooze presets; no unsnooze. |
| 5.2 | 1 | Reply queue exists; no `b` key in the list, no owed lens. |
| 5.3 | 1 | Works; no loading state ("Queue empty" while fetching), hidden keys. |
| 5.4 | 1 | Apply, rename, delete; no create; route sends slugs instead of names. |
| 5.5 | 1 | Unsubscribe works; purge has no preview. |
| 5.6 | 1 | RSVP fails silently; "with comment" opens nothing. |
| 5.7 | 2 | Builder, dry run, history. |
| 5.8 | 1 | Management works; switcher does not scope the mailbox. |
| 5.9 | 1 | Summary, briefing, draft assist, expert present; no whois, sender page orphaned. |
| 5.10 | 1 | Six dashboards; no cadence drift or search groups page; no loading states; chart sizing warnings. |
| 5.11 | 2 | Diagnostics, activity, jobs, deliveries present. |
| 6.1 | 1 | First-session WebSocket starts before the token lands; three handled events do not exist. |
| 6.2 | 1 | Several routes show "empty" while loading. |
| 6.3 | 0 | Legacy 301s hijack `/search`, `/rules`, `/drafts`, `/accounts`, `/diagnostics`, `/subscriptions` on the embedded bridge. |
| 6.4 | 1 | Every daemon error is a 502. |
| 7.1 | 0 | Two dispatch systems; `g s` in the reader also stars. |
| 7.2 | 1 | Many TUI keys missing (`V`, `b`, `Z`/`l`/`v` in list, `D`, `u`, `I`/`U`, `#`, `J`/`K`, `L`, `W`, `B`, `g1`-`g9`). |
| 7.3 | 0 | `?` opens search; two help dialogs stack; `4`/`6`/`7` advertised but unbound. |
| 7.4 | 1 | j/k cursor is synthetic; hover-only row controls; Enter on the star opens the thread. |
| 8.1 | 0 | `app.css` redeclares `:root` after `tokens.css`: all themes collapse to one magenta palette, radius 0, three font families. |
| 8.2 | 1 | Split view truncates subjects to about 10 characters. |
| 8.3 | 0 | At 900 px the list disappears and the reader clips. |
| 8.4 | 1 | Exit and pulse animations defined but unused. |
| 8.5 | 1 | Axe gate on five routes; list lacks semantics. |
| 8.6 | 0 | TanStack devtools bubble in the UI; `/dev` route calls a wrong path; duplicate palette entries. |
| 9.1 | 1 | 13 e2e specs; CI runs only `smoke`. |
| 9.2 | 2 | Main chunk within budget; editors and charts lazy. |
| 9.3 | 1 | `ThreadRoute.tsx` 1352 lines, `useComposeSession.ts` 1341, `SettingsRoute.tsx` 1218. |

## Re-score: 2026-09-27, branch `feat/web-app-parity`

Scored from the Playwright suite against the FakeProvider daemon (`apps/web/e2e`,
71 specs, run twice in full) and a recorded journey at 1440 and 900 px. The
pass bar is **not yet met**: every criterion is at 2 or better, but seven in
sections 1 and 2 are at 2, not 3. What would move each one is in its row.

| # | Before | Now | Evidence, and what 3 would need |
|---|---|---|---|
| 1.1 | 1 | 2 | The list stays mounted under the reader (nested child routes), so cursor and scroll survive; `reader.spec` "n opens the next conversation and Esc returns to the list with the cursor on it". Needs a measured 5k-row scroll run. |
| 1.2 | 0 | 3 | Pending ops projected over every cached lens, retired only after refetch; `mutations.spec` (instant removal, failed archive rolls back only its row and says why). |
| 1.3 | 1 | 3 | Toast Undo, `u` and `z`; search, mailbox and thread caches refresh; moves and label changes now undoable in the daemon; a `u` pressed before the daemon answers waits for it (`triage.spec`, `labels.spec`, `mailMutations.test.ts`). |
| 1.4 | 0 | 2 | Thread and message modes; thread rows act through every message id and confirmations count conversations. Needs a journey spec for the toggle. |
| 1.5 | 1 | 3 | `x`, `V`, `* a/n/r/u/s`, shift-click, bulk bar with count and scope; `triage.spec` "select three with x, archive them together, undo brings all three back". |
| 1.6 | 0 | 2 | Unsubscribe-and-clear previews with `dry_run`; rule dry run; bulk trash and spam confirm with the conversation count. Needs a route (queue) preview in the web. |
| 1.7 | 1 | 3 | `origin.spec`: Esc returns to the label lens and to the search with every parameter; archive-and-advance from search lands on the next result. |
| 2.1 | 0 | 3 | `reader.spec` "a long thread folds read messages, expands on o/X". |
| 2.2 | 0 | 2 | Quotes and signatures fold in plain text and HTML (Gmail, Apple Mail, Outlook markers; inline replies below a quote stay). Unit-tested and seen in the browser; needs a journey spec. |
| 2.3 | 1 | 3 | Sandboxed iframe with no scripts; remote images blocked until `M`, with a per-sender allow list (`reader.spec`, `html-rendering.spec`). |
| 2.4 | 1 | 2 | Formatted, Reader and Plain views, raw headers on `g h`; `keyboard-navigation.spec` covers `R` and `H`. Plain has no key of its own. |
| 2.5 | 1 | 2 | Links dialog on `L` with open and copy, attachments open and download (`html-rendering.spec`), `cid:` images inline. Needs a spec for `L`. |
| 2.6 | 1 | 2 | One date formatter, `plural()` everywhere, 72ch measure. Needs a typography pass at 1920 px beyond `keyboard-navigation.spec` "the reader fills the reading pane at 1920px". |
| 3.1 | 1 | 2 | One compose surface; `c` focuses To, `r` starts in the body (`compose-send.spec`). |
| 3.2 | 1 | 2 | Autosave flushes on hide and unmount (`useComposeAutosave.ts`). |
| 3.3 | 1 | 2 | Send is guarded while pending; undo-send; safety verdict in the confirmation. |
| 3.4 | 2 | 2 | Send later (listed and cancellable on Drafts), send and remind, From, signatures, snippets, drag-and-drop attachments. |
| 3.5 | 0 | 2 | Validation shows on send or blur, in one summary. |
| 3.6 | 1 | 2 | Drafts list plus orphaned-draft recovery (`OrphanedDrafts.tsx`). |
| 4.1 | 2 | 2 | `search-live.spec`; results act like a mailbox (same list, keys and reader). |
| 4.2 | 1 | 2 | Save, sidebar, unread counts, `g 1`-`g 9` (`search.spec`); saved-search create no longer 502s. |
| 4.3 | 1 | 2 | The bridge honours `account` on search; the account switcher scopes lenses. |
| 5.1 | 1 | 2 | Presets on `1`-`9`, natural-language times, `/snoozed` one row per conversation, Wake by click or `w` (`snooze.spec`). |
| 5.2 | 1 | 2 | `b` flags, reply queue with Done, owed-replies lens. |
| 5.3 | 1 | 2 | Screener decisions with key hints and loading state. |
| 5.4 | 1 | 2 | Create, apply, rename, delete; create no longer 502s (`labels.spec`). |
| 5.5 | 1 | 2 | Single unsubscribe and purge with preview; subscriptions page. |
| 5.6 | 1 | 2 | RSVP with undo, and with comment opening a reply; invites page. |
| 5.7 | 2 | 2 | Builder, dry run, history. |
| 5.8 | 1 | 2 | Account switcher scopes the mailbox; management pages. |
| 5.9 | 1 | 2 | Summary, briefing, whois, sender profile, draft assist, find expert; gated on LLM status with a setup link. |
| 5.10 | 1 | 2 | Dashboards as tabs including cadence drift (`keyboard-navigation.spec` "analytics opens dashboards through TUI-style tabs"). |
| 5.11 | 2 | 2 | Diagnostics panels in the URL, activity, deliveries, sync with progress (`sync-progress.spec`). |
| 6.1 | 1 | 2 | Socket connects as soon as the token lands; reconnect visible (`ws-reconnect.spec`, `offline-banner.spec`). |
| 6.2 | 1 | 2 | Skeletons, empty states with a next action, errors with retry (`route-error.spec`). |
| 6.3 | 0 | 2 | Legacy redirects skip browser navigations; `origin.spec` "a hard refresh on /search?q= keeps the query and results". |
| 6.4 | 1 | 2 | Bad input maps to 400/422 with the daemon's message; only IPC and provider failures are 502. |
| 7.1 | 0 | 3 | One dispatcher over the action registry with scopes; `keyboard-navigation.spec` "g chords jump to the TUI's views". |
| 7.2 | 1 | 2 | TUI live keys mapped; deviations listed in the generated keybindings reference. |
| 7.3 | 0 | 3 | `?` opens searchable help for the current pane; palette lists every action with its chord; keys typed straight after ⌘K land in it (`keyboard-help.spec`). |
| 7.4 | 1 | 2 | `aria-activedescendant` list, visible focus, focus returns on close; row controls are mouse affordances with keys (`x`, `w`) because rows are options. |
| 8.1 | 0 | 2 | One token source (`styles/tokens.css`), four themes, DESIGN.md brand. |
| 8.2 | 1 | 2 | Container-query rows keep subject legible in split view (screens at 1440). |
| 8.3 | 0 | 2 | `responsive.spec`: single pane at 900 px, no horizontal scroll at 900, 1100, 1440. |
| 8.4 | 1 | 2 | Brief transitions, removed under `prefers-reduced-motion` (`styles/base.css`). |
| 8.5 | 1 | 3 | `accessibility.spec`: no serious or critical axe violation on the main routes, an open thread, the labels dialog, help and compose, with no tolerated exceptions; faint text removed from copy. |
| 8.6 | 0 | 2 | No devtools in production, no dead routes. |
| 9.1 | 1 | 3 | 21 journey specs; CI job `web-e2e` runs the whole suite. |
| 9.2 | 2 | 2 | Entry chunk 62.5 kB gzipped (zod removed again); editors lazy. |
| 9.3 | 1 | 2 | Largest files: `useComposeSession.ts` 711, `RightRail.tsx` 697, `SearchResultsRoute.tsx` 678 lines. Needs those split. |

Known flake: `triage.spec` "select three with x…" failed once in four full
runs on the "Archived 3 messages" toast assertion, and passed three times in
isolation.
