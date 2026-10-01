# Web visual review, 2026-10-01

Rubric criteria B2 (motion) and B7 (typography), plan item 9 in
`docs/blueprint/21-web-experience.md`. Reviewed on `feat/polish` against the
e2e demo account (alex@demo.mxr.local, 218 messages), in Chromium at 1440 x 900
and 390 x 844, in the Midnight (dark) and Light themes.

Screens: the desk, the inbox list, the reader (split, a five-message
conversation), Reading, Paper trail, and focus mode, plus focus mode with a
Done toast showing. The screenshots aren't committed; the checks below are.

## What was checked

| Check | How | Result |
|---|---|---|
| Durations come from the motion tokens; only transform and opacity move | `scripts/check-motion.mjs`, run by `npm run lint` | Fixed: 21 `transition-colors`, one `transition-all`, a bare `transition`, the button's colour transition list, the sync bar's width transition, and two literal `1500ms` pulses |
| Reading measure 60 to 80 characters | `reading.spec`, Reader and Plain at 1440 and 1920 | Reader 77.2, Plain 72.0 |
| The same age reads the same way | `formatWhen` in `lib/format.ts`, unit-tested at each boundary | Fixed: the list, reader, desk, places and focus now share it (they used three wordings: "4:26 PM", "5h", "2 hours ago") |
| Toasts never cover a primary action | `focus.spec` at 1440 and 390 | Fixed: a toast sat on focus mode's queue keys, including "Done, no reply needed" |
| Text fits its box at 390 | Screenshots | Fixed: the desk's You owe heading wrapped inside its fixed height |
| Tabular numbers, balanced headings, one icon family, no em dashes | Screenshots, existing lint | No change needed |

## Fixed

- **Motion.** Hover and focus colours now change at once. The sync bar
  scales instead of animating its width. The pending pulse reads
  `--motion-duration-pulse`. Reduced motion now transitions opacity only.
- **Dates.** "Just now", "12m", the time today, "Yesterday", the weekday,
  "Sep 3", "Sep 3, 2024". Ahead of now it reads the time today, "Tomorrow",
  the weekday, then the date. Before this, future-dated mail showed only a
  time. The reader no longer adds a second, relative date.
- **Toasts.** Bars that hold a primary action carry `data-toast-keep-clear`
  (focus keys, the composer's Send row, the bulk bar). While a toast shows,
  the stack sits above any of them it would cover.
- **Desk heading on a phone.** It stays on one line; the focus-mode link
  truncates and its key chip gives way when the list is narrow.

## Still open

- **Formatted HTML ignores the measure.** The Formatted view follows the
  sender's layout, so a plain HTML message runs about 95 characters a line
  at 1440. Holding the iframe to the measure would clip 600 to 640 px
  newsletter tables, so it needs its own decision: a wider cap for HTML, or
  the measure only for HTML without fixed-width tables.
- **The reader header at 390** leaves a dangling "·" after "5 messages" when
  the participants wrap to the next line.
- **The context line** ("last spoke today") keeps its own lowercase day
  wording because it sits inside a sentence. It agrees with `formatWhen` by
  day, not by time of day.
- **Durations elsewhere.** The context block, the composer's "Saved 2m
  ago" and the analytics pages keep their own duration helpers; only
  points in time were converged.
- **The waiting variant of reply later** toasts "Back … if nobody replies",
  which doesn't start with the verb table's "Reply later: back".
- **Copy and sound** still need a person (C3, B4).
