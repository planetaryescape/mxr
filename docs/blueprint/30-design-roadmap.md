# 30. Design roadmap: putting the desk under the lamp

Status: proposal, 2026-10-16. Source baseline: `origin/main` at `f209d84ef`.

The order of work for [26](26-web-design-concept.md) to [29](29-assistant-design.md). Rungs are named W1 to W8 (web design) so they don't collide with decision IDs. A rung added later gets a letter (W3b).

## Build philosophy

- Each rung leaves the app better to use from start to finish, and is sized to one working session.
- Each rung lands on `main` through a PR whose checks pass, and ships with the next release BK calls ("ship it" runs the whole release-please chain).
- At most two new unknowns per rung.
- A rung that changes what people see is done only when it passes the design gate ([26](26-web-design-concept.md#the-design-gate)), not when its tests pass. Behaving as specified is the floor.
- Stop after any rung and the app is coherent: no rung leaves half the screens on the old parts and half on the new without a reason written in its PR.
- Every rung updates the docs it touches in the same PR: the site guides under `site/src/content/docs/guides/`, `docs/web-app-controls.md` for persistent controls, and this blueprint if a decision changes (as a new entry in [15](15-decision-log.md)).

## Phase 0: spikes

| # | Spike | Gates |
|---|---|---|
| S1 | Why Now, Messages, Focus & reply, Owed replies and Waiting on disagree about who is waiting on you, read in the daemon at a named SHA | W4 |
| S2 | Recursive `full.css`: first-load size, layout shift and paint cost against `mono.css` | W1 |
| S3 | The OKLCH theme generator reproduces Midnight within ΔE 2 | W8 |
| S4 | Moving the lamp on a 5,000-row list stays inside the `large-list` budget | W2 |

Details are in [31](31-design-open-questions.md).

## Rung W1: The same page everywhere, with a real title

**Previously:** three page frames, four header heights, every screen one grey block at 11 to 13px. **Now:** every page has the same frame, a real title, three text tiers and the square shape rule.

**Promise:** "Every page in mxr has the same header, and I can tell at a glance what each one is."

**Build:**
- The ink depths, text tiers and signal roles as tokens, with today's names as aliases ([27](27-web-design-system.md#colour), D124).
- `full.css` and the type roles ([27](27-web-design-system.md#type), D126).
- The shape rule and `check-shape.mjs` (D127).
- `PageFrame` replacing `ModeFrame`, `Page` and the bespoke headers, with `check-frames.mjs` (D128).
- `apps/web/DESIGN.md` pointing at chapters 26 and 27, and `tokens.css`'s header pointing at 27 (D137).

**Design:** every page's header; touches every journey's first glance. The comp is a static HTML page showing Now, Messages, To do and Settings headers in Midnight and Light at 1440 and 390px, built with the new tokens. BK sees this comp: it is the concept's first appearance.

**New unknowns:** S2 (font cost), and whether every route can take the one frame without a bespoke slot.

**Demo:** open Now, Messages, To do, Updates, Reading, Archive, Inbox, Search results, Settings, Screener and Snoozed in turn; every header has the same height and anatomy; the title is the largest text on each.

**Done when:**
- `npm run lint` includes `check-shape` and `check-frames` and passes; every route renders `PageFrame`.
- `e2e/design-gate.spec.ts` exists and attaches screenshots of the main pages in four themes at three widths.
- The design gate passes, graded against [26](26-web-design-concept.md).

**Left out:** rows, states, navigation and motion.

## Rung W2: One lit row, and Now and Messages calm

**Previously:** five underlined links per row, three selection styles, every row's actions visible. **Now:** one `Row`, the lamp, reason lines and mode marks, on Now and Messages.

**Promise:** "Now and Messages are calm: one thing is lit, and I can see why each person is there."

**Build:** `Row`, `ReasonLine`, `ModeMark`, the `new` mark and the lit-row choice ([27](27-web-design-system.md#rows), D124, D129); Now and Messages rebuilt on them ([28](28-web-screens.md#now), [28](28-web-screens.md#messages)); Messages' letters as ruled sections in the reading voice.

**Design:** journeys 1 and 2. The comp shows Now and a person's page with a lit row and an unlit row side by side, at 1440 and 390px.

**New unknowns:** S4 (lamp performance on long lists).

**Demo:** open Now, press `j` three times, `Enter` on a person, reply, watch the topic fold and the next light, `Esc` back to Now with the light on the next item.

**Done when:** the demo works in the running app; `large-list.spec` stays under its budget with the lamp; no row on Now or Messages shows a sentence-style Also-in or new-sender line; the design gate passes.

**Left out:** the other modes keep their rows until W3.

## Rung W3: Every mode on the same parts, keys in one place

**Previously:** about six row designs and keys shown four ways. **Now:** To do, Updates, Reading, Archive and Inbox use `Row`; keys show on the lit row, the status strip and `?` only.

**Promise:** "Every mode feels like the same app, and the keys I need are always where I look."

**Build:** the mode rows ([28](28-web-screens.md)); `usePageKeys` and the status strip; legends, button chips and hover chips removed; no chips on touch; the `?` sheet rewritten (D130); Archive's filters as the segmented control; To do's checkbox at Body size, without the unexplained `⊙` icon or progress bar.

**Design:** journeys 3, 4 and 7. The comp shows To do, Updates and Archive lit rows.

**New unknowns:** none beyond W2's parts.

**Demo:** tick off a to-do with `Space`, peek an update with `Space`, open a Reading issue with `Enter`, find a record in Archive; the status strip changes per page and never cuts a word.

**Done when:** `keymapParity.test.ts` passes; no `KeyChip` renders under `(pointer: coarse)`; the design gate passes.

**Left out:** tool pages' rows (W5).

## Rung W4: mxr never contradicts itself and never looks stuck

**Previously:** "12 people waiting on you" beside "Nobody is waiting on a reply from you"; five empty layouts; a mail skeleton that never resolves; two banners when the daemon is down. **Now:** one definition per count and one of each state.

**Promise:** "mxr tells me the same thing everywhere, and when something's wrong it says what and what I can still do."

**Build:** one daemon definition of "waiting on you" used by Now, Messages, Focus & reply, Owed replies and the thread line, with "new" counted apart (D132, after S1); badge counts from the page's query; named units; `PageEmpty`, `PageSkeleton`, `PageError` and the offline banner on every frame (D131); plain-language error mapping; a sidebar that keeps its items offline.

**Design:** the "waiting", "failed" and "empty" key moments; journey 6. The comp shows each state on Messages and Archive.

**New unknowns:** S1 (the definitions), and the error mapping's coverage.

**Demo:** with mail waiting, Now, Messages, Focus and Owed show the same count. Stop the daemon: cached content stays with one banner. Open a bad thread id: it offers the way to Inbox, not "Try again".

**Done when:** a daemon test asserts the five surfaces' counts come from one query; an e2e stops the daemon and asserts the banner, cached content and an unchanged sidebar; the design gate passes.

**Left out:** loading states that only appear on a cold cache are checked by the skeleton e2e, not by hand.

## Rung W5: The rail recedes and every destination is reachable

**Previously:** a 30-item rail with a mixed "More"; Settings, Screener, Snoozed, Drafts, Sent, Labels and Tools unreachable on a phone; two left navs in Settings. **Now:** the grouped, dimmer rail, a phone More sheet, one settings nav, and tool pages on `Row`.

**Promise:** "The rail is quiet, I can reach everything from my phone, and Settings is one page."

**Build:** the rail groups (Mail, Views, Tools, Labels) and the dim tiers (D133); the collapsed rail with modes and Inbox only; the More tab and sheet with correct active states; Settings as tabs or drill-in; breadcrumbs from route titles; the search sheet with one input; tool pages (Screener, Activity, Waiting on, Subscriptions, and the rest) on `Row` with the lit-row choices; "Reply later" naming.

**Design:** the map in [26](26-web-design-concept.md#the-map). The comp shows the rail expanded and collapsed, and the phone More sheet.

**New unknowns:** none.

**Demo:** at 390px, reach Settings, Screener, Drafts and a label from More; open Updates and see More lit. At 1440px, collapse the rail and see only the modes and Inbox.

**Done when:** `controls.spec.ts` and `docs/web-app-controls.md` are updated for the rail; an e2e reaches every destination at 390px; the design gate passes.

**Left out:** none.

## Rung W6: Reading and writing feel like reading and writing

**Previously:** reading and writing share the triage UI's density and button styles; compose floats over live rows with a vim mode line. **Now:** the reader page, the Reading reader, compose and search results in the reading voice, with one toolbar style.

**Promise:** "When I read, the email is the page, and when I write, the editor is the page."

**Build:** the reader page ([28](28-web-screens.md#inbox-and-the-reader)): Display title, reading voice, folded earlier messages, the gist block with the model mark, quiet toolbar; the Reading reader's toolbar; the compose sheet with scrim, Title and the shared segmented control, with the vim line only in vim mode; the search results page; `ModelMark` everywhere model text appears (D136).

**Design:** journey 4 and the "opening an item" moment. The comp shows the reader, Reading's reader and compose in Midnight and Paper.

**New unknowns:** sender HTML inside the reading measure (the Auto canvas from #349 must keep working).

**Demo:** open a long thread, read to the end, reply; open a newsletter in Reading and read it to the end; compose a new email.

**Done when:** the reading measure is 64 to 70ch at 1440px; the design gate passes in all four themes.

**Left out:** the assistant (W7).

## Rung W7: The copy desk

**Previously:** a chat app look, underlined links, the model's own step labels and "Thinking". **Now:** wire lines in the daemon's words, a live line, the app's own objects, proofs to sign, in the web panel and the TUI pane ([29](29-assistant-design.md), D135).

**Promise:** "The assistant shows me the actual emails, tells me what it's doing in plain words, and brings me changes to sign."

**Build:** the protocol and daemon changes in [29](29-assistant-design.md#protocol-and-daemon-changes-this-needs) (step titles, `Calling.title`, reply rows, change titles, plain-language turn errors), the OpenAPI spec and generated types; the web panel's quoted prompts, wire lines, live line, chips, rows, proofs, questions, failure note, trust header and empty state; the TUI pane on the same titles and proofs.

**Design:** journey 5 and the "assistant signs off a change" moment. The comp is a static panel showing a finished turn with an answer, rows and a waiting proof.

**New unknowns:** the step-title copy for 72 tools, and the reply-rows heuristic.

**Demo:** against a local model, ask "who is waiting on me?" and see the people as rows; ask to archive a set of emails, sign the proof with `y`, and watch the rows beside fold away; repeat the same in the TUI pane.

**Done when:** every MCP tool has titles (a test like `every_mcp_tool_has_a_gate_rule`); no default view shows a tool name, id or JSON (an e2e asserts it on a recorded conversation); the TUI snapshots are updated; the design gate passes.

**Left out:** dropping single items from a proof (Q5).

## Rung W8: Finishing feels good, and every theme looks designed

**Previously:** motion barely visible, toasts as the only feedback, themes tuned by hand and Paper's badges in error red. **Now:** the four motion jobs, the finishing moment per mode, the landing mark, the toast rule and generated themes.

**Promise:** "Clearing a mode feels finished, I can see changes land, and every theme looks like it was designed."

**Build:** fold-and-advance, the landing underline, overlays' motion and the section put-to-bed moment with its per-mode definition (D134, [27](27-web-design-system.md#motion)); `check-motion.mjs` extended; the toast rule (D140); the theme generator (S3) and the Paper fix (D139); a full design-gate pass across all seven journeys in all four themes.

**Design:** every key moment in [26](26-web-design-concept.md#key-moments); all journeys.

**New unknowns:** S3 (the generator).

**Demo:** clear Messages, then To do, then Updates in one sitting; see each finishing moment once; switch themes and repeat one journey in each.

**Done when:** reduced motion keeps fades only (an e2e with `prefers-reduced-motion`); the generator's Midnight is within ΔE 2 of today's; the full design gate passes and its grade is recorded in `docs/web-app-experience-rubric.md` with screenshots.

**Left out:** editor themes (Q4).

## After W8 (not planned yet)

- A dogfooding week in `docs/dogfooding-log.md` (rubric E2), the evidence the rubric has waited on since 2026-09-30.
- Editor themes if BK wants them (Q4).
- List-plus-detail layouts for To do and Updates at 2560px (deferred on 2026-10-07).
- The SwiftUI client takes this chapter as its design reference (BK, 2026-10-09: "I just mentioned the web app so that we learn the design").
