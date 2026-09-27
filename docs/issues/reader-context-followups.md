# Reader context: known gaps after the first release

**Status:** open · **Found:** 2026-09-27 on `feat/reader` (verifier round 2)

The reader's context block (`GetThreadContext` and `GetThreadGist`, see
`crates/daemon/src/handler/thread_context.rs` and `thread_gist.rs`) shipped
with these known gaps. None of them sends data it shouldn't; they are
staleness, a rare layout shift, and query cost.

## 1. Another tab can show a gist for up to 5 minutes after privacy tightens

Saving LLM settings drops every cached gist in that tab
(`LlmSettingsSection.tsx`), and gist queries are keyed on the model and the
sharing flag. A second open tab keeps its cached gist until the query goes
stale (5 minutes, `threadGistQuery` in
`apps/web/src/features/thread/context/api.ts`). This is display only:
nothing new is sent, and the daemon's cache key already includes the
endpoint and the sharing flag.

Fix: have the daemon push a settings-changed event over the WebSocket, and
invalidate `llm-status` and every gist query on it.

## 2. The gist slot can shift the messages when LLM status arrives late

The reader never waits for `/platform/llm/status`, so the gist slot is
reserved only once the status says a model is configured. AppShell fetches
the status at start, so it is almost always known before a thread opens. If
it arrives after the thread, the slot appears and the messages move down
once.

Fix options: reserve the slot from the last known status (persisted), or
render the slot collapsed and grow it with a transform rather than layout.

## 3. Contact counts can lag sync by the refresh debounce

"41 emails" comes from the `contacts` aggregate, which is refreshed about
10 seconds after sync settles. Mail that just arrived can be missing from
the counts until then. Addresses the aggregate doesn't know yet are
counted live.

## 4. The outbound last-contact search scans recipient JSON

`latest_outbound_elsewhere` in `crates/store/src/thread_context.rs` walks
outbound mail newest first, parsing To, Cc and Bcc JSON on each row, and
stops at the first match. For someone you last wrote to long ago, or only in
this thread, it walks the whole outbound history.

Fix: a recipients table (message id, lowercased address) with an index, or
a `last_outbound_elsewhere` column on `contacts`.

## 5. The reply-time median sorts every pair for heavy correspondents

`reply_latency_median` counts and then sorts a counterparty's reply pairs
through `idx_reply_pairs_party (counterparty_email, replied_at)`, which
doesn't cover `account_id`, `direction` or `latency_seconds`. On the demo
dataset one address has 11,154 pairs and the query takes about 32 ms in a
debug build; typical addresses take under 1 ms.

Fix: a covering index on
`reply_pairs(counterparty_email, account_id, direction, latency_seconds)`,
which lets both the count and the `OFFSET` walk run on the index alone. The
same migration could store `counterparty_email` lowercased (with a backfill)
so lookups no longer pass every spelling the thread uses.
