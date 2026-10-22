# Tracing model calls with a local Langfuse

A build with the `langfuse` Cargo feature records every model call in a
Langfuse on this machine. Setting it up, checking it and reading traces are
in the docs site's
[Trace model calls in a developer build](https://mxr.sh/guides/trace-model-calls/)
(`site/src/content/docs/guides/trace-model-calls.md`). This page is for
changing the code.

## Locked decisions

D150 in `docs/blueprint/15-decision-log.md`:

- No shipped binary enables the feature. Without it the tracing code and the
  OpenTelemetry crates aren't compiled; CI fails if `opentelemetry` enters
  the release feature set's dependency tree.
- Traces go only to a loopback Langfuse, through a client with no proxy and
  no redirects, at an endpoint set in code.
- Spans use the OpenTelemetry API, not `tracing`: the daemon's JSON log
  layer writes span fields to its log files, so `tracing` spans would put
  prompts in the logs.

## Code

- `crates/llm/src/trace/`: the spans. `langfuse.rs` builds them with the
  OpenTelemetry API; `off.rs` is the same calls doing nothing, used without
  the feature. Functions whose callers are all `#[cfg(feature = "langfuse")]`
  (`finish_completion_attempt`, `finish_chat_attempt`, `fail_attempt`) exist
  only in `langfuse.rs`.
- `crates/daemon/src/llm_tracing.rs`: reads the variables, refuses
  non-loopback URLs, runs the OTLP/HTTP exporter and reports its state to
  `mxr doctor`.
- `crates/daemon/src/assistant/`: the `assistant-turn` span.
- `crates/assistant/src/turn.rs`: the tool spans (`call_tool`).

## Verification

```bash
scripts/cargo-test -p mxr-llm --features langfuse --test trace_spans
scripts/cargo-test -p mxr-assistant --features langfuse --lib
scripts/cargo-test -p mxr --features langfuse --lib -- llm_tracing
cargo tree -p mxr --features semantic-local,web-ui -e normal | grep -c opentelemetry   # 0
```
