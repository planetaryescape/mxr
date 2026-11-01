# Use Mastra for the next assistant integration

**Status:** synthetic spike passed; no production migration performed.
**Observed:** 2026-11-01. **Primary source:** branch
`spike/mastra-agent-runtime`,
[`crates/assistant/prototype-mastra/`](../../crates/assistant/prototype-mastra/README.md).

## Decision

Prefer Mastra for the next daemon-owned assistant integration. Its library-only
worker and stdio MCP client fit mxr's local process model, and its native
durable-agent implementation passed the approval-notification crash case that
stalled the tested Eve version. This is enough to choose the next implementation
candidate, not to remove the existing assistant immediately.

The comparison is Mastra core `1.79.0` / libSQL `1.27.0` / MCP `2.5.0` against
the earlier Eve `0.83.2` spike on `spike/eve-agent-runtime`, commit `988bc0394`.
Both used policy-permitted releases. Mastra `1.81.0` and Eve `1.0.0-rc.1` were
too fresh for the machine's npm release-age policy and were not tested. Do not
generalize these results to those releases or claim current Eve is broken.

## Results

The scripted provider drives the actual native `createDurableAgent()` runtime
and three fixture tools. The fixture mailbox, preview token and send operation
identity follow the Eve experiment. A separate Gemma run checks model integration.

| Probe | Mastra result | Earlier Eve result |
| --- | --- | --- |
| Local Gemma search, preview, deny, approve | Passed | Passed |
| Exact selection | Both shipping IDs changed; personal message preserved | Passed |
| Duplicate approval | Refused because run had finished; no second mutation | No second mutation |
| Restart after settled approval suspension | Resumed in about 0.5 seconds including startup | Passed |
| Kill on first received approval chunk | Resumed in about 0.5 seconds including startup | Stalled beyond 30-second observation window |
| Kill after fake send commits, before tool returns | Recovered in about 17 seconds; tool called twice, one effect with application deduplication | One attempt/effect, then stalled beyond 30 seconds |
| Local stdio MCP discovery and call | Passed against a synthetic server on protocol `2025-06-18` | Built-in connection expected HTTP/SSE |
| Stream cleanup alone | Left both suspended snapshots | Not an erasure guarantee either |
| Explicit snapshot deletion | Removed sentinel from queryable rows; resume failed after process restart | Normal retention removed stream/payload but retained prompt title; interrupted-send reset failed |

The send recovery initially returned `skipped: 1`, because the dead worker's
claim had not expired. The embedded host's recovery helper scheduled a later
attempt. The probe waited for the persisted run to settle and required a second
tool invocation before accepting success. It did not count the initial skipped
claim as successful recovery or shorten the default lease.

Mastra replayed the send without requesting another approval. This test's
persistent operation ledger returned the already-recorded result, so the retry
did not create another effect. Without application deduplication or an explicit
unknown-outcome policy, a real send could repeat. Mastra's durability does not
remove mxr's send-safety work.

Native SDK worker startup was roughly 0.4 seconds, with initial RSS around
230-246 MB. Installed dependencies occupied about 187 MB after adding MCP.
No bundling or Docker image build was needed. These are single-machine
observations, not a model-latency or memory benchmark against Eve.

The local Gemma smoke turn took about 8.2 seconds for search, and its ordinary
archive proposal about 11.6 seconds. These timings include model work and are
not a controlled framework-performance comparison.

Selected JSON observations are kept in
[`observations/`](../../crates/assistant/prototype-mastra/observations/).
Full synthetic event logs, model requests and scratch databases are ignored.

## Deletion is an application operation

No Memory or trace exporter was configured. A suspended sentinel-bearing run
appeared in two `mastra_workflow_snapshot` rows, one each for
`durable-agentic-execution` and `durable-agentic-loop`. Calling the public stream
`cleanup()` removed registry/cache state but left both database rows.

Deleting both rows through `deleteWorkflowRunById({ runId, workflowName })`
removed the sentinel from queryable tables. After a fresh worker started,
`resume()` failed and no mutation ran. This is a narrower, reproducible result
than "Mastra erases everything." Memory, traces, evidence logs, caches, active
writers, database pages, WAL and backups need their own lifecycle policy if used.

## Smallest production integration

1. Keep CLI/web/TUI on the daemon's existing assistant requests. Supervise one
   local Node SDK worker and translate its events in the daemon. Do not expose
   the raw worker approval operation as an agent tool.
2. Start with read-only tools on an isolated mxr demo profile. Reuse MCP schemas
   through stdio rather than authoring a TypeScript wrapper per tool. The
   synthetic stdio pass does not prove actual mxr profile/account/workspace
   semantics; verify those before enabling changes.
3. Add one archive preview/approval/commit workflow, with exact-call consent
   enforced by mxr's daemon code and a user-only reversible-change trust setting.
4. Keep operation identity, deduplication and uncertain-send reconciliation in
   mxr. Define erase across every enabled Mastra persistence domain. Package
   Node and libSQL without requiring users to install a developer toolchain.

Still untested: the real daemon adapter, local/cloud consent and routing parity,
cross-model transcript migration, cancellation races, malicious mail, multi-client
approvals, UI rendering, context compaction, skills, subagents, schedulers and
distribution outside this machine. No Rust files, live mailbox, installed mxr
binary or production state were changed.
