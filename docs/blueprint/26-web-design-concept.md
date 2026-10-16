# 26. Web design concept: the night desk

Status: proposal, 2026-10-16. Source baseline: `origin/main` at `f209d84ef`.

This chapter decides what the web app is to the person using it and how it should feel, before anyone specifies a screen. The functions are built and mostly right; what was never designed is the whole. Chapters 21 and 22 say what each mode does. This chapter says how the app is experienced, [27](27-web-design-system.md) turns that into tokens and shared patterns, [28](28-web-screens.md) applies it to every screen, [29](29-assistant-design.md) to the assistant, and [30](30-design-roadmap.md) orders the work. Decisions are D123 to D140 in [15](15-decision-log.md); open questions are in [31](31-design-open-questions.md).

The evidence behind it is in [docs/research/web-design/](../research/web-design/): an audit of every screen on 2026-10-16 ([audit-2026-10-16.md](../research/web-design/audit-2026-10-16.md)) and research into the best agent panels and the best-designed mail and calm productivity apps ([agents-and-mail-apps.md](../research/web-design/agents-and-mail-apps.md)).

## What this is to the person

mxr is where BK deals with everything that arrives by email, at a keyboard, usually in a dark room, once or twice a day. He opens it to answer the few people who matter, do the admin that has a deadline, glance at what services told him, read what he chose to read, and find a record when he needs one (the five modes, D107). He wants to leave knowing nothing was missed and nothing is waiting (D119), having touched as few emails as possible.

What he cares about, in his words:

- **Speed that holds.** "I want the app to feel like vscode but to be snappy and blazing fast, since it's local first" (2026-03-21). "It breaks the blazing fast illussion" (2026-03-22).
- **The keyboard for everything.** "everythign is fully navigable by keyboaard. including anything you want to do not just navigation" (2026-05-11).
- **Dark, quiet, professional.** "i'm a dark mode guy" (2026-03-21). "more professional and more useful without being noisy" (2026-10-14).
- **The thing itself at the centre.** "this email reading screen is abou treading the email and so the email should really take center stage" (2026-05-11).
- **Not Gmail.** "I may as well be using gmail. This should feel like a thought out app that thought about every decision" (2026-09-27).
- **Glance, don't open.** "Glancing at the todo should give me enough information to act if possible" (2026-10-09). Updates is "a notification reel. It tells you things you need to know, hopefully you don't need to open the email" (2026-10-14).
- **Trust in what's hidden.** "since it reorders emails, I'm always thinking: am I missing some important email?" (2026-10-07).

The objects involved are the email (data each mode shows its own way, D112), the people behind it, and the keyboard. The app sits beside a terminal running the TUI; DESIGN.md's scene sentence is "a developer at 11pm with the TUI in one tmux pane". Both should feel like the same product.

## The concept

**The night desk.** mxr is the desk where tonight's mail is put to bed. The room is dark navy; one lamp lights the single thing you are working on, and everything else recedes into the ink until you move the light to it. The desk has sections, like a paper's night desk: Messages are the letters, To do the notices that need action, Updates the wire, Reading the features, Archive the morgue where records are filed. Now is the front page you check first. The reader is the page under the lamp, set in type you can read for a while. The assistant is the copy desk across the room: it does the legwork, writes in the margin, and brings you proofs to sign. When a section is done, it is put to bed, and the desk is clear.

It fits mxr rather than any product. The brand is already "deep-ocean, editorial" and "publishing-meets-shell" (DESIGN.md); the scene is already 11pm; the five modes are already sections with their own forms (D112); and "done here" is already the main verb (D099). The lamp is the one new idea, and it answers the audit's main finding: every element has equal weight, so nothing is lit.

What the concept means in practice:

- **Material:** navy ink in three depths (the room, the desk, the lamp-lit page), ruled lines instead of boxes, type as the texture. No cards, gradients, glass or grain (DESIGN.md).
- **Light:** one lit thing per screen, drawn one step lighter with a cyan hairline. Cyan is the lamp's colour: focus, selection, the one primary action, and change. Yellow is the rare note on the spike: due, overdue, needs you now.
- **Voices:** mono for the wire (meta, times, counts, the assistant's steps, keys), sans for the desk (rows, controls), casual-axis reading type for the pages you read.
- **Mood:** calm, competent, late and unhurried. The app never shouts, never celebrates the routine, and is quietly satisfying at the end.

The metaphor lives in the design. The words on screen stay the product's own (Now, Messages, To do, Updates, Reading, Archive, Inbox, done here, later, let go); nobody reads "morgue" in the UI.

## How the whole fits together

### The mental model

The person thinks in a few nouns, and every surface uses these words and no others:

| Noun | What it is | Never called |
|---|---|---|
| **Now** | The front page: at most ten things across the modes that need you today (D110). | desk, dashboard, home |
| **A mode** | One of five sections, each showing email its own way: Messages, To do, Updates, Reading, Archive (D107). | category, folder, tab |
| **An item** | What a mode shows: a person in Messages, a task in To do, a source in Updates, an issue in Reading, a record in Archive. | email (unless it is one), message |
| **The reader** | The page under the lamp: one conversation, newsletter or record, read in full. | preview pane |
| **Done here** | Finished in this mode; it may still live in another (D108). | archive (unless it is) |
| **Inbox** | Every email in arrival order, the way back to everything. | the real inbox |
| **The assistant** | The copy desk: asks, finds, proposes, and waits for you to sign. | AI, bot, agent |
| **Freshness** | When mail last arrived and when mxr last synced (D119). | status |

### The map

```
 ┌───────────────────────────────────────────────────────────────────────────┐
 │ top bar: account · breadcrumb ·········· search / · assistant ⌘. · compose c │
 ├──────────┬───────────────────────────────────────────┬────────────────────┤
 │ rail     │ the page (one PageFrame)                  │ assistant          │
 │ (dim)    │  list ─────────────┐  reader page         │ (copy desk,        │
 │ Now      │  rows, one lit ◀── │  slides in beside    │  opens beside      │
 │ Messages │                    │  the list; Esc       │  every page, keeps │
 │ To do    │                    │  returns the light   │  its turn running  │
 │ Updates  │                    │  to the row          │  across pages)     │
 │ Reading  │                    │                      │                    │
 │ Archive  │                    │                      │                    │
 │ Inbox    │                    │                      │                    │
 │ Mail ›   │                    │                      │                    │
 │ Views ›  │                    │                      │                    │
 │ Tools ›  │                    │                      │                    │
 ├──────────┴───────────────────────────────────────────┴────────────────────┤
 │ status bar: freshness · this page's 3 to 5 keys ················ ? keys   │
 └───────────────────────────────────────────────────────────────────────────┘
 Always present: rail, top bar, status bar.   Replaces the page: a rail item.
 Opens beside: the reader (from a row), the assistant (⌘.).
 Opens on top: compose (c, bottom-right, with a scrim), search (/), palette (⌘K), ? sheet.
 Back: Esc closes the topmost layer and returns the light to where it was.
 Phone: tabs Now · Messages · To do · Reading · More; the reader and the assistant
 replace the page; More is a sheet with Updates, Archive, Inbox, Mail, Views, Tools.
```

Every place is a URL that restores it exactly, including the lit row and the open reader (shipped in #489, 2026-10-16), so a link can be sent and the browser's back button works.

### The journeys, end to end

1. **The evening clear-down.** Open mxr. Nothing is open yet, by BK's rule: "no messaging app opens with the message already open" (2026-10-09). Now shows the freshness line and the few things that need you. `j`/`k` moves the lamp; `Enter` opens the lit item in its own mode, lit there too; you act (reply, tick off, let go); the item folds away and the light moves to the next; `Esc` returns to Now with the light on the next item. When Now is empty it says so in one line, and each mode you cleared says it was put to bed. *Carries over:* the lit position, the account, the assistant if open. *Must not break:* returning to Now must not reset the light to the top.
2. **Answer a person.** Messages, lamp on a person, `Enter` opens their conversation beside the list with the topic that needs you first; the reply field sits where reading ends (D095); send; the topic folds, the next topic lights, the toast names both (2026-10-07 fix); when the person has nothing left, the light moves to the next person. *Carries over:* the person and topic in the URL; the draft if you leave.
3. **Tick off a to-do.** To do shows the task as the thing to do, with its date (BK, 2026-10-09). If the task needs the email, `o` opens it beside the list; `Space` ticks it off, the row folds, the next lights. *Must not break:* ticking off from the open email advances the list too.
4. **Read.** Reading shows tonight's edition; the lit issue opens as a page under the lamp in the reading voice; `n` goes to the next issue, `b` later, `e` let go, `M` to Marginalia (D121). The last issue put away ends the edition.
5. **Ask the copy desk.** From any page, `⌘.` opens the assistant beside it. Ask "archive the shipping notices older than a month"; one live line says what it is doing; the answer shows the emails as rows; the change arrives as a proof listing them; `y` signs it; the rows in the list beside fold away as the change lands; the step offers Undo. *Carries over:* the conversation across pages and reloads (D122); links open beside the assistant, not in place of it.
6. **Check nothing was missed.** From any page, the status bar's freshness line says when mail last arrived. Now's arrivals line sums every email since you last looked (D119). Inbox is one key away (`g i`).
7. **Find a record.** Archive's answer box takes a question; the best record lights; `o` opens its email in the reader.

### Shared patterns

A new feature reuses these; it doesn't invent its own. Specs in [27](27-web-design-system.md).

| Job | The one way |
|---|---|
| Pick a thing | Move the lamp (`j`/`k`, click, tap). The lit item is the only one with visible actions; lighting never changes a row's height. Lists inside dialogs work the same way (`j`/`k`, `Space`, `Enter`). |
| Say why it's here | One reason, "Here because ...", for the lit item in the status bar, and in the reader. |
| Show it's also elsewhere | Mode marks in the row's trail; detail on the lit row. |
| Act | The verb's key on the lit row, the status strip and `?`; the same verb names on every surface. The change shows at once and never pops back while it is retried (BK, 2026-03-22: "it breaks the blazing fast illussion"; 2026-05-19). |
| Confirm | Only for what can't be undone or reaches another person: a proof sheet that lists exactly what will happen (the preview path, AGENTS.md), one apply with its key. |
| Undo | Every reversible change: a toast with Undo (D140), and `u`. |
| Show "this changed" | A brief cyan underline on the rows that changed, fading (D134). |
| Say what's empty | One empty state per page: what would be here, and the action that fills it. |
| Say it's loading | A skeleton in the shape of the page's rows. |
| Say it failed | Plain words, what still works, the one fix that can work (D131). |
| Mark model text | The reading voice plus a `model` mark naming where it ran (D136). |
| Show keys | Lit row, status strip, `?` (D130). |
| Teach | One hint, anchored to the element it explains, the first time it's needed; never a banner at the top of a page or a tour (D118; BK, 2026-10-07). |

### Across surfaces

The web app, the TUI, the CLI and the assistant are one product:

- **Same words.** Mode names, verbs (done here, later, let go, tick off), reasons and error sentences come from the daemon (`GetModeGuide`, the reason strings, readouts), so a reason reads the same in `mxr todo --format json`, the TUI row and the web row.
- **Same keys.** The keymap parity test (`keymapParity.test.ts`) stays the gate; the lamp is the TUI's cursor.
- **Same states.** Working, waiting on you, done, failed mean the same in the web assistant, the TUI pane and `mxr assistant show`.
- **Same light.** The TUI already has one cursor and a dim/bright palette (`crates/tui/src/theme.rs`); its theme follows the same tiers ([27](27-web-design-system.md#colour)).

### The system's own state

Without asking, the person always knows three things, each in one place:

- **Is my mail current?** The status bar's freshness line (D119), on every page.
- **Is anything in progress?** A running sync, a sweep or an assistant turn shows as one quiet line in the status bar ("Assistant: archiving 14 emails"), with the detail where it happens.
- **Did my change land?** The rows change in place, the toast confirms, and Undo is there.

## Principles

1. **One pool of light.** Each screen has exactly one lit thing; everything else recedes until the light reaches it (D124). If two things compete for the eye, one of them is wrong.
2. **Structure is felt, not seen.** Hierarchy comes from type size, weight and the three ink depths, not from boxes and borders. Rules are hairlines at the softest rule colour, and few (Linear, 2026; DESIGN.md "no cards").
3. **Dense where you work, generous where you read** (D125). Triage is a compact list; reading is a page.
4. **Glance, then act.** A row carries what you need to decide: the thing to do, who, when. Opening is for reading, not for finding out what something is (BK, 2026-10-09 and 2026-10-14).
5. **Every item says why it is here, in one voice** (D111, D129). A sorted app earns trust by showing its work.
6. **One number per fact.** Counts and claims come from one definition and match everywhere (D132).
7. **Every action has a key, shown where you are** (D130). The palette teaches the key beside the command.
8. **Delight is earned at the end.** The routine is instant and silent; finishing a section is the moment that gets craft (D101, D134).

## Key moments

| Moment | What the person should feel | The interaction that makes it land |
|---|---|---|
| Opening mxr | "It's current, and it's calm." | Now arrives with the freshness line already filled and the first row lit (a lit row opens nothing), no message open (BK, 2026-10-09). |
| Moving through a list | Fast, precise, under control. | The lamp jumps instantly on `j`/`k`; the lit row's trail turns into its actions and the status bar says why it's here; no row changes height. |
| Opening an item | The thing itself, centre stage. | The reader page slides in beside the list (180ms) in the reading voice; the list stays, dimmed, with the row still lit. |
| Done here / tick off / let go | Relief, and momentum. | The row folds away (120ms), the light lands on the next row in the same frame, a toast names what happened with Undo. |
| A section put to bed | Finished, properly. | The last item folds, the section's end line appears ("Messages is clear. Next: 3 in To do."), the low-tide scene plays once (1800ms) where the list was. |
| Something can't be undone | Careful, informed. | A proof sheet lists exactly what will happen, by item; the apply key is on it; Esc leaves nothing changed. |
| Waiting | Informed, not stalled. | A skeleton in the page's shape; for the assistant, one live line saying what it is doing now, with elapsed seconds. |
| Something failed | Told plainly, with a way forward. | One message in the page: what happened, what still works, the one fix; cached content stays visible. |
| Empty | Done, not broken. | One line saying what would be here and that there is none, plus the action that fills it if one exists. |
| The assistant signs off a change | In charge. | The proof lists the emails as rows; `y` applies; the rows in the page beside fold away as the change lands, each with the cyan landing mark. |

## Visual language

The tokens are in [27](27-web-design-system.md). In summary:

- **Type.** Recursive only, with all axes (D126). Roles: display 28, title 20, heading 15 semibold, body 14, reading 16 (`CASL` 0.3, line height 1.6, 64 to 70ch), meta 12 mono, eyebrow 11 mono caps at 0.08em. A page has one display or title, its section headings, and body; meta and eyebrows never exceed the body in contrast.
- **Colour.** Three ink depths per theme: room (rail and status bar), desk (the page), lamp (the lit item and the reader page). Three text tiers, not five: primary, secondary, faint. One accent, cyan, for focus, the one primary action, links and change. Yellow only for "needs you now" and due dates, under 3% of a screen. Red only for failure and destructive verbs, green only for success states. Light, Paper and Eclipse are generated from the same rules (D139); Midnight is the default.
- **Shape and depth.** Square surfaces and rows; 3px controls; round only for people, dots and counts (D127). Depth is ink lightness on dark themes and one soft shadow token on light ones; overlays (compose, palette, sheets) sit on a scrim.
- **Iconography.** lucide at 16px in rows and 14px in meta, one stroke weight, never decorative; every mode has one glyph used as its mark everywhere (rail, mode marks, toasts). No emoji.
- **Density.** Work rhythm rows 44 to 56px with one secondary line; reading rhythm at the reading role. The existing compact/regular/comfortable setting scales the work rhythm only.

## Motion

Motion explains change and never decorates (D134). Durations are the existing tokens: fast 120ms, base 180ms, slow 280ms, scene 1800ms; ease-out for arrivals, ease-in for exits, no bounce.

| What moves | How | Why |
|---|---|---|
| The lamp on a key | Doesn't animate. | Keyboard repeat must feel instant (D101). |
| The lamp on a click | 120ms fade of the lit surface. | Ties the click to the result. |
| Reader arriving | Slides 16px in from the right with a fade, 180ms. | Says "this opened beside", not "the page changed". |
| A row leaving | Height collapses and fades, 120ms; the next row lights in the same frame. A key folds only the first press of a burst; repeats drop the row at once. | Shows where the item went and where you are now. |
| A change landing | 2px cyan underline under the changed rows, fading over 600ms. | Shows a change made elsewhere (the assistant, another client, sync) without a toast per row. |
| Overlays | Scrim fades 120ms, sheet rises 8px, 180ms. | Says "on top, Esc to leave". |
| Assistant steps | The live line's text crossfades, 120ms; streamed replies fade in per word, 120ms, never replayed on reload. | The research's streaming rule. |
| Section put to bed | The low-tide scene, 1800ms, once per section per day. | The earned moment. |

Reduced motion keeps only opacity fades of 120ms. The finishing scene shows as its still frame, with its words and no animation: a picture is not motion, and the finish is the reward (D134). `scripts/check-motion.mjs` already guards durations and properties; it gains the rule that keyboard-triggered transitions are instant.

## Hierarchy and restraint

What is primary, secondary and hidden on each main screen. "Hidden" means one hover, one lit row, `?` or the palette away, never removed.

| Screen | Primary | Secondary | Hidden until asked |
|---|---|---|---|
| Now | The summary sentence; the lit item | The four sections' items | Reasons, mode marks' detail, the arrivals breakdown, "Not now" |
| Messages | The people with your turn; the open conversation | Pinned, Recent, the topic list | Quiet (folded), new-sender choice, Also-in detail |
| To do | The task titles and their dates | Who it's from, the source email | Progress detail, Later, Done |
| Updates | The digest's lines, one per source | The cut time and count | Routine (folded, BK's choice 2026-10-14), each update's body (`Space`) |
| Reading | The lit issue's title and dek | The edition list | Sources, unsubscribe, Marginalia (on the lit row and reader) |
| Archive | The answer to the question; the lit record | The ledger | Filters (`g f`), field provenance |
| Reader | The message text | Sender, date, the conversation's earlier messages (folded) | Gist (long threads only, D100), relationship line, every action but reply |
| Assistant | The answer | The folded steps line | Each step's readout, the raw call |

Deliberately left off: in-page key legends (D130), every-row reasons and Also-in sentences (D129), double page titles, count badges on modes that aren't work (Updates, Reading), a second search field, the vim mode line as compose's default chrome (it shows only when vim mode is on), developer notes in `?`.

## Taste references

| Reference | What makes it good | What we take | What we don't |
|---|---|---|---|
| **VS Code** (BK, 2026-03-21) | A dark tool you live in all day; panes, a palette and keys for everything; the editor is the hero. | The workbench shape, the palette, the status bar as the system's voice. | Its chrome density and tab clutter. |
| **Linear** (BK, 2026-03-21) | "Don't compete for attention you haven't earned"; navigation dimmer than content; structure felt, not seen; themes from three inputs. | All four rules (D124, D133, D139). | Its 2026 move to warm greys; mxr's identity is blue. |
| **Superhuman** (BK, 2026-03-21) | Speed as the product; the next action obvious after every action; the command menu teaches shortcuts. | Advance-on-done, the palette's key column, optimistic changes with undo. | Hiding the list while reading; mxr keeps the list beside the reader. |
| **The mxr TUI** (BK, 2026-03-21: "compare this to the terminal tui app") | One cursor, dense rows, keys for everything, no chrome. | The lamp is the TUI's cursor; the work rhythm is its density. | Its lack of a reading voice. |
| **shadcn Lyra** (BK, 2026-03-21/22) | Boxy and sharp. | Square surfaces, 3px controls (D127). | n/a |
| **iMessage / WhatsApp** (BK, 2026-10-02) | One row per person; the conversation is the page. | Messages' shape (D112). | Bubbles for every message; mxr's letters are long. |
| **Things 3** (research) | The open to-do lifts while the rest fades; whitespace and bold type. | The lamp; the fold-away-and-advance. | Its rounded, light material. |
| **Dia** (research; BK uses it) | Novelty spent in one place, the rest plain. | The assistant gets the one new idea (the copy desk); the rest stays quiet. | Its animated flourishes. |
| **Readwise Reader** (research; Marginalia is BK's own) | A reading measure and keyboard reading. | The reading voice and measure. | Its library management. |

## Screens

Mockups and intent for every screen are in [28](28-web-screens.md), and for the assistant in [29](29-assistant-design.md). The rule for each: say what the eye lands on first, what recedes, and what the lit item shows.

## The design gate

A rung that changes what people see is done when, in addition to its tests (D138):

1. **Comp first.** An HTML comp or prototype of the rung's screens and moments, built against these tokens, is published and reviewed against this chapter before the build starts. For rung D1 (the concept's first appearance), BK sees the comp; later rungs proceed unless he objects (D117).
2. **Screenshots in every theme and width.** Each touched screen in Midnight, Light, Paper and Eclipse at 390, 1024 and 1440px, with the assistant closed and open, read against the hierarchy table above: one lit thing, one title, nothing equal-weight that shouldn't be.
3. **Moments used.** Each key moment the rung touches is performed by hand in the running app, keyboard and mouse.
4. **Journeys walked.** The journeys above that pass through the rung are walked end to end, checking the hand-offs: same words, same light, nothing lost between pages.
5. **Reviewed.** A review agent grades the result against this chapter and [27](27-web-design-system.md), and the grade with its screenshots goes in the PR. "Readable and not broken" is not a pass.
