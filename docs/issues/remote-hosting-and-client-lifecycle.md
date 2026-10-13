# Remote hosting needs saved connections and explicit client lifecycles

Status: F01 done in v0.6.94; R1 building. Remote mailbox access remains unshipped. Updated 2026-10-13.

Canonical contract: [remote hosting and clients](../blueprint/24-remote-hosting-and-clients.md). This follows the completed daemon portability preparation and owns its deferred R01 work. The previous 13-stage release sequence remains complete.

Implement `mxr serve` on a Mac or Linux host and `mxr connect` on client installations. One daemon owns SQLite, managed files, indexes and provider work. Clients share a saved target, register with the host, receive lifecycle events, reconnect through an authenticated handshake and deregister when deliberately switching away.

## F01 is released and its installed journey passes

[v0.6.94](https://github.com/planetaryescape/mxr/releases/tag/v0.6.94) ships the local pairing-authority foundation from `b2855177a0fc65b3e8e872af109ac8de43110dfe`, through [PR 443](https://github.com/planetaryescape/mxr/pull/443) and release [PR 444](https://github.com/planetaryescape/mxr/pull/444). Claude Opus 5.5 found no blockers in the complete foundation at `ab35c25c`; the orchestrator reviewed the subsequent small corrections through `a8b79cf1`.

Both published platform binaries passed all 32 checks in the isolated CLI/IPC fixture: approval, revocation, retries and restart. Store tests with an injected clock cover expiry. Archive checksums, separate installer and Homebrew bytes matched. The [release workflow](https://github.com/planetaryescape/mxr/actions/runs/38306908031) passed, and the public [pairing guide](https://mxr.sh/guides/pairing-authority/), CLI reference and automation contract passed production browser checks. The remaining retry-passing CI observations are tracked in [the browser issue](browser-retry-observations.md).

The unreleased R1 candidate at `5cd688d2` uses a dedicated router, an explicit read-operation allowlist and per-operation registration checks. It strips host attachment paths from body responses and keeps host administration local. Its writer uses WAL with `synchronous=FULL` and `fullfsync=ON`; effective-pragmas tests pass. A small synthetic bulk-write comparison has overlapping sample ranges and is not a general performance guarantee. Review also caught and fixed restoring a schema-83 backup before migration: the retirement table introduced in schema 84 is optional at that point. Full private HTTPS installed acceptance, independent review and release remain outstanding.

Source baseline for the specification: `origin/main` at `25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb`. The blueprint links exact code evidence and the official Tailscale references. The local pairing commands described above are released; the remaining proposed commands and wire shapes still require implementation.

## Acceptance requires two clients and one host

Use isolated client homes, a controlled provider and an actual browser. Prove read/reply/attachment workflows, planned restart and unexpected disconnect, uncertain outcome recovery, online and offline switching, eventual deregistration, browser child-session revocation and no accidental use of another workspace. Run the final installed path through the supported Tailscale proxy and both OS supervision arrangements.

A tailnet fixture and supported Mac/Linux supervisor environments are required for later deployment acceptance. The Mac's Tailscale service was stopped at kickoff and has been left stopped. Production VPS provisioning, real mail migration and the native SwiftUI app remain separate work.

## The specification preserves the lifecycle decisions

- Persist registrations; expire presence separately.
- Notify before a controlled stop; establish readiness through a new authenticated handshake after reconnection.
- Deliberate switching revokes this installation and its child browser grants; ordinary disconnection does not.
- Offline switching succeeds locally and reports pending deregistration honestly.
- Keep pending mail and accepted outcomes with their original workspace.
- Supervise hosting with zero clients; respect explicit stop state.

The original specification was documentation-only. The authorized implementation now uses isolated fixture daemons and databases; personal mail, system services and Tailscale settings remain outside that fixture authority.

Specification validation on 2026-10-12: 72 relative links and 10 pinned source references resolved; `git diff --check` passed. The standalone HTML companion rendered at desktop and 390px mobile widths without page overflow; all five lifecycle walkthroughs produced their expected states. The browser reported no page errors, and the accessibility scan reported zero violations with contrast checks still requiring manual review. This was specification validation, not product acceptance.

## R1 must deliver the first remote mailbox

Execution follows [the vertical release ladder](../implementation/remote-hosting.md), starting with the explicitly labelled F01 pairing-authority foundation. The kickoff base was `c0a4b115`; F01 was rebased onto freshly fetched `origin/main` `bf66f451` before review. Durable worktree and ledger paths, model routing, deployed acceptance, website obligations and unresolved fixture choices are in that plan. F01 is complete. R1 starts from released `b2855177`; R2–R5 remain planned. Its next check is two independently paired clients reading one hosted mailbox and switching exactly one client away, including durable offline cleanup.
