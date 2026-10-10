# Daemon portability preserves one workspace authority

Status: implementation specification; no slices implemented by this document. Written 2026-10-10. Code evidence is pinned to freshly fetched `origin/main` at [`80606f4833b651bf9b07975ddd49083d7cb2568a`](https://github.com/planetaryescape/mxr/tree/80606f4833b651bf9b07975ddd49083d7cb2568a). The research worktree matches that commit. This is a source audit and implementation plan, not a report of runtime failures reproduced or changes shipped. The [iOS architecture assessment](../research/ios-architecture.md) records the product choice: a SwiftUI client may depend on a reachable daemon, initially on the laptop or later on a personal VPS.

Keep one daemon in charge of a workspace. Keep SQLite, provider adapters, the existing IPC protocol and the HTTP gateway. Improve the operations those surfaces expose before introducing another client. Each capability must remain available through daemon IPC and CLI JSON/JSONL; web and future clients consume the same behavior.

The completion check for this preparation is a test client using a separate home and filesystem from the daemon. It must read a thread, edit a draft, transfer an attachment, preview an action, submit it, lose the response, reconnect and recover the outcome. A daemon restart and workspace restore must preserve unsent work without blindly repeating provider effects. That proves the boundary without building an iOS app or deploying a VPS.

The [orchestration ladder](../implementation/daemon-portability/orchestration.md) maps these work packages into releases and records the active stage.

## Implement one ready slice at a time

Use this file as the implementation entry point. Read [architecture](01-architecture.md) for crate boundaries, [transports](20-transports.md) before changing connection behavior, and the domain references named in the selected slice. The research note explains alternatives; the requirements and sequencing here govern this preparation work.

1. Fetch the current remote branch, record its SHA, and recheck the selected slice's evidence. The pinned baseline below is historical evidence, not permission to overwrite newer code. Preserve unrelated working-tree changes.
2. Select one slice or named subpart from the readiness table. Record its scope in an existing `docs/issues/` issue, or create one if needed; link this blueprint instead of copying its requirements. Do not start all eight packages in one change.
3. Implement the daemon contract and persistence behavior, then connect the existing clients. Every new user-facing capability needs IPC plus CLI JSON/JSONL and the applicable TUI, web and MCP surface. Remote transport support for a client is a separate choice from capability parity.
4. Add the acceptance tests listed for that slice. Use isolated profiles and controlled providers; inject failures at the persistence, provider and transport boundaries. Follow `.agents/skills/mxr-development/SKILL.md` for focused tests and build requirements. Do not send real mail or change a VPS to prove this blueprint.
5. Update the affected API schemas, generated clients and user documentation in that slice. Mark completion here only with the implementation commit, commands and observable results. Record the exact next action and any blocker in the issue handover.

The final completion check is the separate-filesystem workflow above, plus the restore and uncertain-send tests. A successful connection or a passing happy-path send alone does not establish portability.

| Package | Can start when | First independently useful result | Status |
| --- | --- | --- | --- |
| P01: durable drafts | Now | Web edits and sends retain identity and reject stale revisions | Specified |
| P02: recoverable operations | Now for outcome/provider audit; P01 for send integration | A lost response has a queryable result without blind resend | Specified |
| P03: file resources | P01 defines draft ownership | Compose and attachments work without shared paths | Specified |
| P04: contracts and compatibility | Schema work accompanies P01 onward | Selected web workflow uses generated, checked contracts | Specified |
| P05: reconnection | Event-gap regression can start now; workspace checks use P04 | Event-only outages recover without losing edits | Specified |
| P06: client targeting | Diagnostics now; broader workflows after P01-P04 | Status and doctor report the selected daemon truthfully | Specified |
| P07: supervised/headless operation | Signal and token subparts now | Existing shutdown and credentials survive interruption | Specified |
| P08: recoverable workspace | Inventory now; integrate changes from P01-P03 | Restore into a new profile without replaying old effects | Specified |
| R01: authenticated remote HTTP | A remote consumer is selected; P01-P04 boundary ready | Remote credentials and exposed routes pass gateway tests | Deferred |

## Keep authority and machine ownership explicit

The target has one active daemon per workspace. It owns provider synchronization, classification, rules, scheduled actions, provider credentials, authoritative SQLite state, search and managed attachments. A laptop deployment and a personal VPS deployment use the same domain operations and storage model. Do not introduce a second provider-sync owner or copy a live SQLite database between clients.

A future iPhone app is native SwiftUI, using Swift networking and its own local cache, drafts and pending actions. The phone can display downloaded mail and prepare a reply while disconnected; submission waits for a reachable daemon, and the reply remains pending until its outcome is confirmed. That client storage is a replica of selected application resources, not the daemon schema. A Rust mobile backend is not required. The existing web app is a design reference and a consumer of the same operations.

Use the existing IPC connectors for Rust clients and HTTPS/JSON at the remote gateway. WebSocket events are refresh hints, never the only record of a mutation. No continuous socket is required for correctness. Start future iOS synchronization on foreground/resume and explicit refresh: joining the same LAN does not guarantee that iOS wakes the app ([Apple background-execution guidance](https://developer.apple.com/forums/thread/685525)). Pairing and local-network permissions belong to that future client work.

Keep these invariants through every slice:

- The daemon talks to providers through `MailSyncProvider` / `MailSendProvider`; provider-specific outcome evidence stays inside adapters.
- Preview and execution share the selection path. A changed draft, changed selection or expired authorization cannot silently execute the old review.
- Editors, file pickers, browsers and file openers run on the client machine. Managed resources are resolved to host paths inside the daemon.
- Activity remains local to its host, is never a sync feed or a portable migration payload, and retains the write/privacy rules in [activity-log.md](../activity-log.md).
- Remote targeting cannot fall back to another local workspace. Only one daemon owner may execute automation during relocation.

## Use a small resource and operation vocabulary

These are required semantics, not final Rust type or route names. Extend existing types where they fit; do not add a parallel protocol or generic workflow framework.

| Value | Required meaning |
| --- | --- |
| Workspace identity | Durable identity returned by the selected daemon, stable across ordinary restarts and independent of URL, socket path and build version |
| Recovery generation | Persistent generation changed by restore, so clients cannot silently replay pre-restore pending actions into rolled-back state; ordinary restarts do not change it |
| Draft ID and revision | Stable daemon-owned draft identity plus an opaque local content revision; distinct from a provider's own draft revision |
| Operation ID | Client-retained retry identity scoped to the workspace, recovery generation and stable caller identity; a request/correlation ID is insufficient |
| Reviewed command | Validated domain payload and selection bound to the operation; includes the draft revision and attachment identities for send |
| Attachment ID | Opaque identity for managed bytes, with ownership, size and lifecycle; never a client-supplied daemon-host path |
| Outcome | Persisted acceptance/progress/result, including per-item batch results and an explicit unknown provider outcome |

Clients retain an operation ID before submission and query or resubmit that same ID after a connection failure. Bind it to a canonical validated command, not arbitrary JSON serialization or a short-lived bearer token. A changed payload with the same ID is a conflict. A workspace/generation mismatch blocks replay and preserves unsent local work for an explicit reconciliation choice.

For the first implementation, retain accepted operation identities and terminal receipts without time-based pruning. Large temporary payloads may be cleaned separately when no longer referenced. Add bounded retention only with an explicit expiry/rejection contract and tests proving an old retry cannot execute as new work; the existing 24-hour mutation dedup window alone is insufficient for an offline client.

## Build on the transport and recovery work that already exists

The [shared client already accepts a generic connector](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/client/src/lib.rs#L12-L17), and the [CLI can use a command transport such as SSH](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/server.rs#L511-L526). This work extends the completed [transport-adapter work](../transport-adapters/README.md); it does not start another transport abstraction.

Stored sends already have [receipt lookup by draft ID](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/handler/mutations.rs#L3177-L3194), a [compare-and-set guard before sending](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/handler/mutations.rs#L3297-L3320), and [persisted successful receipts](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/handler/mutations.rs#L3409-L3429). Mutations have a [24-hour deduplication store](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/store/src/mutation_dedup.rs#L15-L59). Extend these mechanisms instead of creating a second job system.

The web client already [keeps displayed data through outages and refreshes on recovery](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/apps/web/src/lib/daemonAvailability.ts#L92-L157). Reconnection work below covers specific missing cases and portable contracts, not a replacement of that client.

## First make the current clients safer and their resources portable

### P01. A draft keeps its identity through editing and sending

The [web send handler deliberately creates a fresh draft ID](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/web/src/lib.rs#L851-L865) because reusing the stored ID currently risks sending pre-edit content. The [web update handler writes a session file](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/web/src/lib.rs#L739-L807), while the [stored-draft update checks draft status but not an expected content revision](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/store/src/draft.rs#L234-L266).

Make daemon-owned stored drafts the durable compose resource. Return a draft ID and revision; update against the expected revision and send the reviewed revision. Persist draft creation with a repeatable client identity so a lost create response does not create another draft. Keep the CLI's `$EDITOR`: its local file is an editing surface that submits content to the daemon. Migrate the web composer onto the same operations. Preserve the previous draft when a concurrent edit is rejected, and invalidate a safety override or preview when its bound content changes.

Current benefit: web and CLI edits share draft state, retries can reuse send receipts, and two editors cannot silently overwrite each other. A content revision can map to HTTP `ETag`/`If-Match`; the [HTTP standard defines conditional writes for this purpose](https://www.rfc-editor.org/rfc/rfc9110.html#section-13.1.1).

Completion: edit the same draft in two clients; the stale update returns a conflict and both versions remain recoverable. Drop a successful send response and retry the same reviewed draft: return the existing receipt. Verify that the latest reviewed body, not a stale stored body, is sent.

Implementation entry points: `crates/store/src/draft.rs`, `crates/daemon/src/handler/mutations.rs`, `crates/web/src/lib.rs`, `crates/web/src/request_types.rs`, and `apps/web/src/features/compose/`. Read [compose](06-compose.md) and [data model](02-data-model.md) before changing stored drafts.

Keep create, read, update, preview and send on one stored resource. Updates check the expected revision atomically with the write. Sending checks the reviewed revision atomically with the send-state transition. Never resolve an edit conflict by overwriting the newer draft or discarding the client's text. A saved draft is authoritative after acknowledgement; any compose file is a client editing artifact. Existing scheduled-send and draft-resumption flows must use the same revision rule.

Acceptance tests must also cover a lost create response, daemon restart after a save, an edit racing with send, and safety-review invalidation after changing recipients, body or attachments. P01 proves replay of an already-recorded successful send receipt. Crash uncertainty between provider acceptance and receipt persistence is completed in P02; do not claim P01 provides exactly-once delivery. Update compose and draft CLI/API guidance with the new conflict behavior.

### P02. An accepted action has a recoverable outcome

The [mutation handler generates a new ID for each invocation](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/handler/mutations.rs#L540-L551). Its current correlation ID is not a persistent retry key. The [web transport maps a rejected fetch to daemon unavailability](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/apps/web/src/api/client.ts#L48-L85), which cannot establish whether a mutation executed before the response was lost.

Accept a stable operation ID for retryable actions, scope it to the workspace and caller, and bind it to the command payload and reviewed selection. Record acceptance before executing effects; persist progress and a queryable outcome. Repeating an ID with different content must conflict. Retention must cover the supported retry window; an expired identity must not silently become a new action. Extend existing mutation jobs, deduplication and send receipts rather than adding a generic workflow engine.

Separate rejected-before-execution, accepted, completed, failed-with-known-outcome, partially completed and outcome-unknown results. Carry machine-readable error codes, retry guidance and operation IDs through IPC, HTTP and CLI output. Cancellation after acceptance is an explicit command; closing a connection is not proof of cancellation. Mutations and batch actions keep the same preview and execution selection path, with stale previews rejected or re-reviewed.

Sending needs its own failure audit. Today the [stored-send path returns a draft to editable status on any provider error](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/handler/mutations.rs#L3361-L3369), and the [SMTP adapter classifies timeouts as retryable](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/provider-smtp/src/lib.rs#L212-L237). [SMTP explicitly allows uncertainty after data submission](https://www.rfc-editor.org/rfc/rfc5321.html#section-4.5.3.2.6). Provider adapters should report the evidence they have; the daemon must retain uncertain outcomes rather than blindly resend. A stable Message-ID alone is not an exactly-once delivery guarantee.

Current benefit: safer web retries, CLI scripts and scheduled sends. Completion: simulate a lost client response, concurrent retry, daemon crash before/after a provider effect, partial batch success and ambiguous send response. Confirm known completed effects are not repeated and uncertainty remains visible. These are acceptance tests to implement, not tests run during this audit.

Implementation entry points: existing send/mutation handlers, `crates/store/src/mutation_dedup.rs`, `crates/protocol/src/types.rs`, `crates/provider-smtp/src/lib.rs`, and `apps/web/src/api/client.ts`. Keep internal mutation IDs for undo/history and link them to the external operation ID rather than replacing their meaning.

Persist acceptance before provider effects and serialize concurrent submissions of the same operation. An accepted command survives the request connection. Distinguish transport failure from a domain rejection, and domain rejection from provider uncertainty. Where cancellation is supported, return its actual result; do not promise cancellation after the provider may have accepted an effect.

| Observed state | Client behavior |
| --- | --- |
| Rejected before acceptance | Show the reason; a corrected command gets a new ID |
| Accepted or executing | Query the operation; resubmitting the same ID cannot start another execution |
| Completed | Return the stored result/receipt |
| Failed with known outcome | Return the recorded failure and safe retry guidance; a deliberate new attempt gets a new ID |
| Partially completed | Show per-item outcomes; resolve or retry only eligible unfinished effects |
| Outcome unknown | Reconcile using provider evidence or require an explicit user decision; never automatically resend |

For a crash after acceptance, restart processing only when the persisted state and provider semantics establish that repeating the next effect is safe. Audit scheduled and batch sends as well as interactive send. Include provider acceptance followed by a lost response and by failed local receipt persistence. Test same ID/different payload, two simultaneous submissions, and replay after the former 24-hour dedup interval. Update CLI JSON, web error/retry presentation and provider error guidance together.

### P03. Files cross the boundary as managed resources

The [draft type contains attachment paths](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/core/src/types.rs#L1581-L1610), and [AttachmentFile returns a host path](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/protocol/src/types.rs#L4075-L4081). The [compose request types accept draft and attachment paths](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/web/src/request_types.rs#L150-L191).

Add daemon-managed attachment identities, upload/finalization and byte download operations. Associate uploaded files with the draft and preserve them through restart. Bound transfers, validate ownership and cleanup abandoned uploads without removing files referenced by drafts. Providers can continue receiving resolved local paths inside the daemon; portability does not require rewriting every provider interface. File pickers, opening a downloaded file and invoking `$EDITOR` belong to the client machine.

Current benefit: predictable attachment lifecycle, fewer path-related compose failures, and useful remote CLI workflows. Completion: the client and daemon have disjoint filesystems; upload, restart, preview and send an attachment, then download another file and open it on the client. A request cannot use a supplied path to read arbitrary daemon-host files.

Implementation entry points: draft/attachment boundary types in `crates/core/src/types.rs` and `crates/protocol/src/types.rs`, compose and download handlers in `crates/web/src/lib.rs`, and the daemon's attachment handlers/store. Read [compose](06-compose.md) before changing attachment ownership.

Use a bounded transfer that stages bytes, verifies size/completeness, then finalizes an immutable attachment resource. A first implementation may require restarting an interrupted upload; resumable chunk protocols are deferred until needed. Reference finalized IDs from drafts, retain referenced bytes across restart, and define garbage collection for abandoned staging files and unreferenced resources. The send preview binds the finalized attachment set.

CLI path arguments remain convenient client inputs: the client reads/uploads those files or writes downloaded bytes. Untrusted API callers cannot ask the daemon to open arbitrary paths. Keep any compatibility path operation explicitly local and outside R01's exposed route set. Test incomplete upload, upload retry, size rejection, another draft/caller's resource, cleanup while a draft references a file, and exact downloaded bytes. Update attachment CLI help and API examples with both local and remote flows.

### P04. API contracts describe supported behavior

The [OpenAPI source explicitly describes an incomplete route inventory](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/web/src/openapi.rs#L17-L31). The [web compose API maintains its own interfaces](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/apps/web/src/features/compose/api.ts#L1-L61). At this SHA, the [workspace is version 0.6.61](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/Cargo.toml#L165) and the [checked-in site spec reports 0.6.58](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/site/public/openapi.json#L13); regeneration alone would not fill missing endpoint contracts. Reuse an existing OpenAPI drift issue when tracking regeneration.

Complete request, success, error and event schemas for the operations above, starting with mailbox/thread/body, drafts, attachments, mutations and operation status. Generate them from the Rust boundary types and reuse the web app's existing generation tooling. Add route/schema coverage and checked-in artifact drift checks. Complete schemas in the same slice that changes an endpoint; do not postpone contract work until the end or rewrite all endpoints at once.

Expose a stable workspace identity, supported API version and feature capabilities through status/handshake data. Separate API compatibility from the daemon's build identity: an older compatible client should work, and an incompatible client should receive an actionable error without attempting to restart a remote server. Specify a tested compatibility window instead of relying on client and daemon updates happening together.

This builds on an existing protocol version. The current [restart predicate also compares package version and build ID](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/server.rs#L657-L665); preserve that local-upgrade advice where appropriate while separating it from remote compatibility.

Current benefit: less manual web/Rust type drift and more dependable scripting and upgrades. Completion: the existing web client consumes generated types for the selected workflow, real responses conform, and supported older/newer client fixtures behave as specified.

Implementation entry points: `crates/web/src/openapi.rs`, `crates/web/src/request_types.rs`, `crates/protocol/src/types.rs`, `apps/web/scripts/gen-bridge-types.mjs`, `apps/web/src/lib/protocolCompatibility.ts`, and the existing OpenAPI tests. Read [transports](20-transports.md) and [HTTP bridge guide](../guides/http-bridge.md) first.

Add identity/capability fields compatibly before clients depend on them. Record the API compatibility policy and fixture versions before claiming cross-version support. Keep exact-build checks only where they govern a local upgrade; incompatible remote clients must fail with an actionable error. Preserve existing local behavior while moving the selected workflow off manually mirrored web types.

Check requests, success/error responses and events against generated schemas, not just a spec version string. Fail CI when a selected route lacks its contract or generation changes a checked-in artifact. Include supported skew, unsupported protocol, unknown optional fields and unsupported capability fixtures. Update the bridge guide and generated API reference using the repository's existing tooling.

### P05. A connection gap triggers authoritative recovery

The [WebSocket bridge forwards live events](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/web/src/lib.rs#L2097-L2158); it does not expose a replay cursor. Existing web recovery marks queries stale after an observed daemon outage, but [socket-state changes trigger a probe while the refresh is gated on daemon-down state](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/apps/web/src/lib/daemonAvailability.ts#L96-L129). Test the case where only the event connection drops while ordinary requests keep succeeding.

Make a recovered event gap trigger bounded authoritative refresh while preserving unsaved drafts. Define how a client distinguishes a deleted resource, an unavailable body and an incomplete list page. Use deterministic pagination for the touched reads; never infer deletion from an item missing from one page. Scope cached data and pending commands to the workspace identity so a host switch cannot submit them to another workspace.

Current benefit: correct web state after sleep, network interruption or event loss, without destroying text being edited. Completion: change and delete messages from the CLI during a web event outage, then reconnect and compare the visible results. Include a stale draft and a workspace switch. A durable incremental change journal, tombstone retention and mobile cache policy wait for an actual offline client or measured refresh cost; do not use the local activity log as a synchronization feed.

Implementation entry points: `apps/web/src/lib/daemonAvailability.ts`, `apps/web/src/lib/ws.ts`, `apps/web/src/hooks/useDaemonEventInvalidation.ts`, their existing tests, and the selected daemon list/read operations.

Coalesce gap recovery into a bounded refresh; avoid fetching entire bodies/attachments whenever a socket opens. Preserve edits while refreshing server state, and surface a revision conflict when needed. Pagination must document a stable ordering and tie-breaker; a scan that cannot guarantee a consistent snapshot must advertise refresh/restart behavior instead of implying completeness. Resource-not-found, body-not-cached and page-exhausted are distinct results.

Test a dropped socket while HTTP remains healthy, lost/lagged events, sleep/resume, deletion, reordered pages and a daemon restart. Switching endpoint to the same workspace may reuse its cache; switching workspace or recovery generation cannot automatically submit pending commands. Update reconnect and offline-state UI guidance. A future mobile client still needs its own cache/outbox implementation; this slice supplies the contract it can use.

## Prepare a daemon that is straightforward to run elsewhere

### P06. Every client reports and uses its selected daemon

The CLI supports generic connectors, but the [TUI still constructs a Unix-socket connection](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/tui/src/client.rs#L15-L29), and the [HTTP bridge connects through a socket path](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/web/src/lib.rs#L2040-L2058). Reuse `mxr-client` and the existing connector resolver where a client needs remote access. Keep the bridge beside the daemon over local IPC; making it independently remote is unnecessary for the VPS architecture.

Make the configured daemon target authoritative for status, diagnostics, account operations and mutations. Label client-local and daemon-host paths separately. Ensure a remote connection failure never autostarts or modifies a different local workspace. Keep interactive helpers such as the editor, browser and file opener on the client; let the daemon expose data and outcomes. Cover the CLI and TUI first, and extend MCP when its remote use is selected. A unified domain surface does not require converting all Rust clients to HTTP.

There is a small fix to do before expanding transport coverage: [status appends local config/data/socket paths](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/commands/status.rs#L35-L57) to the [configured daemon's GetStatus response](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/commands/status.rs#L147-L164). [Doctor probes the local socket](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/commands/doctor.rs#L287-L322) and [opens the local store](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/commands/doctor.rs#L375-L382). Reuse the daemon-owned [GetDoctorReport already used by the bridge](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/web/src/admin_handlers.rs#L57-L65), and keep client-environment diagnostics explicitly identified. Test with different local and target profiles so a misleading result cannot pass by coincidence.

Current benefit: consistent profiles, clearer diagnostics and reliable SSH use. Completion: a CLI and TUI session select the same isolated remote daemon; read, reply, attach, preview and mutate without reading that host's files directly or starting a local daemon. Prove supported version skew and clear errors for unsupported versions.

Implementation entry points: `crates/daemon/src/commands/status.rs`, `crates/daemon/src/commands/doctor.rs`, the connector resolution in `crates/daemon/src/server.rs`, `crates/client/src/lib.rs`, and `crates/tui/src/client.rs`.

P06a is the standalone diagnostics fix: identify client-local checks separately, obtain daemon diagnostics through the configured target, and never open a local store as a substitute for a remote result. P06b connects selected TUI and remaining CLI workflows to the existing connector. Include account setup in the supported-workflow inventory and report unsupported local interaction explicitly rather than silently switching hosts.

Tests use two different profiles and data sets, plus an unreachable remote target. Assert the selected mailbox is shown and no local daemon/store is created or modified on failure. Preserve transport-conformance tests and document target selection in CLI/TUI and transport guidance. The initial portable workflow is read, reply through the local editor, attach, preview and mutate; other workflows require explicit coverage before being advertised as remote-capable.

### P07. Supervised operation survives interruption

Use the existing foreground daemon and diagnostics as the starting point. The [accept loop listens to a shutdown watch and already performs ordered teardown](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/server.rs#L336-L406). Source inspection of the entry point/server path and signal-handler search found no daemon SIGTERM/SIGINT hook into it. Add a process-level regression test, then route those signals through the existing shutdown path; accepted work is completed, persisted for restart or left explicitly uncertain. Keep [bridge liveness](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/web/src/router.rs#L10-L18) distinct from readiness and reuse the existing [health classification](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/server.rs#L675-L688). A temporarily unavailable LLM should produce a useful degraded state for the capabilities that depend on it while unaffected mail operations remain available.

Validate headless account setup and refresh, noninteractive secret access, restart after interrupted persistence and startup without a GUI/keychain session. Fix observed gaps before adding another configuration system. Provide one Linux service recipe with explicit config/data paths and a clean shutdown policy, plus a repeatable isolated integration run. Keep model endpoints explicit; moving a daemon configured to call the laptop's model does not remove that dependency.

One persistence gap is directly visible: the [Gmail token-cache writer truncates and rewrites the live file](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/provider-gmail/src/auth_storage.rs#L118-L130), whereas the [password store already uses durable atomic replacement](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/config/src/secret_store.rs#L282-L314). Reuse that proven persistence pattern without moving provider-specific behavior out of the provider crate. An interrupted replacement must leave the old or new complete token readable; this audit did not reproduce corruption.

Current benefit: more predictable restarts, background operation and troubleshooting on the Mac, as well as a reproducible Linux deployment path. Completion: start without a graphical session, stop during work, restart, make an optional dependency unavailable, and inspect JSON diagnostics without a client attached. No production account or Hostinger instance is needed for the initial test.

Implementation entry points: `crates/daemon/src/main.rs`, `crates/daemon/src/server.rs`, `crates/provider-gmail/src/auth_storage.rs`, `crates/config/src/secret_store.rs`, `crates/daemon/tests/daemon_lifecycle.rs`, and existing daemon health/doctor handlers. Read [configuration](12-config.md) and [OAuth](18-addendum-oauth.md).

P07a routes process signals into existing teardown and tests actual process exit. P07b replaces the token cache atomically with appropriate permissions and durability, testing interruption on either side of replacement. They can land independently before compose work. P07c validates unattended Linux operation and documents the foreground service recipe, restart policy, paths, credential source and optional model dependencies.

Kill or signal only test processes owned by the test. Use fixture credentials and controlled providers; no production token reads are needed. Verify shutdown with pending work, restart from persisted state, missing/unavailable secrets, disk-write failure and an unavailable model. Final accepted-operation recovery depends on P02; the early signal fix must not claim to resolve every in-flight send. Add readiness diagnostics to existing surfaces rather than a second health subsystem.

### P08. A restored workspace is safe to resume

Inventory authoritative state, recoverable provider cache, local drafts, attachments, scheduled actions, rules/config, credentials and rebuildable indexes. Use a SQLite-supported snapshot operation or a controlled stopped-daemon copy; [SQLite's backup API provides a consistent database snapshot](https://www.sqlite.org/backup.html). Database consistency alone does not make external files consistent: coordinate the snapshot with the referenced attachment files and manifest. Reuse existing export and diagnostics components where they fit, but mail export is not automatically a workspace backup.

Define a versioned backup manifest and an inspection/restore preview. Restore into a new profile or empty directory, preserve original data, remap internal file locations and rebuild derived indexes. Keep credentials explicitly handled, and exclude local activity history from portable migration. A restored backup may predate sends that already happened: start automation paused and reconcile or require resolution of uncertain work before enabling execution. Do not allow a restore to blindly replay an old outbox.

Current benefit: recovery from a broken machine or upgrade, and moving mxr to another Mac as well as Linux. Completion: restore an isolated workspace into a different home directory, recover drafts and attachments, rebuild search and verify that queued work is not unexpectedly executed. For a later host migration, stop the source automation owner before enabling the destination. No distributed leader-election system is required.

Implementation entry points: configuration path resolution in `crates/config/src/resolve.rs`, store migrations, managed draft/attachment persistence from P01/P03, and existing export/doctor commands. Read [data model](02-data-model.md), [configuration](12-config.md) and [export](11-export.md).

Start with a controlled stopped-daemon snapshot: verify the writer is stopped, then inventory/copy the authoritative database and managed files as one backup unit. Online backups can follow with explicit coordination. The manifest records format/schema versions, workspace identity, file sizes/checksums and excluded/rebuildable state. Inspect validates completeness before any restore writes. Restore preview and execution share the same inventory and destination checks; reject a nonempty destination.

Rotate the recovery generation on restore, preserve the source backup, and start provider mutations, scheduled sends and other effectful automation paused. Rebuildable indexes may be regenerated; persisted drafts and their attachments must be present before restore is reported successful. Default to a secret-free portable backup with explicit credential re-provisioning; do not create an undocumented plaintext-secret archive. Exclude local activity data from the portable payload, including when it is stored inside an otherwise copied database.

Test corrupt/missing files, unsupported manifest/schema, a different home directory, an old snapshot containing a since-completed send, and client commands carrying the pre-restore generation. Verify recovered draft text and attachment bytes, usable search, visible paused/uncertain work and no unexpected provider effects. Update backup/restore CLI JSON, preview output and the recovery runbook. An actual host move additionally requires stopping source automation before activating the destination.

## R01. Off-machine HTTP requires an authenticated gateway

The [bridge refuses non-loopback binds](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/daemon/src/bridge.rs#L121-L130). Keep that default. A gateway in front of it can terminate HTTPS while the daemon remains local-only. Deliberately disable the [local token bootstrap](https://github.com/planetaryescape/mxr/blob/80606f4833b651bf9b07975ddd49083d7cb2568a/crates/web/src/router.rs#L59-L84) for forwarded traffic; the proxy's loopback address is not the end user's identity.

Before exposing LAN or VPS access, add explicit device/client credentials, revocation and a bounded exposed operation set. Remote mail access must not automatically expose local administrative or arbitrary-file capabilities. Test the gateway itself: unauthorized and revoked clients fail, identity cannot be forged by forwarded headers, WebSocket authentication matches HTTP, and the bootstrap token cannot be retrieved through the proxy. Select a private-network or public-HTTPS deployment then; actual hosting, certificates and paired-device UX are not prerequisites for the earlier reliability work.

R01 adds the remote boundary without changing the loopback default. Inventory and explicitly allow mail-resource operations; classify admin, credential, arbitrary-file and process-control routes separately. A gateway credential grants a stable caller identity with revocation, rather than access to the local bootstrap token. Keep auth out of URL query strings and logs. A local CLI approval or trusted private network is not a substitute for authenticating the remote HTTP caller.

Implementation entry points are `crates/daemon/src/bridge.rs`, `crates/web/src/router.rs`, HTTP authentication and WebSocket upgrade handling. Update [transports](20-transports.md) and the [bridge guide](../guides/http-bridge.md) with the selected proxy/network policy. Test unauthorized/revoked access, forwarded-header spoofing, bootstrap reachability, forbidden routes and HTTP/WebSocket identity parity through the real gateway path. Do not enable a non-loopback listener or provision Hostinger as part of P01-P08.

## Deliver current improvements before exposing another host

The first substantive workflow change is P01, followed by P02 and P03. P04 contract work accompanies each changed operation. P05 can close its existing event-gap case independently and adopt the workspace contract when ready. P06a, P07a and P07b are smaller fixes that can land first. P07c and P08 can proceed independently against the current schema, then include new durable resources before final completion. Broader P06 workflows depend on portable drafts/files and compatibility. R01 is required before the first off-machine HTTP release.

Keep Unix IPC, HTTPS/JSON at the gateway, native-client freedom and one daemon per workspace. Defer Swift code, Rust-on-iOS extraction, Turso/DB replication, gRPC, CRDTs, a persistent global change journal, APNs, Bonjour pairing UI, multi-tenancy and actual VPS migration. None is needed to establish these boundaries.

This document was validated as documentation and against the pinned source. Its proposed product tests have not been executed. No product code, daemon configuration, real mail or VPS state was changed. The readiness table and linked implementation issues own subsequent status; the research note remains background. Start with P01 unless selecting one of the explicitly independent fixes.

## Unresolved choices affect later slices

- What client/API compatibility window should mxr guarantee?
- Which additional Mac workflows beyond the P06 read/reply/attach/mutate baseline must work remotely in the first VPS-ready release, especially account setup?
- When remote HTTP is selected, should the first supported route be a private network or a public HTTPS endpoint?
- What backup destination and credential re-provisioning method should apply to a real host migration? P08 initially uses isolated local data and excludes secrets.
