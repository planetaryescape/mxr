# Web app: persistent controls and why each earns its place

Criterion A9 of `docs/web-app-experience-rubric.md` asks every always-visible
control to be justified by **frequency** (used on most visits) or
**discoverability** (it shows a mode or a key that would otherwise be
hidden). Everything else lives in a More menu and the command palette
(⌘K), with its key.

`apps/web/e2e/controls.spec.ts` holds the same list and fails if a surface
gains or loses a control, so this page and the app can't drift apart.

## Shell

| Control | Where | Reason |
|---|---|---|
| Search mail | Top bar | Frequency: search is how people find mail. Shows `/`. |
| Assistant | Top bar | Discoverability: the sidebar is closed until opened, and `@` or `⌘.` alone hides it from anyone who hasn't read the keys. Opens and closes the assistant. |
| Compose | Top bar | Frequency, and the only way in for someone who doesn't know `c`. |
| Sync now | Status bar | Discoverability: shows sync state and progress; the one manual sync. |
| Latest mail (freshness) | Status bar; topbar on a phone | Frequency: the quick "is my mail current?" glance from any page. Opens the last arrivals and where each went. A failing or stale sync adds a warning link to the account's sync details. While a row on Now or Messages is lit, the sync words give way to "Here because: <reason>" (and a new sender's "1 Messages · 2 Updates · 3 Reading · 4 Block"); a failing sync keeps its warning. |
| keys (`?`) | Status bar | Discoverability: the way into every key. |
| Key strip | Status bar | Discoverability: the page's three to five main verbs (`usePageKeys`), or the registry's live keys on a page that names none. Not buttons. Whole verbs drop from the end when space is short, and the reason shortens first. Hidden under `(pointer: coarse)`. |

### The rail

The rail is dimmer than the page: labels in the second text tier, icons in the
third, the page you are on on the lamp. It holds Now, Messages, To do,
Updates, Reading, Archive and Inbox, then four folded groups: Mail (Starred,
Sent, Drafts, Snoozed, Reply later, All Mail, Spam, Trash), Views (Waiting on,
Owed replies, Screener, Subscriptions, Invites, Deliveries), Tools (Search,
Analytics, Rules, Accounts, Activity, Jobs, Diagnostics, Settings) and Labels,
each with a 6px square of its colour. Only Now, Messages and To do carry a
count (and the Screener, while senders wait). Its items are fixed: a stopped
daemon never adds, removes or moves one.

| Control | Where | Reason |
|---|---|---|
| Mail, Views, Tools, Labels (fold) | Rail | Discoverability: names the groups and keeps the rail short. |
| Collapse / expand sidebar | Rail footer | Frequency: a person who wants the room, once. The collapsed rail is the modes and Inbox only, each with its key in a tooltip. |
| More places | Collapsed rail footer | Discoverability: opens the same sheet as the phone's More, so Mail, Views, Tools and Labels stay reachable when the rail is narrow. |
| Theme | Rail footer | Frequency: the quick light/dark switch. |

### The phone

Five tabs: Now, Messages, To do, Reading and More. More opens a sheet (on the
overlay ink over a scrim) with Updates, Archive, Inbox and the rail's four
groups, so every destination is reachable. The lit tab always matches the page:
Updates, Archive, Inbox, a conversation in the inbox, every tool page and
Settings light More. The sheet traps focus, closes on Esc and on any
navigation, and the page's keys stay under it while it is open. The top bar's
search is the only search field.

## Page states

One empty, one loading and one error state serve every page (`PageEmpty`,
`PageSkeleton`, `PageError` in `components/PageParts.tsx`); none adds a control
beyond the one action it names.

| State | What it shows | The one action |
|---|---|---|
| Empty | Top-aligned in the content column: a heading for what isn't here, one line naming the next useful place ("Next: 3 in To do"). A mode may use its 24px glyph; no other icon. | The action that fills the page, if there is one (Check for mail). |
| Loading | The page's own `Row`s with placeholder bars, drawn only after 150 ms. | None. |
| Error | A heading in plain words, a line saying what still works, and the raw error under Details. | Try again (a retry can help), Copy start command (mxr isn't running), Copy update command (protocol mismatch), Open token settings (this browser isn't recognised), Go to Inbox (a bad or unknown id). Never Try again for a lookup that can't succeed. |
| Offline | Cached content stays. One banner under the top bar: "mxr isn't running. Showing what was loaded at 21:04." The rail keeps every item. | Copy start command (`mxr restart`). |

`e2e/daemon-stopped.spec.ts` and `e2e/route-error.spec.ts` hold these.

## Assistant panel

| Control | Where | Reason |
|---|---|---|
| Trust state ("asks first" or "trusted") | Panel header, beside the title | Discoverability: a mode that changes who is responsible must be visible. Its menu says what runs without asking and turns it on or off. |
| Past conversations | Panel header | Frequency: the way back to a conversation. Rows with `j`, `k`, `Enter`, `d`. |
| New conversation | Panel header | Frequency. |
| Close | Panel header | Discoverability: shows `⌘.`. |
| Stop | Live line, only while a turn runs | Frequency: ends the turn. |
| Apply (`y`), send back with a note (`n`), exact call (`x`) | A waiting proof | The one primary of the panel, with the two quiet ways out. The keys answer only while the proof has focus and never while typing. |
| Options (`1` to `6`), Answer, Skip | A waiting question | The question's answer and its way out. |

## Page header

Every page wears one frame (`PageFrame`, docs/blueprint/27): the title, a
one-line purpose, at most one primary button and a **⋯** menu, with tabs
under the title when the page has sections. Nothing else goes in the header.
Filters and pickers (the Screener's account picker, Analytics' time range)
sit at the top of the page's content.

| Control | Where | Reason |
|---|---|---|
| Primary action | Page header, at most one | Frequency: the page's main verb (New rule, Add account, Let go of all, Save changes, Leave in Focus). |
| More actions (⋯) | Page header | The page's other actions, each with its words, in one menu. |

## Lists (inbox, labels, saved searches)

| Control | Reason |
|---|---|
| More actions (⋯) | The list's only header control. It holds Conversations / single messages, which names the grouping that is on. Discoverability: a mode people otherwise can't see. It moved here from its own icon button in W1 (the page frame has one menu and no other controls). |

Row buttons (archive, trash, read, snooze, star) appear on hover only, and
the bulk bar only while rows are selected.

**Removed:** the list's Refresh button. The list follows daemon events on
its own, and Sync now (status bar, palette) covers a manual check.

## Desk

The heading's counts are links to each lane in full, and the "Everything
else" line links to the places. The heading has no buttons: the desk is read
and acted on with the same keys as a list.

| Control | Where | Reason |
|---|---|---|
| Done (check) | End of every row | Frequency: putting an item away is what the desk is for. Discoverability: it shows that one action (Done, `e`) exists in every lane. It shows on hover and on the cursor's row with a mouse, and always, quietly, on touch. |

## Reader

| Control | Reason |
|---|---|
| Close | Frequency: every thread is closed. Shows Esc. |
| Previous / next conversation | Frequency: moving through the queue. Shows N / n. |
| Archive | Frequency: the most common verb. Shows e. |
| Snooze | Deferral is a first-class verb (A5). Shows Z. |
| Formatted / Reader / Plain | Discoverability: which view is on. The shared segmented control. |
| More | Everything else, each with its key. |
| Per message: star, reply to this message, more | Replying to an earlier message and starring one message need a target the thread-level keys don't give. |

Reply sits at the end of the thread (the reply field), where reading ends,
not in the toolbar.

One toolbar style (W6): the conversation reader and the Reading reader use the
same quiet button (`ToolButton`, no border, secondary text, key in the tooltip).
Reading's toolbar sits in the reading column, starting where the title does:
Later, Let go and Highlight keep their words; Original, Marginalia and
Unsubscribe are icons named in their tooltip, so the bar fits one row. On
phones every label drops to an icon. The reading measure is 66ch in both.
The gist, the AI overview and AI drafts carry the model mark (`gemma4 · local`).

## Compose and search

| Control | Reason |
|---|---|
| Popout, fullscreen, close | The sheet's surfaces; popout and fullscreen sit over a scrim. |
| Rich text / Markdown | The shared segmented control; the vim mode line shows only in Markdown. |
| Search: Exact / Hybrid / Meaning | Discoverability: the mode that is on. |
| Search: Filters | Everything else (Show, Sort, Account, narrow-by chips), behind `g f`, so one result does not sit under a dozen controls. |

## Focus & reply

| Control | Reason |
|---|---|
| Leave | Shows Esc; the way out of a mode. |
| Send and next, Skip, Done (no reply needed), Snooze, Send and remind, Draft in your voice | Discoverability: focus mode is a mode with its own keys, each shown on its button. |

## To do, Updates, Reading, Archive

| Control | Where | Reason |
|---|---|---|
| Let go of all | Page header of Reading, and of Updates while it has a digest | Frequency: letting go of the lot is what the page is for. Previews first. Its key (A) is in the tooltip, not on the button. |
| Row checkbox (16px) | Start of every To do row | Frequency, and the only pointer way to tick off. `Space` does the same on the lit row. |
| Lit row's verbs | The trail of the lit row, and under a pointer hover | Discoverability: the keys that work on this row, as "key verb". They replace the time and marks on the same line, so no row changes height. To do: Space done, o email, v details. Updates: Space peek, e let go, t to-do, o email, a ⋯ menu. Reading: Enter read, b later, e let go, D unsubscribe when offered. Archive: o email, Enter document. |

**Removed in W3:** the key legends under To do, Updates, Reading, Archive and the
Inbox; every chip inside a button (To do's catch-up and expired lists, Reading's
Let go of all, Updates' Let go of all); the hover key hints on unlit Updates
and Reading rows; To do's bordered "Open email" button, its runway bar and its
provenance chip (the lit row's `o email`, the window in words and one
"unchecked" word replace them); Updates' headline and cut line (one count,
"31 updates from 12 sources"); Archive's round kind pills and its `[` `]` year
chips; Reading's card and its unsubscribe sentence. **Moved:** Archive's kind
filter is a segmented control; the Reading reader's toolbar is quiet buttons
with their keys in tooltips; Updates' more actions stay in a ⋯ menu on the lit
row. Per-sender Sweep, Move sender and Unsubscribe show next to what they act on.

## Toasts

D140: a toast confirms what happened and offers Undo; it is not a place for
decisions. Rules the app holds:

- One at a time. A new toast replaces the one before; the activity log keeps
  them all.
- Bottom right, above the status bar or the phone's tab bar; top right in
  Focus, where the composer sits at the bottom. When the lit row is under the
  toast's corner it takes the other corner, so it never covers the lit row.
- One line saying what happened ("Ticked off: Fix payment"), with an `Undo u`
  button when the change can be undone. No "Press u to undo" second line.
- Gone after 5 seconds, or 8 with Undo. Hovering or tabbing into it pauses the
  timer. `u` still undoes after the toast is gone, for as long as the undo
  lives.
- An error that needs a decision is a message in the page, not a toast.
