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
| Compose | Top bar | Frequency, and the only way in for someone who doesn't know `c`. |
| Sync now | Status bar | Discoverability: shows sync state and progress; the one manual sync. |
| all keys (`?`) | Status bar | Discoverability: the way into every key. |
| Key hint strip | Status bar | Discoverability: the current view's five most useful keys (not buttons). |

The sidebar holds places (Desk, Inbox, Reply queue, Waiting on, Snoozed,
Reading, Paper trail, Screener) with folded More, Labels and Tools groups.

## Lists (inbox, labels, saved searches)

| Control | Reason |
|---|---|
| Conversations / single messages | Discoverability: shows which grouping is on, a mode people otherwise can't see. |

Row buttons (archive, trash, read, snooze, star) appear on hover only, and
the bulk bar only while rows are selected.

**Removed:** the list's Refresh button. The list follows daemon events on
its own, and Sync now (status bar, palette) covers a manual check.

## Desk

The heading's counts are links to each lane in full, and the "Everything
else" line links to the places. No buttons: the desk is read and acted on
with the same keys and row buttons as a list.

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
| Send and next, Skip, Snooze, Send and remind, Draft in your voice | Discoverability: focus mode is a mode with its own keys, each shown on its button. |

## Places

| Control | Reason |
|---|---|
| Sweep all (Reading, Paper trail) | Frequency: sweeping is what a place is for. Shows S. |

Per-sender Sweep, Move sender and Unsubscribe show under an open bundle,
next to what they act on.
