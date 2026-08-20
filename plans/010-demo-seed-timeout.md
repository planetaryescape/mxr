# Plan 010 — `mxr demo` dies with "IPC request timed out after 120 seconds" (#179)

Orchestrated fix, 2026-08-19. Issue: https://github.com/planetaryescape/mxr/issues/179 (Kubuntu 26.04; `mxr demo` → "Seeding demo mailbox..." → `Error: IPC request timed out after 120 seconds`). `mxr demo` is the README's front door, so this was treated as P1. Reproduced on macOS with the shipped v0.6.19 at only 5,000 messages (died at the semantic prewarm at 137 s).

Statuses: DONE | SHIPPED (version) | FOLLOW-UP.

## Root cause

1. The CLI's `IpcClient::request` has a 120 s wall-clock timeout; the daemon's `SyncNow` / `ReindexSemantic` / `RebuildAnalytics` handlers block until the work finishes. The demo issued them on the capped path, then reconnected and re-issued `SyncNow` on failure (queuing a second sync behind the first).
2. The seed itself was slow and memory-hungry: the fake provider materialised all 50k messages at daemon construction (~480 MB before the socket bound) and returned them as one batch; the sync engine wrote ~8 transactions per message through SQLite's single writer; Tantivy indexing ran synchronously on the tokio runtime.
3. The semantic reindex embedded one message at a time (ONNX batch of ~2.5 texts), persisted progress only at the end, re-embedded messages the post-sync ingest had already done, held the worker for the whole pass, and capped `all_message_ids` at 10,000 per account.
4. Found at full scale: `DaemonEvent::NewMessages` carried all 27,500 envelopes of a backfill page, exceeded the 16 MiB IPC frame cap, and the failed encode silently tore the client's connection down.

## Phases

| Phase | PR | Release | What | Result (16-core Mac, release build) |
|---|---|---|---|---|
| M0 measure | — | — | Shipped-binary reproduction, isolated HOME | 5k: seed 12 s, died at ReindexSemantic 137 s |
| M2b semantic | #189 | v0.6.22 (v0.6.21's Linux build hung on the azure apt mirror) | Cross-message embedding batching + length sort; stepped resumable job with live progress; idempotent reindex (+ `--force`); ANN build off-worker, streamed; 10k cap removed; one pass at a time; reclaim on restart | 2k reindex 269 → 86 s; repeat 168 → 2.5 s; throughput flat with corpus (≈75 chunks/s) ⇒ 50k ≈ 28 min so clients bound their wait; 20k peak RSS 1570 → 1130 MB |
| M1 CLI | #191 | v0.6.22 | Every long call over `request_with_events` + progress; no SyncNow retry; progress-based waits (`StallWatch`); demo active before prewarms; bounded semantic wait; `NewMessages` cap 500 + `total`; `serve.rs` encode vs transport failures (oversized event → drop + `EventsLagged`, oversized response → error reply); `MXR_IPC_TIMEOUT_SECS`; `demo_cli` e2e | 50k demo completes (183 s pre-M2) instead of dying at 120 s |
| M2 seed | #193 | v0.6.23 | Fake provider pages lazily (10k/page, derived ids, `DEMO_SEED_VERSION` 5); `Store::apply_sync_upserts` (one txn / 500 msgs) used by every provider; Tantivy in `spawn_blocking`; `sync_in_progress` true between pages + loop wake; `healthy` independent of in-progress; attachment ids scoped per account; seed tripwire in `demo_cli` | 50k seed 90 → 26 s; RSS at start 480 → 156 MB; peak 1205 → 422 MB; whole demo 96 s |
| ci | #194, #196 | — | `release.yml` apt mirror hardening; manual `demo-e2e.yml` (Linux, 50k, budget) | Linux 4-vCPU run on main @ v0.6.23: exit 0 in 222 s (seed 24 s, analytics 156 s, semantic 2 s, LLM 40 s), 50,000 messages, search + stop OK — https://github.com/planetaryescape/mxr/actions/runs/32285701285 |
| M3 background sync | #197 | v0.6.24 (pending at time of writing) | `SyncNow { background }`; live `AccountSyncStatus.progress` via engine hook; one shared sync finalizer (loop/manual/reaper); analytics repair once per backfill (debt discharged on the has_more edge, incl. empty last page); reaper aborts on no-progress; RAII background claim; TUI/web/CLI on the new path; D058 | 50k demo 103 → 67 s, idle at 70 s, peak RSS 300 MB; manual sync ack 29 ms mid-backfill; web sync 1.5 ms |

Shipped-binary smoke (brew, isolated HOME): v0.6.22 2k demo 12 s; v0.6.23 default 50k demo exit 0 in 182 s (usable at 43 s).

## Review gates used
Worker (Opus) → orchestrator adversarial review (read every diff, ran tests, real binary) → independent fresh-context Opus reviewer per phase (Codex personal quota was exhausted 2026-08-19; re-run Codex on the merged commits is a follow-up) → CodeRabbit/cubic on the PR → CI → admin squash merge → release-please → release.yml → brew smoke.

## Follow-ups (as recorded 2026-08-19; closures added after the follow-up wave below)
- ~~Semantic `search()` asks HNSW for `limit` candidates and filters source kinds AFTER → small-limit fielded hybrid queries can return nothing; also the cause of the known flaky `execute_search_uses_dense_source_kinds_for_fielded_queries` (~1 %/run, pre-existing). Over-fetch before filtering.~~ **DONE #200** (over-fetch factor derived from an exhaustive match over `SemanticChunkSourceKind`; the flake was root-caused to hnsw_rs recall on tiny graphs and fixed at the fixture level).
- ~~hnsw_rs memory ≈ 5.9 KB/vector (16 per-layer Vecs + per-neighbour Arcs + vector copy) ⇒ ~1.1 GB at the 50k demo; mmap-backed points (`PointData::S`) is the lever that scales.~~ **EVALUATED AND REJECTED** in the wave: `PointData::S` is reload-only, so it cannot cut the rebuild peak — the number that actually hurts — and it costs a self-referential borrow plus a large on-disk file. Still open in the reduced form below.
- ~~Semantic `use_profile` switching back to a profile with an in-memory index serves the older index until the deferred build lands (needs a per-profile pass epoch). After "semantic vectors ready" the idle-sync `backfill_active_limited(200)` briefly flips the profile to `indexing`.~~ **DONE #200** (per-profile pass epoch; profile-targeted settle; a backfill with nothing missing no longer starts a pass, so it no longer flips the profile or clears `last_error`).
- ~~`upsert_envelope_tx` resolves by natural key and keeps the OLD row id while dependents write the new `envelope.id` (pre-0.4.52 rows on re-sync fail the page loudly now; fix = upsert returns the id it wrote).~~ **DONE #202**, recorded as **D059**.
- ~~`ResponseData::Status` should carry an explicit `degraded: bool` (clients infer it from `accounts.is_empty()`).~~ **DONE #203/#206** — wired through the CLI table (`unknown`, not zeros), the TUI and the web chrome.
- ~~`SyncProgressData.total` is always `None` (no provider reports a batch total).~~ **DONE #202** — `SyncBatch::remaining_estimate` (resume-correct count-down; exact for the fake provider, `None` for Gmail and IMAP, which cannot count the remainder cheaply) fills the denominator, so `mxr sync --wait` prints `3,000/16,500`.
- ~~Curated-50 demo dataset still uses random `ThreadId`s per regeneration.~~ **DONE #202** — derived `ThreadId`s, so a persisted curated profile re-syncs into the same threads.
- ~~Pre-existing flakes: `mxr-web::compose_session_send_forwards_draft_account_id` (HTTP + fake IPC startup race), `activity::tests::rapid_fire_ephemeral_writes_coalesce_into_one_row_with_count` (under load).~~ **DONE #201** — the first was not a race but a real product bug (see the wave table); the second now settles on a worker barrier instead of a sleep.
- ~~Web: `npm audit` fails the non-required `Web Unit Tests` job (12 advisories, `npm audit fix` available).~~ **DONE #201** — lockfile-only, no major crossed; `dompurify` 3.4.2 → 3.4.13 is the one that matters, since the web app sanitises untrusted email HTML with it.
- ~~README example `mxr search "from:alice is:unread"` returned 0 results on the seeded 50k demo — verify the dataset/query still match.~~ **DONE #201** — not a dataset problem: `from:` is a whole-address `TermQuery` on an untokenised field, so `from:alice` can never match. README, quick start, the search guide and the skill now use full addresses.
- ~~Search worker death is logged but not surfaced in `mxr doctor`/feature health.~~ **DONE #203/#206** — `FeatureHealthReport.search`, one reading per report, and `doctor --check` exits non-zero on it.
- Run Codex (gpt-5.6-sol) review over the merged commits when quota resets. **STILL OPEN.**

## Follow-up wave (2026-08-19/20)

Four parallel workers plus a hotfix, all merged; released together as **v0.6.25** (`chore(main): release 0.6.25`, #204).

| PR | What | Highlights |
|---|---|---|
| #200 | Semantic search and status honest about index passes | Over-fetch ANN candidates before the source-kind filter (factor derived from an exhaustive match, so a new variant breaks the build). Per-profile pass epoch: a search after a finished reindex, activation or backfill waits for the deferred ANN rebuild instead of answering from the pre-pass index, and an index whose rebuild has burnt its five attempts errors — the hybrid path degrades to lexical plus a "semantic retrieval failed" note — rather than silently serving stale results. Settling is per profile. A backfill with nothing missing starts no pass, so the idle loop stops flipping `ready → indexing → ready` and stops clearing a recorded `last_error`. The ~1 %/run fielded-search flake root-caused to hnsw_rs recall (an 8-point graph fails to return every point 7–8 % of the time even at `ef` 200) and fixed in the fixture. |
| #201 | Compose flush bug, two flake fixes, web audit debt, doc fixes | **A real product bug, not a flake:** `write_private_async` handed the write to tokio's blocking pool and dropped the `File` without waiting, so `create_draft_file_async` could return a path whose file was still truncated — compose reads the draft straight back, so the web bridge answered a compose start with "Missing YAML frontmatter delimiters (---)" under load. That was the `mxr-web` compose-session flake (3 failures in 1,500 loaded runs before, 0 after). Activity recorder tests settle on a worker barrier instead of a 50 ms sleep (44 failures in 400 loaded runs → 0). `npm audit` cleared, lockfile-only. Docs: `from:` is whole-address. |
| #202 | Store id integrity, plus a progress denominator | `upsert_envelope_tx` returns the id it actually wrote; `apply_sync_upserts` takes the page by `&mut` and repoints envelope, body and attachments before the dependent writes, and the engine reads `upserted_message_ids` back off the page — so index entries, reply pairs, `NewMessages` and semantic ingest all name the same id as the rows (**D059**). Vacated threads now reported in `threads_changed`. Riding along: `SyncBatch::remaining_estimate` → `AccountSyncStatus.progress.total`, and derived curated-demo `ThreadId`s. |
| #203 + #206 | Daemon identity, and status that does not lie | The `ps` fallback adopted any process whose command line was `<exe file name> daemon --instance <instance>` and then signalled it — every checkout builds a binary called `mxr`, and `mxr demo` pins one instance name across profiles, so two checkouts restarted each other's daemons. Identity is now proven before a signal (see **D060**). `Status.degraded`, `FeatureHealthReport.search`, and `doctor --check` exiting on a report that is healthy only when nothing in it is an Error. |
| #205 | Hotfix | `tokio::time::timeout` polls its future before enforcing the deadline, so the degraded-status test's zero budget against a real in-memory snapshot was a race — it held for 15 local runs, then failed twice in a row on CI and blocked every PR. `get_status_within_budget` now takes the snapshot future, so the test decides whether it resolves at all. |

CI at the end of the wave: `demo_cli` runs in the workspace nextest jobs
(bounded by `.config/nextest.toml` at five minutes, since its waits are
progress-based and carry no short deadline), `demo-e2e.yml` stays as the
manual Linux run at the shipping count, `npm audit` is clear, and the two
long-standing flaky tests are fixed at the root (compose 3/1,500 → 0/1,500,
activity 44/400 → 0/400, both under load).

### Follow-ups still open after this wave
- Partial `from:`/`to:` matching product-wide would need a tokenised mirror field alongside the exact one and an index-version bump. Until then the whole-address rule is the documented contract.
- `mxr.skill` packaging is still manual (wants a build script plus a CI diff check).
- `install_profile` still writes `Ready` / clears `last_error` on the ingest and search paths, so a status can be overwritten there.
- hnsw memory beyond streaming: mmap points are rejected (above); `max_nb_connection` 16 → 8 is the remaining lever and needs a recall measurement on real embeddings first.
- Demo unread count is constant at 71 regardless of `--messages`, and `label:newsletters older_than:30d` is empty below roughly 50k.
- Vacated-thread ids are lost if a later chunk fails mid-page. Exceptional path; clients repair on `SyncCompleted`.
- `ps`-text environment parsing still truncates a value that itself contains ` IDENT=`, and a hand-started daemon with spaces in its exe path and no pid file is unadoptable. Both fail toward not killing anything.
- CI: `npm audit` runs before the web tests in one job, so one gate can mask the other. `.config/nextest.toml` needed `git add -f` (globally gitignored on this machine).
- Orphan file `crates/sync/src/test_support.rs` — not declared in `lib.rs` since 6ccd3a81.
- Re-running the demo surface seeder prints `UNIQUE constraint failed: drafts.id` noise: the idempotency check string-matches "exist".
- Load-sensitive tests to watch: `async_mutation_job_reports_progress_and_undo_ids_for_large_batch` (20 s budget) and `mxr-transport uds::stop_accepting_refuses_new_connections_but_defers_unlink` (pre-existing tracked flake).

## Learnings worth keeping (full ledger in the session; the durable ones)
- Two demo daemons from same-named binaries kill each other (pid fallback by exe name + instance) → unique binary names; never run a bare `mxr <cmd>` against a demo socket without the demo env; isolate with HOME (macOS) / XDG (Linux) + a short `MXR_SOCKET_PATH`; tests must write the config to BOTH roots.
- `Store::in_memory()` aliases reader+writer → cannot detect "reader can't see uncommitted rows"; use a file-backed store for those tests.
- Second-granularity "changed since before" checks break silently when the operation gets faster than the resolution (the 60 s `sync_interval` stall).
- "Run once at the end of a backfill" must fire on the `has_more` true→false edge — the last page can be empty.
- `Framed::split()` defers encoding to flush; use `tokio::io::split` + `FramedWrite` to tell encode errors from transport errors.
- `.config/` is globally gitignored on BK's machine (`git add -f` for nextest config).
- Prove every new regression test fails on the old code; at a ~1 % flake rate you need ~400 runs per commit to attribute it.
- Measure the thing, not the measurer (a histogram probe inflated the RSS it measured); two RSS points give the marginal cost, one gives nothing; a drop probe separates live structure from allocator drift.

## Codex retro review (gpt-5.6-sol, 2026-08-20, range 532c1ed1..141ebbbb)

Ran after the quota reset, per the review-gate plan. 12 findings (3 high / 6 medium / 3 low); no data-loss path. Orchestrator verification verdicts inline. Areas Codex checked and found clean: background-sync claim/join locking, fake-provider paging + `remaining_estimate`, stored-id retargeting, the NewMessages cap, chunked lexical commits, the compose flush, protocol additivity, preview/dry-run parity, provider boundaries.

| # | Sev | Finding | Verdict |
|---|---|---|---|
| 1 | High | `StallWatch` is only consulted on the `observe` path; the reconnect/degraded/transport-error branches `continue` past it, so a perpetually failing status poll waits forever instead of dying at 600 s (`demo.rs:644`, `:711`) | CONFIRMED (mechanism read); narrow trigger |
| 2 | High | Identity-sidecar writes ignore errors (`server.rs:822` `let _ = fs::write`); with no record, the pid-file path falls back to the weak argv check and skips the profile recheck before signalling — a recycled pid pointing at *another profile's* mxr daemon can be killed | PARTIALLY CONFIRMED (silent write + weak fallback are real; kill needs pid recycling into another mxr daemon) |
| 3 | High | No lifecycle lock around recovery: a second CLI that observed the same broken socket can unlink the pid file/socket the first CLI's replacement daemon just wrote (`server.rs:1337`) | PLAUSIBLE, not independently verified |
| 4 | Med | `install_profile()` on the search path persists `Ready`/clears `last_error` mid-pass; startup reclaim only sees rows still marked `Indexing` | KNOWN (already in the open list) + new reclaim detail |
| 5 | Med | `BackgroundSyncClaim::Drop` spawns an unconditional async `sync_in_progress=false` with no run-generation check; a delayed cleanup can clear a *newer* sync's flag (`state.rs:575`) | CONFIRMED as a race window (needs panic + instant retrigger) |
| 6 | Med | Vacated-thread tombstones lost when a later 500-row chunk fails mid-page | KNOWN (already in the open list) |
| 7 | Med | Deferred-ANN dirty marker removed before a fallible read and not restored on error; after 5 failed builds it is dropped without epoch advance → stale index served as `Ready` | PLAUSIBLE, not independently verified |
| 8 | Med | ANN over-fetch (`max(64, limit×4)`) still does not *guarantee* a requested source kind appears in the window | TRUE BY DESIGN — heuristic, documented; a guarantee needs kind-partitioned retrieval |
| 9 | Med | Degraded Status is wire-additive but behaviorally incompatible: pre-0.6.25 clients read a degraded (empty) snapshot as authoritative idle | TRUE — inherent to the additive approach; mitigations are all worse (erroring breaks more) |
| 10 | Low | `classify_health` ignores `degraded`: JSON can emit `degraded:true` + `health_class:"healthy"` (`status.rs:177`) | CONFIRMED |
| 11 | Low | `NewMessages.total` defaults to 0 for events from older daemons and the renderer prints it verbatim: `new_messages=0 shown=3` (`events.rs:58`) | CONFIRMED |
| 12 | Low | Both process probes split the command line on whitespace, so an exe path with spaces breaks discovery | KNOWN (open list; fails toward not killing) |

Fix wave pending a scope decision; candidates in order: 1, 2 (log the write failure + require the profile recheck when no record exists), 5 (generation stamp), 10, 11, 7, 3. Full report: session scratch `codex-retro.md`.
