# 27. Web design system: tokens and shared patterns

Status: proposal, 2026-10-16. Source baseline: `origin/main` at `f209d84ef`.

The concept is in [26](26-web-design-concept.md). This chapter is what a builder needs to make every screen out of the same parts: the tokens, the shared components and the lints that keep them shared. It is the web app's design source (D137); `apps/web/DESIGN.md` points here. Code paths are relative to `apps/web/src/`.

What exists today, from the audit ([research](../research/web-design/audit-2026-10-16.md)):

- **Tokens:** `styles/tokens.css` (510 lines) defines colour per theme, a ten-step type scale, radii, row heights, shell sizes and motion. There are no shadow tokens.
- **Lints:** `scripts/check-type-scale.mjs` blocks literal font sizes, and `scripts/check-motion.mjs` guards durations and animated properties. Both run in `npm run lint`.
- **Page frames:** `components/ModeFrame.tsx` and `components/Page.tsx`, plus bespoke headers.
- **Shared parts:** `components/PageParts.tsx` (`PageEmpty`, `PageError`), `KeyChip` (93 uses), `RuledRow`.

The parts are mostly there; screens bypass them or use them three ways.

## Colour

Each theme defines the same roles. Values below are Midnight (the default) and Light; Paper and Eclipse are generated from their three inputs (D139). Contrast ratios were computed on 2026-10-16 with the WCAG 2.1 formula.

### Ink: three depths and an overlay

| Role | Token | Midnight | Light | Used for |
|---|---|---|---|---|
| Room | `--ink-room` | `#080f17` | `#e8f0f5` | Rail, status bar, top bar. The darkest; it recedes. |
| Desk | `--ink-desk` | `#0d161f` | `#f4f9fc` | The page: lists and the space around them. |
| Lamp | `--ink-lamp` | `#17202a` | `#e7f2f8` | The lit item, and the reader page. In Light it is a faint cyan tint, because a near-white lamp on the pale desk didn't show (W1 comp). |
| Overlay | `--ink-overlay` | `#1f2a35` | `#fdfeff` (never pure white, DESIGN.md) | Compose, palette, sheets, menus, on a scrim. |
| Scrim | `--scrim` | `rgb(4 9 14 / 0.55)` | `rgb(16 38 59 / 0.18)` | Behind overlays. |

These map onto today's `--sidebar`, `--background`, `--surface` and `--surface-elevated`, which become aliases so existing classes keep working during the move.

### Text: three tiers, not five

| Role | Token | Midnight | Contrast on desk / lamp | Light | Contrast on desk / lamp |
|---|---|---|---|---|---|
| Primary | `--text-1` | `#e2e9f0` | 14.9 / 13.4 | `#10263b` | 14.5 / 15.1 |
| Secondary | `--text-2` | `#a6b2c0` | 8.5 / 7.6 | `#3e566b` | 7.2 / 7.5 |
| Faint | `--text-3` | `#808fa0` | 5.5 / 5.0 | `#5a6e80` | 5.0 / 5.2 |

Today's five tiers (`--foreground`, `--fg-2`, `--muted-foreground`, `--subtle-foreground`, `--faint`) collapse to these three; the audit found `fg-2` and muted indistinguishable at 12 to 13px. Faint is the floor for any text, including meta and eyebrows: every tier passes 4.5:1 on room, desk and lamp. Faint is not used on the overlay surface at sizes under 14px (4.4:1 there). The rail's labels use `--text-2` and its icons `--text-3`, so navigation is dimmer than the page (D133).

### Signal colours

| Role | Token | Midnight | Light | Rule |
|---|---|---|---|---|
| Lamp (accent) | `--signal` | `#5cd5fc` | `#006f93` | Focus rings, the one primary action, links, change landing. Never decoration. |
| Lamp soft | `--signal-soft` | `#01394a` | `#d5eff7` | Selected segment, text selection. |
| Needs you | `--cue` | `#f8ca65` | `#7a5400` | Due and overdue dates, "your turn" marks. Under 3% of a screen. |
| Failure | `--danger` | `#fd8a8c` | `#b42330` | Failed states and destructive verbs only. |
| Success | `--ok` | `#68d7a1` | `#1f7a4d` | Success states only ("healthy", "delivered"). Not for "done", which is the row leaving. |

Unread is shown by the text alone: the primary line is semibold in `--text-1`, the secondary joins it in `--text-1`, and a read row drops to normal weight in `--text-2` (BK, 2026-10-30: "if a row is unread then make the text bold and closer to white ... if it's now read then it's no longer bold and is a bit greyed out"). Nothing is drawn on the leading edge: the old neutral unread bar sat 6px outside the row's edge while the lit hairline sat on it, so one edge carried two states at two insets and a read-but-lit row still showed blue. The lit row is the `--ink-lamp` surface and nothing else, which is the one mark of where you are. Weight and text tier are non-colour cues, so the state does not rely on colour alone. In Paper, the accent is rust (`hsl(14 75% 38%)`); count badges and the sidebar's unread dot use `--text-1` instead of the accent there, fixing the "reads as error red" finding. They read `--unread-marker` and `--unread-marker-foreground`, which are the accent in the other themes and `--text-1` and the desk in Paper.

### Generating themes

`styles/themes.ts` (new) generates each theme's roles from three inputs in OKLCH: a base (the desk), an accent, and a contrast level, as Linear does. Steps are additive ΔL in OKLCH from the desk. On dark themes room is 0.03 darker, lamp 0.04 lighter and overlay 0.08 lighter. On light themes room is 0.027 darker, lamp 0.03 darker (a tint, as the W1 comp set Light's lamp, because a lamp lighter than a pale desk doesn't show) and overlay 0.018 lighter, which is near-white and never pure white. Text tiers are solved for target contrast on the desk: 14.9, 8.5 and 5.5 on dark themes (Midnight's values, which the first-draft targets of 14, 8 and 5.2 missed by ΔE 2.3 on text-1) and 14.5, 7.2 and 5.0 on light ones, with text-1's chroma at half the desk's on dark themes.

Every other role is a rule or an explicit override. Rules, borders, input fills and the shadcn aliases are lightness steps from the desk by polarity; status colours (cue, danger, ok, pending, star) have a fixed hue and a lightness and chroma per polarity; the text on a status fill is near-black on dark themes and near-white on light ones, tinted with the fill's hue; the chart colours are the accent, then the status colours, then orange and teal. The eight account pairs (`--account-<slot>` fill and `--account-<slot>-ink` text, chapter 33) are their own role: lighter fills with dark ink on dark themes, deeper fills with light ink on light ones, each pair at 4.5:1 or better, and used for nothing but the account mark. `--accent` in CSS is a neutral hover surface; the generator's accent is `--signal`. A theme can set `ink` (text hue and chroma), `neutralMarks` (unread marks and count badges in `--text-1`, used by Paper) and `overrides` per role.

The generator is `styles/themeGenerator.ts` with each theme's inputs in `styles/themes.ts`. `npm run gen:themes` writes `styles/themes.generated.css`, which `tokens.css` imports, so the CSS stays static. `npm run lint` fails when that file is stale. Spike S3's result is in [31](31-design-open-questions.md#s3-result-2026-10-16): generated Midnight is within ΔE OK 2 of the hand-tuned values on every role. The contrast-increased block is generated from the same inputs.

### The TUI follows the same tiers

`crates/tui/src/theme.rs` maps the three text tiers to its foreground levels, room to the hint bar and overlay to modals, in Midnight. The TUI does not paint a desk, and `--ink-lamp` is too close to a terminal's own black to show a cursor, so the cursor line stays a blue tint and is not the lamp. Painting a desk is open work.

## Type

Recursive Variable with all axes (`@fontsource-variable/recursive/full.css`, D126). The `MONO` axis gives mono; the `CASL` axis gives the reading voice.

| Role | Class | Size / line height | Axes | Used for |
|---|---|---|---|---|
| Display | `type-display` | 28 / 34 | `wght` 650, `MONO` 0 | The reader's title; page titles at 1024px and wider. |
| Title | `type-title` | 20 / 26 | `wght` 600 | Page titles under 1024px; sheet and dialog titles. |
| Heading | `type-heading` | 15 / 20 | `wght` 600 | Section headings inside a page, a person's name on their page. |
| Body | `type-body` | 14 / 20 | `wght` 420 | Row primary lines, controls, menus, the assistant's answer. |
| Reading | `type-reading` | 16 / 26 | `wght` 400, `CASL` 0.3 | Email text in reader mode, newsletter text, model-written prose, compose. Measure 64 to 70ch. |
| Meta | `type-meta` | 12 / 16 | `wght` 450, `MONO` 1, tabular figures | Times, counts, addresses, reasons' labels, the assistant's wire lines, key chips. |
| Eyebrow | `type-eyebrow` | 11 / 14 | `wght` 550, `MONO` 1, uppercase, 0.08em | Section labels inside lists ("YOUR TURN"), one style everywhere. |

Rules:

- A page has one Display or Title, then Headings, then Body. Never two titles (the 2026-10-14 pass found every page title shown twice).
- Secondary lines in rows are Body at `--text-2`, not a smaller size. Size steps mean hierarchy; colour steps mean importance within a level.
- Meta never sits at higher contrast than the Body it describes.
- `scripts/check-type-scale.mjs` gains a second rule: inside `features/`, the classes `text-xl` and up, and `font-semibold` with `text-md` or larger, may only appear through the role classes. The reader title allowlist entry goes away because Display is 28px.

Today's `--text-*` steps stay as the scale under the roles.

## Shape and depth

- **Radius (D127):** `--radius-control: 3px` for buttons, inputs, segmented controls, chips and menus. Surfaces, panes, rows, the reader page, sheets and toasts are square. `rounded-full` only for avatars, the unread dot, count badges and the assistant's live spinner. A new lint, `scripts/check-shape.mjs`, flags `rounded-full` outside `components/Avatar.tsx`, `components/CountBadge.tsx` and `components/UnreadDot.tsx`, and any `rounded-lg` or larger.
- **Depth:** on dark themes, depth is ink lightness only (room, desk, lamp, overlay). On light themes, the overlay adds `--shadow-overlay: 0 12px 32px -12px rgb(16 38 59 / 0.28)` and the lamp adds nothing. Drawers use `--shadow-drawer` (today's raw `-16px 0 40px -12px rgb(0 0 0 / .45)` in `styles/app.css`, made a token). No other shadows.
- **Rules:** hairlines (1px) at `--rule-soft` between groups, not between rows; rows are separated by space and the lamp. `--rule` (stronger) only for the page header's bottom edge and the reader page's leading edge.

## Spacing and layout

- **Base:** 4px. Named steps: `--space-row-x: 16px` (row padding), `--space-gutter: 24px` (between list and reader), `--space-page-x: clamp(16px, 3vw, 40px)`, `--space-section: 24px` (between sections in a list).
- **Shell:** rail 232px (collapsed 56), top bar 48, status bar 28, assistant 400 (320 to 720, D122), list 420 to 600 resizable (#299).
- **Frames:** list column max 72rem centred on wide screens; reader page measure 64 to 70ch inside a max 61.25rem column (#297).
- **Work rhythm rows (D125):** compact 44px, regular 52px, comfortable 60px, each with one primary and at most one secondary line. Today's 34/56/72 tokens are replaced; 34px can't hold a secondary line at 14px.

## Motion

Tokens stay: `--duration-fast` 120ms, `--duration-base` 180ms, `--duration-slow` 280ms, `--duration-scene` 1800ms; `--ease-out` `cubic-bezier(.23,1,.32,1)`. The moments and their motion are in [26](26-web-design-concept.md#motion). Two additions:

- `--duration-land: 600ms` for the change-landing underline.
- **"Section put to bed"** is defined per mode, for the finishing moment (D134): Now has no items; Messages has no Your turn; To do has no item due today or earlier; Updates' current digest is let go; Reading's edition is read or let go; Archive has no finishing moment. The scene plays once per section per day, recorded in local storage (a per-viewer convenience; losing it only replays the scene). With reduced motion the scene is its still frame (D134, W8c).

`scripts/check-motion.mjs` gains: handlers bound to keyboard events must not start transitions (checked by convention: transitions only in components that receive a `via: "pointer" | "key"` prop and skip on `"key"`). Shipped in W8b: a file that takes `LampVia` or `LeaveVia` may name a `transition`, an `animate-*` class or a fold only behind `via === "pointer"` or a row's `leaving`.

**One key folds, a repeat doesn't (W8b).** A single `e` shows where its row went: it folds like a pointer's done. Presses within 600ms of the last one drop their row at once, so clearing a list by key stays instant and the speed gate (j, k, s, e) is unaffected. Reduced motion never folds.

## Iconography

lucide only, stroke 1.75, 16px in rows and controls, 14px in meta and the status bar, 20px in the phone tab bar (touch targets). Each mode has one glyph, used for its rail item, its mode mark and its toasts: Now `Sun`, Messages `MessagesSquare`, To do `ListChecks`, Updates `Bell`, Reading `Newspaper`, Archive `Archive`, Inbox `Inbox`. Look-alike icons in the rail's collapsed state (clock, stopwatch, hourglass; shield twice) are replaced: the collapsed rail shows only the modes and Inbox (D133). No icon stands for AI (D141): a model feature is named for what it does and wears a task icon if the slot needs one (`PenLine` to draft, `TextQuote` to summarize), never the sparkle, a wand or a robot; who wrote the output is shown by `ModelMark` in words (D136). `check-ai-marks.mjs` enforces it. Icons never appear without a label except in the collapsed rail, the top bar's three buttons and the reader toolbar, and each has a tooltip with its key.

## The page frame

One component, `PageFrame` (replaces `ModeFrame`, `ModeHeader`, `PageTitle`, `Page`, `PageSection` and the bespoke headers; D128):

```
┌─────────────────────────────────────────────────────────────────────────┐
│ EYEBROW (optional, type-eyebrow)                                         │
│ Title (type-display ≥1024, type-title below)        [primary] [⋯]      │
│ One line saying what this page is for, ≤ 12 words (type-body, text-2)    │
│ [tab] [tab] [tab]   (optional, underline tabs)        freshness (meta)  │
├─────────────────────────────────────────────────────────────────────────┤ ← --rule
│ content                                                                  │
```

- Slots: `eyebrow?`, `title`, `purpose`, `primary?` (at most one, filled), `overflow?` (a ⋯ menu), `tabs?`, `freshness?`. Nothing else goes in the header.
- Height is fixed per breakpoint: 88px without tabs, 120px with tabs, at 1024px and wider; the purpose line hides under 640px.
- The purpose line is the mode's header line from `GetModeGuide` (D118), so it matches the TUI.
- Detail pages (a person, a reader page, a record) use the same frame inside their pane, with the item's name as the Title and its key facts as the purpose line.

## Rows

One anatomy, `Row` (D129), used by every list. A mode supplies the content of each slot; it can't add slots.

```
┌─lead─┬─primary line ─────────────────────────────────┬─trail──────────────┐
│ (A)  │ Pat Colleague                                  │ ◇ ▢   2h           │  ← mode marks, time
│      │ RustConf 2026 invite · "can you confirm by Fri"│                    │  ← secondary (one line)
│      │                                                │                    │  ← lit: trail becomes  r reply  e done ⋯
└──────┴───────────────────────────────────────────────┴────────────────────┘
```

- **Lead:** avatar (people), checkbox (To do), source mark (Updates, Reading) or amount (Archive). Nothing sits in the gutter outside it.
- **Primary line:** Body; semibold in `--text-1` when unread or the row needs you, normal in `--text-2` when read or calm.
- **Secondary line:** Body, `--text-2`, one line, truncated with an ellipsis (never clipped mid-word, an audit finding on phones).
- **Reason:** for the lit row, shown in the status bar's left side ("Here because: ...", meta, `--text-2`, the daemon's reason string, D111) and in the reader. Never a third line in the row: lighting a row must not change its height (D124's amendment). While a row is lit the status bar's left side reads "● <freshness short> · Here because: <reason>"; with nothing lit it shows the normal freshness line, and a failing sync keeps its warning in that place (W2). On phones, which have no status bar, the reason shows on the item's own page (the person page, the reader), not in the row (W2).
- **Trail:** marks (an attachment clip, a star, and a 14px glyph per other mode the item is in, all `--text-3` with tooltips), then the account mark while the view mixes accounts (a chip in the account's colour holding its tag, D147, chapter 33), then the time in meta. On the lit row the marks are replaced by the actions with keys, on the same line; the account mark and the time stay, the actions sitting in front of them, so the row you are on never loses when it arrived (D148). `Row` draws the account mark itself from the row's `account_id`, so no mode adds or forgets it. A conversation's message count follows the sender in meta (BK, 2026-05-11: "i need some indication that this is a conversation thread").
- **Marks, not sentences:** "Also in To do: ..." becomes the To do mark; "New sender. Keep in Messages? ..." becomes a `new` mark (meta, `--cue`, after the name). A new sender's lit row shows, in place of verbs, four mode-glyph chips with the keys `1` Messages, `2` Updates, `3` Reading and `4` Block (names in tooltips); the status bar spells it out while the row is lit: "New sender: 1 Messages · 2 Updates · 3 Reading · 4 Block" (W2). The digits answer only while a new sender is lit and never while typing in a field; everywhere else they stay the numbered tabs.
- **Lit:** `--ink-lamp` background, no leading-edge mark, actions in the trail. Same height as unlit. Hover on a pointer device shows the actions without lighting the row.
- **Leaving:** height and opacity to 0 in 120ms (pointer) or at once (key), the next row lit in the same frame.
- **Everywhere:** pickers and lists inside dialogs (labels, move to, snooze, the history list) use `Row` and the same keys: `j`/`k` move, `Space` toggles, `Enter` confirms, `Esc` leaves (BK, 2026-04-22: "I'm not able to navvigate with j and k ... and space to select/deselect and enter to confirm", and "why not a list like the oneon the messages list?").

## When an email arrived

An open email always shows when it arrived, as an absolute date and time in the viewer's local time zone, without hovering, clicking or unfolding anything (D148). It is set in meta (`--text-2`), not large, and it is never only relative.

- **The shape:** `Tue 21 Oct, 14:02`, the weekday, day, month and 24-hour or 12-hour time as the viewer's locale has it. A year other than the current one is added: `Tue 21 Oct 2025, 14:02`. Today's mail adds how long ago: `Today, 14:02 · 2h ago`. One formatter, `formatReceived` in `apps/web/src/lib/format.ts`, makes it everywhere; the TUI has the same rule in its own helper.
- **Where:** every message header in the reader, folded messages included (the folded line keeps the time on its right); each message in the Messages conversation; the To do details, the Updates peek and its email preview, the Reading reader (issue and article view, where it is the email's arrival, not the fetch), the Archive record's source email, Focus, and the assistant's opened links, which use the reader. The tooltip keeps the long form (`formatLongDate`) with the year and the time zone.
- **Lists stay short:** rows keep the compact time (`2h`, `Mon`, `Sep 3`) in the trail, and the lit row keeps it. Updates lines, which had none, show the time of their latest email the same way.
- **The TUI** follows the same rule: the reader's `Date:` line is local time, not UTC; lens readers (Messages, To do, Updates, Reading, Archive, places) show the date and time, never a day alone.

## Reason and model marks

- `ReasonLine` renders "Here because: ..." with the daemon's string. On a phone it sits under the person page's header (`PersonPane`), because a phone has no status bar to show the reason. The reader doesn't use it. A mail that isn't from a person shows its place as `WhyHereLine` at the end of the thread, on purpose: it "sits where reading ends, so a late answer never shifts the conversation above it."
- `ModelMark` renders `model · local` or `model · cloud` in meta beside any model-written text (gists, summaries, the assistant's answers in the reader), with a tooltip naming the model and a link to its provenance (D136).

## Keys

`KeyChip` stays the one key renderer. Placement (D130):

- The lit row's actions, as "verb key" pairs.
- The status bar's context strip: the page's three to five main verbs, supplied by the page through one hook (`usePageKeys`). Today's status-bar hints cut off mid-word when the assistant is open; the strip drops verbs from the end, whole.
- The `?` sheet and the command palette, with the key beside each command.

No legends under lists, no chips inside buttons, no chips on touch layouts (`(pointer: coarse)` hides them).

## Buttons and controls

| Kind | Look | Rule |
|---|---|---|
| Primary | Filled `--signal`, `--ink-room` text | At most one per page frame and one per sheet. Compose's Send, a proof's Apply, an empty state's fill action. |
| Secondary | 1px `--rule` border, `--text-1` | Other actions that need a visible button. |
| Quiet | No border, `--text-2`, hover `--ink-lamp` | Toolbar actions, row actions on the lit row. |
| Destructive | Quiet or secondary with `--danger` text | Never filled. |
| Segmented | Square group, 3px outer radius, selected segment `--signal-soft` with `--text-1` | Every either-or choice: reader view, search mode, density, Archive filters, Analytics grouping. Replaces round pills and filled toggle groups. |
| Tabs | Underline in `--signal` | Sections of one page only. |
| Chip | 3px radius, `--rule` border, meta | Recipients, query terms, filters applied. |

Every control has a visible hover and focus state, and controls sit at least 8px apart, so each reads as its own pressable thing (BK, 2026-05-11: "Are they buttons? Are they links? ... they don't have any hover state ... they don't even appear like they are distinct elements"). A two-or-more-way choice always shows which option is selected (BK, 2026-05-11, twice). Labels show their colour as a 6px square before the name, in the rail and in chips (BK, 2026-05-11: "labels should show their colors").

## Overlays

- **Compose:** a sheet bottom-right (new) or inline under the thread (reply), on `--ink-overlay`, with the scrim for the floating sheet. Vim's mode line shows only when vim mode is on.
- **Search, palette, `?`:** centred sheets on the scrim. Search shows one input (the top bar's field becomes a button while the sheet is open, fixing the two-inputs finding).
- **Proof sheet:** the confirmation for anything that can't be undone or that reaches another person (send, delete, unsubscribe, sweep, bulk changes): a Title naming the change in words ("Move 14 emails to Trash"), the items as compact rows, the one Apply (primary, with its key) and Cancel (Esc). The preview path that fills it is the same one the mutation uses (AGENTS.md).
- **Menus:** `--ink-overlay`, 3px radius, items in Body with their key in meta.

## Feedback

- **Toasts (D140):** bottom-right (top-right in Focus), one at a time, square, `--ink-overlay`, the mode glyph, what happened, Undo with `u`. 5s, or 8s with Undo.
- **Change landing (D134):** the `Landing` wrapper underlines changed rows for 600ms when an `MailChanged` event arrives from anything but this page's own action.
- **In progress:** one line in the status bar (`StatusActivity`), for sync, sweeps and assistant turns.

## States

One of each per page frame (D131):

- **Empty:** `PageEmpty`, top-aligned in the content column: a Heading saying what isn't here ("Nobody is waiting on you"), one Body line with the next useful place ("Next: 3 in To do"), and the fill action if there is one. No icon by default; a mode's empty state may use its mode glyph at 24px. Replaces `Centered`, `EmptyLens`, `FocusFinish`, `NowEmpty` and the assistant's own empty state.
- **Loading:** `PageSkeleton`, built from the page's own `Row` with placeholder bars, so its shape matches. Shown only after 150ms, so cached pages never flash.
- **Error:** `PageError`: a Heading saying what happened in plain words, a Body line saying what still works, and one action that can fix it. The raw error goes under a "Details" disclosure. A lookup that can't succeed (a bad id) offers the way out ("Go to Inbox"), never "Try again".
- **Offline:** cached content stays; one banner under the top bar, "mxr isn't running. Showing what was loaded at 21:04.", with "Copy start command". The sidebar keeps its items. The recovery card's "protocol upgrade" text appears only when the daemon answered with a protocol mismatch.

## Lints and tests that keep it shared

| Check | Where | What it stops |
|---|---|---|
| `check-type-scale.mjs` (extended) | `npm run lint` | Literal sizes; titles and headings outside role classes. |
| `check-motion.mjs` (extended) | `npm run lint` | Animated layout properties, non-token durations, transitions on key-driven changes. |
| `check-shape.mjs` (new) | `npm run lint` | `rounded-full` outside people, dots and counts; radii above 3px. |
| `check-frames.mjs` (new) | `npm run lint` | A route component that doesn't render `PageFrame`; a `PageFrame` with more than one `primary`. |
| `e2e/design-gate.spec.ts` (new) | CI, Web E2E | Screenshots of every main page in four themes at 390, 1024 and 1440px, attached to the run for the review in the design gate ([26](26-web-design-concept.md#the-design-gate)). It asserts one lit row after a `j`, one page title, and no key chips under `(pointer: coarse)`. |
| `keymapParity.test.ts` (existing) | Web unit tests | Web and TUI keys drifting. |
