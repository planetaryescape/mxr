# Daemon (outside the handlers)

Paths are relative to `crates/daemon/`.

### The index lock, not the socket probe, decides which daemon owns the runtime
**Chosen**: `run_daemon_with_overrides` takes the search-index write lock before it touches the socket file. A busy daemon whose socket probe times out still holds the lock, so the second start bails with the socket intact. `run_stdio` takes the same lock, so `--stdio` cannot collide with a socket daemon and fails with a clear stderr message.
**Rejected**: Removing a "stale" socket first, then failing on the lock. That orphaned the running daemon with no socket and wedged IPC for good.
**Where**: `src/server.rs`, `run_daemon_with_overrides`, `run_stdio`.

### Every post-bind exit runs one ordered teardown, and the socket is unlinked last
**Chosen**: The serving body runs in a guarded scope and teardown always follows. Order: stop accepting first (so new clients get connection-refused, not a hang), drain connections and tasks, send traces, then release each transport. `UdsListener::cleanup` removes the socket file only if it is still ours, and the pid file cleanup stays daemon-level for the same reason.
**Rejected**: Unlinking the socket early, or unconditionally. A successor spawned during the drain window may already have re-bound the path, and deleting it leaves a live, syncing daemon that no client can reach.
**Where**: `src/server.rs`, `run_daemon_with_overrides` (teardown block).

### Process handlers install after startup checks and binds succeed
**Chosen**: Stop handlers are polled by the accept loop, with no detached task, after all binds work. A partial bind cleans up the earlier listeners' sockets before failing. Only the lock holder initialises tracing, so a refused second start never opens an exporter. Initialisation is serialised with desired-state writes so a stop committed during a multi-minute start wins.
**Rejected**: Detached signal tasks (they can outlive an embedded daemon invocation).
**Where**: `src/server.rs`, `run_daemon_with_overrides`.

### Startup repairs what the previous daemon's death left behind, off the startup path
**Chosen**: At boot the daemon clears `sync_in_progress`, ends assistant turns and bulk jobs that died with the old process (bulk jobs keep the counts they had), retries owed deletion cleanups on a timer, and marks scheduled sends whose outcome was never recorded as `interrupted` after surfacing each so the user can check. Syncing itself starts in the background loops with no blocking initial sync.
**Rejected**: A blocking initial sync, and leaving stale rows (a bulk job "running" forever, a silent lost send).
**Where**: `src/server.rs`, `run_daemon_with_overrides`, `run_startup_maintenance`.

### Startup index maintenance repairs only the lexical index, and marks success after the rebuild
**Chosen**: The Tantivy index rebuilds from SQLite. The marker for reader-text changes is written only after a completed rebuild, so an interrupted rebuild runs again next start. Semantic chunks stay an optional layer outside this path. If sync fills a gap before maintenance runs, nothing is rebuilt. During the rebuild window searches are held off stale or short indexes, and the hold ends when the rebuild is done or finds nothing to do. Indexing never runs `render.html_command`, because a rebuild would spawn it once per HTML message.
**Rejected**: Writing the marker when the index opens, and semantic repair on the mandatory path.
**Where**: `src/server.rs`, `run_startup_maintenance`; `src/state.rs`, `AppState::new`; `src/reindex.rs`, `reindex_in_batches`.

### Reindex answers from SQLite while the index is wiped and refilled
**Chosen**: A guard marks the index incomplete for the whole rebuild; only a complete rebuild clears it. A rebuild that fails part way drops the guard unfinished, so search keeps answering from SQLite. The change sequence is taken before a page is read, so a live update that lands after that point is resolved by the worker, and a delete applied during an overlapping rebuild still wins.
**Rejected**: Letting search read the partially built index.
**Where**: `src/reindex.rs`, `reindex_in_batches`.

### The CLI never kills or unlinks on a pid it cannot still prove is the daemon
**Chosen**: Daemon identity is checked twice (verification, then a re-check just before SIGTERM, and again before SIGKILL after the graceful-exit wait), because the pid may be recycled in each window. A record that names another pid or profile rejects the pid file outright. With no record (an older daemon) the check falls back to argv. A `ps` that will not answer is "no evidence", not a rejection, in both phases. Same-second start times (`lstart` has one-second resolution) fall back on the rest of the evidence. A scan-sourced pid was identified by profile alone, so the profile must hold again: an unreadable environment rejects.
**Rejected**: Treating silence as rejection (it stranded live daemons: pid file cleared, successor spawned, successor dies on the index lock). Using the argv `daemon` subcommand check alone (a neighbouring profile's daemon passes it). Restarting one of two live daemons for a profile (it just picks a side).
**Where**: `src/server.rs`, `still_the_same_process`, `pid_file_verdict`, `live_daemon_pid`, `fallback_live_daemon_pid_without_pid_file`, `recover_broken_running_daemon`; `src/process_probe.rs`.

### The pre-signal re-check does not weigh the `--instance` name
**Chosen**: Verification may reject on the instance heuristic because a rejection falls through to the `ps` scan, which can adopt the daemon by canonical executable and environment. The re-check cannot: its "no" goes straight to clearing the pid file and unlinking the socket of a daemon never signalled. `--instance` is a marker the daemon discards, so it is not worth that.
**Rejected**: One shared check for both phases (it made them disagree and one transient `ps` failure stranded a live daemon).
**Where**: `src/server.rs`, `still_the_same_process` and its tests.

### Recovery re-probes before it blames a losing successor
**Chosen**: If another client's replacement binds the socket first, the successor spawned here loses the index lock and exits fast. Before reporting failure, recovery re-probes, because losing that race proves the replacement is alive. A late recovery must not unlink the files a live replacement wrote. A daemon answering on our socket is never restarted, whichever pid the earlier scan found. Someone else's replacement is judged by the same live-socket check as every other path.
**Rejected**: Reporting the successor's exit as a failure (it sends the user to debug a daemon that is running).
**Where**: `src/server.rs`, `recover_broken_running_daemon`.

### Restart and maintenance shutdown wait for the old process, not only its socket
**Chosen**: The old pid is captured before shutdown clears the pid file. The wait has two phases: socket gone (the daemon unlinks it at the very end of shutdown), then process gone for up to drain 5s plus runtime tasks 5s. Spawning the successor inside that window loses a race against the old daemon's cleanup. `reset --hard` relies on the daemon being fully gone before it mutates shared state.
**Rejected**: Treating socket-gone as done (it caused the `reset_cli` flake).
**Where**: `src/server.rs`, `restart_daemon_process`, `shutdown_daemon_for_maintenance`.

### Autostart waits past the normal window while the child lives
**Chosen**: A dead child fails fast with the log tail. A live one keeps getting time, because startup after an upgrade runs migrations, WAL recovery or an index rebuild before the socket binds. Autostarted daemons detach so SIGHUP from a shell does not stop them.
**Rejected**: Declaring failure at the window, which tempts the user or a wrapper into a second daemon against a half-migrated store.
**Where**: `src/server.rs`, `spawn_daemon_process`.

### Local lifecycle management applies only to local daemon targets
**Chosen**: Autostart, stale-socket probes and pid-file recovery run only for the default local socket. `tcp://` and `cmd://` values in `MXR_DAEMON_ADDR` and saved paired remotes own their reachability, so the daemon's own bind falls back to the default socket path and the TUI never probes the local socket for them. A degraded status answered by a live ping is enough to launch the TUI. Destructive `reset` probes the same socket the daemon binds, and fails on a malformed `MXR_DAEMON_ADDR`.
**Rejected**: Probing the bare per-instance path (reset could call a live daemon stopped and delete its state). Spawning or ping-blocking on remote targets.
**Where**: `src/server.rs`, `ensure_daemon_running`, `ensure_daemon_supports_tui`; `src/commands/reset.rs`, `discover_reset_context`; `src/commands/doctor.rs`.

### Account repair talks only to a local daemon, otherwise writes disk directly
**Chosen**: `repair_account` probes only the local unix socket. A running local daemon is the secrets store's single writer, and doing the write there also reloads its cached credential. With no local daemon, the in-process disk-first write runs.
**Rejected**: Going through the `MXR_DAEMON_ADDR` connector (a `tcp://` value would ship local credentials to a remote daemon, a `cmd://` value would spawn a process).
**Where**: `src/commands/accounts.rs`, `repair_account`; test in `tests/integration/accounts_repair_cli.rs`.

### Credentials resolve lazily and disk-first, and a bad one degrades one account
**Chosen**: IMAP and SMTP secrets resolve at connect, sync or send time, never at construction. A bad or unreadable secret makes that account unhealthy, and boot continues. Disk is read first. A keychain hit mirrors to disk and an empty keychain value counts as absent. `MXR_KEYCHAIN=off` makes disk the only source and a miss final.
**Rejected**: An eager keychain read at boot (it bricked the whole daemon over one account). Mirroring an empty secret to disk.
**Where**: `src/provider_credentials.rs`, `imap_config_with_credentials`, `disk_first_password_with`; `src/state.rs`, `create_providers_from_config`.

### Sync passes: one claim per account, a detached pass keeps its entry, a pass wakes its loop
**Chosen**: A handed-off claim keeps its entry (and generation) while the detached pass runs, so an older claim's cleanup cannot clear `sync_in_progress` on a live sync. The generation is stamped under the claim lock. A pass nobody finalised (panic or shutdown) clears the flag best-effort, with the startup reconciler as the fallback. A caller that was waiting is told its sync outlived the wait but stays alive and is not flagged as an error. A blocking caller reports the detach itself so the reaper does not emit a second terminal event. The reaper joins the aborted task before releasing the provider guard, because `abort` only lands at an await point. A finished pass wakes the sync loop with `notify_one` so the account does not wait a full interval at `sync_in_progress`. The last error stays until a pass succeeds.
**Rejected**: Dropping the entry on handoff; releasing the guard on abort without the join (two syncs write at once).
**Where**: `src/state.rs`, `claim_background_sync`, `Drop`; `src/loops.rs`, `begin_sync_pass`, `reap_detached_sync`, `finalize_sync_pass`, `run_sync_pass`.

### An account stays busy while a backfill has more pages or a background sync is queued
**Chosen**: Status reads busy for a batch with `has_more` and for an acked background sync still waiting on the provider lock. Both events `SyncError` and `OperationFailed` are always sent, since a client may watch only one. Every failure records when the next try happens.
**Rejected**: Reporting idle at the end of one pass (`mxr demo` and `mxr sync --wait` stop short).
**Where**: `src/loops.rs`, `finalize_sync_pass`, `sync_loop_for_account`.

### Whole-table repairs run once per backfill, on the backfill's last page, not per page
**Chosen**: The analytics repair and the contacts handover happen on the edge where the backfill ends, outside the "page carried messages" branch, because providers end a backfill with an empty final page (Gmail after a `nextPageToken`, IMAP when the last UID chunk is all deleted). Contacts collect into a deferred set per account. Post-sync work for deleted mail also runs outside `count > 0`. Idle ticks that sync and owe nothing do not rescan.
**Rejected**: Per-page repairs (a query storm), and gating on upserts (it left deleted mail's semantic data, attachments and contacts behind).
**Where**: `src/loops.rs`, `finalize_sync_pass`.

### The post-sync fan-out runs on a detached task and starts no automatic summaries
**Chosen**: Semantic ingest, contacts, relationship profile, rules and analytics run off the sync loop. Each worker has a bounded channel. Summaries are generated only when the user opens a thread. The envelope slice for notifications is taken before the query and sorted newest first.
**Rejected**: Inline fan-out (10+ minutes between "Gmail has new mail" and "mxr shows it") and automatic summary backfill (unbounded tasks saturated the runtime on a 100k-message backfill).
**Where**: `src/loops.rs`, `finalize_sync_pass`, `post_sync_classification`.

### The IDLE watcher snapshots the reload generation before it reads the provider
**Chosen**: `idle_loop_for_account` marks the reload receiver first, then reads the provider, then re-fetches if a reload landed in between. `reload_accounts_from_disk` swaps the provider and then bumps the generation, so a bump seen after marking happens-after the swap. A test seam injects a reload into the read window.
**Rejected**: Marking after the read (a concurrent reload could install a fresh provider while the watcher opens on the stale one and never notices, the stale-credential bug the loop exists to fix). The sync loop re-fetches each cycle and tolerates at most one stale cycle.
**Where**: `src/loops.rs`, `idle_loop_for_account`, `sync_loop_for_account`; `src/state.rs`, provider reload.

### Scheduled sends clear `send_at` and record an attempt marker in one transaction before sending
**Chosen**: At-most-once. A crashed attempt cannot re-fire, and a crash mid-send leaves an unresolved marker that startup reports. The claim is held from claim to outcome so edits wait. A failed send keeps the draft because the error carries a single-use override token and the resend command. Timers fail closed: a reminder never fires on a pass whose reply check failed.
**Rejected**: Clearing after the send (a retry could send twice).
**Where**: `src/loops.rs`, `process_due_scheduled_sends`, `process_due_timers_with_gap`; `src/server.rs`, startup maintenance.

### Rule fan-out yields between messages and gives each execution its own mutation id
**Chosen**: A yield between messages keeps the single writer connection free for sync, snooze wake and activity writes. A per-execution `mutation_id` lets a re-run (for example after a restart mid-batch) dedupe inside the 24h window.
**Rejected**: One batch holding the writer; one id per rule.
**Where**: `src/loops.rs`, `apply_rules_to_messages`.

### Background loops subscribe to shutdown before they spawn, and some are fire-and-forget
**Chosen**: A receiver created inside the task can miss a shutdown sent just before the first run, so loops subscribe first. Activity prune, history prune, mutation dedup and undo prune, and view refresh are not joined at shutdown: losing the last pass is harmless. Mutation dedup rows expire at 24h, so they prune hourly together with the undo log and `PRAGMA optimize`.
**Rejected**: Joining every loop during shutdown.
**Where**: `src/loops.rs`, `spawn_history_prune_loop`, maintenance loops; `src/llm_background.rs`, `spawn`; `src/server.rs`.

### Deleting mail records an owed cleanup and runs the fan-out off the delete path
**Chosen**: The delete commits first. Vector forget is bounded because sync waits on the same lock, and a timed-out forget is not retried: vectors go at the next index rebuild. A reused id arriving through another path does not clear the owed cleanup.
**Rejected**: Holding the lock across the batch, which stalls sync mid-write.
**Where**: `src/message_deletion.rs`; `src/server.rs`, startup retry.

### Demo seeding is judged by the store's own count and the account's progress, not one reading
**Chosen**: The wait keeps the last usable reading as its fingerprint. A failed poll feeds that reading back, so every non-advancing shape (all failing, alternating, two failure modes) hits the same stall clock. A daemon error stops the wait, a transport failure reconnects, a degraded snapshot waits without reconnecting. Failure is checked before completion, because a failed pass clears `sync_in_progress` and `last_synced_count` keeps the last good page. The stored message count is the finish line when the sync lands inside the same second as the startup sync. Account progress inside a page counts as movement.
**Rejected**: Trusting `sync_in_progress` after a transport error, resetting the clock on failures, using `last_synced_count` across reruns.
**Where**: `src/commands/demo.rs`, `wait_for_demo_sync`, `wait_for_seeded_count`, `sync_demo_account`.

### `mxr sync --wait` compares errors against a baseline taken before the trigger
**Chosen**: The daemon acks at once, so a failed sync leaves an idle account with an error that looks like an old one. The pre-trigger error is the baseline. `None` means no reading, never an empty baseline, which would make every old error look new. Polling continues through stalled or degraded status reads, and the progress line prints only when the text changes.
**Rejected**: Judging failure by `last_error` alone.
**Where**: `src/commands/sync_cmd.rs`, `run`.

### The demo runs beside the real profile and leaves a marker before the slow prewarms
**Chosen**: Demo config moves default bridge ports. The sticky marker lives outside the demo data dir and is written before prewarms, so a Ctrl-C during a long prewarm still leaves a usable demo. The semantic reindex runs on its own connection with `force: false`, watched by counters, and a rejection is reported at once.
**Rejected**: Marker after prewarm; blocking on a half-hour reindex call.
**Where**: `src/commands/demo.rs`, `run`, `ensure_demo_config`, `prewarm_demo_runtime`, `await_semantic_prewarm`.

### Long-running CLI requests use the no-deadline path
**Chosen**: Deliveries backfill, draft eval, doctor and semantic rebuilds, and compose assist call `request_with_events`, which has no timeout, because each can run for minutes and exceeds the 120s cap (#179). Progress goes to stderr so stdout stays clean. An older daemon rejects the new request instead of replaying mail.
**Rejected**: The default IPC timeout.
**Where**: `src/commands/deliveries.rs`, `draft.rs`, `doctor.rs`, `semantic.rs`, `compose_assist.rs`.

### `mxr daemon` byte pipe: every exit path terminates the process explicitly
**Chosen**: Upstream (stdin to socket) and downstream (socket to stdout) run separately. Downstream is the lifetime anchor and its errors are not normalised; upstream socket errors after the daemon left are. When the daemon closes, the result returns without waiting on upstream, whose stdin read is parked in an uncancellable blocking read. After piping starts there is no `?`, because a normal return would hang runtime shutdown.
**Rejected**: A single joined pump (hangs on the stdin read).
**Where**: `src/commands/daemon.rs`, `run`, `pump_upstream`.

### Concurrent writers: the CLI holds the config lock from read to write
**Chosen**: The running daemon also saves config, so the CLI holds the file lock across read and write. In-process writers come first, then the file lock, always in that order. A fallback `mxr` cannot use is left out so the rest of `[llm]` (including turning AI off) still applies, and `mxr llm status` explains.
**Rejected**: Last-writer-wins; failing the whole change over one unusable fallback.
**Where**: `src/commands/config.rs`, `run`, `apply_llm_change_to_running_daemon`; `src/state.rs`, config update and `sync_llm_from_disk`.

### Wire-protocol framing: separate halves, queued events first, canned replies for what cannot be framed
**Chosen**: `serve_client_connection` uses framed read and write halves, not `Framed::split()`, because `SplitSink` defers `encode` to the next flush and would put encode failures back on the flush path. Events queued before a response go out before it (this arm outranks the event arm), so a CLI that stops at the response sees them. An unframeable response (usually past the codec cap) gets an error reply, not a hang-up. An event too big for a frame is dropped and the client resyncs, since dropping the connection also loses the in-flight response. The first frame a lagged client sees is the resync signal.
**Rejected**: `Framed::split()`; closing the connection on an oversized frame.
**Where**: `src/serve.rs`, `serve_client_connection`, `send_frame`.

### Token gate sends inline; requests route to a bulk or hot lane
**Chosen**: Pre-auth frames go inline on the sink so `Authenticated` always precedes any event. The gate arm runs only while unauthenticated. A declared identity is fixed after authentication. Slow work (LLM, downloads, rebuilds) uses a bounded bulk pool and everything else the hot pool, so a burst of LLM calls cannot starve fast commands.
**Rejected**: Spawning the gate reply; one shared pool.
**Where**: `src/serve.rs`, `serve_client_connection`; `src/serve/ipc_conformance.rs`.

### Bridge: loopback only, stable port, no managed bridge when the port is taken
**Chosen**: Daemon-hosted serving is loopback-only until remote bridge TLS, token distribution and Host/CORS policy are a validated product decision. If the preferred port is busy the daemon runs without its managed bridge and `mxr web` reports the conflict with process-owner diagnostics. A cached port that stops answering is cleared and the configured port is checked next. The web sign-in code is never printed in a URL.
**Rejected**: Binding non-loopback; picking a random port (the local URL must stay stable); printing the coded URL.
**Where**: `src/bridge.rs`, `spawn_bridge_loop`; `src/server.rs`; `src/commands/web.rs`.

### Activity recording keeps counts and ids, never content
**Chosen**: The mapper records kind and counts for compose, ids and counts for reading and highlights, and nothing for records (amounts, references, account numbers) until the catalog has an ids-and-counts shape. Previews are not activities. No wildcard arm, so a new `Request` variant breaks the daemon unit tests until it is logged or skipped on purpose. Pause drops entries silently. The cache evicts the lowest `ts`.
**Rejected**: Logging bodies, notes, quotes; a wildcard arm.
**Where**: `src/activity/mapper.rs`, `map_request`; `src/activity/mod.rs`, `worker_loop`; `src/activity/privacy.rs`.

### Logging never captures secrets, and logging setup cannot block config recovery
**Chosen**: async-imap and tungstenite are filtered out of both log sinks at TRACE because they log LOGIN passwords, message bodies and paired credentials. Tracing init does not load config strictly, so `mxr config` can open bad TOML. The logger's config read does not use `load_config` (its missing-file DEBUG event would re-enter the writer). Errors from parsing the LLM keys file never name its contents. Log reads do not initialise or rotate the files they read, and a purge does not hold the archive lock across a database wait.
**Rejected**: Default dependency log levels; recursive config loading in the writer.
**Where**: `src/lib.rs`, `safe_dependency_log`, `init_tracing`; `src/logging.rs`; `src/commands/logs.rs`; `src/llm_keys.rs`.

### Config-driven providers: setup first, then the address registry; every restart owes reconciliation
**Chosen**: The registry is built after provider setup inserts configured accounts, so the first sync sees primaries. It starts empty (direction reads Unknown). Repair debt lives in memory, so a restart marks every stored account as owing a reconciliation on its next pass. Config-only accounts get the colour and tag insertion will store, in key order, with configured tags reserved. A pass that cannot get a current address set classifies Unknown.
**Rejected**: Classifying from a stale address set; trusting in-memory debt across restarts.
**Where**: `src/state.rs`, `AppState::new`, `create_providers_from_config`, `reload_accounts_from_disk`; `src/loops.rs`, `run_sync_pass`; `src/account_identity.rs`.

### Status degrades honestly: no readings print unknown, not zero
**Chosen**: A degraded snapshot prints "unknown" where the daemon sent filler zeros, empties `sync_statuses` so the health class cannot read healthy, but still reports the version fields read without the database. A count with `total` falls back to the sample size for older daemons that never send `total`.
**Rejected**: Printing filler values with a contradicting note.
**Where**: `src/commands/status.rs`, `render_status`; `src/server.rs`; `src/commands/events.rs`, `render_event`.

### Mutations and compose: what the CLI refuses and what it only warns about
**Chosen**: An HTML body never opens `$EDITOR` (round-tripping it through markdown destroys the markup). An unresolved reference is a warning. `--draft` is computed locally as "not sending" instead of relying on clap's conflict. Draft files go to the 0700 scratch dir with 0600 perms. Selection de-duplicates threads keeping the newest message and over-fetches as a margin, capped. If `--first` and `--limit` both arrive, `--first` wins. Non-interactive confirmation refuses rather than guesses. Reply, forward and send previews carry a token so the daemon refuses a send if anything changed. Web sign-in codes stay out of URLs.
**Rejected**: Opening the editor on HTML; world-readable temp files; unbounded over-fetch.
**Where**: `src/commands/mutations/compose.rs`, `selection.rs`, `src/commands/messages.rs`, `src/commands/labels.rs`.

### CLI parsing: duration units are peeled by character
**Chosen**: `parse_duration_ms` and `parse_duration_secs` take the last character, not the last byte, so a multibyte trailing character fails to match instead of panicking at `split_at`. Units are ASCII.
**Rejected**: Byte slicing.
**Where**: `src/commands/activity.rs`, `parse_duration_ms`; `src/commands/history.rs`, `parse_duration_secs`.

### Remote hosting stop and setup survive crashes
**Chosen**: Setup persists the old receipt and the new intent before replacing a service file, so a crash on either side of the rename reconciles without adopting unrelated contents. A tagged daemon exiting during its own drain counts as a successful stop even if the final reply is lost. A hosting-only stop leaves local provider work running (launchd kills children in a stopped job's group). The daemon's socket and index lock stay the ownership arbiter.
**Rejected**: Replacing the file first; requiring the final Shutdown reply.
**Where**: `src/commands/serve/hosting.rs`, `setup`, `stop`; `src/commands/serve/runner.rs`.

### Test hygiene that encodes a decision
**Chosen**: The activity invariants test scans from `CARGO_MANIFEST_DIR` (the workspace root, since the `mxr` package manifest is the root `Cargo.toml`) and skips build output, `node_modules` and dot-directories, because `.claude/worktrees/` holds full checkouts that would count another branch's edits. Demo CLI tests use a uniquely named binary so a developer's real demo daemon is never adopted. The reset test waits for the process to exit, not the socket.
**Rejected**: `../..` from the manifest dir, and the stock `mxr` binary name.
**Where**: `tests/integration/activity_invariants.rs`, `workspace_root`, `walk`; `tests/integration/demo_cli.rs`; `tests/integration/cli_journey.rs`.

### Smaller choices worth a line
- `request_timeout` resolves once; `mxr demo` sets its env before any request goes out. Where: `src/ipc_client.rs`. A clean close from the daemon stays retryable rather than claiming a version mismatch.
- Shell hooks never fetch remote data to build a payload; a stale cache pointer or a directory is ignored. Where: `src/loops.rs`, `shell_hook_payload`.
- `looks_like_email` is deliberately loose (one `@`, a dot in the domain) to tell a message id from a sender, not to validate RFC 5322. Where: `src/commands/mutations/mod.rs`.
- Sync progress totals add the remaining count to what is stored, and reset on restart because the account re-pages from scratch. Where: `src/loops.rs`, `src/state.rs`.
- `hostile_filenames_stay_inside_target_dir` pins that a CLI attachment copy cannot escape `--dir`. Where: `src/commands/mutations/attachments.rs`.
