# Some web selections aren't in the URL

Open. Opening an email puts it in the URL everywhere it can be opened, so a copied link reopens it: the mailbox, label, category and saved lists (`/m/<mailbox>/<thread>`), Search, To do, Updates, Reading, Archive, the desk, Owed, the reply queue and Snoozed. `/m/archive/<thread>` (All Mail) opens any email wherever it now is, which is what the assistant's links use. The router (TanStack Router) keeps search params typed through `validateSearch`, so no extra library is needed for the rest.

What a copied link loses, found in a code audit of `origin/main` at 858958976 (paths under `apps/web/src/`):

- **Highlighted rows.** The row cursor in To do (`features/todo/TodoRoute.tsx`, `cursorId`/`expandedId`), Updates (`features/updates/UpdatesRoute.tsx`, `cursorId`/`peekId`), Reading's edition (`features/reading/ReadingRoute.tsx`, `cursorKey`) and Now (`features/now/NowRoute.tsx`, `cursorKey`) lives in component state.
- **Archive's ledger.** The open record, section, filter, query and subscription are component state (`features/archive/ArchiveRoute.tsx`).
- **A person's topic in Messages.** Clicking a person gives `?person=` only; the topic joins the URL after a topic click (`features/messages/MessagesRoute.tsx`). A pasted link shows whichever topic is the default by then.
- **`/messages/<thread>`.** Opens nothing when the thread isn't in Messages' data, such as an archived email.
- **Now's cards.** The Updates card goes to bare `/updates`, and the Reading card opens the email rather than the reader item (`features/now/nowItems.ts`, `itemPath`).
- **Focus mode** keeps its current item in a reducer (`features/focus/useFocusSession.ts`), and the Reading reader's Issue/Article toggle is state (`features/reading/ReadingReader.tsx`).

The fix for each is the same move: put the selection in the route's `validateSearch` and update it with `replace: true`, as Messages does for `person`.
