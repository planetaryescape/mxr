# Some web selections weren't in the URL

Resolved. A copied link, or a reload, now lands where you were. Each route declares its selection in `validateSearch` (TanStack Router) and the page rewrites it with `replace: true` through `useSearchParam`/`useReplaceSearch` (`apps/web/src/hooks/useSearchParam.ts`), so moving a cursor adds no history. `PlaceLayout` takes a `keepSearch` list so a list's own params stay while an email opens beside it and come back when it closes; the reader's own params (To do's `link`, `mid`) don't carry to the next email.

| Where | In the URL now |
|---|---|
| To do | `row` (cursor), `source` (unfolded source); a row in a folded band opens the band |
| Updates | `row`, `peek`; a routine line opens Routine |
| Reading | `row`; a digest's link unfolds its digest. The reader's back and `n` keep it |
| Reading reader | `as=issue\|article`; `article` on an item without one is ignored, and a link never fetches an article by itself |
| Now | `row`. A to-do opens To do with `row` on it, the Updates card opens on its first line, and the Reading pick opens its item in the reader (`o` still opens the email) |
| Archive | `record`, `section=subscriptions`, `sub`, `q` (written once typing settles), and the filter as `year`, `issuer`, `kinds`, `checked`, `pdf`, `group`, `min`, `max`. A link's record wins over the best match of the link's own question |
| Messages | Opening a person writes their default `topic`, so the link keeps that topic after the default moves. Undo after landing on a person still goes back |
| `/messages/<thread>` | A conversation Messages doesn't hold redirects to `/m/archive/<thread>` (All Mail) |
| Focus & reply | `at`, the conversation on screen; a link puts it first when it is still queued |
