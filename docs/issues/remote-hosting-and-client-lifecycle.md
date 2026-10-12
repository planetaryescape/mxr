# Remote hosting needs saved connections and explicit client lifecycles

Status: specified, implementation not started. Updated 2026-10-12.

Canonical contract: [remote hosting and clients](../blueprint/24-remote-hosting-and-clients.md). This follows the completed daemon portability preparation and owns its deferred R01 work. The previous 13-stage release sequence remains complete.

Implement `mxr serve` on a Mac or Linux host and `mxr connect` on client installations. One daemon owns SQLite, managed files, indexes and provider work. Clients share a saved target, register with the host, receive lifecycle events, reconnect through an authenticated handshake and deregister when deliberately switching away.

## The next action is the contract and allowlist inventory

Start R01a against a freshly fetched `origin/main`. Inventory every remote HTTP and IPC operation, including indirect paths to host files, credentials, scripts and process control. Define registration/session identity, authenticated forwarding context, lifecycle messages, versioning and atomic selection/switch persistence before exposing a listener.

Source baseline for the specification: `origin/main` at `25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb`. The blueprint links exact code evidence and the official Tailscale references. Commands and proposed wire shapes are not installed features.

## Acceptance requires two clients and one host

Use isolated client homes, a controlled provider and an actual browser. Prove read/reply/attachment workflows, planned restart and unexpected disconnect, uncertain outcome recovery, online and offline switching, eventual deregistration, browser child-session revocation and no accidental use of another workspace. Run the final installed path through the supported Tailscale proxy and both OS supervision arrangements.

No implementation blocker has been observed because implementation has not started. A tailnet fixture and supported Mac/Linux supervisor environments are required for deployment acceptance. Production VPS provisioning, real mail migration and the native SwiftUI app remain separate work.

## The specification preserves the lifecycle decisions

- Persist registrations; expire presence separately.
- Notify before a controlled stop; establish readiness through a new authenticated handshake after reconnection.
- Deliberate switching revokes this installation and its child browser grants; ordinary disconnection does not.
- Offline switching succeeds locally and reports pending deregistration honestly.
- Keep pending mail and accepted outcomes with their original workspace.
- Supervise hosting with zero clients; respect explicit stop state.

Documentation-only handover: validate relative links and source anchors, inspect the HTML companion, and retain proposed status. No daemon, database, provider, system service or Tailscale setting should change while drafting this specification.

Specification validation on 2026-10-12: 72 relative links and 10 pinned source references resolved; `git diff --check` passed. The standalone HTML companion rendered at desktop and 390px mobile widths without page overflow; all five lifecycle walkthroughs produced their expected states. The browser reported no page errors, and the accessibility scan reported zero violations with contrast checks still requiring manual review. Product acceptance remains unexecuted. Resume at R01a, using the blueprint's source table and acceptance matrix.

## Orchestration started

Execution now follows [the vertical release ladder](../implementation/remote-hosting.md), starting with the explicitly labelled F01 pairing-authority foundation. Source base is origin/main c0a4b11553088e3c95326b2cb69d10fb8c1979fd. Durable worktree and ledger paths, model routing, deployed acceptance, website obligations and unresolved fixture choices are in that plan. No implementation stage is complete yet.
