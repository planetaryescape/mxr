# Web app experience rubric (v2)

**Changelog.** v2 (2026-09-30): adds A11 triage at a glance, A12 one
vocabulary across clients, section D (budgets measured on a real mailbox)
and section E (independent grading and a dogfooding log). v1 (2026-09-28):
sections A to C.

`docs/web-app-rubric.md` is the floor: correctness, parity with the TUI, and
the triage loop working. It passed in v0.6.33, and the app still felt like
Gmail in navy. This rubric covers the rest: does the app treat email the way
people actually use it, and does it feel good to use every day.

It is built from two pieces of research, summarised with sources in
[Evidence](#evidence) at the end. The short version:

- People use the inbox as a todo list, and it is a bad one. 75% defer mail
  every day, and 32% mark mail unread just to remember it (Microsoft Research,
  CHIIR 2019).
- The stress comes from the pile's count and its constant arrival, not from
  the work (Kushlev and Dunn; Gloria Mark, CHI 2012).
- People keep the innovations that remove a decision: pin and sweep, bundles,
  snooze, reply queues, speed. They drop the ones that add a decision: manual
  screening of everyone, categories they can't correct, AI that is wrong or slow.
- Speed is what people pay for. AI that slows down opening an email is a
  reason people churn (Superhuman reviews, 2025).
- Delight works when it is rare, earned and optional. Frequent keyboard actions
  should not animate at all.

mxr has an advantage no hosted client has. The daemon already knows who you
owe a reply, what you promised, who has gone quiet, how you write to each
person and which senders are bulk mail. It does all of that locally. The
current web app hides that knowledge in side rails and dashboards and leads
with a Gmail-style list sorted by arrival. The rubric's job is to turn that
around.

## Scoring

Same scale as the parity rubric: 0 absent, 1 present but flawed, 2 solid and
verified in a browser against the FakeProvider daemon, 3 best in class.

**Pass bar:** every criterion at 2 or better. Sections A (the model) and B1 to
B3 (speed, motion and feedback) must reach 3. From v2, a 3 in section A also
needs the dogfooding log (E2), and every score of 2 or more is graded by
someone who did not build it (E1). A criterion scores 2 only with a
Playwright journey or a recorded session behind it. Timing criteria need a
measured number in an e2e test, not an impression.

## A. The model: rethink the conventions

Every row names the convention it breaks and why.

| # | Criterion | Convention it breaks | How to check |
|---|---|---|---|
| A1 | **Obligations first.** The landing view answers "what needs me?" before "what arrived?": replies I owe, promises I made that are due, threads waiting on someone else, and new mail from people. Arrival order is one lens, not the home. | Inbox sorted by arrival as the home screen. | Open the app: the first screen shows owed replies and due promises from the daemon, each with the reason it is there. |
| A2 | **Counts mean work.** The only badges are things that need action (owed, due, screener decisions). Newsletters and receipts never carry an unread count. | Unread count as the measure of the inbox. | 30 unread newsletters leave the badge at zero; one owed reply makes it one. |
| A3 | **Kinds of mail get kinds of treatment.** Mail from people, reading (newsletters), paper trail (receipts, notifications, deliveries) and unknown senders each have their own place and treatment. Every placement says why and can be corrected with one key, and the correction is remembered. | One undifferentiated pile, or opaque auto-categories you can't fix. | Open a message in Paper trail: "Here because: automated sender, has List-Unsubscribe." Press the correction key: it moves and future mail follows. |
| A4 | **Sweep, don't process.** Bulk mail is grouped by sender or kind and cleared in one action. Pin the exceptions and sweep the rest, with a preview computed by the daemon's dry-run path. | Handling bulk mail one message at a time. | Paper trail with 14 items: pin 1, sweep, preview says 13, confirm, undo restores 13. |
| A5 | **Deferral is a first-class verb.** Reply later, snooze, remind-me-if-no-reply and waiting-on are each one key, and each takes natural-language time. | Mark unread or star as a makeshift todo. | `h` then "tue 9" snoozes to next Tuesday 09:00, shown before commit. Send with "remind in 3d if no reply": the thread returns if nobody answers. |
| A6 | **Focus and reply.** One mode works through the reply queue: the thread on one side, a draft in your voice on the other, send-and-next, and progress through the queue. | Replying scattered across the day, one open thread at a time. | Enter focus mode with 4 queued: reply to each with keys only, then the queue is empty. |
| A7 | **Context before content.** A thread opens with what matters: the one-line gist, what they are asking of you, your open promises in the thread, and how you know this person (last contact, usual reply time). The ask is highlighted in the body. | Raw messages, newest at the bottom, work out the context yourself. | Open a thread with a question: the briefing states the ask and the body highlights it. No layout shift when the briefing lands. |
| A8 | **Promises are caught both ways.** When you send "I'll get you the deck by Friday", the app offers to remember it before the send finishes. Promises made to you show on that person's context. | Commitments live in your head. | Send a draft containing a dated promise: a reminder is offered with the date filled in, and it shows under Due on Friday. |
| A9 | **Chrome earns its place.** Every persistent control is justified by frequency or discoverability. Reply sits where reading ends (the bottom of the thread), not in a toolbar above it. Rare actions live in the command bar. The sidebar holds places, not a list of 20 folders. | Toolbars of icon buttons; a sidebar of every folder and label. | Count persistent controls in the reader against the current app; each one has a stated reason in this doc or the component. |
| A10 | **Time is shown as it matters.** "Waiting 5 days", "due tomorrow", "Maya usually replies in 2 hours, it has been 2 days". Relative when recent, absolute when it matters, always in the user's zone. | A timestamp column. | Waiting lane shows age and cadence drift from the daemon, not the send time. |
| A11 | **Triage at a glance.** Each conversation from a person says, in its row, what it is about and what (if anything) they want from you, so you can decide what to open without opening it. Lines arrive as they are ready, never slow or shift the list, and say where they came from. Opening a short conversation shows the ask marked in the message, not a repeat of the gist. | A list of subject lines and raw snippets; summaries you only see after opening the email. | Desk and inbox rows show "Asks: …" and the gist as each lands, with no row moving (`row-gists.spec`); the large-list gate and key-to-paint budget hold with gist lines on every row; hover names the model; no model means rows as before. |
| A12 | **One vocabulary across clients.** The same verb has the same key in the web app and the TUI, so what you learn in one works in the other. | Each client inventing its own keys. | `keymapParity.test.ts` compares the web action registry with the TUI keymap (`docs/reference/tui-keymap.json`, written by the TUI's keymap test) and fails on a verb bound to different keys, unless `KEYMAP_DIFFERENCES` lists it with a reason. |

The inventory of persistent controls and the reason for each is in
[web-app-controls.md](web-app-controls.md).

## B. The feel

| # | Criterion | How to check |
|---|---|---|
| B1 | **Speed budget.** Keydown to paint under 50 ms at p95 for `j`, `k`, `e`, `s`, and opening a cached thread. No spinner or skeleton for local data under 300 ms; once shown, a skeleton stays at least 400 ms. AI work never blocks reading or triage. | An e2e test records keydown-to-paint with `performance.now()` and a double `requestAnimationFrame`, and asserts the budget. |
| B2 | **Motion with a job.** Motion is used only for orientation (list to reader, lane to lane, a dialog appearing from its trigger) and for state that can be undone. Repeated keyboard actions do not animate. Shared tokens for easing and duration; no `ease-in` on UI; only `transform` and `opacity`; `prefers-reduced-motion` keeps fades and drops movement; an in-app override exists. | Grep: no ad hoc `duration-*` values outside the tokens. Keyboard archive of 10 rows shows no transition. Reduced motion run shows no transforms. |
| B3 | **Every verb has feedback and a way back.** For each action (archive, trash, snooze, reply later, label, move, send, unsubscribe, sweep), the trigger, rule, feedback and undo are defined (Saffer's four parts) and implemented. Undo replaces confirmation wherever the daemon can undo. | A table in the code maps each verb to its feedback; e2e covers undo for each. |
| B4 | **Earned delight.** Clearing a lane or the desk gets a crafted moment that fits the brand, rare by nature and never scolding. Sound is optional, off by default, synthesised locally, short and soft, and it never plays for navigation or in a background tab. No points, no streaks. | Clear the desk: the moment appears once. With sound on, send plays one short tone; `j` never does. |
| B5 | **Natural language where people think in words.** Every time field (snooze, send later, remind, reply later) takes phrases like "tomorrow", "tue 9", "in 3d", "next month". The recognised part is highlighted, the resolved time is shown before commit with implied parts muted, ambiguous input offers choices, and the resolution matches what the daemon stores. | Type "fri 3": chips offer 03:00 and 15:00, 15:00 preselected; commit; the daemon reports the same instant. |
| B6 | **The app teaches its keys.** Tooltips, menus and the command bar show shortcuts. After the same pointer action three times, a quiet hint names the key once. | Click archive three times: one hint "Press E". It never appears again for that action. |
| B7 | **Typographic craft.** Tabular numbers for counts and times; `text-wrap: balance` on subjects and headings, `pretty` on prose; reading measure 60 to 80 characters; one icon family; consistent relative dates; correct plurals; no em dashes in copy. | Visual review at 1440 and 390 wide; lint for mixed icon libraries. |
| B8 | **Honest, quiet system states.** Offline, syncing, AI working and AI unavailable are visible without shouting and never block input. Errors say what happened and what to do. | Stop the daemon mid-session: a calm banner, input still works on cached data, recovery is automatic. |
| B9 | **Pointer and touch feel direct.** Press feedback on buttons (scale 0.97), popovers grow from their trigger, hover styles only on hover-capable devices, touch rows swipe with colour showing the pending action and a longer throw for destructive ones. | Manual pass on desktop and a 390 px touch viewport. |

## C. Trust

| # | Criterion | How to check |
|---|---|---|
| C1 | **Privacy you can see.** Remote images and trackers are blocked by default, and the reader names what was blocked and from whom. Anything AI-generated says whether a local or cloud model produced it and whether your history was used. | Open a newsletter with a pixel: "Blocked 2 trackers (Mailchimp)." A draft says "Local model, based on 5 of your replies to Maya." |
| C2 | **AI shows its work.** Routing reasons, briefing sources and draft sources are one step away, and AI is never the only way to do something. | Every AI surface links to its evidence. |
| C3 | **Never guilt the user.** No red badges for backlog size, no streak loss, no "you have 3,412 unread" nag. Copy is calm and specific. | Copy review against this rule. |

## D. Real scale

The demo mailbox has about a hundred conversations. These budgets are
measured read-only against a real, large mailbox (never by writing to it),
and each measurement is recorded with its date and the mailbox size.

| # | Criterion | Budget | How to check |
|---|---|---|---|
| D1 | **Desk compose.** `GetDesk` for all accounts. | < 300 ms warm | `time mxr desk --format json` on the real install, second run. |
| D2 | **Owed replies.** `ListOwedReplies`. | < 1 s warm | `time mxr owed --format json`, second run. |
| D3 | **Places.** Reading and Paper trail lists. | < 100 ms warm | `mxr reading` / `mxr paper-trail` timings from the daemon log (`place listed elapsed_ms`). |
| D4 | **List scroll.** A real inbox scrolled in the web app. | Within the large-list gate (60k list blocks per step) and key-to-paint p95 < 50 ms | Run `large-list.spec`'s measurement against the real bridge, or a recorded session with the same probe. |
| D5 | **New SQL.** Every query a rung adds. | Recorded, no regression past the budgets above | `sqlite3` read-only (`?mode=ro`) on the real DB: counts and timings only, no content. |

## E. Independent grading

| # | Criterion | How to check |
|---|---|---|
| E1 | **Evidence for every score of 2 or more.** Each such score cites a spec, a measurement or a recorded session, and is graded by a reviewer who did not build the work. | Every row in Scores names its evidence and its grader; a builder's own score is a proposal. |
| E2 | **Dogfooding for 3s in A.** A score of 3 in section A needs a dogfooding log: BK's own week of daily use, with what worked, what didn't, and when he reached for another client. | A dated log in `docs/` covering at least five working days. |

## Scores

Scored on 2026-09-28 on `feat/delight` (rung 6, on top of v0.6.38's
places), from the Playwright suite against the FakeProvider daemon (131
tests) and screenshots at 1440 and 390 px in the dark and light themes. The
final full run passed all 131 under a machine load of 110 to 336 (an
earlier run's one failure, `verbs.spec` picking a conversation row for
snooze, is fixed the way `snooze.spec` does it). A score of 2 needs a journey in the suite; 3 is reserved for what a keyboard
user would notice and prefer.

**Pass bar (v1): met.** Every criterion is at 2 or better, and all of section A
and B1 to B3 are at 3. Under v2 it is not met: the new criteria below are
below 2 in places, and section A's 3s wait on the dogfooding log. What is left at 2 is in
[docs/issues/experience-rubric-gaps.md](issues/experience-rubric-gaps.md).

| # | Score | Evidence |
|---|---|---|
| A1 | 3 | The app opens on the desk with lanes and a reason on every row (`desk.spec` "the app opens on the desk: lanes with reasons, and work-only badges"); the inbox is `g i` away and can be made home ("a person who prefers arrival order…"). |
| A2 | 3 | The desk badge equals owed plus due and the inbox carries no number (`desk.spec`, same test); Reading has no unread counts (`places.spec` "Reading shows every issue already open, with a reason and no unread counts"). |
| A3 | 3 | `places.spec`: every Reading issue and Paper trail bundle says why it is there; "moving a sender from Reading to People takes it to the desk, and u brings it back". |
| A4 | 3 | `places.spec` "Paper trail: pin one, sweep the bundle without it, undo puts the rest back" and "S previews a sweep of the whole place from the daemon's dry run". |
| A5 | 3 | `natural-time.spec` "fri 3" offers 15:00 and 03:00 and stores what the preview showed; `snooze.spec`; `desk.spec` done waiting; `focus.spec` send and remind. |
| A6 | 3 | `focus.spec` "g F works through the queue: send and next, skip, snooze, then a calm finish", plus undo inside the countdown and a phone layout. |
| A7 | 3 | `reader-context.spec`: facts as the thread opens, the ask lands without moving the messages and is marked in the body. |
| A8 | 3 | Both ways, each with its own owner: `focus.spec` "a dated promise in a reply is offered with its time and kept as a reminder" (yours, on send) and `reader-context.spec` "promises both ways show on the person's context, each with its own owner" (yours and theirs in the context block). |
| A9 | 3 | Every persistent control is listed with its reason in [web-app-controls.md](web-app-controls.md); `controls.spec` fails if a surface gains or loses one. The list's Refresh button couldn't justify itself and is gone. Reply sits at the end of the thread (`reader-context.spec` "r opens the reply at the end of the thread"). |
| A10 | 3 | `desk-time.spec`: an owed row reads "2d · usually 4h" in the theme's warning colour when late, and a Waiting row reads its age ("5d") in the muted colour. |
| B1 | 3 | Keydown to paint p95 on the desk: j 16.1, k 16.7, e 18.9 ms; inbox: j 17.0, k 16.9, e 16.9 ms (`speed.spec`, budget 50). 5,000 rows: 20,572 list coverage blocks per scroll step (`large-list.spec`, gate 60,000). `loading-states.spec`: a 100 ms list or thread load shows no skeleton, a 600 ms one shows it for at least 400 ms (`useDelayedPending`, `useDelayedPending.test.ts`). A held `e` archives one conversation (`delight.spec` "holding e archives exactly one conversation", `dispatcher.test.ts`). |
| B2 | 3 | Motion tokens only (no ad hoc `duration-*` or `ease-in` in `src`); `natural-time.spec` reduced and full motion, rows never animate the cursor; `delight.spec` "with reduced motion the low tide is a still frame". |
| B3 | 3 | `features/mail-actions/verbFeedback.ts` gives every verb its trigger, optimistic feedback, toast words, sound and undo path; toasts and sounds read from it, and `verbFeedback.test.ts` fails if a state-changing action has no entry. `verbs.spec` runs archive, read and archive, trash, spam, star, unread and snooze, checks each toast's words against the table and undoes each with `u`; reply later undoes from its toast. Labels, move, done waiting, sweep and move sender have undo journeys in `labels.spec`, `desk.spec` and `places.spec`. |
| B4 | 2 | `delight.spec` "clearing the desk by keyboard earns low tide once, not on a revisit" (motion under 2 s, then still); sound is off by default, previews in the browser, plays on archive and never on `j`; send plays once (`feedback.test.ts`). Not yet heard by a person on real speakers. |
| B5 | 3 | `natural-time.spec`: highlighted phrase, resolved time before commit, choices for "fri 3", and the daemon stores the previewed instant. |
| B6 | 2 | Shortcuts in tooltips, menus, the palette and `?` (`keyboard-help.spec`); `delight.spec` "three pointer archives earn one key hint, and it never comes back". Sidebar and palette clicks don't count toward hints. |
| B7 | 2 | Global `tabular-nums` on times, keys, mono counts and status; `text-wrap: balance` on headings and `pretty` on prose; one icon family (phosphor removed, oxlint blocks other icon packages); no em dashes in UI copy (grep); quieter focus composer; reading measure 55 to 90 characters (`reading.spec`). Relative dates were not audited screen by screen. |
| B8 | 2 | `offline-banner.spec`, `ws-reconnect.spec`, `route-error.spec`. |
| B9 | 2 | Press feedback on buttons for fine pointers (`base.css`), popovers grow from their trigger; `delight.spec` touch: "a swipe right archives the row, colour shows it first, and undo brings it back", "a short drag does nothing". No manual pass on a real phone yet. |
| C1 | 2 | `reader-context.spec` "the privacy line names what was blocked and from whom"; `reader.spec` blocks remote images until `M`; the blocked-image placeholder is a small labelled chip (screenshot). |
| C2 | 2 | `reader-context.spec` "with no model there is no AI slot"; the gist's ask is a verified quote from the message. Draft assist's sources were not checked for this score. |
| C3 | 2 | Copy review: counts are facts ("95 messages"), never nags; empty states are calm ("Low tide. Nobody's waiting on you."); no streaks or points anywhere. |

### v2 criteria (proposed 2026-09-30, to be graded independently)

Proposed by the builder of `feat/glance`; under E1 these are proposals until
an independent reviewer grades them.

| # | Proposed | Evidence |
|---|---|---|
| A11 | 2 | `row-gists.spec`: desk rows reserve the gist line, "Asks: …" replaces the reason and the gist lands under it, no row moves (bounding boxes equal), the top visible row is asked for first, hover names "Local model …"; inbox rows swap the snippet for the gist in the same box; with no model, rows are unchanged and the lists stop asking. `large-list.spec` "5000 rows with gist lines stay under the same budget": 20,712 list blocks per step (20,895 without gists, gate 60,000). Key-to-paint p95 with gist lines on every row: desk j/k/e 17.2/17.0/17.1 ms, inbox 17.2/17.6/17.4 ms (budget 50). `reader-context.spec`: a short conversation shows no gist but marks the ask; a long one shows it without moving the messages. Daemon tests cover cache without a model call, order, dedup, bounded writers, the event, people only and the privacy gate. Not 3: no dogfooding log (E2), and a local model fills a screen slowly (gemma4 on Ollama: about 7 to 9 s for the first line, then 2.5 to 3.2 s each, one at a time). |
| A12 | 2 | `apps/web/src/lib/actions/keymapParity.test.ts` compares the web action registry with the TUI keymap that `crates/tui/src/runner/tests/keymap.rs` writes to `docs/reference/tui-keymap.json`, and fails on a verb bound to different keys unless it is listed in `KEYMAP_DIFFERENCES` with a reason (merged in #252). Not 3: nobody has yet used both clients for a week and reported the keys carried over (E2). |
| D1 | 2 | 0.27 s end to end for `mxr desk` on 2026-09-28 (v0.6.36), mailbox of about 110,000 messages. Needs a re-measure on the current release. |
| D2 | 1 | 1.0 s warm for `mxr owed` on 2026-09-28 (v0.6.36), same mailbox: at the budget, not under it (6.8 s cold). Proposed 2 (builder's proposal, grader decides): on 2026-09-30 `ListOwedReplies` ranks in SQL over `idx_messages_owed` and takes 0.19 s warm in process (0.23 to 0.34 s under a load average of 21), down from 0.81 to 1.20 s, on a copy of the same store (110,075 messages, 88,839 candidate threads on the main account). The rows are identical to the old ranking. `owed_replies::scale_tests` guards the plan and records timing. Needs `time mxr owed --format json` on the real install once released. |
| D3 | 2 | Paper trail 34 ms, Reading 36 ms warm on 2026-09-28 (v0.6.38), same mailbox; the first cold call after a restart took 6 s. |
| D4 | 0 | Only measured on a synthetic 5,000-row inbox, not on the real one. |
| D5 | 2 | 2026-09-30, 109,894 messages, read-only: the gist batch's cache lookups for the 40 newest conversations take 0.5 to 2 ms, and for the 40 largest (1,705 messages) 7 to 12 ms; the briefing lookup uses its unique index. `feat/glance` adds no new SQL. Earlier rungs' timings are in the delivery ledger. |
| E1 | 0 | Every score in this document so far was proposed by the worker who built it. |
| E2 | 0 | No dogfooding log yet. Until one exists, the 3s in section A above are v1 scores; under v2 they stand at 2. |

### Independent grade (2026-09-30)

Graded read-only by Codex (`gpt-6-sol`, which built none of this) at
`d77374a5` (the glance branch, merged as v0.6.42). It read the source and the
test assertions rather than test names, and ran nothing, so the builders'
reported test runs and real-mailbox timings stay unverified by the grader.
Scores of 2 or more need evidence the grader checked in the repo; evidence
that is only a test name, a comment or a doc claim caps a score at 1; E2
caps section A at 2.

**Verdict under v2: fail.** 11 of 31 criteria are below 2, and every A
criterion is capped at 2 because there is no dogfooding log.

| # | Builder | Grade | Why the grade differs |
|---|---:|---:|---|
| A1–A4, A6–A10 | 3 | 2 | E2 cap. Also: A2 has no 30-newsletter fixture, A3 doesn't assert future mail follows a correction, A9's `controls.spec` filters message-card controls before comparing. |
| A5 | 3 | 1 | Reply later is an untimed flag, so there's no timed, one-key reply later or waiting-on flow. |
| A11, A12 | 2 | 2 | Supported. Browser gists are stubbed; real-model accuracy and latency are open. |
| B1 | 3 | 2 | No budget for `s` or cached-thread open; the key probe records the next paint without asserting the intended change. |
| B2 | 3 | 2 | `transition-colors` in `Sidebar.tsx` and a literal 1500 ms duration in `app.css` break the transform/opacity and token rules. |
| B3 | 3 | 2 | The per-verb undo journey misses send and unsubscribe. |
| B4, B6, B9 | 2 | 2 | Supported for what is tested. |
| B5 | 3 | 2 | The end-to-end journey covers snooze only. |
| B7 | 2 | 1 | No recorded visual review; `reading.spec` accepts 55–90 characters against the rubric's 60–80. |
| B8 | 2 | 1 | Nothing asserts cached mail stays usable while the daemon is stopped. |
| C1, C2 | 2 | 1 | Draft assist shows no local/cloud or history provenance and no draft sources. |
| C3 | 2 | 1 | No independent copy review. |
| D1, D3 | 2 | 2 | Accepted as dated, sized records; unverified by the grader. |
| D2 | 1 | 1 | 1.0 s is at the budget, not under it. |
| D4 | 0 | 0 | No real-mailbox scroll measurement. |
| D5 | 2 | 1 | The delivery ledger the timings cite isn't in the repo. |
| E1 | 0 | 1 | This section is the independent grade. |
| E2 | 0 | 0 | No log. |

What to do next, ordered by impact, is tracked in
[docs/issues/experience-rubric-gaps.md](issues/experience-rubric-gaps.md).
The grader also proposed four criteria for v3: how quickly gists become
useful on a real mailbox and how accurate their asks are; cross-client
convergence (a change in CLI, TUI or web shows everywhere without a
refresh); reminder reliability across restarts, offline periods and clock
changes; and partial batch recovery.

The parity floor in `docs/web-app-rubric.md` still holds: the suites it cites
(`triage`, `reading`, `reader`, `search`, `labels`, `snooze`, `responsive`,
`accessibility`, `keyboard-navigation`, `large-list`) passed in the same run.

## Evidence

Triage and deferral:
- Microsoft Research, email triage (CHIIR 2019):
  https://www.microsoft.com/en-us/research/wp-content/uploads/2019/02/Email_Triage_CHIIR19.pdf
- Whittaker and Sidner, email overload: https://dl.acm.org/doi/10.1145/238386.238530
- Paul Graham on the inbox as a todo list: https://paulgraham.com/ambitious.html

Stress:
- Kushlev and Dunn, checking email less:
  https://dunn.psych.ubc.ca/wp-content/uploads/2010/11/kushlev-dunn-email-and-stress-in-press1.pdf
- Gloria Mark, CHI 2012: https://ics.uci.edu/~gmark/Home_page/Publications_files/CHI%202012.pdf

Search beats filing: https://dl.acm.org/doi/10.1145/1978942.1979457

Products that broke conventions:
- HEY: https://www.hey.com/how-it-works/
- Google Inbox: https://en.wikipedia.org/wiki/Inbox_by_Gmail
- Superhuman speed: https://blog.superhuman.com/superhuman-is-built-for-speed/
- Superhuman on game design: https://blog.superhuman.com/game-design-not-gamification/
- Why Superhuman users stay: https://review.firstround.com/how-superhuman-built-an-engine-to-find-product-market-fit/

What people drop:
- HEY Screener fatigue:
  https://medium.com/@jonathan.goyvaerts/hey-com-a-review-and-why-i-wont-be-switching-8de2ee6b311e
- Apple Mail categories: https://discussions.apple.com/thread/255889425

Motion:
- Emil Kowalski: https://emilkowal.ski/ui/you-dont-need-animations and
  https://github.com/emilkowalski/skills/blob/main/skills/review-animations/STANDARDS.md
- Rauno Freiberg: https://rauno.me/craft/interaction-design
- Apple HIG: https://developer.apple.com/design/human-interface-guidelines/motion

Feedback and undo:
- Aza Raskin, "Never use a warning when you mean undo": https://alistapart.com/article/neveruseawarning/

Sound:
- Material sound guidance: https://m2.material.io/design/sound/applying-sound-to-ui.html
- Asana celebrations: https://zapier.com/blog/asana-celebrations/

Natural language:
- Todoist dates: https://www.todoist.com/help/articles/introduction-to-dates-and-time-q7VobO
- chrono: https://github.com/wanasit/chrono

Response time: https://www.nngroup.com/articles/response-times-3-important-limits/
