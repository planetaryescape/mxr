# Undoing a Done in Messages once showed "Couldn't find this person"

Status: fixed 2026-10-29 (fix/messages-undo-done-person). Found 2026-10-28 while rewriting the Messages e2e journey (branch `test/e2e-high-signal-only`).

## What happened

One local Chromium run marked a conversation Done in Messages, then pressed `u`. The daemon restored the conversation, and the toast said "Restored: Reply acceptance". But the page showed "Couldn't find this person" where the restored conversation should have been.

The same journey passed in the three other full runs and in 10 repeated runs (`--repeat-each=5` for that test and the edited triage test).

## Cause

The web page sent the neighbouring person's row id as the topic. Messages handed its `advanceTo(id, topicId?)` straight to `useAdvanceOnRemoval` as `onAdvance`. That hook calls `onAdvance(to, from)`, where the second argument is the row it moved off. When undo brings back a row that Done took out of the list, the hook reopens it with `onAdvance(back.from, back.to)`. So `advanceTo` opened the restored person with the neighbour's row id as `topic`. The bridge rejected `topic=person:noor@tidewater.example` with a 400 (`UUID parsing failed`), and `explainError` turned that lookup failure into "Couldn't find this person". The types didn't catch it because a function with fewer parameters is assignable to the hook's two-argument callback.

It only showed up sometimes because of a race between two list refreshes:

- Done's own list refresh lands before the undo. The person has left the list, so the hook remembers the move, and undo's refresh triggers the broken reopen. The "Restored: …" toast follows because the page is no longer where Done left it.
- Undo's refresh cancels Done's refresh before it lands. The person never left the list, the hook does nothing, and Done's own `onUndone` reopens the right topic.

Evidence: with logging of the person fetches, list fetches, selections and the undo path, plus a watcher for the not-found copy, `messages.spec.ts --repeat-each=30` showed the not-found page in 10 of 30 runs at 2fc4b7a8c. Each failure had the same order: `list ok` without the person, then `undo response`, then `list ok` with the person, then `select <person> topic=person:noor@…` from `useAdvanceOnRemoval`, then `person ERR … 400`.

## Fix

`SelectionAdvance` in `apps/web/src/features/messages/MessagesRoute.tsx` now passes only the target row on (`onAdvance: (id) => onAdvance(id)`). The restored person opens on their own default topic. After `onUndone` clears the settled thread, the default-topic effect names that thread in the URL. Done's undo handler now counts "already back on that person" as staying, so both outcomes of the race end on the done topic with the same "Undone" toast. The losing order no longer adds a "Restored: … Open" toast for the person already on screen. The other `useAdvanceOnRemoval` callers (Inbox reader, places, To do, Now) already took the arguments by name.

The new Vitest test in `MessagesRoute.test.tsx`, "a person Done took out of the list reopens on their own topic when undo brings them back", recreates the losing order and fails without the fix. It also checks that the undo shows "Undone" and no "Restored" toast. The bridge mock rejects a non-thread topic the way the real one does. With the fix, the instrumented e2e passed 30 of 30 runs, and the unmodified spec passed 20 of 20.
