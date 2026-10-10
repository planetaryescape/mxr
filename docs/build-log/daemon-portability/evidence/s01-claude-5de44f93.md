I'll start with the manifest and the full diff, then read the complete source around the changed functions.

The diff is fully read. Now the complete source of the changed files and their surrounding dispatch/connection helpers.

Now the dispatch in `lib.rs` and the server-side connection helpers.

Now the transport crate: address resolution, the cmd connector, and its error types.

Now the shared client crate (`connect_with`, error types) and the TCP connector's `describe()`.

Next: freshness (the other `status` branch now skipping autostart), demo env handling, the binary entry point, and config path helpers.

The daemon-side `GetDoctorReport` is confirmed reused. Now the `dial-stdio` bridge path (it calls `ensure_daemon_running` on the target side) and demo env handling.

Checking the test registration and dev-dependencies the new integration test relies on.

Checking documentation and agent skills that consume these JSON shapes, since remote-target JSON is now nested.

Checking whether any caller pattern-matches `IpcClient::connect()` error text, since the cmd branch now replaces it.

Reading the committed test file directly to confirm it matches the patch (no `#[ignore]`, no hidden gating).

Checking workspace lint configuration, since the manifest says clippy (via the pre-PR gate) ran on an earlier commit, not the final head.

Checking the Unix connector's endpoint text (it surfaces in the explicit-Unix-target connect error the tests assert on).

Quick check on `bug-report` (another client-generated diagnostic surface) for how it treats `MXR_*` env values, to decide whether it's in or out of the redaction claim's reach.

Let me try to confirm the checked-out commit against the manifest head (read-only, inside the review directory).

`.git/HEAD` matches the manifest head. The packed `origin/main` ref differs from the manifest base, so checking for a loose ref that would override it.

HEAD matches the manifest. Let me pin exact line numbers in the new test file for the findings.

**GREEN: I found no blockers in S01 at head `5de44f93a8ae3cf8ff90d6a53738ea98bf2366db`.**

The head matches `.git/HEAD` in this directory. The tree `631da5f926a3f3f41dd1f020f568a9b3c6354ff2` is as the manifest states; I can't check a tree hash by reading files. Base is `80606f48…`. I read the full patch and the complete changed files. I also read the command dispatch in `lib.rs`, the connection helpers (`server.rs`, `ipc_client.rs`, `mxr-client`, transport `addr`/`cmd`/`tcp`/`uds`) and the daemon's own `collect_doctor_report`. I didn't run any tests; the results below come from the manifest. All paths are at `5de44f93`.

## Each part of the promise holds in the source

- **No autostart or fallback for a selected target.** Nothing before dispatch starts a daemon (`lib.rs#L37-L95`).
  - `status` skips `ensure_daemon_running` whenever `MXR_DAEMON_ADDR` is non-blank (`lib.rs#L857-L862`). That also covers `--watch` and `--freshness`.
  - `doctor` only calls it for the local default (`doctor.rs#L42-L51`). For a selected target it fetches that daemon's own `GetDoctorReport` (`doctor.rs#L305-L317`).
  - `collect_report` is the only code that probes the local socket or opens the local store and index, and it runs only for the local default.
  - The removed behaviour was a real hazard. For an explicit `unix://` target, autostart used to delete a stale selected socket, clear the client's pid file, and spawn a daemon with the client's profile at the selected path (`server.rs#L541-L568`, `#L1892-L1907`).
- **Watch.** Each 2-second iteration reconnects through `IpcClient::connect()` with no autostart. A failed reconnect ends the watch with the same error (`status.rs#L215-L290`).
- **Client paths are kept apart from daemon facts.**
  - `client_local.socket_path` now comes from `mxr_config::socket_path()`, not the selected-address resolver (`diagnostics.rs#L16-L25`).
  - Remote status nests under `daemon_target` and `client_local` (`status.rs#L58-L66`).
  - Remote doctor output nests under `daemon` (`doctor.rs#L319-L364`).
- **`--reindex` is rejected first.** The check comes straight after a side-effect-free env read, before any connect, autostart or delete, whatever flags come with it (`doctor.rs#L36-L40`).
- **Protocol and build differences.**
  - Remote `restart_required` depends on the protocol only (`status.rs#L237-L245`).
  - The remote notes never mention `mxr restart` or `doctor --reindex` (`#L180-L205`).
  - Doctor uses the daemon's own report and does no build comparison on the client side.
- **JSONL.** Every remote path prints one compact line (`doctor.rs#L334-L337`, `#L353-L364`, `status.rs#L69`). Local `--store-stats`/`--index-stats` in JSONL are now compact too, which is a fix. Errors go to stderr.
- **Redaction.**
  - The connection metadata shows `cmd://<command hidden>`.
  - Only `CmdConnector::describe()` holds the command's arguments, inside `TransportError::Connect` (`transport/src/cmd.rs#L55-L104`). Only `connect_with` produces that error, and `IpcClient::connect` replaces it with a new error for `cmd://` that doesn't carry the original along (`ipc_client.rs#L61-L74`).
  - Errors after connecting don't include the arguments.
  - TCP endpoints don't include the token, and a rejected token gets the daemon's fixed "invalid daemon token" (`serve.rs#L105`).
- **Reuse and compatibility.**
  - `GetStatus` and `GetDoctorReport` are unchanged, and no protocol types changed.
  - Local-default status keeps its top-level path fields with the same socket value (`AppState::socket_path()` delegates to `mxr_config::socket_path()`).
  - All four fixes the worker reported are present in the source and each has a test.

## Should-fix

### 1. A `cmd://` bridge that starts but can't reach the daemon gets a misleading protocol message
The new redacted message only covers a command that fails to start. For `cmd://`, the connection succeeds as soon as the child process starts, because there's no handshake (`client/src/lib.rs#L179-L194`).

So the most common real failure is the first request failing. That happens with something like `cmd://ssh -T badhost mxr daemon dial-stdio`. The user then sees "Connection closed. The running daemon may be using an incompatible protocol. Restart the daemon after upgrading." (`ipc_client.rs#L169-L171`), or "IPC error: Broken pipe".

Nothing leaks and nothing starts. But the message blames the protocol, and running `mxr restart` with a `cmd://` or `tcp://` target restarts the local default daemon (`server.rs#L586-L594` via the fallback at `#L479-L483`).

The source path is confirmed; which of the two messages appears depends on timing, which I haven't tested. Tests cover only a program that doesn't exist.

### 2. An explicit `unix://` pointing at your own daemon now loses local repair, with no hint why
"Selected" just means `MXR_DAEMON_ADDR` is non-blank (`diagnostics.rs#L2-L3`). Take a user who runs one local daemon via the documented `MXR_DAEMON_ADDR=unix:///run/mxr/mxr.sock` (`09-cli.md#L171`). They now lose three things:
- `doctor --reindex` is rejected.
- `doctor` can't diagnose the store offline when the daemon is down.
- `status` no longer autostarts.

That matches the stage's definition of "local default", so it isn't a contradiction. But the rejection message should say how to get local repair back: unset `MXR_DAEMON_ADDR` or use `MXR_SOCKET_PATH`. Alternatively, treat a `unix://` path equal to `mxr_config::socket_path()` as local.

The same message calls selected-daemon diagnostics "read-only" (`doctor.rs#L39`). That's not accurate: `--rebuild-analytics`, `--refresh-contacts`, `--recompute-link-counts`, `--reindex-semantic` and `--backfill-semantic` still change the selected daemon (`#L53-L234`).

### 3. The watch test has about 300 ms of headroom
The test requires two requests within 2.3 s (`selected_daemon_diagnostics.rs#L226-L239`), but the loop sleeps 2 s between them (`status.rs#L286`). The second request needs three processes to start (the client plus two bridges) while other tests run in parallel. It passed per the manifest; that it will flake under CI load is my inference. Poll until the count reaches 2, with a deadline of about 10 s.

### 4. No test pins the local-default status JSON shape
The only JSON test with `remote_target: false` doesn't check `data_dir`, `socket_path` or the absence of `daemon_target` (`status.rs#L300-L350`).

## Nits

- **Stale code and table order.**
  - The comment at `status.rs#L40-L41` is stale.
  - The local status table now prints `Socket:` first (`#L71-L117`).
- **Docs and the agent skill lag the change.**
  - The docs mention `--check`/`--store-stats` but not `--index-stats` (`09-cli.md#L181`, `observability.md#L34`).
  - The agent skill's `status`/`doctor` entries don't describe the `daemon` nesting, so an agent reading `.healthy` with a target set gets null (`.agents/skills/mxr/references/commands.md#L213-L217`, `#L697-L708`).
  - `01-architecture.md#L109` still describes autostart and doctor's socket probe for `unix://` targets.
- **The new test isn't isolated from `HOME`.** An active sticky-demo marker would switch it to the demo profile (`demo.rs#L74-L83`, `lib.rs#L53-L55`). The smoke script sets `HOME`; the test doesn't. The local fake daemon's request counter is also discarded (`…#L266`); asserting it stays at 0 would prove there's no fallback directly.
- **Committed docs include machine-local paths and an agent session ID** (`orchestration.md#L3`, `verifier-routing.json#L3-L5`, `plan.md#L7`). There are no secrets; whether that belongs in a public repo is your call. `ios-architecture.md#L118` is also out of date: `orchestration.md` now records Opus routing working.
- **Pre-existing, outside S01:** the daemon suggests `mxr daemon --restart` (`status_helpers.rs#L342`, `#L420`, `#L444`), which isn't a valid flag (`cli/mod.rs#L56-L78`). The remote doctor table now tells users to run its suggestions on the daemon host.

## Practical limits

- **Transports not run.** TCP and real off-machine SSH weren't exercised; I traced TCP in the source only.
- **A stopped remote daemon gets started by the bridge.** Over `cmd://ssh … dial-stdio`, the remote end calls `ensure_daemon_running()` (`commands/daemon.rs#L59-L64`). So status and doctor can't show "remote daemon down"; it gets started on its own host, not locally.
- **Every call still writes to the client.**
  - Each run, including a rejected `--reindex`, creates the client's `logs/mxr.log` (`lib.rs#L1595-L1601`).
  - The TCP path may `chmod` the client's token file (`config/resolve.rs#L184-L214`).
  - Both are client-local and were already there.
- **Commands outside S01.**
  - Other commands with an explicit `unix://` target still autostart with the client's profile; that's later P06b work.
  - Plain local `doctor --format jsonl` can still append "Next:" text (`doctor.rs#L289-L299`).
- **Exact-head checks.** The full test suite and clippy last ran on earlier source, so CI will be their first run on this exact head.
- **Base check before merge.** This clone's `refs/remotes/origin/main` is `dfb23d10…`, because it was cloned from a local worktree. Re-fetch GitHub `origin/main` to confirm the base before merging.

The blueprint, research note, delivery plan and smoke script are plans and don't change behaviour beyond diagnostics. The smoke script isolates `HOME` and only stops its own daemon instance.
