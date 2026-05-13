# Phase 4 — Search + Saved Searches

Goal: search is navigation. Two entry points (top-bar input and `Cmd-K`) with shared state. Live debounced dropdown, full results page with token chips, lexical/semantic/hybrid toggle, save-search side panel pinning lenses to the sidebar.

## Deliverables

1. **Top-bar search input** that morphs into the command palette on `Cmd-K`/`:`.
2. `/` focuses the top-bar input.
3. **Live dropdown** under input (debounced 120 ms): top 5 messages, top 3 threads, top 2 contacts.
4. **Results page** at `/search?q=...&mode=lexical|semantic|hybrid&account=&sort=`.
5. **Token chips** above results: `from:alice`, `has:attachment`, `older_than:7d`, etc. parsed from query and removable.
6. **Sort toggle**: relevance / newest / oldest.
7. **Save-search side panel**: name, color, pin to sidebar checkbox.
8. **Sidebar Lenses section** lists saved searches with live counts.
9. `/m/saved/$slug` route opens a saved search.
10. **Inline syntax help** (popover from `?` button) listing operators.

## Bridge endpoints used

- `GET /api/v1/mail/search?q=&mode=&account=&sort=&limit=`
- `GET /api/v1/platform/saved-searches` — list.
- `POST /api/v1/platform/saved-searches/create` { name, query, search_mode }.
- `POST /api/v1/platform/saved-searches/delete` { name }.
- `POST /api/v1/platform/saved-searches/run` { name, limit }.

## Files

```
src/features/search/
  SearchInput.tsx                  # the top-bar input + dropdown
  SearchResultsRoute.tsx           # /search page
  SearchTokenChips.tsx             # parse query + render chips
  SearchSyntaxPopover.tsx          # ? help popover
  SearchModeToggle.tsx             # lex / sem / hybrid
  SearchSortToggle.tsx
  SaveSearchSidePanel.tsx          # right-rail form
  useSearchQuery.ts
  useSavedSearches.ts
src/lib/
  searchSyntax.ts                  # parser: query string → tokens
src/features/sidebar/
  LensesSection.tsx                # update with saved-searches integration
```

## Query parsing

Approximate Gmail-style operators: `from:`, `to:`, `cc:`, `subject:`, `label:`, `has:attachment`, `is:unread`, `is:starred`, `older_than:7d`, `newer_than:1d`, `before:`, `after:`. Free-text words are matched against body+subject.

The bridge already parses these; we don't need to be authoritative locally — we just **render** the parsed tokens for visual feedback. To do that, we either:
- Re-parse with a small regex parser locally (good enough), or
- Call a bridge `parse` endpoint if one exists.

For v1 ship the local regex parser; if it diverges from server in edge cases, that's acceptable since the server is authoritative on the result set.

## Saved searches → sidebar

Saved searches with `pin: true` show in the sidebar Lenses section. Live counts come from a periodic `GET /api/v1/platform/saved-searches` (every 60 s) plus invalidation on `LabelCountsUpdated` / `MailUpdated` WS events.

## Verification

1. Press `/` → top-bar focuses, type `from:alice` → dropdown shows live results.
2. Hit Enter → URL becomes `/search?q=from%3Aalice` and full results page renders.
3. Click `from:alice` chip's × → URL updates to `/search?q=`, results refresh.
4. Toggle Semantic → mode=semantic in URL, fetch repeats, results re-rendered.
5. Open right rail "Save this search", name "Alice's mail", check pin → sidebar shows new lens.
6. Click sidebar lens → opens `/m/saved/alices-mail`, results render the saved query.
7. Reload → saved search persists.

## Decisions

- 2026-05-10 — Local syntax parser; ship a 50-line regex, accept divergence in edge cases.
- 2026-05-10 — Live dropdown shares state with command palette via Zustand `searchStore`. `/` and `Cmd-K` both populate this store; the visible UI is whichever is mounted.
