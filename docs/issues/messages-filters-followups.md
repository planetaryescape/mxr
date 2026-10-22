# Messages filters: follow-ups

## Filter change from /messages/$threadId drops the list to the skeleton

Repro (covered by a test, which waits for the answer rather than asserting
the mid-refetch state): mount `/messages/<thread>?turn=mine` with the list
and the person page cached, click Unread. The Messages body falls back to the
loading skeleton while the new filter's query is pending, so the person page
unmounts for that moment and comes back when the answer lands.

Invariant: a filter change within one account keeps the previous answer on
screen (`placeholderData` in `apps/web/src/features/messages/api.ts`). That
holds on `/messages?...` in the test suite, but not on the path route, where
the observer's previous query was not passed to the placeholder function in
the test run. Cause not yet traced; the first check is whether the path
route's observer is recreated on the search change.

Not a regression on the query-string route, which keeps its page.
