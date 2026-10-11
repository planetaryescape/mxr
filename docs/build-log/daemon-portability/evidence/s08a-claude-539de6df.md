Not GREEN: there is one blocker. The real-binary smoke treats a relationship-worker shutdown timeout as a failure, but this candidate's own shutdown design produces exactly that timeout right after the fake sync. The other areas you asked me to attack hold up.

I reviewed head `539de6dfc774d4878a1614d1f39e469cb90ebf9a`, tree `734366fec18ef7f5f30db3f33c843dbe4f5087ce`, base and merge base `2c55a735e2b013910cb7222211fa8d6156d6204c` (from `REVIEW-MANIFEST.json`). I read the files in the review clone. Without shell access I couldn't confirm HEAD with git, but every hunk I compared from `FULL-DIFF.patch` matches the files on disk. I ran nothing.

## Blocker: the smoke's SIGTERM stop is expected to fail on the relationship worker's queue

**What the source shows:**
- **The first sync sends every message to the relationship worker.**
  - The sync loop syncs immediately at startup (`skip_sleep = true`, `loops.rs` L972).
  - The fake dataset is 55 messages in one page (`SYNC_PAGE_SIZE = 10_000`, `provider-fake/src/lib.rs` L34; count asserted at L790). So the first pass isn't treated as a backfill.
  - Because of that, the post-sync step hands every message ID to the relationship worker (`loops.rs` L1140-1147).
- **That gives the worker 28 contacts.** Contact extraction keeps every distinct inbound sender except the account's own addresses (`store/src/contact_style.rs` L145-163, L258-269). The fixture has 28 distinct senders, none of them the account address (`provider-fake/src/fixtures.rs` L141-623).
- **Shutdown waits behind all 28.** The relationship worker works through its whole pending queue, sleeping 250 ms per contact, before it reads the next command (`relationship/src/service.rs` L55-57, L87). A Shutdown sent right after the sync therefore waits at least 28 × 250 ms = 7 s.
- **The shutdown budget is 5 s.** `shutdown_runtime_tasks` gives all four service handshakes one shared 5 s deadline (`state.rs` L1587-1595, called from `server.rs` L415).
  - A missed handshake logs `service shutdown signal timed out` (`state.rs` L46).
  - Once the deadline has passed, every remaining task logs `runtime task drain timed out`, even tasks that already finished (`state.rs` L1602-1609).
- **The smoke rejects both strings.** It sends SIGTERM straight after `sync --wait`, one `search` and one Ping (`s08a_smoke.py` L272-275), and fails on either string (L200-207). Default foreground logging (`mxr=info` written to stdout, `lib.rs` L1588-1620) puts both WARN lines into the log the smoke reads.

**Estimated, not run:** `sync --wait` polls every 100 ms (`sync_cmd.rs` L320), so the gap from the first sync to SIGTERM is well under 1 s on a normal machine. The handshake only finishes inside the deadline if about 2 s pass first. The SIGINT stop after restart should be clean: the restarted daemon's sync returns nothing from a completed cursor (`provider-fake/src/lib.rs` L380-394), so no work is handed to the relationship worker.

**Why the Rust tests don't catch it:**
- The lifecycle test only asserts `SIGTERM received` and `daemon shutdown drain completed` (`daemon_lifecycle.rs` L547-551). The "drain completed" line is logged after timeouts too (`state.rs` L1630-1633).
- `runtime_shutdown_bounds_a_blocked_relationship_request` (`state.rs` L3007-3061) asserts timeout-then-abort as the correct outcome. That is the outcome the smoke rejects.

**Impact:** the required real-binary check fails on its first stop. No user data is at risk. The daemon still stops inside the bound and exits 0. What gets dropped is queued relationship analytics, and acknowledged mail stays in SQLite.

**Ways out (your call):**
- **Change the worker.** Make the relationship worker check for Shutdown between contacts, for example a biased `select!` of `rx.recv()` against the 250 ms pacing sleep. Queued contacts are dropped, which the abort already does today, but without the 5 s stall. As a side effect, callers enqueueing contacts would no longer wait behind the whole queue for their acknowledgement.
- **Change the smoke's rule.** Keep the bounded abort as the design and let the smoke allow a relationship handshake timeout, while connection-drain timeouts and service failures stay fatal.
- **Change the smoke's timing.** Have it wait for the relationship queue to empty before signalling. Nothing exposes that today, so in practice this becomes a sleep.

## Everything else I attacked holds

- **Signal registration and lifetime:** the handlers install after every bind and before the pid file is written (`server.rs` L200-209). If installation fails, the listeners are cleaned up and the services stopped; there's no pid file yet. Only the accept loop listens for signals, and `run_daemon_with_overrides` is called once per process (`lib.rs` L117).
- **Select cancellation:** tokio's `Signal::recv` is cancel-safe, and the helper's only await is its inner select. So when another branch (accept, join or IPC shutdown) wins, no signal is lost. A signal that arrives during the pid write or bridge startup is picked up when the loop first polls. The non-Unix version keeps one pinned `ctrl_c()` future across loop iterations, which is also safe.
- **Owned cleanup:** the signal branch drops into the existing teardown in order: stop accepting, drain connections, drain runtime tasks, remove the owned socket, clear the owned pid and identity files (`server.rs` L411-427). The serve result is `Ok`, so the exit code is 0.
- **Active IPC while stopping:** connections stop reading once shutdown is flagged, finish in-flight requests, then close (`serve.rs` L311-323, L469-471). Connections drain before the services stop, so in-flight requests still have working services.
- **The state.rs correction:** each `request_shutdown` is safe to cut off with a timeout. A cancelled send queues nothing; a cancelled wait only makes the worker's acknowledgement fail.
  - The base code never sent Shutdown to the relationship or contacts workers, so every shutdown spent its full 5 s waiting on the relationship worker. An idle daemon now stops promptly.
  - Search and semantic shutdown go from unbounded to bounded. Both hold rebuildable data: startup rebuilds the search index when its count doesn't match (`server.rs` L1947-1978), and semantic recovers interrupted passes on start (`semantic/src/service.rs` L170).
  - Contacts refresh answers Shutdown promptly unless a refresh is mid-run (`contacts_refresh_handle.rs` L53-68).
- **Repeated start:** the restarted process registers its own handlers. A start that overlaps a daemon still shutting down is refused by the search-index lock with the existing "Daemon already running" message.
- **Stdio mode:** shutdown on stdin EOF (`server.rs` L74-113) installs no signal handlers and uses the same drain, which is now quick when idle.
- **Existing code that sends SIGTERM:** broken-daemon recovery still escalates to SIGKILL after 5 s (`server.rs` L1499-1507), and the `cli_journey` test waits up to 12 s for exit (`cli_journey.rs` L1395-1407).
- **Tests:**
  - The three new state.rs tests are deterministic. The in-memory fixture queues no relationship work, and an abort closes the worker's channel, so the blocked test's polling loop ends.
  - The lifecycle test uses dependencies the crate already has. It clears `MXR_*` and `RUST_LOG` and overrides `HOME` and `XDG_CONFIG_HOME`.
  - The smoke's hand-built IPC frames match `IpcCodec`: a 4-byte big-endian length prefix and the `type`, `cmd`, `status` and `kind` tags.

## Nonblocking notes

- `state.rs` L1602-1609 logs `runtime task drain timed out` for tasks that already finished once the deadline has passed. One slow service therefore makes the log claim every loop hung. Warning only when `!is_finished()` would fix that.
- The lifecycle test's "ordered runtime drain should complete" assertion (`daemon_lifecycle.rs` L548-551) passes even when the drain timed out.
- tokio never restores the default signal behaviour, so a second SIGINT or SIGTERM during the drain is swallowed. The same applies while runtime shutdown waits on blocking work. Only SIGKILL cuts it short. That matches the new forced-stop sentence in the architecture doc, but pressing Ctrl-C twice won't force-quit.
- This one predates this change: a client that arrives while the daemon is stopping finds the socket refusing or missing. It then sends SIGTERM, waits 5 s, SIGKILLs the stopping daemon and starts an unsupervised replacement (`server.rs` L641-666, L1487-1547). The supervisor's own restart then fails with "Daemon already running".
- The `cfg(not(unix))` branch is never compiled. `nix` is an unconditional dependency (`Cargo.toml` L470) and `server.rs` imports it unconditionally (L25-27). That isn't a regression, but it shouldn't be cited as portability evidence.
- The architecture doc line names `--foreground`, but the handlers apply to every socket daemon, including the autostarted `mxr daemon`.

## Limits

The blocker's timing is an estimate from source; running the smoke settles it. Linux CI and the real-binary smoke are still outstanding. Blocking work, such as a search-index batch already in flight, can keep the process alive after its socket and pid file are gone. It holds the index lock until that work finishes or the supervisor force-stops it, as the architecture sentence says.
