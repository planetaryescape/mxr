# Local mail view performance audit

Use this audit to reproduce the Messages stalls and distinguish daemon work from browser rendering. The mailbox used for the baseline contained 111,345 messages in a 5.7 GB SQLite database. Measurements contain timings and aggregate counts only; mail, credentials and screenshots are not included.

## Reproduce the measurements

```sh
python3 scripts/perf-mail-views.py --repeat 3 --budget-ms 250
```

The script times read-only CLI requests for Messages filters, Now, the rail, open to-dos, Updates, Reading, Archive and search. It validates their JSON replies without printing them and exits nonzero if a request fails or exceeds the budget. Reading uses `--peek`; the to-do list does not mark runway items seen. Use `--case attention` to isolate one view.

The first sample is not necessarily cold: the daemon may already have that view cached. Use a disposable database snapshot and a separate daemon profile for startup, migration and cold-cache experiments. Never switch development binaries against the normal mailbox to run a benchmark.

For browser measurements, record resource timing for `/api/v1/mail/people`, document navigation entries, loading-state duration, and `layout-shift` entries. Check filter element identity and its bounding rectangle before and after a change. A fast HTTP response alone does not prove that the content became usable promptly.

## Released baseline

These are individual observations from the installed optimized `v0.6.164` binary, not averages:

| Request | Wall time | Result |
|---|---:|---|
| Daemon status | 36 ms | Success |
| Messages, attention | 2,692 ms | One attention row |
| Messages, all conversations | 10,879 ms | Failed |
| Messages, your turn | 3,985 ms | One attention row |
| Messages, waiting on them | 5,091 ms | Success |
| Messages, unread | 3,858 ms | Success |

A browser filter change took 5,533 ms, almost entirely time to first byte. The document navigation count stayed at one: the apparent reload was the route unmounting its body and replacing it with a page skeleton. Loading-to-content transitions also moved the filter count line and list. Status remained fast, so this was not uniform daemon unresponsiveness or browser paint cost alone.

## Findings and changes

### Lists loaded full classifier explanations

A recent-window facet query returned 3,425 records and approximately 92 MB of JSON. Asks explanations retain source zones and their evidence; the list needed their verdicts. Decoding those explanations on each view multiplied the cost of otherwise bounded local reads.

`AskSummary` in the store now holds the view's verdicts, ownership and context. Schema 102 maintains `ask_view_summaries` in the same transaction as the authoritative assessments. Revisions, policy and timezone checks still apply. Detail and membership reads retain their evidence; the list does not truncate the authoritative explanation. Backup reconstruction derives the summaries from the copied assessments.

This is a one-time projection backfill on upgrade. A separate SQLite probe over the real mailbox took about 55 seconds to create 111,352 summaries under concurrent local build load. That is migration work, not a normal navigation cost; measure it separately from steady-state requests.

### Settled views repeated materialization

All 93,626 tracked conversation inputs were settled at the first inspection. Nevertheless, each sender view reloaded full threads, sender histories, fingerprints and asks preparation before reading the stored facets again. Instrumented debug reads spent several seconds in that materialization even when it produced zero new answers.

Views now read the revision-validated facets first and materialize only missing or changed threads. The facet query also keeps requested IDs as its outer loop, rather than scanning an account's assessment history for a small selection. Regression tests count SQLite VM work as the mailbox grows.

The same redundant materialization existed in `AccountKinds::load`, used by Now, Reading, Updates and rail counts. It now uses the same validated stored facets and fetches full threads only for pending candidates. Rail quiet-count computation also omits the asks evidence quotes that a membership detail page exposes; counting a quiet thread does not need its full explanation.

Settled place candidates now omit body evidence and raw sender context entirely. The shared resolver still loads complete pending threads before placing them; unresolved detail reads retain their raw-signal fallback. Stored-kind reads do not construct reply-request signals first, and timer classification shares one purpose analysis rather than parsing the same authored text twice.

### History read bodies that it could not display

All conversations previously loaded historical body evidence before discarding non-conversation threads and limiting visible rows. History now selects possible correspondence from stored homes, includes pending and stale candidates, and returns whole threads without historical body evidence. Stored membership supplies the participant geometry. Body previews remain limited to displayed rows; opening a conversation still reads its full cached source.

History selection now separates stored Messages homes, outbound mail, changed thread generations and changed sender generations. The home index excludes the large asks explanations. This avoids materializing every account's effective-thread view just to select one account's history. Sender-only invalidation is tested separately from a direct message change.

The missing/stale-home fallback now checks inbound mail only: the outbound union already includes every other direction. A covering home-revision index avoids fetching authoritative JSON pages to compare a generation. Alternating read-only probes returned the same 30,054 envelopes in 517–541 ms with the original plan and 472–481 ms with the covering plan. This is a bounded improvement, not evidence that the complete history view meets its latency target.

A later query plan exposed another table read per inbound candidate: the existing owed index did not cover message IDs. Schema 102 adds a covering conversation-input index. A rollback-only probe returned identical envelopes and reduced the full envelope query from 2,087 ms to 1,394–1,550 ms under local load. The facet read now separates home JSON from compact asks and materializes effective generations once per requested thread. Alternating probes returned identical 53,489 facets in 410–465 ms before and 321–351 ms after. These probes isolate query changes; neither establishes end-to-end acceptance.

Recent-window reads also skip body evidence for settled mail without timers. Ordinary lanes use their stored asks and homes; the raw human-response classifier now runs only after finding a live reminder, not before checking whether a reminder exists. Pending mail and threads with explicit reply-later/reminder state retain full evidence: a visibility correction must not make an automatic reply count as a human response or stop a real reply from clearing a timer. Raw sender-template history remains available to the resolver and to bulk mail's human-response check, rather than being fetched for every settled list.

Updates' sender-history window previously selected full evidence columns before applying its per-sender rank limit. It now ranks envelope IDs first and omits body evidence altogether: history compares cached update facts, and `facts_for` independently reads bodies when a fact is absent or stale. A traced Now request spent 240 ms reading that unused historical evidence. Hidden mail is excluded before ranking; selected history retains all envelope/state fields, and detail reads retain full evidence. CPU sampling also found repeated UUID-to-string allocation inside the history sort; it now compares UUIDs directly, preserving chronological and storage-order tie breaks.

### A binding error broke large history views

Sender-history bulk-header detection mixed numbered `?1` and anonymous `?` bindings. SQLx binds these independently, so the ID-list JSON became the first `LIKE` pattern. Large batches exceeded SQLite's pattern-size limit. Every marker now has an explicit numbered binding. The regression test uses more than 50,000 bytes of IDs and checks the final header marker as well.

### Polling met cache expiry

The web client polls Messages every 60 seconds; daemon views also expire after 60 seconds. Previously, unchanged views were left to expire, making ordinary polling trigger cold rebuilds. The refresher now renews recently used views before expiry. Mutation and store-generation checks still prevent serving stale data.

Needs attention, Your turn, Unread and Past your pace also repeated the same complete attention query under separate cache keys. A fresh unfiltered attention snapshot can now answer those narrower views directly, with the requested filter and empty-state wording. Pinned faces are rebuilt after narrowing, rather than filtering an already capped Pinned band. This reuse never crosses accounts, never uses history as attention and ends as soon as the store, mutation generation, local hour or cache age changes.

Settled sender reads reuse the generation-validated home's `written_to` observation instead of re-parsing every outbound recipient for a one-row list. The raw classifier still performs the live recipient lookup. Rule-treatment reads check the treatment table before joining, rather than walking an account's entire mailbox when there are no treatments. Nonempty treatment reads keep their existing account-selection plan.

### Loading replaced the pane tree

Messages now keeps the pane, header, filter controls and open reader mounted while a different view loads. History rows never stand in for attention rows, and account changes never display another account's mail. Loading uses the real list column and row component; the filter status row reserves its space. Ready data is not held behind a minimum skeleton duration. Superseded list and person requests consume React Query's cancellation signal.

After these layout changes, a deliberately slow filter response preserved the same filter element and bounding rectangle with zero observed layout shift. Desktop and phone layouts were checked separately.

### TUI events waited behind replies

The TUI's shared IPC worker forwarded events only after its in-flight request completed. It now forwards events while awaiting that reply and discards abandoned queued safe reads without discarding mutations. A gated fake daemon test requires an event to reach the UI before the reply is released.

Investigating the native terminal check also exposed a key-dispatch bug: a selected Not-sure question consumed `m`, `u`, `x` or `e` as a classification answer even when it was the suffix of a `g` navigation chord. Pending navigation now resolves before those one-key answers. The regression requires the navigation action, no queued mail mutation, and both questions still unanswered; existing direct-answer tests retain the single-key behavior. The terminal fixture uses a separate client config with a disabled, provider-free account solely to bypass onboarding. Earlier onboarding-blocked checks are not interaction proof.

The same check exposed startup work outside Messages: the TUI synchronously loads 5,000 envelopes before its first frame. The daemon reloaded each envelope's account labels and label memberships, although the store's envelope query already included provider label IDs. Traced envelope requests took 1.9–7.3 seconds under local load. The page, ID-list and thread handlers now use the already hydrated envelopes without those per-row queries. Existing thread-label coverage and expanded account-page, label-page and ID-list coverage check that label badges survive the removal.

## Verification requirements

Compare optimized binaries on the same snapshot, with equivalent background activity. Do not compare debug timings with release timings as a speedup claim. Check cold view construction and repeated navigation separately, then verify the installed release against the normal mailbox. Keep mail mutations and sending out of performance testing.

### Latest optimized measurements (not release acceptance)

The schema-102 optimized artifact, including timer-first reads, compact history refinements and duplicate-label removal, produced these first/repeated requests on the same snapshot. This artifact precedes the gate-driven Tantivy upgrade described below. No heavy commands owned by this audit ran during the sweep. Tracing was disabled. The first request builds each view; later requests use validated caches where available.

| Request | First view build | Repeats |
|---|---:|---:|
| Attention | 230 ms | 19–22 ms |
| All conversations | 1,807 ms | 30–32 ms |
| Your turn | 18 ms | 19 ms |
| Waiting on them | 341 ms | 19 ms |
| Unread | 19 ms | 18–19 ms |
| Now | 529 ms | 20–21 ms |
| Rail | 666 ms | 20 ms |
| Updates | 307 ms | 23–24 ms |
| Reading | 209 ms | 23–24 ms |
| Archive | 135 ms | 124–126 ms |
| Search | 31 ms | 27–28 ms |

Your turn and Unread reused the earlier attention snapshot; they are not independent cold builds. Attention now passes the 250 ms budget, but History, Waiting on them, Now, Rail and Updates do not. The performance work remains incomplete; installed-release verification is still pending. The isolated schema upgrade and startup maintenance completed in 16 seconds and 36 seconds respectively, measured separately from navigation.

The native TUI check passed with the same optimized binary: first frame in 800 ms, `g m` navigation in 8 ms and Help after filter keys in 5 ms. Its terminal emulator answers capability queries; earlier runs that did not answer them lost keystrokes and spent four seconds timing out, so they are not interaction or startup-performance proof. This smoke test checks real rendering and input; the gated IPC test separately establishes that events arrive before a deliberately slow reply is released.

Increasing SQLx's row buffer did not improve the historical envelope read. Aggregating the same rows into one JSON result also failed to reduce its cost. A 32 MiB SQLite page cache improved that microbenchmark only modestly, so none of these tuning changes is included.

### Dependency gate

The final local gate initially passed formatting, scoped Clippy and architecture checks, but new advisories published on 26 October blocked `cargo deny`: RUSTSEC-2026-0391, RUSTSEC-2026-0398 and RUSTSEC-2026-0403 affected Tantivy's existing `zstd 0.13.3` / `zstd-safe 7.2.4` dependencies. Updating Tantivy to 0.27 selects the patched `zstd 0.14.3` / `zstd-safe 8.2.0` line without exceptions. Both Tantivy releases support index formats 4 through 7. All 46 `mxr-search` tests and the complete scoped Rust gate pass after the upgrade, including advisories, bans, licenses and sources. Final optimized timings and installed-release verification remain pending.

### Navigation lookups under verification

The Now and rail traces also show repeated scans of every outbound message's recipient JSON for small new-sender checks. Schema 103 adds an indexed, transactionally maintained recipient projection, retaining one supporting row per outbound message and address. Editing recipients, changing direction or account, and deleting mail update the projection in the same transaction. A check now starts with its requested addresses instead of parsing the account's sent history. To, Cc, Bcc, case folding, duplicate support, account movement, deletion and migration backfill have regression coverage.

Updates history now takes each sender's bounded page through an account/sender/date index before loading full envelope state, rather than ranking every matching message. It preserves case-insensitive sender matching, duplicate-request set semantics, account boundaries and filtering of hidden mail before the per-sender limit. Now's independent People, To do and inbox/digest branches run concurrently through the existing reader pool. These changes were measured in the following sweep.

The first schema-103 optimized sweep, also including Tantivy 0.27, returned these cold/repeated times with tracing disabled and no other audit-owned build running:

| Request | First view build | Repeats |
| --- | ---: | ---: |
| Attention | 239 ms | 19–20 ms |
| All conversations | 1,645 ms | 29–32 ms |
| Waiting on them | 410 ms | 20 ms |
| Now | 312 ms | 19–20 ms |
| Rail | 381 ms | 19 ms |
| Updates | 149 ms | 22 ms |
| Reading | 177 ms | 22–24 ms |
| Archive | 131 ms | 122–167 ms |
| Search | 30 ms | 26–28 ms |

Updates now passes the budget. History, Waiting on them, Now and rail still fail it. Startup with the navigation migration took 3,148 ms; maintenance completed by 22,722 ms. The new binary opened the existing search index, and the read-only search benchmark succeeded.

A diagnostic trace then exposed work after Now's old completion log: the coming-up strip ran after the snapshot, adding about 100 ms. The strip and first-run state now start alongside the snapshot, and the existing completion log includes their cost. Rail's independent quiet count also starts alongside its snapshot. Messages prepares its window/history envelopes and sender context alongside the desk, still adding any older timer/attention threads and applying the desk's authoritative turns afterward. These further concurrency changes need their own optimized measurement.

The latest scoped Rust gate passes, as do 83 daemon navigation tests, 12 focused store navigation tests and all 28 backup/restore tests. The returned-thread regression now uses mail 400 days old and checks Attention, All, Waiting, Now and rail. The source changes are committed separately from the measured audit; final optimized and installed-version gates remain open.

An experimental packed-envelope history projection still needed roughly 260–470 ms for the primary account's SQL read, before Rust composition and the desk. It is not included. Replacing full envelopes with a packed copy does not eliminate enough work to satisfy the cold History gate.
