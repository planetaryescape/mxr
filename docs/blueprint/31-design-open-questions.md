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
| Q3 | What does "waiting on you" mean: Messages' Your turn including new senders, or only people you've written to before? Today the surfaces disagree (S1). | Decided by the orchestrator under D132 (2026-10-16), BK to confirm: the total is Messages' Your turn rows; it splits into "people you write with" (You owe) and "others who wrote to you" (every New from people row, repeat senders included). Built in W4a. |
| Q4 | Does BK still want editor colour themes (catppuccin, gruvbox, nightfox, kanagawa, one), as he asked for the desktop app on 2026-04-22? The web app has Midnight, Eclipse, Paper and Light, and nobody revisited the request. | Open; blueprint assumes not now, and the generator (D139) makes them three values each if yes. |
| Q5 | Should a proof let you drop single items before applying ("archive these 14, but not that one")? Today approval runs exactly the stored call (D122), so dropping items needs the daemon to accept a narrowed selection and record it as the user's edit. | Open; blueprint assumes whole-proof apply, or send back with a note. |
| Q6 | Is square still BK's taste ("i'm not expecting to see rounded corners", 2026-03-22, for the desktop app)? D127 makes surfaces square and controls 3px. | Open; blueprint assumes yes. |
| Q7 | BK's message of 2026-10-22 ended mid-sentence: "drafting should also understand all the things in /humanizer and". What came after "and"? | Open; blueprint assumes nothing more for the composer ([34](34-composer.md)). |
| Q8 | When Draft for me finishes what you typed, should it keep your sentences word for word (changing one only for a typo or to join it to a new sentence), or may it smooth your wording too? Polish is the action that rewrites your words. | Open; blueprint assumes word for word (D153). |
| Q9 | With `llm.allow_cloud_relationship_data` off, may you pick a cloud model for one draft? It would see only the conversation you're answering, not your past mail, and the picker would say "this conversation only". The assistant refuses cloud picks in that state (D145) because it reads across the mailbox. | Open; blueprint assumes yes, labelled (D156). |

## Spike results

### S3 result (2026-10-16)

Run twice. First as a spike script (culori, on `origin/main` at `1e0bbc3c5`) that fitted generator rules to the committed `tokens.css`, then as the generator itself on `feat/web-w8a-themes` (W8a): `styles/themeGenerator.ts` generates every role from a base, an accent and a contrast level, and `styles/themes.test.ts` holds Midnight as committed before the generator as a frozen fixture and asserts every role is within ΔE OK 2 (Euclidean distance in OKLab, times 100).

| Check | Result |
|---|---|
| The spec as first written (base is the desk, additive ΔL steps, text targets 14 / 8 / 5.2 on the desk), Midnight | Fails on text-1 only, at ΔE 2.30. Today's tiers sit at 14.89 / 8.47 / 5.52 on the desk, not 14 / 8 / 5.2. |
| Targets 14.9 / 8.5 / 5.5, text-1 chroma at half the desk's | Passes. The chroma change takes text-1's ΔE2000 from 2.96 to 0.27. |
| Generator, Midnight, all 71 declared roles (aliases included) | Max 1.33 (the primary's text), mean 0.22. |
| Generator, Midnight, contrast-increased block | Max 0.92, mean 0.34. |
| Light, same comparison | Max 1.99, mean 0.29. |
| Eclipse | Max 6.50 on the text-on-fill colours, mean 1.21, and 19.1 on `chart-6`, which moves from violet to teal on purpose. |
| Paper | Max 5.72 on `cue`, mean 1.74, and 32.4 on `unread-marker`, which moves from rust to `--text-1` on purpose (D139). |

Findings that changed the spec:

- The steps are additive ΔL. Midnight's room is 0.031 below the desk, its lamp 0.044 above and its overlay 0.084 above (the spec said 0.07).
- Light and Paper's lamps are darker tints (ΔL about -0.025 and -0.035), not "2% lighter on light themes". The W1 comp already set Light's lamp to a tint; the spec was wrong.
- Many tokens had no generator rule: status colours, rules and borders, input fills, the shadcn aliases, the charts, the text on status fills and the contrast-increased block. Each now has a rule fitted to Midnight or an explicit per-theme override. CSS `--accent` is a neutral hover surface; the generator's accent is `--signal`.
- No single rule for the text on a status fill reaches ΔE 2 on Midnight (best uniform fit 2.33), because today's five values were tuned one by one. The generator carries a lightness and chroma per fill.

Conclusion: the generator reproduces Midnight within ΔE OK 2 on every role, so Midnight does not become a different app. Light is within 2 as well. Eclipse and Paper move more because their hand-tuned values were less regular; the biggest moves are the text-on-fill colours (near-black either way), Paper's `cue` and Eclipse's chart palette. The checks run in `npm run test` and `npm run lint`.

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

### S1 result (2026-10-16)

Run at f209d84ef by reading each surface's daemon query. Every surface was built from the same desk lanes (`compose_desk`) except the thread line, the Waiting on view and Screener, but each counted them differently.

- Desk lanes (`crates/daemon/src/handler/desk_lanes.rs`, `thread_row`): a thread is somebody's turn when its newest message is inbound, in the inbox, from a Person and not Copied. It is You owe when you wrote in the thread or to them (contacts, 30-day window) and drops after `turn_decay` (max of three times their cadence and 7 days). Everything else is New from people, including repeat senders you never answered ("wrote to you"), inside 7 days (30 for allowed senders).
- Now (`now.rs`): the headline and People total counted distinct counterparty emails over You owe plus New from people. The rail's Messages badge added the Waiting on lane to that.
- Messages (`messages.rs`, `messages_view.rs`): Your turn counted rows, so a person with several threads is one row, a group thread is its own row and two addresses of one person (`person_links`) are one row. Now counted emails, so groups and merged people disagreed.
- Owed replies (`owed.rs`): the You owe lane only, as conversations.
- Focus & reply (web `useFocusSession.ts`): the You owe lane, so it said nobody was waiting while Now counted people who had written for the first time or repeatedly.
- Thread line (`thread_context.rs`): "you owe" whenever the newest message was not yours and not bulk, ignoring inbox, sender kind, whether you write with them, decay and Copied shape.
- Waiting on (Messages with `turn=theirs`): the daemon cleared Your turn before it chose the empty sentence, so the view said "Nobody is waiting on you." for the inverse question.
- Screener: the page was the store's all-time queue in any folder; the badge was inbox mail first seen in the last 14 days. On the fake data that is 28 against 17, and the repeat senders first seen 16 days ago are the difference.

Conclusion: one bug (Waiting on's sentence), three real definition gaps (thread line, Focus, Screener) and one unit mismatch (emails against rows). D132 now says what the shared definition is. Built in W4a: `crates/daemon/src/handler/turn_units.rs` counts rows with Messages' own keys and splits by lane; Now, the rail, Messages, Owed and the desk's `people` field read it.
