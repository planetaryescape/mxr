# Remote hosting needs saved connections and explicit client lifecycles

Status: F01 shipped in v0.6.94; R1 shipped in v0.6.95. R2–R5 remain planned. Updated 2026-10-13.

Canonical contract: [remote hosting and clients](../blueprint/24-remote-hosting-and-clients.md). This follows the completed daemon portability preparation and owns its deferred R01 work. The previous 13-stage release sequence remains complete.

R1 ships foreground `mxr serve` and saved `mxr connect` targets for authenticated mailbox reads. One daemon owns SQLite, managed files, indexes and provider work. Replies and attachments, browser sessions, and client presence and shutdown lifecycle remain later stages; R5 covers unattended Mac/Linux hosting.

## F01 is released and its installed journey passes

[v0.6.94](https://github.com/planetaryescape/mxr/releases/tag/v0.6.94) ships the local pairing-authority foundation from `b2855177a0fc65b3e8e872af109ac8de43110dfe`, through [PR 443](https://github.com/planetaryescape/mxr/pull/443) and release [PR 444](https://github.com/planetaryescape/mxr/pull/444). Claude Opus 5.5 found no blockers in the complete foundation at `ab35c25c`; the orchestrator reviewed the subsequent small corrections through `a8b79cf1`.

Both published platform binaries passed all 32 checks in the isolated CLI/IPC fixture: approval, revocation, retries and restart. Store tests with an injected clock cover expiry. Archive checksums, separate installer and Homebrew bytes matched. The [release workflow](https://github.com/planetaryescape/mxr/actions/runs/38306908031) passed, and the public [pairing guide](https://mxr.sh/guides/pairing-authority/), CLI reference and automation contract passed production browser checks. The remaining retry-passing CI observations are tracked in [the browser issue](browser-retry-observations.md).

The R1 implementation, release and installed HTTPS acceptance are recorded in the [release ladder](../implementation/remote-hosting.md#r1-shipped-in-v0695-and-passes-installed-https-acceptance).

Source baseline for the original specification: `origin/main` at `25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb`. The blueprint links the design evidence and official Tailscale references. F01's local pairing-authority commands shipped in v0.6.94; R1's foreground gateway, saved connections and read operations shipped in v0.6.95. R2–R5 remain proposed.

## Acceptance requires two clients and one host

Use isolated client homes, a controlled provider and an actual browser. Prove read/reply/attachment workflows, planned restart and unexpected disconnect, uncertain outcome recovery, online and offline switching, eventual deregistration, browser child-session revocation and no accidental use of another workspace. Run the final installed path through the supported Tailscale proxy and both OS supervision arrangements.

The R1 acceptance used the published Linux daemon on the approved VPS, serving two isolated native Mac client profiles over Tailscale HTTPS/WSS. It passed 88 checks; both original Serve configurations were restored, the Mac backend is stopped, and temporary runtime files were removed. Unattended Mac/Linux hosting remains authorized R5 work and still needs supported supervisor acceptance. Persistent deployment on BK's personal VPS, real mail migration and the native SwiftUI app remain separate work. Full receipts are in the [release ladder](../implementation/remote-hosting.md#r1-shipped-in-v0695-and-passes-installed-https-acceptance).

## The specification preserves the lifecycle decisions

- Persist registrations; expire presence separately.
- Notify before a controlled stop; establish readiness through a new authenticated handshake after reconnection.
- Deliberate switching revokes this installation and its child browser grants; ordinary disconnection does not.
- Offline switching succeeds locally and reports pending deregistration honestly.
- Keep pending mail and accepted outcomes with their original workspace.
- Supervise hosting with zero clients; respect explicit stop state.

The original specification was documentation-only. The authorized R1 fixture used synthetic mail and isolated client homes, temporarily ran the published Linux daemon on the VPS, and used Tailscale Serve HTTPS/WSS directly. It restored the original Serve configurations and removed the temporary runtime. Personal mail, persistent service setup and lasting tailnet changes remain outside that temporary test authority.

Specification validation on 2026-10-12: 72 relative links and 10 pinned source references resolved; `git diff --check` passed. The standalone HTML companion rendered at desktop and 390px mobile widths without page overflow; all five lifecycle walkthroughs produced their expected states. The browser reported no page errors, and the accessibility scan reported zero violations with contrast checks still requiring manual review. This was specification validation, not product acceptance.

## R1 shipped in v0.6.95

Execution follows [the vertical release ladder](../implementation/remote-hosting.md), starting with the F01 pairing-authority foundation. F01 shipped in v0.6.94 and R1 shipped in v0.6.95. R2–R5 remain planned. The ladder records the merged source, release artifacts, installed acceptance receipts and website deployment state.

## R4 must finish recovery after selection changes

Source: R1 candidate `102ccc378954523fcc1fcfccc3b94ba713b638b2`, reviewed on 2026-10-13 before merge. These follow-ups belong to the planned connection-lifecycle stage:

- A TUI retains its initial selection generation when reconnecting. `next_event` rejects a changed generation, but the idle reconnect path installs another connection with that same generation. End the old session explicitly instead of repeating that loop. See `crates/client/src/selected_daemon.rs:117`, `crates/client/src/lib.rs:838` and `crates/tui/src/ipc.rs:221`. R1 tells users to close and reopen clients after changing selection.
- The CLI retains an expired pairing request ID and keeps trying to claim it. Add an explicit way to cancel or restart a pending pairing while preserving another selected registration. See `crates/daemon/src/commands/connect.rs:108` and `crates/store/src/remote_authority.rs:208`. R1's `connect --local --confirm` cancels pending pairing and switches the current selection to local.
- Cleanup's retry interval is process-local, and CLI exit waits up to five seconds for its cleanup task. Persist retry scheduling so successive short commands share backoff; provide an explicit previewable way to forget unreachable cleanup intents. See `crates/client/src/remote_connection.rs:221`. Preserve cleanup authority isolation and pending intent after unacknowledged requests.
- A restored host removes the registration needed to acknowledge its old cleanup credential. The forget-intent path must also cover permanently rejected registrations and invalid pairing intents persisted by an older client. See `crates/client/src/remote_connection.rs:186` and `crates/store/src/remote_authority.rs:347` at `f614f897`. Do not present a pending retry as proof that the old host still grants access.
- A selection change after a local request completes can replace its response with a selection error. Explain that the operation may have applied and retain the existing operation identity for recovery. See `crates/client/src/lib.rs:595`. Include this race in R2 send recovery and R4 switching acceptance.
