# Evaluate runtime replacement against the assistant we already ship

**Checked:** 2026-11-01, production source `f6382442829d06731465e6ad4db29a5684dd1919`.
**Status:** synthetic experiments only. The shipped assistant still uses Rust;
neither Eve nor Mastra is integrated into the daemon.

Compare a candidate with the existing assistant before budgeting a migration.
The spikes establish recovery behaviour on specific versions, not a net
maintenance saving or a new set of user-facing mail capabilities.

## Inspect the evidence

The prototypes and selected observations are preserved in local commits, not
in this branch's production tree. Inspect them without switching branches:

```sh
git show ebae50492:docs/issues/mastra-runtime-evaluation.md
git show ebae50492:crates/assistant/prototype-mastra/observations/scripted.json
git show ebae50492:crates/assistant/prototype-mastra/observations/scripted-repeat.json
git show ebae50492:crates/assistant/prototype-mastra/observations/local-model-smoke.json
git show ebae50492:crates/assistant/prototype-mastra/observations/stdio-mcp.json
git show 988bc0394:docs/issues/eve-runtime-evaluation.md
```

These commits are on the branches `spike/mastra-agent-runtime` and
`spike/eve-agent-runtime`. A clone does not fetch them by default, so run
`git fetch origin spike/mastra-agent-runtime spike/eve-agent-runtime` first.
Each prototype README gives reproduction commands. Tests used synthetic mail only.
The local-model smoke used Ollama `gemma4:latest`; deterministic crash probes
used a scripted provider through the real framework runtime.

## Separate existing behaviour from the candidate's gain

| Concern | Shipped Rust assistant | What the Mastra spike establishes |
| --- | --- | --- |
| Tools and model calls | MCP in-process, streamed replies, local/cloud routing | Local SDK worker and synthetic stdio MCP calls work |
| Approvals | Stored calls, previews, duplicate-decision refusal, user-only trust | Synthetic deny/approve/exact-selection probes pass |
| History | Append-only transcript, questions and compaction | Production transcript/compaction parity untested |
| Interrupted execution | Running turns become interrupted; waiting approvals remain | Interrupted fake tool call is replayed after restart |
| Send safety | Retained operation identity and outcome reconciliation | One fake effect from two attempts, because fixture code deduplicates |
| Deletion | Transaction deletes conversation and messages | Stream cleanup retains snapshots; explicit snapshot deletion prevents resume |

Read `crates/daemon/src/assistant/mod.rs` (`start_turn`),
`crates/store/src/assistant.rs` (`mark_interrupted_assistant_turns`) and
`crates/assistant/src/turn.rs` (`model_input_with`) to verify the current recovery
contract. The daemon clears pending approvals before execution. After a crash,
it does not automatically rerun the interrupted assistant call. The next model
request gets an in-memory interrupted result, without rewriting stored history.
This is deliberately conservative about uncertain effects, not an absence of
all persistence or restart handling.

The candidate removal area is the generic loop, question handling and
compaction, not all assistant code or shared `mxr-llm` routing. Approval policy,
mail trust boundaries, operation identity, grounded links, failure readouts and
client IPC remain application responsibilities. Verify send handling in
`crates/daemon/src/handler/mutations.rs` and `send_recovery.rs`; the fixture's
deduplication is much simpler than real provider reconciliation.

## What the experiments add

Mastra core `1.79.0`, libSQL `1.27.0` and MCP `2.5.0` were compared with Eve
`0.83.2`. The machine's seven-day package-age policy excluded Mastra `1.81.0`
and Eve `1.0.0-rc.1`. These are tested-version findings, not claims about their
latest releases.

- Both candidates passed ordinary approval and restart after settled suspension.
- Mastra recovered after the parent received the first approval event in about
  0.5 seconds including worker startup. Eve did not settle that probe within
  the 30-second observation window. This does not locate every internal
  checkpoint or test every possible crash instant.
- Two Mastra full runs recovered the interrupted fake send in about 17.3
  seconds, with two tool attempts, one deduplicated effect and no reapproval.
  Eve's continued send turn did not settle within 30 seconds.
- Mastra's first recovery claim reported `skipped: 1` while the dead worker's
  lease was live. Only later replay and persisted settlement counted as
  recovery. A successful helper return alone was insufficient evidence.
- Mastra's stdio client discovered and called a synthetic search tool on MCP
  protocol `2025-06-18`. Real mxr tool authorization, workspace and account
  restrictions were not exercised.
- Mastra `cleanup()` left two sentinel-bearing snapshots. Explicit deletion of
  `durable-agentic-execution` and `durable-agentic-loop` removed the sentinel
  from queryable SQLite rows; resume failed after restart. Eve retention left
  the prompt-derived session title, and retirement of the interrupted send
  failed. Neither result proves physical erasure from pages, WAL or backups.

Inspect the separate attempt/effect counters and before/after marker tables in
the JSON observations above. The observations are saved measurements; this
documentation closeout did not rerun the prototypes.

## Keep the recommendation conditional

Mastra is the better next integration candidate on this evidence. A migration
is not yet justified solely by features we already ship. The experiment adds a
Node worker, native libSQL distribution and another data lifecycle; its saved
startup samples show 239–246 MB worker RSS on one machine.
It did not measure net code removal, packaging size or long-term maintenance.

Before deciding to replace Rust, compare one daemon-owned adapter with the code
it actually retires. Require read-only tools, then one exact-selection archive
approval through existing IPC and a client, on an isolated demo profile. Keep
human approval authority in the daemon and preserve the original operation ID
on tool replay. Verify cancellation, model routing/consent, transcript migration
and deletion before expanding the tool set. Subagents, scheduling and eval
integration remain untested possibilities, not demonstrated migration benefits.

Use the [assistant blueprint](../blueprint/25-assistant.md) for shipped behaviour
and the [user guide](../../site/src/content/docs/guides/assistant.md) for normal
operation. No framework setup is required to use the current assistant.

## Remaining documentation-source drift

The CLI's authored help still says every change waits for approval
(`crates/daemon/src/cli/mod.rs`, captured by the assistant help snapshot).
The generated CLI reference repeats it. Current gate rules allow local draft
creation without approval and user-enabled reversible changes. Correcting the
help source and its snapshot requires a separate code change; do not hand-edit
the generated reference to hide that mismatch.
