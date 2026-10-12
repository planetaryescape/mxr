# Remote hosting keeps one daemon in charge of registered clients

Status: proposed implementation contract, 2026-10-12. Commands, routes, fields and timing defaults below are requirements to implement, not descriptions of released behaviour. Source baseline: [`origin/main` at `25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb`](https://github.com/planetaryescape/mxr/tree/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb). Implement after the completed [daemon portability preparation](23-daemon-portability.md); this expands its deferred R01 remote boundary. Track execution in [the remote hosting issue](../issues/remote-hosting-and-client-lifecycle.md).

Implement `mxr serve` on the computer that owns the mail workspace and `mxr connect` on each client computer. CLI, TUI, MCP and `mxr web` must honour the saved connection. The host knows which clients are registered, which sessions are active, and which operations it has accepted. Clients receive planned shutdown notices, reconnect after interruptions and deregister when deliberately switching away.

The completion check is two separate client homes using one hosted workspace through authenticated HTTPS: read, reply, attach, submit, lose a response, restart the host, recover the outcome, then switch one client to local while the other continues. Include an actual browser session. Repeat the switch while the host is unavailable and prove eventual deregistration without sending old work to the local daemon. Use controlled providers throughout.

## One host owns the database, index and provider work

```text
Mac CLI / TUI / MCP ---- HTTPS + WSS ----+
Browser web app ------- HTTPS + WSS ----+--> Tailscale Serve
Future SwiftUI app ---- HTTPS + WSS ----+       |
                                        remote gateway (loopback)
                                               |
                                          local Unix IPC
                                               |
                                          mxr daemon
                                               |
                                SQLite, managed files, search index
                                providers, sync, classification, sending
```

Keep SQLite and managed attachments on the daemon host. Rebuild search indexes there. Clients receive resources and outcomes through the daemon surface; they never open the server's database or index. A future SwiftUI app owns a local cache, drafts and pending intentions, with Swift networking. Building that app is outside this work.

Support a Mac host and a Linux VPS with the same connection contract. Select Tailscale-private HTTPS for the first release. A sleeping or disconnected Mac is unavailable; a VPS removes that dependency without promising uninterrupted service. Do not add Turso, database replication, gRPC, public internet hosting, APNs or a second provider-sync daemon.

Keep the HTTP gateway beside its daemon. `mxr web` on a remote client opens the selected server's web app; it does not need to run a local forwarding bridge. Local mode retains the local web flow.

## The existing seams cover transport and recovery, not hosting setup

All source links in this table are pinned to the baseline commit. Re-fetch before implementation and recheck these entry points.

| Verified baseline | Implementation consequence |
| --- | --- |
| [`TransportAddr::resolve`](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/crates/transport/src/addr.rs#L103) reads `MXR_DAEMON_ADDR`, otherwise uses a Unix socket. | Add a saved connection without breaking explicit transport overrides. |
| [`SelectedDaemonTarget`](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/crates/client/src/selected_daemon.rs#L80) bounds remote setup, checks protocol compatibility and refuses local fallback. | Extend this shared path; retain its failure semantics. |
| [`mxr web`](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/crates/daemon/src/commands/web.rs#L26) has separate remote-host routing; its local server uses a Unix socket. | Route normal web launching through the saved selection. |
| [`local_token_handshake`](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/crates/web/src/router.rs#L59) trusts a loopback peer when enabled. | A reverse proxy must reach a separate remote router with this endpoint absent. Proxy loopback is not caller authentication. |
| [`ClientKind`](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/crates/protocol/src/types.rs#L511) describes a surface such as CLI or web. | Add authenticated registration identity separately; a caller-supplied surface is not authority. |
| [`bridge_events`](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/crates/web/src/lib.rs#L2533) forwards events and handles application keepalive; the [browser client](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/apps/web/src/lib/ws.ts#L1) reconnects. | Extend the event lifecycle and authentication; do not build an unrelated notification service. |
| [`shutdown`](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/crates/daemon/src/handler/admin.rs#L49) requests shutdown immediately; the [server](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/crates/daemon/src/server.rs#L349) owns connection tasks and ordered teardown. | Put preview, admission control and lifecycle notification ahead of the existing teardown. |
| [IPC protocol version](https://github.com/planetaryescape/mxr/blob/25aebe7573c6f80f98c68260bf5dc4c3b5ac4ddb/crates/protocol/src/lib.rs#L39) is 12. | Version new wire contracts deliberately; do not infer compatibility from package versions. |

## Serving and connecting have separate command scopes

These examples specify the proposed UX. `--dry-run` is read-only and uses the same target resolution and validation as execution. Interactive mutations show the preview and ask for confirmation; noninteractive execution requires `--confirm`. All results and failures support `--format json`; event watching supports JSONL. Secrets never appear in normal structured output.

```sh
# On the host, preview before enabling persistent hosting.
mxr serve --tailscale --dry-run
mxr serve --tailscale --confirm
mxr serve status --format json

# On a client, inspect the target before pairing and saving it.
mxr connect https://mail.example-tailnet.ts.net --dry-run
mxr connect https://mail.example-tailnet.ts.net --name "MacBook"
mxr status --format json
mxr
mxr web

# Switch this client installation back to local.
mxr connect --local --dry-run
mxr connect --local --confirm

# On the host, inspect clients before disabling remote access.
mxr serve clients --format json
mxr serve stop --dry-run
mxr serve stop --confirm
```

| Command | Required behaviour |
| --- | --- |
| `mxr serve --tailscale [--https-port PORT]` | Always targets the current computer's explicit local profile, even if its saved client connection is remote. Reuses that profile's daemon, installs or updates its supervised hosting configuration and reports the endpoint. Repeating it is idempotent. |
| `mxr serve status` | Reports desired hosting state, daemon/gateway/service-manager health, endpoint, workspace identity, paired clients, active sessions, in-flight operations and setup drift. Works locally even when the daemon is stopped. |
| `mxr serve clients` | Lists local-host registrations and active sessions with names, surfaces and last-seen times. The full inventory requires host administration authority. |
| `mxr serve clients revoke CLIENT_ID` | Previews and revokes that registration, credentials and descendant browser sessions. Supports `--dry-run` and `--confirm`. Does not cancel accepted mail operations. |
| `mxr serve pair approve REQUEST_ID` | Local-host approval of the displayed client challenge and requested permissions. Supports preview/confirmation; SSH console access can perform this on a VPS. |
| `mxr serve stop` | Persists remote hosting as disabled, notifies remote clients, drains remote admission and removes only mxr's proxy configuration. Leaves local daemon/provider work running. Keeps registrations for a later explicit re-enable. |
| `mxr connect URL` | Validates HTTPS, server/workspace identity and compatibility, pairs when necessary, then atomically saves the selection. Repeating it for the current valid registration does not create another client. |
| `mxr connect --local` | Stops this installation's use of the previous server, deregisters it or records pending cleanup, then selects its local profile. Never copies mail or activates a restored workspace implicitly. |
| `mxr status`, `mxr doctor` | Honour the effective selection, identify its source and distinguish client-local diagnostics from server state. Unreachable remote state is an error, not permission to inspect a different local workspace. |

Extend the existing `mxr restart` with the same lifecycle preview and confirmation for a hosted local daemon. Add `mxr daemon stop` for a controlled full local stop, also with preview/confirmation. These host administration commands must not silently target a saved remote server. Full stop leaves the supervised daemon stopped until explicitly started; `mxr serve stop` only disables remote access. Document that distinction in CLI help and status.

## The saved connection applies to every client surface

Resolve ordinary daemon clients in this order: an existing explicit per-command transport option where supported, `MXR_DAEMON_ADDR`, saved connection, default local Unix socket. Show the winning source in status. An environment override is temporary and cannot rewrite, deregister or silently bypass a saved pairing. Retain existing Unix, SSH `cmd://` and loopback TCP support as explicit advanced transports.

`mxr connect` configures the supplied URL or local selection explicitly; it must detect an environment override that would mask the saved result. Report the conflict before mutation and ask the operator to remove it. `mxr web` must either honour the effective target with a known web origin or report that the transport has no web mapping. It must never open an unrelated local workspace. Keep `--remote-host` as an explicit legacy override with a visible target; do not claim that it creates a saved registration.

Store the selected origin, server ID, workspace identity/recovery generation, registration ID, credential reference and a monotonically changing local selection generation. Keep credentials in the platform's protected credential storage; use the repository's headless credential policy on Linux. Keep client selection outside portable daemon backups.

Structured connection status includes `effective_target`, `selection_source`, `selection_generation`, `registration_state`, `session_state` and pending deregistration count. Return typed errors for pairing required, revoked registration, incompatible protocol, changed workspace, draining service and conflicting selection override. Unavailability is retryable within the caller's deadline; authentication or identity failures require action. A completed switch exits successfully when its selection is committed, with a separate `deregistration: pending` field and human warning if remote cleanup remains. Do not label that response fully disconnected from the old server.

One desktop installation/profile shares its registration across CLI, TUI and MCP. Concurrent processes hold separate sessions. Long-lived clients notice selection-generation changes, stop old requests and show the new target. Preserve an open editor's draft under its original workspace. A client that cannot safely change in place asks to reopen; it must not keep submitting unnoticed.

Browser tabs belong to an origin. A tab cannot silently turn into a different server's app. `mxr web` opens the selected origin; a tab associated with the departed registration becomes disconnected and preserves its drafts. A browser launched through that installation receives a child grant, so deregistering the installation closes those browser sessions too. An independently paired browser or phone has its own registration and is unaffected.

Verification: run CLI, TUI, MCP and web with a saved remote selection, a conflicting environment override, an offline host and a concurrent target switch. Inspect both servers for unexpected requests or local daemon autostart.

## The gateway authenticates each registered client

Tailscale provides private reachability and HTTPS termination. Its [Serve documentation](https://tailscale.com/docs/features/tailscale-serve) describes tailnet access controls and the loopback proxy. mxr still authenticates callers with its own revocable credentials. Do not authorise requests from a forwarded identity header, `ClientKind`, display name, client-supplied ID or loopback source address.

Create a dedicated loopback listener/router for remote hosting. Leave the existing local bridge's behaviour separate. The remote router must omit the local-token bootstrap, filesystem-serving routes and implicit local administration access. Build a reviewed allowlist of individual operations; the existing broad IPC categories are insufficient because a category can mix safe reads and administrative effects.

| Boundary | Contract |
| --- | --- |
| Pairing | Client starts a bounded pending request and proves possession of its generated secret. Display the same challenge on client and local host approval. Approval is single-use, expires and grants only the requested supported mail capabilities. Do not expose mail before approval. Bound pending requests and attempts. |
| Native/CLI credentials | Issue an opaque per-registration credential; store only its verifier on the server. Authenticate over TLS. Never put credentials in command arguments, URLs, logs, diagnostics or the activity log. |
| Browser credentials | Use a secure, HttpOnly, same-origin cookie and CSRF/Origin validation for mutations and WebSocket upgrades. For CLI-assisted launch, exchange a short-lived, single-use code entered on the pairing page for a child grant. Open a clean URL; do not put the long-lived CLI token in its fragment or query. |
| Revocation | Commit revocation before acknowledging it; invalidate existing sessions and child grants as well as future requests. Recheck authority at operation admission, including on open WebSockets. |
| Remote permissions | Permit the reviewed mail/resource, self-session and lifecycle-observation operations. Keep provider credentials, arbitrary paths, shell execution, configuration of executable hooks, process control, backup/restore and broad diagnostic exports local-admin only. |
| Forwarding to daemon | The gateway supplies a server-validated session context. Bind it to the authenticated registration and its permitted operations. Public requests cannot forge that context or invoke internal forwarding/admin wrappers. Apply the same check to HTTP and WSS. |

Use the existing HTTP/JSON resource handlers for browser and future Swift clients. For Rust CLI/TUI/MCP, add a WSS transport adapter carrying the existing `IpcCodec` byte stream in bounded binary WebSocket messages. This keeps one daemon request model and the existing `mxr-client` connection logic. WebSocket message boundaries are not IPC frame boundaries; test fragmentation, coalescing, limits, backpressure and cancellation. Use maintained TLS/WebSocket libraries, not custom TLS or framing protocols.

Expose the WSS transport through the same authenticated, allowlisted gateway; never tunnel arbitrary privileged Unix IPC. Keep the browser's JSON event stream as an event surface. Both deliver the same lifecycle semantics. Advertise the negotiated paths and capabilities from the HTTPS origin; reject cross-origin credential redirects. Remote client authentication and attribution must survive gateway-to-daemon forwarding.

## Registrations persist while presence expires

Treat a registration as an explicit ongoing relationship with one server. Treat a session as one currently active client process or browser. A missing heartbeat ends presence, not the registration. Do not interpret either as proof a person is looking at the app.

| Record | Required fields and lifetime |
| --- | --- |
| Server identity | Stable host installation ID, workspace identity and recovery generation. A fresh daemon start also gets a new process/boot ID. A restored installation requires explicit identity and credential handling. |
| Registration | Server-issued ID, installation/profile identity, display name, creation/last-seen timestamps, capabilities, credential verifier, optional parent registration and active/revoked state. Survives daemon restart; explicit deregistration ends it. Names are bounded display data, never identifiers. |
| Session | Server-issued ID, registration ID, surface, client/protocol version, server boot ID, last activity, lease expiry and connection state. In-memory presence resets on daemon restart. HTTP requests and event connections must refer to the same authenticated session. |
| Accepted operation | Existing workspace-bound operation ID and outcome, with registration attribution where appropriate. Its lifetime is independent of the socket and registration. |
| Pending deregistration | Client-local record containing old origin/server/registration IDs, operation ID, retry state and a credential restricted to revoking that registration. Survives client exit and contains no mail actions. |

At pairing, issue a separate cleanup capability that can only deregister this registration and its children. This lets a client delete its mail credential when switching offline while retaining the ability to notify the old host. The server stores only its verifier. Repeated deregistration is idempotent, including when the original success response was lost; retain the minimum evidence needed for safe retry.

Keep registration state server-owned, separate from user-activity history. Coalesce last-seen writes. Activity remains host-local, honours `MXR_ACTIVITY=off`, and never becomes a remotely exported presence journal. Ordinary clients may inspect their own registration/session and general service availability; listing other clients requires local administration.

Initial timing defaults are engineering choices to test: 30-second foreground heartbeats, 90-second presence expiry, exponential reconnect delay starting at one second and capped at 30 seconds with jitter. Negotiate them in the handshake. Short CLI requests do not spawn background heartbeat processes. Sleeping/offline clients remain registered and appear last-seen; their expired sessions do not block shutdown.

Store credential verifiers and pairing state in the daemon's persistence layer with explicit backup filtering. Portable backups must not re-enable old grants or revive revoked credentials after restore. Restore clears remote hosting authority and sessions, rotates server security identity as required and requires re-pairing; preserve the existing workspace recovery safeguards. Do not copy authentication state blindly alongside mail SQLite.

## A fresh handshake establishes that the server is back

Proposed lifecycle messages are additive contract requirements, subject to protocol versioning. The examples show logical payloads; use the existing transport envelopes when implementing them.

```json
{
  "event": "server_stopping",
  "server_id": "server-example",
  "boot_id": "boot-example",
  "lifecycle_revision": 7,
  "scope": "daemon",
  "reason": "restart",
  "restart_expected": true,
  "retry_after_ms": 2000,
  "drain_deadline": "2026-10-12T18:00:30Z"
}
```

`scope` distinguishes the whole daemon from remote access alone. `reason` distinguishes restart, explicit stop, hosting disabled and maintenance. `restart_expected` is an intention, never a promise. A slow or disconnected recipient cannot prevent shutdown. No mail content or client inventory belongs in this event.

On each authenticated connection, return `server_ready` only after checking service admission state, registration, workspace identity and protocol compatibility. Include server/boot IDs, lifecycle revision, workspace/recovery generation, negotiated capabilities, session ID, lease parameters and relevant resource-resync requirements. Subscribe to lifecycle changes without a gap between the readiness snapshot and event registration; recheck the revision if necessary. A process that is draining must not send a stale ready response.

```text
connecting -> authenticating -> synchronising -> ready
ready -> server_stopping -> reconnect_wait -> authenticating
ready -> connection_lost -> reconnect_wait -> authenticating
any remote state -> switching -> local_selected
auth revoked / incompatible version / changed workspace -> action_required
```

There is no broadcast over a closed connection. A client learns that the server is back through its next successful authenticated handshake, then obtains current resource state and renders ready. Health checks alone cannot prove mail readiness. Reconnect long-lived clients with bounded backoff; resume when an inactive client next runs. Stop retries on revoked credentials or incompatible versions and explain the action required. Do not retry ordinary one-shot CLI commands indefinitely.

Do not replay old `server_stopping` events into a new boot. Ignore stale lifecycle revisions. If events were lost, use the existing resynchronisation/outcome contracts rather than inventing a complete replay journal. Reconcile uncertain accepted mutations by their existing IDs before issuing another mutation. Never retry an uncertain send as a new send.

## Controlled shutdown reports impact and has a deadline

A shutdown preview must report scope, registered client count, active session names/surfaces, offline last-seen clients, accepted in-flight operations, durable scheduled work and the applicable deadline. Counts are a snapshot, not a promise that nobody will connect next. Scheduled work matters even when no client is active.

1. Resolve a local host administration command and preview its actual target. Bind confirmation to the server/workspace identity and requested scope; recheck identity at execution.
2. Atomically enter draining and close admission for affected new mail operations and sessions. Close the race between preview and execution; recompute impact and report newly observed clients without permitting them to reopen admission.
3. Emit `server_stopping` through affected event/IPC connections. Keep bounded status, outcome and lifecycle access during drain. Reads are best-effort until the deadline.
4. Drain accepted operations within the configured deadline. Preserve durable outcomes, pending intentions and uncertainty using the existing operation contracts. Do not classify every interrupted provider call as failed or safe to resend.
5. Close affected connections and perform the existing ordered teardown. Persist desired service state before acknowledging an explicit stop so the supervisor does not undo it.

For `mxr serve stop`, limit draining to remote admission, close the remote listener and remove its Tailscale mapping; accepted daemon operations may continue. For full daemon stop/restart, include daemon workers and scheduled operations. Signals from an OS/service manager use the same bounded drain without interactive confirmation. A forced termination or power loss cannot promise a notice; clients must handle unexplained loss correctly.

Keep hosting alive with zero connected clients. Supervision recovers crashes and starts an enabled service after reboot. An intentional full stop persists the stopped state; an explicit start clears it. Client reconnects and stale registrations cannot reverse that decision. Do not add a client-triggered wake-up system in this release.

Acceptance requires observing both the client notice and the actual server transition, including a client that never reads the notice and a provider operation that exceeds the shutdown deadline.

## Switching away deregisters without moving pending work

Apply the same transaction to remote-to-local and remote-A-to-remote-B changes. Temporary network loss, closing a TUI, ending a CLI process or sleeping a phone does not invoke it.

1. Preview the old and new selections, this installation's affected sessions/child browser grants, outstanding operations and locally retained drafts. Validate/pair a new remote target before retiring the old selection. Pairing failure leaves the old selection intact; abandoned new pairings must be cleaned up.
2. Durably record switch intent and the old registration's cleanup capability. Increment the local selection generation, stop admitting old-target work in this installation and notify long-lived clients. Keep retained data partitioned by server/workspace identity.
3. Ask the old server to deregister this registration. The server commits revocation, closes its sessions and child grants, then acknowledges. Keep accepted-operation records and other clients intact. If the call fails or its reply is lost, retain idempotent pending cleanup.
4. Commit the new selection and remove the old mail credential. Recover interrupted switches from the intent record; never revive the old selection automatically after its credentials may have been revoked.
5. Retry pending cleanup on subsequent client use and while a long-lived client is running, with bounded timeouts/backoff. This may contact the former server only for deregistration. It must not block local mail or require a new always-running client daemon.

When the old server is unreachable, report `selection_applied: true` and `deregistration: pending`, not successful deregistration. Its presence lease expires, but its durable registration remains until notification succeeds or a host administrator revokes it. Completion is eventual only when connectivity and another client run allow a retry. Show pending cleanup in local connection status and doctor.

A forgotten remote registration cannot act for an application that has erased its mail credentials, but offline revocation is not instantaneous and an already-open remote session may survive until expiry. Close locally owned connections immediately. Never treat lease expiry as credential revocation. Re-pairing creates a new registration ID; delayed cleanup of the old ID cannot revoke it.

Existing drafts, attachments, queued actions and uncertain outcomes remain attached to their original workspace/recovery generation. They must be queryable when that workspace is deliberately connected again. Deregistration is neither cancellation nor deletion of accepted mail work. A browser tab at the previous origin displays the disconnected state and retains its local drafts; it does not start using the newly selected local workspace.

## Hosting setup owns only mxr's service configuration

`mxr serve --tailscale --dry-run` inspects local profile locks, recovery state, Tailscale availability/login, HTTPS prerequisites, the chosen tailnet endpoint/port, conflicting Serve mappings and the OS supervisor. It must not install software, approve tailnet settings, pair a device or alter a service. Execution reports missing prerequisites with actionable commands rather than silently reconfiguring unrelated infrastructure.

Run the daemon as the account that owns its data; do not give the daemon root privileges to simplify installation. Support a user-scoped macOS launchd service and a Linux systemd service arrangement suitable for unattended hosting. Distinguish macOS login startup from pre-login boot service availability. If Linux user lingering or system-level setup requires privileges, preview that exact prerequisite and report incomplete setup until it is satisfied.

Tailscale's [Serve CLI](https://tailscale.com/docs/reference/tailscale-cli/serve) supports a loopback proxy, `--bg` persistence, status inspection and disabling a particular mapping. Record the exact mxr-owned mapping and supervisor configuration. Never call global `tailscale serve reset`, replace an unrelated mapping or enable Funnel. Reject a conflicting port with an explanation and allow an explicit alternative port. Do not change global tailnet policies automatically.

Order setup so the authenticated remote router is ready before publishing the proxy. Record progress so retries can reconcile partial service/proxy installation. Remove only changes owned by this setup when rolling back; detect concurrent operator changes rather than overwriting them. Verify the resulting HTTPS origin, certificate, auth rejection, authorised handshake and WebSocket path. A saved configuration alone is not successful hosting.

Stopping hosting persists disabled state first, prevents remote admission and removes the owned mapping even if the daemon is already down. If proxy removal fails, keep the remote gateway closed and report remaining cleanup. Re-running `mxr serve` resumes the recorded configuration without duplicate daemon owners. Host service/process lifecycle remains independent of client selection.

## The VPS move uses the existing ownership transfer

Hosting makes an existing workspace reachable; it does not perform migration. A future operator runbook must use the [backup/recovery contract](23-daemon-portability.md#p08-a-restored-workspace-is-safe-to-resume) and the [published recovery guide](https://mxr.sh/guides/backup-recovery/):

1. Inspect pending drafts, uncertain sends and scheduled work; stop the source owner and create/inspect its stopped-profile backup.
2. Restore on the destination, provision provider credentials there, rebuild indexes and inspect the paused recovery state. Re-pair clients rather than copying remote credentials.
3. Confirm the source owner is stopped before explicitly activating the destination. Enable hosting and reconnect clients to the destination workspace identity through the approved recovery flow.
4. Verify reads, search, draft/attachment integrity, pending outcomes and one provider-work owner before declaring the move complete.

Switching a Mac client to local later does not authorise reactivating its old migrated daemon. Retain the stopped/retired source-owner state; require explicit recovery/ownership action if it is to become the owner again. Do not automatically fail back to an old mailbox copy.

## Each implementation stage proves a usable boundary

Execution follows the [vertical release ladder](../implementation/remote-hosting.md). The table below retains component ownership and coverage; it is not the release order.

Use the existing crate boundaries. `core` owns domain identifiers, `protocol` owns DTOs/events, `store` owns persisted registrations/revocation, `daemon` owns authority and lifecycle, `config` owns host/client settings, `transport` owns the WSS adapter, `client` owns shared selection/reconnect, and `web` remains an IPC client and authenticated gateway. OS service and Tailscale commands belong in host CLI integration. Web, TUI and MCP consume the shared contract. Avoid adding unrelated code to already large protocol/web files; follow their existing module seams.

The proposed remote routes are `/api/v1/remote/pairings`, `/sessions`, `/self`, `/self/deregister`, `/browser-grants` and `/ipc`, all under that prefix. Reuse existing mail resource routes and event streaming with the remote authentication layer. These names are schema inputs to finalise in R01a, not live endpoints. Expose reusable domain operations through typed IPC and CLI JSON too. Keep host installation and other local-only administration outside the remote allowlist; client parity does not grant remote OS privileges.

| Stage | Depends on | Implementation and acceptance |
| --- | --- | --- |
| R01a: identities and contracts | Portability preparation | Add registration/session IDs, self-deregistration and lifecycle DTOs, exact operation allowlist and compatibility fixtures. Specify selection/switch persistence and auth-context forwarding. Every remote operation has a CLI/IPC mapping or explicit local-only exclusion. |
| R01b: authenticated gateway | R01a | Persist pairing/revocation, separate remote router, browser cookie/CSRF handling and native credentials. Prove bootstrap absence, per-operation authorisation and immediate revocation on open sessions in an isolated HTTPS fixture. Keep hosting disabled by default. |
| R01c: saved connections | R01b | Add WSS connector, pairing CLI, persistent selection and client attribution. CLI/TUI/MCP read, reply and transfer files using a separate host filesystem. Prove no local fallback and clear version/workspace errors. |
| R01d: presence and server lifecycle | R01b, R01c | Add leases, registry/status, race-free ready/stopping delivery, shutdown preview/drain and restart recovery. Prove notification loss, slow clients, uncertain sends and fresh-session registration after daemon restart. |
| R01e: switching and remote web | R01c, R01d | Add switch intent/recovery, cleanup-only capability, child browser grants and origin-aware `mxr web`. Prove online/offline deregistration, lost acknowledgement, concurrent sessions and original-workspace draft retention in a real browser. |
| R01f: supervised Tailscale hosting | R01b through R01e | Add `mxr serve` setup/status/stop, supervisor state and owned proxy reconciliation. Prove Mac and Linux service behaviour, repeated setup, partial failure, restart, explicit stop and preservation of other services. |
| R01g: installed acceptance and runbooks | R01f | Run the complete two-client scenario through the actual supported HTTPS proxy with installed artifacts. Publish hosting, pairing, switching, revocation and VPS migration guidance; update CLI/API/generated clients and the readiness status. |

Do not expose a real mail workspace before R01b's negative authentication checks and R01f's deployment gates pass. Early stages may ship dormant support. Before each implementation push, fetch current remote state and preserve other work. Release-please owns versions and tags; when a stage is authorised to ship, verify the release artifacts rather than treating a merge as completion. This blueprint does not authorise personal VPS provisioning or mailbox migration.

## Acceptance includes failures across the real gateway

| Scenario | Required observable result |
| --- | --- |
| Two clients on separate homes | One workspace/provider owner; each registration visible, all applicable CLI/TUI/MCP/web actions use the selected server. |
| Pairing timeout, rejected approval, repeated connect | No usable unapproved credential; no duplicate active registration or partial saved selection. |
| Anonymous, wrong, revoked or forged identity | HTTP and WSS reject access; spoofed proxy headers, surface names and session IDs never grant authority. |
| Bootstrap and restricted operations | Real reverse proxy cannot retrieve local tokens, call shutdown, change executable rules, read arbitrary host paths or export secrets. |
| Browser auth | Cookie scope, CSRF and WebSocket Origin checks hold; launch URLs and diagnostics contain no bearer credentials. Parent revocation closes child sessions. |
| Socket/framing failure | Partial/coalesced IPC frames, oversized messages, backpressure, timeouts and cancellations preserve framing and bounded memory. |
| Foreground, offline and short-lived clients | Lease/status distinguish active from last-seen; CLI exit and heartbeat expiry do not deregister. Restart clears stale sessions. |
| Planned restart | Clients see stopping, stop new submissions, reconnect, validate ready and refresh resource state. No duplicated provider effect after an uncertain response. |
| Crash or missed shutdown event | No invented shutdown reason; reconnect/handshake restores correct state without relying on event delivery. |
| Shutdown/readiness race | A connection racing with drain cannot receive usable stale readiness or admit a new mail mutation. Slow clients do not extend the deadline. |
| Hosted full stop versus hosting stop | Full explicit stop stays stopped despite reconnects/supervision; hosting stop closes only remote access and local sync continues. |
| Online switch | Old registration and its child sessions are revoked; another device continues; no old-workspace command reaches the new daemon. |
| Offline switch and lost deregistration ACK | New selection works immediately with visible pending cleanup; later bounded retry removes only the old registration. |
| Crash at every switch boundary | Intent recovery chooses a coherent selection, preserves drafts and never revives potentially revoked credentials. |
| Open editor or old browser tab | Draft stays with its original workspace; old tab cannot silently submit to local or another remote origin. |
| New pairing before old cleanup succeeds | Cleanup of an old registration ID cannot revoke the newly paired registration. |
| Version, server or recovery identity changes | Client stops with an actionable incompatibility/identity state; queued work is held for explicit reconciliation. |
| Setup/reboot/partial failure | Enabled service resumes under the supported supervisor; stopped state persists; retries create one owner and preserve unrelated Serve mappings. |
| Backup restore | Old tokens, pairing codes and sessions cannot regain access; restored workspace remains under existing recovery controls. |
| Privacy and structured errors | No credentials, full mail content or activity history leak through status, events, diagnostics or logs. JSON distinguishes selection success from cleanup pending. |

Use fake providers and isolated homes for deterministic fault injection. Run focused crate tests through `scripts/cargo-test`, then `cargo build -p mxr` before implementation handoff, using the shared build queue. Run touched web checks and actual browser acceptance. CI owns workspace-wide Rust checks. The deployed acceptance must include macOS and Linux supervision and an authorised tailnet fixture; mocks cannot prove proxy or reboot behaviour. Record missing environment access as a blocker, not a passed check.

## Documentation and handover follow the implementation

Update the transport blueprint, CLI reference source/generator, remote API/OpenAPI schemas and generated clients as each surface ships. Add public operator guides only for implemented behaviour. Record each stage's commit, test environment, acceptance evidence and release result in the existing issue and linked implementation reports. Mark proposed commands as available only after installed acceptance.

For this specification, validation is limited to source-reference checks, relative-link checks and the HTML companion. Product, provider, supervisor and tailnet acceptance remain unexecuted. The next implementation action is R01a's contract/allowlist inventory against a fresh remote SHA.

## Unresolved choices have defaults and explicit decision points

- Which macOS Tailscale distribution and Linux service account/supervisor arrangement are the first supported deployment fixtures? Resolve in R01f before claiming unattended startup; the proposed default is launchd at user login on Mac and an unattended systemd arrangement on Linux.
- What credential lifetime and revoked-registration retention window should ship? Resolve in R01b; explicit revocation and cleanup-only retries are required regardless of duration.
- What remote API/client compatibility window should be supported? Start with explicit negotiated capabilities and exact IPC compatibility for the WSS adapter; choose and test any broader window in R01a rather than promising it implicitly.
- Are the proposed 30/90-second presence intervals and reconnect cap acceptable under actual network interruptions? Validate in R01d; timing changes must not alter the registration or shutdown semantics.
