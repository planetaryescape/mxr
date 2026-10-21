# Web app search ignores `[search]` config defaults

Status: open. Found during G04 (search honours its configured defaults).

The daemon and the web `/search` HTTP route honour `[search] default_sort` and
`max_results` when a request leaves `sort` or `limit` out. The bundled web app
never leaves them out: `apps/web/src/features/search/SearchResultsRoute.tsx`
defaults the sort control to `"relevance"` and always sends
`limit: SEARCH_PAGE_LIMIT` (100).

Fixing it needs a decision on what the sort control shows when the user hasn't
picked one: probably "Default (<configured order>)", sending no `sort` until the
user changes it, and omitting `limit` so the daemon's page size applies.
