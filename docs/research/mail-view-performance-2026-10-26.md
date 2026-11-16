# Local mail view performance audit

Use this audit to reproduce the Messages stalls and distinguish daemon work from browser rendering. The mailbox used for the baseline contained 111,345 messages in a 5.7 GB SQLite database. Measurements contain timings and aggregate counts only; mail, credentials and screenshots are not included.

## Reproduce the measurements

```sh
python3 scripts/perf-mail-views.py --repeat 3 --budget-ms 250
```

The script times read-only CLI requests for Messages filters, Now, the rail, open to-dos, Updates, Reading, Archive and search. It validates their JSON replies without printing them and exits nonzero if a request fails or exceeds the budget. Reading uses `--peek`; the to-do list does not mark runway items seen. Use `--case attention` to isolate one view.

The first sample is not necessarily cold: the daemon may already have that view cached. Use a disposable database snapshot and a separate daemon profile for startup, migration and cold-cache experiments. Never switch development binaries against the normal mailbox to run a benchmark.

## Shipping decision

The final optimized source candidate, `775e19513`, fixes the multi-second navigation regression, but does **not** consistently meet the adopted 250 ms All-conversations target. On the 111,345-message snapshot, All measured 254 ms in the full sweep and 290 ms as the first view after an independent daemon restart. The 250 ms timing gate therefore remains red; the explicit "ship it" instruction is being applied as a decision to release the substantial improvement with this remaining performance gap documented, not to relabel it as a pass. See [the cold-history follow-up](../issues/messages-cold-history-budget.md).

The full sweep's first-view times were Attention 154 ms, Waiting 209 ms, Now 214 ms, rail 227 ms, Updates 129 ms, Reading 170 ms, Archive 78 ms and search 28 ms. Mine and Unread reused Attention and are not independent cold builds. All repeated at 28–31 ms; those cached responses are not cold-read proof. The All reply remained identical to the earlier source output except `generated_at` (844,337 bytes). The build took 9 minutes 15 seconds. Startup was 2,728 ms and maintenance completed by 22,776 ms; navigation measurements followed maintenance, with no prior All-response cache entry. Full ready-PR CI, release and installed-mailbox checks are still pending.

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

A diagnostic trace then exposed work after Now's old completion log: the coming-up strip ran after the snapshot, adding about 100 ms. The strip and first-run state now start alongside the snapshot, and the existing completion log includes their cost. Rail's independent quiet count also starts alongside its snapshot. Messages prepares its window/history envelopes and sender context alongside the desk, still adding any older timer/attention threads and applying the desk's authoritative turns afterward.

The optimized artifact at `19b2f6f38` then measured cold Attention at 199 ms, Waiting on them at 241 ms, Updates at 122 ms, Reading at 159 ms, Archive at 147 ms and search at 27 ms. Waiting now passes too. All remained at 1,467 ms, Now at 284 ms and rail at 283 ms; these three still block acceptance. Repeated cached reads were 17–31 ms. Startup took 2,740 ms and maintenance completed by 22,299 ms, separately from the navigation sweep.

The latest scoped Rust gate passes, as do 83 daemon navigation tests, 12 focused store navigation tests and all 28 backup/restore tests. The returned-thread regression now uses mail 400 days old and checks Attention, All, Waiting, Now and rail. The source changes are committed separately from the measured audit; final optimized and installed-version gates remain open.

An experimental packed-envelope history projection still needed roughly 260–470 ms for the primary account's SQL read, before Rust composition and the desk. It is not included. Replacing full envelopes with a packed copy does not eliminate enough work to satisfy the cold History gate.

### Remaining critical-path reads

The next Now/rail trace found two more mailbox-wide selections. The desk read and validated 418 open commitments before keeping only actionable own dates inside its window. Moving that selection ahead of evidence validation returned the identical due set in a read-only probe, reducing the primary account's SQL time from about 23 ms to 1 ms. The filter retains inclusive date boundaries, provenance, active-evidence checks, account scoping and the rule that completed to-dos suppress their promise.

Subscription-ending mail also scanned the mailbox because its query lacked the account prefix of the sender/date index. Enumerating all stored accounts, including disabled ones, lets the same lookup use account, sender and date. A read-only probe over 493 requested senders returned the identical 162 rows in 23–26 ms instead of 68–74 ms. The desk also reads at most three accounts concurrently, retaining their original order for the Screener link. Five focused store tests, 86 daemon tests and the complete scoped Rust gate pass; a fresh optimized sweep is still required for these changes.

A sizing prototype of thread-level history synopses read 20,555 generation-matched rows (8.2 MB) in 51–59 ms, with another 43–76 ms of Python JSON decoding. This is not a correctness-validated implementation and is not included. Unlike the packed-envelope experiment, it avoids reloading every historical envelope and both full facet sets. Any implementation must preserve exact participant geometry, names, done watermarks, future-date handling, account/policy/timezone/input-generation validation and selected-thread previews, including invalidation during mutations. A complete cached response is not a substitute for that incremental projection or for measuring cold composition.

The optimized bounded-signal artifact at `1c22d7525` measured cold Attention at 216 ms, Waiting at 239 ms, Now at 231 ms, Updates at 132 ms, Reading at 166 ms, Archive at 85 ms and search at 27 ms. Mine and Unread reused the complete Attention response. All remained at 1,667 ms and rail at 314 ms. Startup was 4,358 ms, with maintenance complete by 23,912 ms. This sweep still fails acceptance.

### Thread history projection under verification

Schema 104 now implements a rebuildable, per-thread synopsis rather than caching an All response. It retains participant geometry, ordered name observations, message IDs, the legacy Done watermark, unread state, and the newest eligible stored messages. Input generations retire it on source, sender, receiver or classifier-authority changes. Separate view tokens also cover read flags, inbox labels and facet writes at the same generation. Each write compares both observed tokens in the writer statement. Account, policy, timezone, shape configuration and owned-address context are checked on reads. Future-dated mail sets an explicit expiry; backward clock movement also rejects a synopsis.

Turns, Done marks, people links, contact facts, ranking, limits and narrowing are still composed live. Only the selected rows load full threads for previews; person and conversation detail retain their complete reads. Deleting any source message removes the entire thread synopsis in that same transaction. Logical backups omit these rebuildable projections; exact schema 97–103 upgrade fixtures cover their addition.

Candidate IDs are a separate generation-checked input projection, not display rows. Both the global assessment-input clock and a durable same-generation facet-write clock must match. This avoids rechecking every excluded historical inbound message on every unchanged list, without freezing threads that later become conversations. A missing or expired set runs the original shared selection and publishes only if neither observed clock changed. The selection still retires when a future date becomes eligible. A covering-index probe reduced the stale-home branch alone from 486–751 ms to 100–103 ms; the shared SQL now uses that covering message index explicitly.

The first synopsis implementation passed 100 focused daemon tests (two reports ignored). Subsequent runs passed 69 Messages/modes/Now tests (two reports ignored), 34 store/backup tests, and all 15 projection/deletion tests. The final full store run passed 485 tests, with two timing benchmarks ignored. Regressions cover previously excluded threads, same-generation facet edits, read/inbox edits, label-role changes, partial-thread deletion and late builders. The rail's per-account quiet counts now run with bounded concurrency. The scoped Rust gate and a fresh debug build pass. The optimized build and cold measurements remain pending. None of these unmeasured changes establishes performance acceptance.

The optimized build at `a4b07ea6e` completed in 10 minutes 4 seconds. Its cold sweep measured Attention at 150 ms, All at 352 ms, Waiting at 236 ms, Now at 218 ms, rail at 284 ms, Updates at 140 ms, Reading at 177 ms, Archive at 86 ms and search at 28 ms. Mine and Unread reused Attention. Startup was 2,764 ms, with maintenance complete by 22,800 ms. All and rail still fail the 250 ms gate. The private before/after All replies were identical apart from `generated_at`, including all 844,337 bytes of rendered reply structure and values. No acceptance is inferred from the 18–37 ms cached reads.

### Remaining repeated reads

The persisted projection contains 20,927 thread synopses across three accounts, totalling 14.45 MB. On the primary account, its 20,555-row SQL query took 66–70 ms through direct SQLite, but about 160 ms through the traced SQLx row stream. A single typed JSON page avoids those per-row handoffs and decodes each synopsis only once. Its direct-SQLite probe took 90–94 ms; an optimized app measurement is still required.

A traced All rebuild also loaded 205 full bodies, including attachment metadata, while producing list previews. Those reads spanned about 177 ms with tracing enabled. List previews now batch just body text for each bounded quote-lookback window; conversation detail retains full attachments, with an explicit non-inline-attachment regression. The preview path preserves missing bodies, stored empty plain text and HTML alternatives.

Rail separately repeated the same inbox selection and classification for its quiet count and Now snapshot. They now share that classified inbox; complete thread membership still decides which candidate threads count as quiet. Digest composition and quiet membership run concurrently after the shared read. Six focused store tests and 123 daemon tests pass for these follow-ups (three mailbox reports ignored). The scoped Rust gate also passes. Fresh builds and optimized measurements are pending.

The `110270f0d` optimized build took 12 minutes 3 seconds. Rail passed at 242 ms, but All still took 379 ms; its output remained identical except for `generated_at`. Reading measured 251 ms, so this sweep did not pass the full gate. The traced All request spent 309 ms loading and decoding the primary account's persisted facts; the typed page alone did not solve the bottleneck.

The next change keeps decoded thread facts in memory, not ready-made Messages responses. Each access still reads the assessment-input clock, facet clock, summed per-account view revisions, account existence and next future-message eligibility boundary. The context and valid-time interval must also match. A build whose before/after tokens differ is not retained. Existing startup maintenance prepares these same facts; a first All response must still compose live turns, Done state, people links, contact facts, rankings, filters and previews. The token query uses the account/thread revision index and covering account/date index; its snapshot probe took less than 0.1 ms. Six focused store tests and 70 daemon tests pass (two mailbox reports ignored), including read invalidation, same-generation facet writes, source/account removal, live people links and future-date expiry. This is not evidence that the final view meets the budget; the final gate and a fresh optimized measurement remain required.

The same trace showed roughly 150 ms of sequential selected-preview work after history was available. Those independent previews now overlap at most four rows at a time, retaining row order, account boundaries and the same quote-lookback and gist rules. No unselected conversation text is loaded. The final Messages run passed 34 tests (one mailbox report ignored), and the scoped Rust gate passed after this change. The optimized build and cold sweep remain pending.

## Web ownership checks wait behind background work

The 2026-11-16 reproduction used the Linux x86-64 release `v0.6.241`
(`27865d406fc2c7cbb7aa75ff559613712fa3416d`) with 50,000 synthetic messages.
The fix starts from `origin/main` at
`f7bf2bd35c70420a9453488bb90235a69416ed75`. The two refs differ only in tests.

Time the HTTP path as well as the CLI. During background work, authenticated
HTTP reads measured 9,380 ms for lexical search, 6,930 ms for Messages, and
14,373 ms for the client shell. The traced search handler itself took 29 ms.
These diagnostic samples overlapped a local build and are not an acceptance
benchmark. A second run followed a restart, with query tracing off. Search
took 116–154 milliseconds. Messages took 487–605 milliseconds and the shell
took 179–185 milliseconds. Idle measurements alone miss the failure.

The bridge's workspace middleware and the shared client's ownership checks
call `GetArchiveWorkspace`. The client checks `GetStatus` for the ownership
capability first. Status waits for a full semantic snapshot, even though it
only uses runtime counters. The workspace store method also attempts an
insert on every call, including when the identity exists. Both steps can wait
behind background work before the requested mail read starts. A transaction
held the demo's SQLite writer for five seconds. An authenticated lexical HTTP
search then took 5,544 ms. The transaction rolled back without changes.

The fix is commit `15ff0af33e95e1386523c4f97ca3e56c885298eb` on
`fix/web-mail-latency`. It reads an existing workspace identity through the reader pool and
retains the conflict-safe insert for first use. It does not cache the identity:
recovery generation changes remain visible. Semantic work publishes its last
runtime counters to the service handle. Status reads those counters without
queuing a command or loading profile records. Ownership and capability checks
remain in place.

The source paths are the [workspace read](../../crates/store/src/mutation_jobs.rs#L73),
[status response](../../crates/daemon/src/handler/diagnostics/mod.rs#L1478),
and [runtime counters](../../crates/semantic/src/service.rs#L340) at that commit.

Run the regressions through the shared build queue:

```sh
agent-work run -- scripts/cargo-test -p mxr-store --tests existing_workspace_identity_does_not_wait_for_the_writer
agent-work run -- scripts/cargo-test -p mxr --lib status_does_not_wait_for_queued_semantic_work
```

Both tests failed before their fixes. The first holds a file-backed store's
writer and requires the initialized identity within 500 ms. It also checks that
generation rotation remains visible. The second blocks semantic ingest on its
background permit and requires status, with runtime counters, within 500 ms.

### The fixed demo passes the one-second HTTP check

The optimized build with `semantic-local,web-ui` and the production web build
passed. The default-feature Rust tests passed: 560 store tests, 48 status tests,
and 15 archive-recovery tests. Two store benchmarks remain ignored.

The fixed binary ran against the same 50,000-message demo database on port
42839, with query tracing off. The store suite ran during these measurements.
Each HTTP range contains three samples:

| HTTP read | Immediately after restart (ms) | Later reads (ms) |
| --- | ---: | ---: |
| Lexical search | 48–78 | 41–75 |
| Messages | 334–996 | 15–16 |
| Client shell | 125–965 | 123–127 |

Later Messages reads reuse the daemon's response cache. They do not measure a
cold composition. The writer-contention probe returned search results in
49 milliseconds while an independent transaction held the writer for five
seconds. The earlier candidate probe took 53 milliseconds with the lock held
throughout that request. Both transactions rolled back without changes.

Browser checks loaded Messages and the default Exact search with a connected
daemon. After a fresh page navigation, the Messages request finished 145
milliseconds after navigation began. Search's first page request took 73
milliseconds. These are network timings, not paint timings. CLI Messages reads
took 29–37 milliseconds and lexical search took 54–67 milliseconds over three
samples each.

Startup HTTP reads still approached one second during background work. This
run did not retest the separate [250 ms cold All-conversations budget](../issues/messages-cold-history-budget.md).

### The first local check used a copied binary

The first check used `/tmp/mxr-demo-v0.6.241/mxr-fixed`, copied from the
optimized build. The demo is at `http://localhost:42839/messages`. Its config
is `/home/bk/.config/mxr-demo` and its data is `/home/bk/.local/share/mxr-demo`.
Local reproduction scripts and aggregate timings are in
`/tmp/mxr-demo-v0.6.241/`. The browser stays on Messages.

The first-fix checks are complete. Broader performance acceptance remains open,
as the real-mail and full-view checks below show. The source is on
`fix/web-mail-latency`.
The source commit contains the fix and regressions. The documentation commit
records this handover. The source and documentation commits are local.


## The real mailbox still has slow All-conversations reads

The live route on 2026-11-16 is `https://mxr.justlivin.life`. Caddy forwards
it to `https://hume-2.tail4d8fc3.ts.net:18741`. Live Caddy configuration and
Hume's running processes showed this route. The fleet record matched it at
`1c93d8f86de81c5d7442d52854266dd51a6fd20d`.

Hume runs v0.6.240, whose source tag resolves to
`04bcfdff98ffa47951e6cb2469b4e62333f70c77`. The running daemon reports that
version and IPC protocol 32. Its database contains 112,411 messages across
three accounts. Hume still runs the released code.

A probe used the existing paired client to sign in through Caddy. Credentials,
cookies and mail stayed inside the Hume process. Only aggregate timings and
response types left the machine. Each row contains three signed-in HTTP reads.

| Real HTTP read | Samples (ms) |
| --- | --- |
| Messages, default attention | 179, 191, 223 |
| Messages, All conversations | 2460, 902, 1358 |
| Messages, waiting on them | 313, 308, 252 |
| Lexical search, `is:unread`, limit 20 | 65, 103, 65 |

Messages returned its structured response type. A separate signed-in search
check took 59, 57 and 65 ms. It validated nonempty result groups and the
numeric total. The earlier search samples checked HTTP status only. These requests used the running
daemon and can reuse caches. They do not establish cold-load performance.
The public health endpoint took 46 ms. Health does not measure mailbox reads.
No live service upgrade, restart, configuration edit or mail mutation occurred.

Hume also has the sticky `demo-active` marker in its normal configuration
directory. In v0.6.240, `run_parsed_cli` applies that marker before dispatching
non-demo commands. Ordinary CLI probes can therefore reach the demo while the
existing browser gateway still serves real mail. The measurements above used
the normal bridge or the signed-in gateway directly. Before an approved live
upgrade or restart, exit sticky demo mode and verify the normal data and socket
paths. This investigation changed no marker or service.


The full local view sweep also fails. Source `e9aa8e9b3` includes the shared
ownership/status fix. Its optimized binary read the 50,000-message demo.
Query tracing was off. First requested All took 399 ms.
Waiting on them took 519 ms. Now took 1706 ms, Updates 1544 ms, and Reading 15999 ms.

Reading's next two reads took 2762 ms and 21 ms. Views ran in sequence in an existing daemon, so these
are not independent cold restarts. Repeated cached results cannot satisfy the
250 ms cold-composition target.

The shared composition changes are commit `9c1557aa258c5e4bca4921f5c9bc223c0604db1f`
on `fix/mail-read-performance`. The final checks below supersede the pending
optimized and HTTP verification. Do not report broad performance completion
from the first fix.


### Shared composition work remains expensive after the first fix

Temporary stage probes on base `e9aa8e9b3` reproduced the remaining work.
The instrumented optimized binary read a disposable copy of the demo database.
Reading's derived item versions and done marks were reset only in that copy.
This diagnostic run overlapped focused tests and is not an acceptance benchmark.

Now spent 476 ms and 250 ms loading the two accounts' inbox candidates.
Their facet reads took another 331 ms and 300 ms. The pending-thread count
was zero for both accounts. Updates and Reading repeated the same settled
reads. The new store trace operations are `places.mode_candidates` and
`assessment.facets_for_messages`. They emit elapsed time and row counts only.

Reading loaded 2,837 issues and rebuilt their item caches in 1,035 ms.
Only 47 issues appeared. The other 2,790 expired. Expiry uses source dates,
so it can precede extraction. Source history must still include expired issues.
Finished issues must still supply the reading-speed estimate, and Later must
still rebuild saved links after an extractor change.

A paired SQLite probe compared the inbox query with `UNION` and `UNION ALL`.
All candidate rows matched exactly across both accounts in three runs.
Removing the redundant deduplication saved 66–78 ms across the accounts.
The selections cannot overlap: the second excludes messages in the inbox.
The query-plan regression failed before the change and passed afterward.


### The shared fix preserves view behavior

Commit `9c1557aa258c5e4bca4921f5c9bc223c0604db1f` overlaps at most three
independent account reads while retaining account order. Both inbox composition
and Messages history/window preparation use this bound. The store combines
disjoint inbox and filed selections with `UNION ALL`.

Reading resolves source windows before extraction. It rebuilds displayed issues,
saved Later items, and finished issues needed for the reading-speed estimate.
Source history, expiry marks and saved link keys retain their existing rules.
The first regression run caught the pace dependency. The final Reading run
passed all 22 tests, including pace, expiry, re-extraction and account scoping.

The other focused view checks passed 111 tests, with three real-mailbox reports
ignored. The full store suite passed 561 tests, with two timing benchmarks
ignored. The optimized build and scoped Rust gate passed. The cold HTTP checks
below still fail their budgets. These tests establish behavior, not latency.


## Hosted reads also queued on authentication and invalidated their caches

Source `9c1557aa2` still used the single writer connection for native and
browser authentication. The browser gateway authenticates, touches its presence
lease, then admits each API operation. Each step checks the credential again.
A fast local-bridge probe therefore did not establish a fast hosted path.

The original native implementation is commit
`974ec3d54312e283504fd138e014392b60be6082` (PR #443, 2026-10-12).
Its comment states that the writer transaction protects completed revocations
from an older reader snapshot. Browser sessions inherited that choice in
`f02c7fd3b913f0f162bf30d6726f5a1218487472` (PR #906). Preserve that authority
requirement while removing unrelated writer serialization.

Commit `cfe55a100d1c95baa0e43869763236f5cdcce74d` checks each credential,
parent registration, current workspace owner and recovery pause in one
fresh reader statement. It retains constant-time verifier comparisons.
Pairing, approval, claim and revocation still commit through the writer.
The vendored SQLx executor resets statements on completion or drop
(`vendor/sqlx-sqlite/src/connection/execute.rs`), so these autocommit statements
do not retain a prior request's snapshot.

The file-backed regression holds an older reader snapshot and an uncommitted
revocation on the writer. Both authentication forms must complete within
500 ms and then reject credentials after revocation commits. It failed on the
old path and passed after the change. Separate checks cover workspace-generation
rotation and paused recovery. The focused authority run passed 20 tests,
including the portable-backup authority test.

The gateway's presence heartbeat also called the generic mutation invalidator.
Every browser API request therefore discarded ready mail views. The regression
built a view, sent an authenticated browser heartbeat through the real request
handler, and checked that the view remained valid. It failed before the fix.
Presence updates now leave mail views intact. Safety classification remains
unchanged. Mail mutations still invalidate views, and each remote read still
checks authority before using a cached view. Parent revocation remains effective
when a cached response exists. All 32 focused remote/cache tests passed.

## Final local checks preserve replies but fail cold-load budgets

Source `cfe55a100d1c95baa0e43869763236f5cdcce74d` built with
`semantic-local,web-ui` in release mode. The required default `cargo build -p mxr`
also passed. `scripts/pre-pr-rust-gate` passed formatting, scoped all-feature
Clippy, architecture checks and cargo-deny 0.20.2. No temporary debug probes remain.

The matched harness restored one frozen 50,000-message database for every run.
It reset derived Reading items only in this disposable fixture. Both builds
used the real browser gateway, paired browser credentials and the same routes.
The gateway ran on loopback behind an expected proxy host, without Caddy or TLS.
The baseline was the first shared fix, `15ff0af33`, not the released binary.
Tracing was off. No heavy build jobs owned by this work ran during measurement.
Other host workloads remained. These are not isolated single-user speed claims.

Each run waited for `updates facts warmed` and then three seconds. That log
does not establish that every startup task finished. The first request had no
completed response-cache entry. Later requests could reuse valid responses.

| Fixed gateway route | First request (ms) | Next two requests (ms) |
| --- | ---: | ---: |
| Messages, attention | 350 | 31, 23 |
| Messages, All | 600 | 608, 69 |
| Messages, your turn | 319 | 13, 13 |
| Messages, waiting on them | 479 | 21, 20 |
| Now | 1881 | 18, 16 |
| Rail | 3517 | 15, 13 |
| To do | 35 | 34, 30 |
| Updates, without marking seen | 1480 | 35, 37 |
| Reading, without marking visited | 1326 | 1584, 16 |
| Archive | 168 | 144, 117 |
| Lexical search | 54 | 46, 40 |

Independent restarts still failed the one-second gateway budget.
First requests took 1438 ms for All, 2338 ms for Now, 1620 ms for Reading and 2734 ms for Rail.
The concurrent attention/rail/shell/search wave took 851/2913/772/521 ms.
The final verdict fails both the 250 ms CLI and 1000 ms gateway budgets.
The separate browser visit flows also remain expensive.
Reading with `mark_visit=true` took 1007/861/829 ms.
Updates with `mark_seen=true` took 1252/1112/1115 ms.
Cached peek results do not establish fast visit flows.

Normalized replies matched across all 11 mail cases through CLI and gateway.
The comparison excluded only generated time, age and live runway progress.
Reading kept all 47 items, their fields and ranking buckets.
The comparison sorted existing equal-rank ties by stable IDs and retained meaningful ranking fields.

Evidence is in `/tmp/mxr-mail-read-profile/final-acceptance.jsonl`,
`final-verdict.json`, `visit-acceptance.jsonl`, `rust-gate-final.log`, saved replies and the harness there.
The final copied binary is `/tmp/mxr-demo-v0.6.241/mxr-hosted-fixed`.
Its SHA256 is `eceefee8064957ea8532a7f48e33a5f965ee3ea3f407b78a3d6b1bbb33dac23f`.
Its package version remains 0.6.241. This is an unpublished local build.

## Hume live verification and cold composition remain open

The later [Hume results](hume-mail-read-performance-2026-11-16.md) complete the
isolated real-mailbox check at source `4ed2969d8` and supersede this section's
pending fixture work. The one-second gateway budget passes on that snapshot.
The 250 ms budget and live-upgrade check remain open.

Hume still runs v0.6.240. A later read-only aggregate found 112,414 messages
across three accounts, with 1955 inbox messages. The synthetic stress fixture
has 36,891 inbox messages. Do not extrapolate its timings to the real mailbox.

The next live step is to release the shared fixes and exit sticky demo mode on Hume.
Then upgrade its daemon and gateway and repeat signed-in real-mail reads.
This requires BK's approval under the rule for changes outside this repository.
No release, push, service restart, marker removal or live upgrade occurred.

Cold composition needs further profiling. Rail's quiet count still calls full
membership placement in batches of 100 threads. That path loads full threads,
sender context, timers, to-dos and archive records before counting quiet threads
(`crates/daemon/src/handler/modes.rs:134,800` at `cfe55a100`). A replacement
must preserve lane, Done, classification, whole-thread and account semantics.
Profile this path before changing it. The cold All issue remains open. No
earlier ship instruction authorizes this branch's release.

The demo now uses `/tmp/mxr-demo-v0.6.241/mxr-preview-fixed` at source
`4ed2969d8`, with its existing isolated profile at
`http://localhost:42839/messages`. The source and audit remain local on
`fix/mail-preview-evidence-performance`. The Hume results hold the latest handover.
This session does not authorize Obsidian writes outside the repository.
