---
title: Trace model calls in a developer build
description: Build mxr with the langfuse feature and see every prompt, reply and token count in a Langfuse on your machine.
---

Build mxr from source with the `langfuse` feature, run [Langfuse](https://langfuse.com) on your machine, and every model call mxr makes shows up there with what it sent and what came back. Use it to find out why a feature's answer was wrong, empty or slow.

Released mxr can't do this. Homebrew, `install.sh` and the GitHub release binaries are built without the feature, so their tracing code and its OpenTelemetry crates aren't compiled in. You need a source checkout and Docker.

## Run Langfuse on this machine

Clone Langfuse and add an override next to its `docker-compose.yml`:

```bash
git clone https://github.com/langfuse/langfuse.git
cd langfuse
```

```yaml
# docker-compose.override.yml
services:
  langfuse-web:
    ports: !override
      - 127.0.0.1:3000:3000
    environment:
      TELEMETRY_ENABLED: "false"
      LANGFUSE_INIT_ORG_ID: mxr
      LANGFUSE_INIT_PROJECT_ID: mxr-local
      LANGFUSE_INIT_PROJECT_PUBLIC_KEY: pk-lf-mxr-local
      LANGFUSE_INIT_PROJECT_SECRET_KEY: sk-lf-mxr-local
      LANGFUSE_INIT_USER_EMAIL: dev@mxr.local
      LANGFUSE_INIT_USER_PASSWORD: mxr-local
  langfuse-worker:
    environment:
      TELEMETRY_ENABLED: "false"
  minio:
    ports: !override
      - 127.0.0.1:9090:9000
      - 127.0.0.1:9091:9001
```

```bash
docker compose up -d
```

Langfuse publishes its web port and MinIO on every network interface and reports anonymous usage by default. The override binds them to loopback, turns the reporting off, and creates a project, its API keys and a login on first start. Fixed keys and a simple password are fine because nothing outside this machine can reach them.

Open `http://localhost:3000` and sign in as `dev@mxr.local` with password `mxr-local`. The `LANGFUSE_INIT_*` values apply only to a new database; after changing them, recreate it with `docker compose down -v`.

## Build and start mxr with tracing

From your mxr checkout:

```bash
cargo build -p mxr --features langfuse
export LANGFUSE_BASE_URL=http://localhost:3000
export LANGFUSE_PUBLIC_KEY=pk-lf-mxr-local
export LANGFUSE_SECRET_KEY=sk-lf-mxr-local
export LANGFUSE_TRACING_ENVIRONMENT=development   # optional
target-cli/debug/mxr restart
```

Restart with the binary you just built. A daemon started by another `mxr` on your `PATH`, or by launchd, has neither the feature nor these variables, and the daemon reads them only when it starts.

## Check that the daemon is tracing

```bash
target-cli/debug/mxr doctor
```

Under `Findings:`, the daemon says where traces go:

```text
  · [Daemon] LLM tracing is on, to the Langfuse at http://localhost:3000
```

If it refused the URL, the finding is a warning that says why:

```text
  ! [Daemon] LLM tracing is off: https://cloud.langfuse.com isn't on this machine. Traces hold your mail, so mxr sends them only to a Langfuse on localhost
      → Fix the LANGFUSE_* variables, then run `mxr restart`
```

A tracing build with the variables unset lists an info finding saying tracing is off. A released mxr has no tracing finding at all.

## Read a trace

Use a model feature, for example `mxr summarize <thread-id>` or `mxr assistant send "<message>"`. Traces arrive within a few seconds; the daemon sends anything still queued when it stops.

In the Langfuse UI, open the `mxr-local` project and its traces. From a terminal, the Langfuse CLI reads them with the same three variables:

```bash
npx langfuse-cli api observations list --fields core,basic,io,model,usage --json
```

## What each trace holds

Each feature call is a trace named by its config key (`summarize`, `commitments`, `assistant`, ...), the names `[llm.overrides.<key>]` and `mxr llm status` use. Under it, each request to a model is a generation with:

- the messages sent and the reply, in OpenAI's message format;
- the model's thinking, when it returns any (Ollama's `reasoning`, Claude's thinking blocks);
- token usage, model parameters, finish reason, endpoint and any error.

A provider fallback, or the retry with `reasoning_effort: none` for a thinking model that spent its budget on reasoning, is a generation of its own, so the trace shows why a call was slow or empty. A call cut off by a timeout ends with a warning instead of an output.

An assistant turn is one `assistant-turn` trace with its model calls and tool calls nested in order, and each conversation is a Langfuse session. A tool call's metadata holds the `tool_call_id` it answers, the id in the `tool_calls` of the generation before it.

## Traces never leave this machine

Traces hold whole prompts and replies, which is other people's mail. The daemon sends them only when `LANGFUSE_BASE_URL` is `localhost`, `127.x.x.x` or `::1`. It refuses Langfuse Cloud, LAN addresses and lookalikes such as `localhost.evil.example`, and its exporter uses no proxy and follows no redirect. `OTEL_EXPORTER_OTLP_*` variables can't move it.

Tracing changes one thing a model server receives: streamed requests to OpenAI-compatible servers (the assistant on Ollama) carry `stream_options: {"include_usage": true}`, because those servers report token usage on a stream only when asked.

## Stop tracing

```bash
unset LANGFUSE_BASE_URL LANGFUSE_PUBLIC_KEY LANGFUSE_SECRET_KEY
target-cli/debug/mxr restart
docker compose stop        # in the langfuse checkout; keeps the traces
```

`docker compose down -v` deletes the traces and the Langfuse database.
