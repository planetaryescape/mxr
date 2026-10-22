# Tracing model calls with a local Langfuse

A developer build can record every model call mxr makes, with what it sent
and what came back, in a [Langfuse](https://langfuse.com) running on your
machine. Use it to see why a feature's answer was wrong, empty or slow.

It is behind the `langfuse` Cargo feature, which no shipped binary enables.
Without it the tracing code and its OpenTelemetry crates aren't compiled, so
release, Homebrew and `install.sh` builds can't trace at all.

## What a trace shows

Each feature call is a trace named by its config key (`summarize`,
`commitments`, `assistant`, ...), the names `[llm.overrides.<key>]` and
`mxr llm status` use. Under it, each request to a model is a generation with
its messages, reply, thinking when the model returns it, token usage, model
parameters, finish reason, endpoint and any error. A provider fallback, or
the retry with `reasoning_effort: none` for a thinking model that spent its
budget on reasoning, is its own generation. A call cut off by a timeout ends
with a warning instead of an output.

An assistant turn is one `assistant-turn` trace with its model calls and
tool calls nested in order, and each conversation is a Langfuse session.

## Traces go only to a Langfuse on this machine

Traces hold whole prompts and replies, which means other people's mail. The
daemon sends them only when `LANGFUSE_BASE_URL` is `localhost`, `127.x.x.x`
or `::1`, never through a proxy and never after a redirect. Langfuse Cloud
and LAN addresses are refused, and `mxr logs` says why. With the variables
unset, a `langfuse` build traces nothing.

## Run Langfuse locally

Clone [Langfuse](https://github.com/langfuse/langfuse) and put this override
next to its `docker-compose.yml`. Langfuse publishes its web port and MinIO
on every network interface and reports anonymous usage by default; the
override binds everything to loopback, turns that off, and creates a project,
its API keys and a login on first start, so there is nothing to set up in the
UI. Fixed keys and a simple password are fine here because nothing outside
this machine can reach it:

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

Start it with `docker compose up -d`. The UI is at `http://localhost:3000`,
login `dev@mxr.local` / `mxr-local`. The init values apply only when the
database is new; after changing them, recreate it with `docker compose down -v`.

## Build and run mxr with tracing

```bash
cargo build -p mxr --features langfuse
export LANGFUSE_BASE_URL=http://localhost:3000
export LANGFUSE_PUBLIC_KEY=pk-lf-mxr-local
export LANGFUSE_SECRET_KEY=sk-lf-mxr-local
export LANGFUSE_TRACING_ENVIRONMENT=development   # optional
target-cli/debug/mxr restart
```

Restart with the binary you just built: a daemon started by another `mxr`
on your `PATH`, or by launchd, has neither the feature nor these variables.
The daemon reads the variables when it starts, and `mxr logs` shows
`LLM tracing to the Langfuse at ...` when it is on. Traces arrive within a
few seconds; the daemon sends what is still queued when it stops.

Read traces back from the terminal with the Langfuse CLI, which needs the
same three variables:

```bash
npx langfuse-cli api observations list --fields core,basic,io,model,usage --json
```

## Code

- `crates/llm/src/trace/`: the spans. `langfuse.rs` builds them with the
  OpenTelemetry API; `off.rs` is the same calls doing nothing, used without
  the feature.
- `crates/daemon/src/llm_tracing.rs`: reads the variables, refuses
  non-loopback URLs and runs the OTLP/HTTP exporter.
- `crates/daemon/src/assistant/`: the `assistant-turn` and tool spans.
- `crates/llm/tests/trace_spans.rs`: the span tree and attributes, run with
  `scripts/cargo-test -p mxr-llm --features langfuse --test trace_spans`.

Open gaps are in [`docs/issues/llm-tracing-followups.md`](../issues/llm-tracing-followups.md).
