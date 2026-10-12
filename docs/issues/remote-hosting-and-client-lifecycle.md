# Remote hosting needs saved connections and explicit client lifecycles

Status: F01 independent review passed; final PR checks and release acceptance pending. No remote-hosting release yet. Updated 2026-10-12.

Canonical contract: [remote hosting and clients](../blueprint/24-remote-hosting-and-clients.md). This follows the completed daemon portability preparation and owns its deferred R01 work. The previous 13-stage release sequence remains complete.

Implement `mxr serve` on a Mac or Linux host and `mxr connect` on client installations. One daemon owns SQLite, managed files, indexes and provider work. Clients share a saved target, register with the host, receive lifecycle events, reconnect through an authenticated handshake and deregister when deliberately switching away.

## F01 has passed independent review and needs release acceptance

Claude Opus 5.5 reviewed the full foundation diff at `ab35c25c`, based on `origin/main` `bf66f451`, and found no blockers. The isolated CLI/IPC fixture passed 32 checks against that candidate binary. [PR 443](https://github.com/planetaryescape/mxr/pull/443) still needs the final ready-PR checks, release-please release, published-binary acceptance and deployed documentation check. The controlled fixture checks approval, revocation, retries and restart; store tests with an injected clock check expiry.

The subsequent R1 stage must inventory every remote HTTP and IPC operation, including indirect paths to host files, credentials, scripts and process control, before exposing a listener. Keep host authority outside forwarded requests and remove host filesystem paths from allowed mail responses. Resolve revocation durability across power loss before promising that an acknowledged revocation survives it; the current store uses WAL with `synchronous=NORMAL` (`crates/store/src/pool.rs`, reviewed at `ab35c25c`).

Source baseline for the specification: `origin/main` at `25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb`. The blueprint links exact code evidence and the official Tailscale references. Commands and proposed wire shapes are not installed features.

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

## Orchestration started

Execution follows [the vertical release ladder](../implementation/remote-hosting.md), starting with the explicitly labelled F01 pairing-authority foundation. The kickoff base was `c0a4b115`; F01 was rebased onto freshly fetched `origin/main` `bf66f451` before review. Durable worktree and ledger paths, model routing, deployed acceptance, website obligations and unresolved fixture choices are in that plan. No implementation stage is complete yet.
