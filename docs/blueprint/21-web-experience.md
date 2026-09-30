# 21. Web experience: the desk, not the pile

The web app's product model, what shipped between v0.6.34 and v0.6.42, and
the work still needed to pass the experience rubric. The rubric
(`docs/web-app-experience-rubric.md`) is the scoring source of truth. This
document says what to build and in what order. Settled choices are logged
as D095 to D102 in [15-decision-log.md](15-decision-log.md). Don't
re-debate them without new evidence.

## The model

mxr's advantage over hosted clients is local history: it already knows who
you owe, what you promised, who has gone quiet, how you write to each
person, and which senders are bulk mail. The web app leads with that
knowledge instead of arrival order.

| Convention | mxr instead | Shipped in |
|---|---|---|
| Inbox by arrival as home | **The desk**: You owe, Due, Waiting on, New from people, each row saying why | v0.6.36 |
| Unread counts | Badges count only work (desk, reply queue, screener) | v0.6.36 |
| One pile, or opaque categories | **Reading** and **Paper trail**, each message saying why it's there, with a one-key correction remembered per sender | v0.6.38 |
| Processing bulk mail one by one | Pin the exceptions, **sweep** the rest, with a dry-run preview and undo | v0.6.38 |
| Mark unread to remember | Reply later, snooze and remind-if-no-reply, each taking natural-language time | v0.6.34, v0.6.37 |
| Reply buttons above the thread | The reply field sits where reading ends | v0.6.35 |
| Raw thread, work it out yourself | Context first: the ask highlighted in the message, promises both ways, how you know the person | v0.6.35 |
| Summaries after you've opened the mail | **Gists and asks in the list**, so you decide before opening | v0.6.42 |
| Promises live in your head | Promises detected on send and offered as reminders | v0.6.37 |
| Replying scattered through the day | **Focus & reply**: one sitting through everyone you owe | v0.6.37 |
| Archive as the only way to put mail away | **Done**: archived, read, and gone until someone writes again | v0.6.40 |
| Feel as an afterthought | Low tide, optional sound, key hints, touch swipe, held keys that never repeat destructive actions | v0.6.39 |
| Web and TUI keys drifting apart | One key vocabulary, enforced by a parity test | v0.6.41 |

The guide that walks a user through this is
`site/src/content/docs/guides/your-day.md`.

## Standards every change keeps

These came from bugs found during delivery. Each one has a test or a gate
behind it.

- **Real scale.** Time every new query read-only against a real, large
  mailbox (`sqlite3 'file:…/mxr.db?mode=ro'`, counts and timings only).
  Demo datasets hid a 120-second `mxr owed` timeout on a 110k-message store.
- **Never re-render the list.** Nothing app-wide may subscribe to state
  that changes on scroll or per keystroke. `useVirtualizer` callbacks must
  be stable. `e2e/large-list.spec.ts` counts V8 coverage blocks per scroll
  step (gate 60,000).
- **Keys.** Keyboard-repeated actions never animate, and held keys never
  repeat destructive actions (`REPEATABLE_ACTION_IDS`). Web and TUI keys
  match (`keymapParity.test.ts`).
- **Previews match mutations.** A sweep commits only what its preview
  listed (a preview token), and every chunk rechecks pins and place
  membership under a per-account gate.
- **AI never blocks reading or triage.** Model text shown to a user is
  either a verified quote from the message or labelled as a summary with
  its provenance. A gist is announced only if no message arrived while it
  was written.
- **Verify on the merge commit.** Read its check-runs, not only the PR's.

## Plan: pass rubric v2

An independent grade on 2026-09-30 failed v2: 11 of 31 criteria scored
below 2. Section A is capped at 2 until a dogfooding log exists. The detail
per criterion is in the rubric's "Independent grade" section. The work
below is ordered by impact on the user. Each item names the criteria it
moves and the check that proves it.

### 1. Show where AI drafts come from (C1, C2)

Generated drafts say which model wrote them (local or cloud), whether the
person's history was used, and which past emails shaped the voice, with
each source inspectable.

- **Check:** a browser journey drafts a reply and asserts the provenance
  line and a source list that opens the cited message.
- **Keep:** the privacy gate (`relationship_data_allowed`) and `pin()`
  decide what reaches a cloud model. The disclosure reports what actually
  happened, not what was configured.

### 2. Timed deferral (A5)

Reply later and waiting-on take a time ("tue 9", "in 3d") through the
shared natural-time parser, and come back on their own. Waiting-on returns
the thread when nobody has replied by then.

- **Check:** an e2e sets reply later for "in 2d" from one key plus a typed
  time. A clock-injected daemon test brings it back. Waiting-on with no
  reply resurfaces exactly once.
- **Keep:** clients send the resolved instant, never the phrase.

### 3. `mxr owed` under one second (D2)

`mxr owed` runs in 1.0 s warm on the real mailbox, which is at the budget,
not under it. The owed list is also noisy: 88k candidates on the main
account, because it isn't filtered to people. Push the desk's
people/inbox/recency filters into the owed SQL (pending BK's decision on
whether the Owed page adopts the desk's people filter).

- **Check:** `time mxr owed --format json`, second run, under 1 s on the
  real mailbox, recorded in the rubric with date and mailbox size.

### 4. Real-mailbox scrolling (D4)

Measure scrolling and key-to-paint in the web app against the real bridge,
with gist lines on, instead of a synthetic 5,000-row list.

- **Check:** run the `large-list` and `speed` probes against the real
  install (read-only session), and record blocks per scroll step and p95 in
  the rubric.

### 5. Wider speed gate (B1)

Add `s` (star) and opening a cached thread to `speed.spec`. The probe must
assert that the intended change happened before it accepts a timing
sample.

- **Check:** the new cases fail if their state change doesn't paint.

### 6. Real-model gists (A11)

Browser journeys stub the model today. Measure time to useful coverage (the
share of visible people rows with a gist after 30 s and after 120 s) and
ask accuracy against the source messages, on representative conversations
with the user's configured local model.

- **Check:** an ignored benchmark test with recorded results. An accuracy
  sample reviewed by a person, logged in `docs/`.

### 7. Usable while the daemon is stopped (B8)

Stop the daemon mid-session. Cached lists and threads stay readable, keys
still move, mutations queue or explain themselves, and recovery is
automatic.

- **Check:** an e2e that stops the daemon through the e2e control port,
  asserts reading and navigation, then restarts it and asserts recovery.

### 8. Complete undo matrix (B3)

Cover send (inside the undo window) and unsubscribe in the per-verb undo
journey, and document what is truly irreversible.

- **Check:** `verbs.spec` loops every verb in `verbFeedback.ts` and fails
  if one has no journey or no documented irreversibility.

### 9. Motion and typography audit (B2, B7)

Remove `transition-colors` in `Sidebar.tsx` and the literal 1500 ms
duration in `app.css`, since only motion tokens and transform/opacity are
allowed. Tighten `reading.spec` to a 60–80 character measure. Audit
relative dates so the list, reader, desk and places read the same way for
the same age. Record a 1440 and 390 px review.

- **Check:** a lint or grep rule for non-token durations and colour
  transitions, the tightened `reading.spec`, and a dated review note.

### 10. Dogfooding log (E2)

BK uses the web app for five working days and logs what worked, what
didn't, and when he reached for another client. Only this can lift section
A to 3.

- **Check:** a dated log in `docs/` covering at least five working days.

### Needs a person, not code

- **Sound (B4):** listen on real speakers: volume curve and the low tide
  figure.
- **Touch (B9):** swipe thresholds and vertical scrolling on a real phone.
- **Copy (C3):** an independent copy review across the app.

### Open decisions (BK)

- Should a Done item stay gone even after someone replies? Today it comes
  back when anyone writes in the conversation.
- Should `mxr owed` and the Owed page use the desk's people filter?
- Should a sender moved to People land in New from people rather than You
  owe?

### Candidate criteria for rubric v3

Proposed by the independent grader. Adopt them only after the v2 plan
lands.

- **Gist usefulness:** time to useful coverage on a real mailbox, and ask
  accuracy.
- **Cross-client convergence:** a change in CLI, TUI or web shows in every
  open client without a refresh.
- **Reminder reliability:** snoozes, due promises and no-reply reminders
  return exactly once across restarts, offline periods and clock changes.
- **Partial batch recovery:** a bulk action that fails partway reports
  exactly what changed and can restore only that.

## Out of scope

- Automatic LLM classification of mail kinds. Placement stays rule-based
  and explainable (D097).
- Points, streaks or scolding copy (D101).
- Opening a web-only capability that the CLI and TUI can't reach. Daemon
  IPC and CLI JSON come first, then the clients.

## Verification

- The rubric's Scores and "Independent grade" sections are updated after
  each item above, by a grader who didn't build the item (E1).
- `docs/issues/experience-rubric-gaps.md` mirrors this plan and is trimmed
  as items land.
