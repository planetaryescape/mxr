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
| Assistant | Top bar | Discoverability: the sidebar is closed until opened, and `⌘.` alone hides it from anyone who hasn't read the keys. Opens and closes the assistant. |
| Compose | Top bar | Frequency, and the only way in for someone who doesn't know `c`. |
| Sync now | Status bar | Discoverability: shows sync state and progress; the one manual sync. |
| Latest mail (freshness) | Status bar; topbar on a phone | Frequency: the quick "is my mail current?" glance from any page. Opens the last arrivals and where each went. A failing or stale sync adds a warning link to the account's sync details. While a row on Now or Messages is lit, the sync words give way to "Here because: <reason>" (and a new sender's "1 Messages · 2 Updates · 3 Reading · 4 Block"); a failing sync keeps its warning. |
| all keys (`?`) | Status bar | Discoverability: the way into every key. |
| Key hint strip | Status bar | Discoverability: the current view's five most useful keys (not buttons). |

The sidebar holds the rail (Now, Messages, To do, Updates, Reading, Archive,
Inbox) with folded More, Labels and Tools groups. Screener, Reply queue,
Waiting on, Snoozed and Subscriptions sit under More.

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
| Formatted / Reader / Plain | Discoverability: which view is on. |
| More | Everything else, each with its key. |
| Per message: star, reply to this message, more | Replying to an earlier message and starring one message need a target the thread-level keys don't give. |

Reply sits at the end of the thread (the reply field), where reading ends,
not in the toolbar.

## Focus & reply

| Control | Reason |
|---|---|
| Leave | Shows Esc; the way out of a mode. |
| Send and next, Skip, Done (no reply needed), Snooze, Send and remind, Draft in your voice | Discoverability: focus mode is a mode with its own keys, each shown on its button. |

## Reading and Updates

| Control | Where | Reason |
|---|---|---|
| Let go of all | Page header of Reading, and of Updates while it has a digest | Frequency: letting go of the lot is what the page is for. Previews first. Shows A. |

Updates rows hold no persistent buttons. A row's keys and More menu replace
its trailing value while the row is focused or hovered (always on touch), and
the Space peek holds the rest. Per-sender Sweep, Move sender and Unsubscribe
show next to what they act on.
