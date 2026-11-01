# Evaluate Eve as mxr's local agent runtime

Throwaway spike on `spike/eve-agent-runtime`. It does not change mxr, connect to
a daemon, open a real mailbox, send mail, or use cloud models. Its question is
whether a local Eve process can replace the generic assistant runtime while
mxr keeps authority over mail operations.

## Run the experiment

Use Node 24 or newer. From this directory:

```sh
agent-work run -- npm ci --legacy-peer-deps --ignore-scripts --no-audit --no-fund
agent-work run -- npm run build
npm run info
npm run experiment
```

The default experiment starts the real production Eve server and a scripted
OpenAI-compatible model endpoint on loopback. It searches three fake messages,
previews an archive, denies one request, approves another, kills its own server,
resumes a checkpointed approval, repeats that approval, then retires the sessions.
The mailbox must show exactly one execution against the two shipping messages.
The scripted model isolates runtime behaviour; it is not evidence of model quality.

To exercise a real local model already installed in Ollama:

```sh
npm run experiment -- --local --smoke-only
```

The default model is `gemma4:latest`. Set `SPIKE_MODEL` and `SPIKE_MODEL_URL` to
select another local OpenAI-compatible server. No model is pulled or shared
Ollama process restarted. The smoke run exercises search, denial and approval,
but does not run the scripted crash cases.

These failure probes may exit nonzero. They bound the recovery observation to
30 seconds; a timeout does not prove that recovery is impossible:

```sh
npm run experiment -- --immediate-crash
npm run experiment -- --send-crash
```

The immediate-crash probe removes the one-second checkpoint allowance. The send
probe commits a fake external effect, then crashes before the tool returns.
Its operation ledger is fixture code, not an exactly-once guarantee from Eve.

Each invocation writes JSON results, event transcripts, server logs and scratch
workflow state under a fresh ignored `evidence/` directory. It generates a fresh
route password, uses a separate working directory and stops only its own process
group. `observations/` contains selected synthetic results kept with this spike.
There are no changes to Rust, the installed mxr binary, or its state directories.

## Configuration and version limits

- Eve is pinned to `0.83.2`, the newest release permitted by this machine's
  seven-day npm release-age policy on 2026-11-01. The latest release was
  `1.0.0-rc.1`; it was not installed or tested.
- Use `node_modules/eve/docs/` for this version's APIs. Current website examples
  use a different client/event protocol. For example, this version uses `host`,
  `{ session, response }`, `input.requested`, `requestId`, and `reset()`.
- Only `mail_search`, `mail_archive` and `mail_send` are exposed. Default tools
  and root-copy delegation are disabled. No tool asks for a sandbox.
- CLI telemetry and local traces are explicitly disabled. The selected model
  is a direct local provider, not Vercel AI Gateway.
- `experimental.workflow.retention: 0` removes run payloads when sessions end.
  It does not erase every metadata field. The user prompt remains as the run's
  `$eve.title` in this version.
- The production build prewarmed a Docker template even with default tools
  disabled. This spike's session tools do not need Docker, but a packaged mxr
  integration must avoid that unnecessary build dependency.

Read [the evaluation issue](../../../docs/issues/eve-runtime-evaluation.md)
for the decision, remaining gates and mxr integration work.
