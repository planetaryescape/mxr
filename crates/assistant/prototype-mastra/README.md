# Test Mastra as mxr's local agent runtime

Run this throwaway spike on `spike/mastra-agent-runtime` to reproduce the local
approval, crash and deletion probes. It never connects to mxr, opens real mail,
or sends an email. No production code changes.

## Run

Use Node 24 or newer, from this directory:

```sh
agent-work run -- npm ci --ignore-scripts --no-audit --no-fund
npm run experiment
npm run mcp-probe
```

The main experiment uses a scripted OpenAI-compatible model endpoint and the
real Mastra native durable agent. It forks an SDK worker over Node IPC, with a
separate local libSQL database per invocation. No Mastra CLI, HTTP server,
Studio, Docker, Redis, Inngest or cloud account is involved.

The MCP probe separately checks discovery and invocation over stdio against a
synthetic MCP server that negotiates protocol `2025-06-18`. It is not a test
against mxr's actual MCP server or its authorization policy.

For the real model smoke run, have `gemma4:latest` available in an existing
local Ollama service:

```sh
npm run experiment -- --local --smoke-only
```

You can set `SPIKE_MODEL` and `SPIKE_MODEL_URL` to another local OpenAI-compatible
model endpoint. The script does not pull models or restart Ollama. To check
just search/approval/deletion without the crash probes, run
`npm run experiment -- --smoke-only`.

Every run writes its observations, full synthetic events, model requests and
worker logs under a fresh ignored `evidence/` directory. `observations/` keeps
selected portable results. Failure returns a nonzero exit code. A run lease can
delay recovery after a crash by around 15 seconds; the observation deadline is
30 seconds, as in the Eve spike.

## What is being tested

- Search returns both shipping notices, without mutations.
- Preview and real archive use the same IDs and preview token.
- Decline changes nothing. Approve changes exactly the two selected IDs.
- Repeating an approval does not execute it again.
- Crash after settled suspension, then resume from a new process.
- Kill at the first approval chunk received by the parent, then resume.
- Commit a fake send, kill before returning its result, recover after the
  default run lease expires, and verify actual tool replay and terminal state.
  A skipped recovery claim alone is not counted as success.
- `cleanup()` versus deleting both persisted workflow snapshots; check a
  sentinel across queryable SQLite tables and attempt resume after restart.

The fake send deliberately holds its first result for 60 seconds so the runner
can kill the worker after the effect. Its persistent operation ledger prevents
a second effect when Mastra replays the call. That ledger is application code,
not a Mastra exactly-once guarantee.

## Configuration and limits

Pinned packages: `@mastra/core@1.79.0`, `@mastra/libsql@1.27.0`, and
`@mastra/mcp@2.5.0`. These satisfy the machine's seven-day npm release-age
policy. The latest core release, `1.81.0`, was not installed or tested.

The worker registers `createDurableAgent()` with persistent storage and
`recovery: { durableAgents: 'auto' }`. It explicitly invokes
`mastra.recoverAllDurableAgents()` as the embedded host's startup recovery hook
for the interrupted-send case. It uses default claim timing, with no forced
takeover. The worker exposes only three synthetic-mail tools and does not
configure Memory, observability exporters, skills or subagents. It sets
`MASTRA_TELEMETRY_DISABLED=1` before importing Mastra.

Snapshot deletion here is logical deletion in the configured store, not secure
erasure of SQLite pages, WAL, backups, logs or this experiment's evidence files.
The experiment does not establish production authorization, prompt-injection
resistance, UI parity, application packaging or native-driver portability.

Read [the evaluation](../../../docs/issues/mastra-runtime-evaluation.md) for
measured results and the next integration boundary.
