# `is:owed-reply` searches are filtered after paging, so `has_more` and paging are wrong

**Status:** open · **Found:** verifier review of the bulk `--search` cap notice
(faf22f43d) · **Severity:** should-fix

## What happens

`execute_search_indexed` (`crates/daemon/src/handler/diagnostics/search_execute.rs`,
around L138-L200) runs the query and pages it, then applies two filters to the
page it got back:

- `filter_to_cached_mail_accounts` (~L308) drops results from accounts that are
  not visible.
- `filter_to_owed_threads` (~L236-L299), only for `is:owed-reply`, keeps results
  whose thread is owed a reply right now. Owed-reply depends on the current
  time and cadence, so it is computed per request and is not in the index.

Both run after the page of `limit` results was cut, not before. The page holds
the first `limit` candidates; the owed filter then shrinks it to however many of
those happen to be owed. Neither filter refills the page, and neither updates
`has_more` or `next_offset`, except that both reset them when the page ends up
empty.

## Effect

- Pages come back uneven: a page of `limit` candidates can hold a few owed
  threads, or none.
- A page whose candidates are all filtered out sets `has_more: false` and clears
  `next_offset` (the empty-page reset at the end of each filter), so a client
  paging an owed-reply search stops while later pages still hold owed threads.
- `total` is the index total for the candidates, not the number of owed threads.
- The bulk `--search` cap notice reads `has_more` from the daemon, so for an
  owed-reply search it describes the candidates before the filter. It can say
  "More than 1,000 messages match; only the newest N are selected" when fewer
  than 1,000 owed threads exist (N is then the owed subset of the newest 1,000
  candidates). It can also stay silent when the first page was filtered to
  nothing, although owed threads exist further back.
- `OWED_REPLY_FETCH_CAP` (5,000 rows per account) is a second silent cut on the
  owed side.

## Fix options

1. Filter before paging: resolve the owed thread ids first, then add them to the
   query as a thread-id restriction so the index pages the right set. This is the
   exact fix and keeps `has_more` and `total` meaningful. It needs a thread-id
   term the index can filter on.
2. Page until full: after filtering, fetch further raw pages until `limit` owed
   results are collected or the index is exhausted, computing `has_more` from the
   one-extra-result trick that `live_lexical_page` already uses for visible
   results. No index change, but an unselective owed filter scans many pages.
3. Refuse to page: reject `--offset` with `is:owed-reply` and document that the
   result is the owed subset of the first `limit` candidates. Cheapest, and it
   leaves the notice approximate.

The same shape applies to `filter_to_cached_mail_accounts`; option 2 covers both.
