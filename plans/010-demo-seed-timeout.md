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

## Follow-ups (real, not done here)
- Semantic `search()` asks HNSW for `limit` candidates and filters source kinds AFTER → small-limit fielded hybrid queries can return nothing; also the cause of the known flaky `execute_search_uses_dense_source_kinds_for_fielded_queries` (~1 %/run, pre-existing). Over-fetch before filtering.
- hnsw_rs memory ≈ 5.9 KB/vector (16 per-layer Vecs + per-neighbour Arcs + vector copy) ⇒ ~1.1 GB at the 50k demo; mmap-backed points (`PointData::S`) is the lever that scales.
- Semantic `use_profile` switching back to a profile with an in-memory index serves the older index until the deferred build lands (needs a per-profile pass epoch). After "semantic vectors ready" the idle-sync `backfill_active_limited(200)` briefly flips the profile to `indexing`.
- `upsert_envelope_tx` resolves by natural key and keeps the OLD row id while dependents write the new `envelope.id` (pre-0.4.52 rows on re-sync fail the page loudly now; fix = upsert returns the id it wrote).
- `ResponseData::Status` should carry an explicit `degraded: bool` (clients infer it from `accounts.is_empty()`).
- `SyncProgressData.total` is always `None` (no provider reports a batch total).
- Curated-50 demo dataset still uses random `ThreadId`s per regeneration.
- Pre-existing flakes: `mxr-web::compose_session_send_forwards_draft_account_id` (HTTP + fake IPC startup race), `activity::tests::rapid_fire_ephemeral_writes_coalesce_into_one_row_with_count` (under load).
- Web: `npm audit` fails the non-required `Web Unit Tests` job (12 advisories, `npm audit fix` available).
- README example `mxr search "from:alice is:unread"` returned 0 results on the seeded 50k demo — verify the dataset/query still match.
- Search worker death is logged but not surfaced in `mxr doctor`/feature health.
- Run Codex (gpt-5.6-sol) review over the merged commits when quota resets.

## Learnings worth keeping (full ledger in the session; the durable ones)
- Two demo daemons from same-named binaries kill each other (pid fallback by exe name + instance) → unique binary names; never run a bare `mxr <cmd>` against a demo socket without the demo env; isolate with HOME (macOS) / XDG (Linux) + a short `MXR_SOCKET_PATH`; tests must write the config to BOTH roots.
- `Store::in_memory()` aliases reader+writer → cannot detect "reader can't see uncommitted rows"; use a file-backed store for those tests.
- Second-granularity "changed since before" checks break silently when the operation gets faster than the resolution (the 60 s `sync_interval` stall).
- "Run once at the end of a backfill" must fire on the `has_more` true→false edge — the last page can be empty.
- `Framed::split()` defers encoding to flush; use `tokio::io::split` + `FramedWrite` to tell encode errors from transport errors.
- `.config/` is globally gitignored on BK's machine (`git add -f` for nextest config).
- Prove every new regression test fails on the old code; at a ~1 % flake rate you need ~400 runs per commit to attribute it.
- Measure the thing, not the measurer (a histogram probe inflated the RSS it measured); two RSS points give the marginal cost, one gives nothing; a drop probe separates live structure from allocator drift.
