# 31. Design open questions and spikes

Status: proposal, 2026-10-16. Covers chapters [26](26-web-design-concept.md) to [30](30-design-roadmap.md).

## Spikes (run before the rung they gate)

For each, record what was run, the result, the date and the conclusion here under a "Result" heading, then update the chapter it affects.

| # | Question | Why it matters | Blocks | Affects |
|---|---|---|---|---|
| S1 | Why do Now ("12 people waiting on you"), Messages' Your turn, Focus & reply ("Nobody is waiting"), Owed replies ("You don't owe anyone") and Waiting on disagree on the same mail? Read each surface's daemon query at a named SHA and list the definitions. The audit compared only what the screens say. | D132 needs one definition; if a surface is simply wrong, that's a bug fix, not a design choice. | W4 | [27](27-web-design-system.md), D132 |
| S2 | What does `@fontsource-variable/recursive/full.css` (about 305 KB Latin) cost against `mono.css` (about 73 KB) on a fresh PWA install: transfer, layout shift with `font-display: swap`, and paint time for the reading voice? | D126 assumes the cost is one cache fill. If shift or paint is visible, preload the font or subset the axes. | W1 | [27](27-web-design-system.md#type) |
| S3 | Does the OKLCH generator, given Midnight's base, accent and contrast, reproduce today's Midnight roles within ΔE 2? | Midnight must not feel like a new app after W8. | W8 | [27](27-web-design-system.md#generating-themes), D139 |
| S4 | Does moving the lamp on a 5,000-row list stay inside `e2e/large-list.spec.ts`'s budget (60,000 coverage blocks per step) and key-to-paint p95 under 50ms? | The lamp must never re-render the list (21's standard "never re-render the list"). | W2 | [27](27-web-design-system.md#rows) |

## Product questions for BK

| # | Question | Status |
|---|---|---|
| Q1 | Is "the night desk" the right concept for the web app: one lit thing on a dark desk, the modes as sections, Now as the front page, the reader under the lamp, the assistant as the copy desk? BK never named a concept; this is the blueprint's proposal from his statements (D123). | Open; blueprint assumes the night desk. |
| Q2 | Does "dense where you work, generous where you read" match what BK wants? In March BK asked for compact and terminal-like ("too much paddings, i want more of a terminal aesthetic"); since May he has asked for the email and the editor to "take center stage". Nobody has asked whether the March density still holds for lists (D125). | Open; blueprint assumes the split. |
| Q3 | What does "waiting on you" mean: Messages' Your turn including new senders, or only people you've written to before? Today the surfaces disagree (S1). | Open; blueprint assumes Your turn for people you've written to, with first-time senders counted apart as "new" (D132). |
| Q4 | Does BK still want editor colour themes (catppuccin, gruvbox, nightfox, kanagawa, one), as he asked for the desktop app on 2026-04-22? The web app has Midnight, Eclipse, Paper and Light, and nobody revisited the request. | Open; blueprint assumes not now, and the generator (D139) makes them three values each if yes. |
| Q5 | Should a proof let you drop single items before applying ("archive these 14, but not that one")? Today approval runs exactly the stored call (D122), so dropping items needs the daemon to accept a narrowed selection and record it as the user's edit. | Open; blueprint assumes whole-proof apply, or send back with a note. |
| Q6 | Is square still BK's taste ("i'm not expecting to see rounded corners", 2026-03-22, for the desktop app)? D127 makes surfaces square and controls 3px. | Open; blueprint assumes yes. |

### S4 result (2026-10-16)

Run at W2 on `feat/web-w2-rows` (base `e690a20de`), against the e2e daemon's fake mailbox. The daemon caps Messages' Recent band, so no list in production has 5,000 `Row`s yet; the specs stand in for the long lists W3 moves onto `Row` by growing Recent to 5,000 people from a real row (`e2e/helpers/bulkPeople.ts`) and pressing `j` with the lamp.

| Measure | Where | Result | Budget |
|---|---|---|---|
| Code executed per `j` (V8 block coverage over `/src/` and the virtualizer, 60 presses) | `e2e/large-list.spec.ts` "5000 people: each j moves the lamp inside the same code budget" | before the fix below: 12,554,585 per step; after: 31,848 per step | 60,000 |
| Keydown to paint, p95, production build | `e2e/speed.spec.ts` "the lamp moves under budget on 5,000 people (spike S4)", 20 presses each way | `j` 26.8 ms, `k` 25.5 ms | 50 ms |

Conclusion: moving the lamp is inside both budgets, so the lamp can be built on `Row` for long lists. Three findings:

- The first run failed by a factor of 200. `MessagesRoute` passed `useAdvanceOnRemoval` an `isPresent` that scanned all rows, and the hook calls it once per row on every lamp move, so one `j` was quadratic. A `Set` of row ids fixed it. The same shape (a per-row scan inside a per-move effect) will fail the budget in any list W3 converts.
- Rows are memoised (`Row`, and the Messages person row on primitives and stable callbacks), so a lamp move re-renders the two rows it touches. The list shell still maps all 5,000 elements per move (about 20,000 of the 31,848 blocks, linear in the list). Virtualise the long lists (Inbox already is) or move the lit id into a per-row store subscription if a list has to hold more than a few thousand rows unvirtualised.
- Looking up a row's membership inside the row (not in the list's map) removed another 20,000 blocks per step. Keep per-row lookups inside the memoised row.
