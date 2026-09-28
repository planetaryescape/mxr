# Web app experience rubric

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
B3 (speed, motion and feedback) must reach 3. A criterion scores 2 only with a
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

## Scores

Scored on 2026-09-28 on `feat/delight` (rung 6, on top of v0.6.38's
places), from the Playwright suite against the FakeProvider daemon (119
tests; 118 passed in one full run under a machine load of 110 to 330, and
the one failure, `places.spec` "Paper trail: pin one, sweep…", passed on a
rerun: its sweep job outlasted a 5 s toast wait) and screenshots at 1440 and
390 px in the dark and light themes. A score of 2 needs a journey in the
suite; 3 is reserved for what a keyboard user would notice and prefer.

**Pass bar: not met.** Every criterion is at 2 or better, but A8, A9, A10,
B1 and B3 are at 2 where the bar asks for 3. Each has a concrete follow-up in
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
| A8 | 2 | `focus.spec` "a dated promise in a reply is offered with its time and kept as a reminder". Promises made *to* you are only covered by `contextFormat.test.ts`, not a journey. |
| A9 | 2 | The reply toolbar is gone and reply sits at the end of the thread (`reader-context.spec` "r opens the reply at the end of the thread"); the sidebar holds places. There is no inventory of persistent controls with a stated reason for each. |
| A10 | 2 | Desk rows say "22h · usually 47m" (screenshots; `deskCopy.test.ts`), but no journey asserts the age and cadence text. |
| B1 | 2 | Keydown to paint p95 on the desk: j 16.1, k 16.4, e 15.7 ms; inbox: j 15.8, k 16.1, e 15.6 ms (`speed.spec`, budget 50). 5,000 rows: 20,435 list coverage blocks per scroll step (`large-list.spec`, gate 60,000). The 300 ms no-spinner and 400 ms minimum-skeleton rule is not implemented. |
| B2 | 3 | Motion tokens only (no ad hoc `duration-*` or `ease-in` in `src`); `natural-time.spec` reduced and full motion, rows never animate the cursor; `delight.spec` "with reduced motion the low tide is a still frame". |
| B3 | 2 | Undo for archive, trash, snooze, labels, move, sweep, send (`mutations.spec`, `triage.spec`, `labels.spec`, `snooze.spec`, `places.spec`, `focus.spec`). There is still no table in the code mapping every verb to its trigger, rule, feedback and undo. |
| B4 | 2 | `delight.spec` "clearing the desk by keyboard earns low tide once, not on a revisit" (motion under 2 s, then still); sound is off by default, previews in the browser, plays on archive and never on `j` ("sound: off by default…"); send plays once (`feedback.test.ts`). Not yet heard by a person on real speakers. |
| B5 | 3 | `natural-time.spec`: highlighted phrase, resolved time before commit, choices for "fri 3", and the daemon stores the previewed instant. |
| B6 | 2 | Shortcuts in tooltips, menus, the palette and `?` (`keyboard-help.spec`); `delight.spec` "three pointer archives earn one key hint, and it never comes back". Sidebar and palette clicks don't count toward hints. |
| B7 | 2 | Global `tabular-nums` on times, keys, mono counts and status; `text-wrap: balance` on headings and `pretty` on prose; one icon family (phosphor removed, oxlint blocks other icon packages); no em dashes in UI copy (grep); quieter focus composer; reading measure 55 to 90 characters (`reading.spec`). Relative dates were not audited screen by screen. |
| B8 | 2 | `offline-banner.spec`, `ws-reconnect.spec`, `route-error.spec`. |
| B9 | 2 | Press feedback on buttons for fine pointers (`base.css`), popovers grow from their trigger; `delight.spec` touch: "a swipe right archives the row, colour shows it first, and undo brings it back", "a short drag does nothing". No manual pass on a real phone yet. |
| C1 | 2 | `reader-context.spec` "the privacy line names what was blocked and from whom"; `reader.spec` blocks remote images until `M`; the blocked-image placeholder is a small labelled chip (screenshot). |
| C2 | 2 | `reader-context.spec` "with no model there is no AI slot"; the gist's ask is a verified quote from the message. Draft assist's sources were not checked for this score. |
| C3 | 2 | Copy review: counts are facts ("95 messages"), never nags; empty states are calm ("Low tide. Nobody's waiting on you."); no streaks or points anywhere. |

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
