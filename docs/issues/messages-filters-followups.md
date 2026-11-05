# Messages filters: follow-ups

## Filter change from /messages/$threadId may drop the list to the skeleton

Repro: mount `/messages/<thread>?turn=mine` with the list and the person page
cached, click Unread. The Messages body can fall back to the loading skeleton
while the new filter's query is pending, so the person page unmounts and comes
back when the answer lands.

Invariant: a filter change within one account keeps the previous answer on
screen (`placeholderData` in `apps/web/src/features/messages/api.ts`). The
placeholder only receives the previous query when the observer survives the
change.

Status: not reproduced in the app's route shape. Probe at `d574d0e29`, with
`/messages/$threadId` nested under `/messages` as in `apps/web/src/routeTree.gen.ts`:
the skeleton never shows, the person page stays mounted, and the path becomes
`/messages` (`MessagesRoute.test.tsx`, nested probe, sampled every 5 ms for 100 ms
after the click). The existing test mounts `/messages` and `/messages/$threadId`
as sibling routes (`mountMessages`), so `MessagesRoute` remounts there and the
new observer has no previous query. That remount explains the flash in the test
harness, not in the app.

Left open: confirm in a browser on the real path route before changing code.
If the flash reproduces there, the cause is the observer lifecycle in the route
tree, not `placeholderData`. Making the test harness nest the routes is a
follow-up; it does not fix the app.

Not a regression on the query-string route, which keeps its page.
