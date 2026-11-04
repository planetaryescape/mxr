# A local model needs a cloud fallback, and endpoint outages use up an input's retries

Checked at `origin/main` `3bb328db2` (release 0.6.209) on 2026-11-04.

The planned setup runs the daemon on an always-on home server and sends model work to Ollama on BK's MacBook over Tailscale. The MacBook is often asleep or away, so the endpoint is unavailable for hours or days at a time. Two gaps make that setup lose work.

## A compatible-endpoint model has no fallback route

Fallback exists only for hosted model names. `claude-*` goes to Anthropic, then OpenRouter, and `gpt-<n>` goes to OpenAI, then OpenRouter (`crates/llm/src/routed.rs:1`, `site/src/content/docs/guides/llm-features.md`). A local model such as `gemma4:latest` on a compatible `base_url` has one endpoint. When that endpoint is down, every feature fails until it returns.

The change needed is an ordered, opt-in chain for compatible-endpoint models, for example local Ollama first and an OpenRouter model second. The fallback must meet these conditions:

- It reuses `RouteHealth`, so a batch of background work makes one failed attempt per route rather than one per item.
- A step from a local route to a cloud route passes the same gates as a cloud-primary configuration (`llm.allow_cloud_relationship_data`, `PrivacyBlocked`). A feature blocked from the cloud waits for the local route instead of falling back.
- Each route can name its own model and context window, because the local and cloud models differ.
- Output keeps the existing label naming the model and provider that answered.
- `mxr llm status` shows the chain and the route in use.

## Endpoint outages are charged to the input being processed

Background inputs, such as one contact's commitments or one message's delivery details, go through the attempt ledger. A failed input retries after 1 h, 4 h, 16 h and then daily. After `LEDGER_MAX_ATTEMPTS` (5) it is given up until its content changes. The ledger is in memory, so a restart allows one more try (`crates/llm/src/background.rs:64-73`).

The shared breaker opens after 5 consecutive failures for 5 minutes, doubling to 60 minutes (`background.rs:17-19`, `crates/llm/src/lib.rs:515-545`). While it is open, callers get `CircuitOpen` and record nothing:

- commitments: `crates/relationship/src/commitments.rs:124`
- relationship summaries: `crates/relationship/src/summary.rs:116`
- deliveries: `crates/deliveries/src/extract.rs:140`

The failures before the breaker trips, and every half-open probe, are real inputs, and those failures are recorded against the input (`commitments.rs:127`, `summary.rs:119`, `extract.rs:144`). `Unreachable`, `Timeout`, `RateLimited` and 5xx errors count the same as an unusable reply. During a multi-day outage, each probe charges one input. Inputs that reach five failures are skipped until new mail arrives or the daemon restarts, even though nothing was wrong with them.

The fix is to keep the two kinds of failure apart:

- **Endpoint failures go to the breaker only.** `Unreachable`, `Timeout`, `RateLimited` and 5xx do not touch the ledger. A large local model's cold load can exceed the background timeout, so `Timeout` belongs here too.
- **Input failures go to the ledger.** An unusable or empty reply, or a request-specific 4xx, is recorded against the input as today.
- **The breaker probes with a cheap request.** It checks the endpoint with something like `GET /v1/models` rather than spending a real input.
- **Recovery is prompt.** When the breaker closes, inputs skipped during the outage are attempted without waiting out a ledger delay.

Thread gists use a separate pull-based queue and already recover. A failed gist backs off for 10 minutes (`crates/daemon/src/handler/thread_gists.rs:41`) and is queued again the next time the list loads. Delivery extraction falls back to its heuristic result on a model failure (`extract.rs:145`). Whether that message is enriched again after the endpoint recovers was not checked.

## Acceptance

- With a fallback configured, stopping the local endpoint moves eligible features to the next route after one failed attempt, and restoring it moves traffic back.
- A simulated 48-hour endpoint outage with an injected clock leaves no input given up. After recovery, every pending input is attempted once.
- A feature blocked from cloud routes does not fall back, and its work resumes on the local route.

## Open questions

- Should background work fall back to the cloud, or wait for the local route while foreground requests fall back?
- Which OpenRouter model should be the default, and do the data terms of OpenRouter and its upstream providers cover other people's mail?
- Should the ledger and breaker state persist across daemon restarts?
