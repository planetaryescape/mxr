# A local model needs a cloud fallback, and endpoint outages use up an input's retries

Checked at `origin/main` `3bb328db2` (release 0.6.209) on 2026-11-04. Both halves shipped on 2026-11-05: the outage half first, then the fallback chain; see "What shipped" below each section. The OpenRouter data-terms question is still open.

The planned setup runs the daemon on an always-on home server and sends model work to Ollama on BK's MacBook over Tailscale. The MacBook is often asleep or away, so the endpoint is unavailable for hours or days at a time. Two gaps make that setup lose work.

## A compatible-endpoint model has no fallback route

Fallback exists only for hosted model names. `claude-*` goes to Anthropic, then OpenRouter, and `gpt-<n>` goes to OpenAI, then OpenRouter (`crates/llm/src/routed.rs:1`, `site/src/content/docs/guides/llm-features.md`). A local model such as `gemma4:latest` on a compatible `base_url` has one endpoint. When that endpoint is down, every feature fails until it returns.

The change needed is an ordered, opt-in chain for compatible-endpoint models, for example local Ollama first and an OpenRouter model second. The fallback must meet these conditions:

- It reuses `RouteHealth`, so a batch of background work makes one failed attempt per route rather than one per item.
- A step from a local route to a cloud route passes the same gates as a cloud-primary configuration (`llm.allow_cloud_relationship_data`, `PrivacyBlocked`). A feature blocked from the cloud waits for the local route instead of falling back.
- Each route can name its own model and context window, because the local and cloud models differ.
- Output keeps the existing label naming the model and provider that answered.
- `mxr llm status` shows the chain and the route in use.

### What shipped, 2026-11-05 (rung 2)

- `[llm.fallback]` (`model`, default `gpt-6-luna`; optional `context_window`) is opt-in: no table, no fallback (`crates/config/src/types.rs`, `LlmFallbackConfig`). `mxr config set llm.fallback.model <id>` sets it and refuses an id `routes_for_model(.., Endpoint::Default)` doesn't route; an empty value removes it.
- It applies to any effective config (base or per-feature override) whose model `base_url` serves. `RoutedProvider` takes the endpoint as its first route (`RoutedProviderConfig::endpoint`), keyed in the shared `RouteHealth` as `RouteKey::Endpoint` (base URL and model), so a batch makes one failed attempt on it. The endpoint needs no stored key and never appears in the key store. Hosted answers are labelled `<model> via <Provider>`; endpoint answers keep the model's own name.
- When the endpoint's wait ends, the request that takes the half-open probe checks `GET <base_url>/models` before sending the prompt. A failed check spends no prompt and that request continues to the fallback; a passed check recovers the route and fires the recovery signal.
- Privacy (`crates/daemon/src/state.rs`, `llm_fallback_chain`): relationship features without `allow_cloud_relationship_data` get the endpoint alone under the same `RouteHealth`, so their background work is put off with `CircuitOpen` while it is out and resumes on it. With a remote `base_url` they stay blocked, as before. Any provider with a fallback reports `leaves_machine() = true`, so drafts carry relationship data only with the opt-in.
- Background budget: the endpoint gets its share of the background timeout like any route (about 20 s of the default 45 with one fallback provider). A slow local model that times out counts as out.
- `mxr llm status` and its JSON show `fallback_model`, `fallback_error`, and per feature `endpoint`, `fallback` and `fallback_withheld` (all additive with serde defaults). An unroutable fallback is refused on reload; a daemon that loads one runs without it and says why.

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

Thread gists use a separate pull-based queue and already recover. A failed gist backs off for 10 minutes (`crates/daemon/src/handler/thread_gists.rs:41`) and is queued again the next time the list loads. Delivery extraction falls back to its heuristic result on a model failure (`extract.rs:145`).

Nothing re-ran skipped inputs before the fix (checked at `2f7fb9d31`, release 0.6.210). The relationship worker only sees a contact when one is enqueued (`crates/relationship/src/service.rs:55`, `:76`): after a sync with new mail (`crates/daemon/src/loops.rs:1156`), at the end of an initial backfill (`loops.rs:716`), or when mail is deleted (`loops.rs:1283`). A summary skipped with `CircuitOpen` returned `Ok(false)` (`summary.rs:116`) and was dropped from the queue. The delivery scan runs only on newly synced message ids (`loops.rs:1215` calling `crates/daemon/src/handler/deliveries.rs:94`) and on an explicit `mxr deliveries scan` (`deliveries.rs:118`). A shortlisted message whose model call failed degraded to the heuristic (`deliveries.rs:237`) and was never scanned again, so it was not enriched after the endpoint recovered.

The to-do resolution worker had the same charging problem in its own durable queue: any model error other than disabled, blocked or `CircuitOpen` used one of its five attempts (`crates/daemon/src/handler/todo_resolution/worker.rs:260-262`).

### What shipped, 2026-11-05

- `LlmError::Endpoint` carries every route-class failure that used to be `LlmError::Other` (5xx, 408, 529, 404 or an unknown-model 400, 401-403 other than `Unauthorized`, a redirect, an unreadable reply, and "no LLM route is working"). It reads exactly as before. `LlmError::is_endpoint_failure()` is true for it and for `Unreachable`, `Timeout`, `RateLimited`, `Unauthorized` and `CircuitOpen`. The conversion happens in `AttemptError::new` (`crates/llm/src/failure.rs`).
- Endpoint failures never touch the ledger. `FeatureLlmRuntime::record_background_failure` puts the input off instead; any other failure is recorded as before. Relationship summaries, commitments and delivery extraction use it. The to-do resolution worker treats an endpoint failure like `CircuitOpen`: the attempt is handed back and the pair waits five minutes.
- Only endpoint failures count toward the breaker. An empty reply, a refusal or a request-specific 400 counts as the endpoint working.
- When the breaker's cooldown ends, one caller checks the endpoint while every other caller gets `CircuitOpen`. `OpenAiCompatibleProvider` checks with `GET {base_url}/models` (at most 10 s, no redirects, its API key). A failed check re-opens the breaker with the doubled cooldown and spends no input. A 404 from the listing counts as answering, because some compatible servers have no listing. Providers without a check (and `RoutedProvider`) keep using the next real request as the check.
- A daemon loop (`crates/daemon/src/llm_background.rs`) re-runs put-off inputs once each: relationship contacts go back to the relationship worker, and messages go back through the delivery scan, so a delivery that fell back to the heuristic during an outage is enriched once the endpoint answers. It runs when the breaker closes after an outage, when a `RouteHealth` route recovers, or, every 30 s, once the cooldown is over and the cheap check passes.
- The breaker, the ledger and the put-off inputs are saved to `llm-background.json` in the data directory: wall-clock times, atomic write, mode 0600, at most every 30 s when changed and once on shutdown. A missing, corrupt or other-version file is logged and ignored. Tests and the demo instance never write it.

What remained after this: the fallback chain, since shipped (see the first section). `RouteHealth` is still in memory only. Without a fallback, recovery takes up to one breaker cooldown (at most 10 minutes, shortened from 60 because the check is a cheap `GET /models`) after the endpoint returns. With a fallback, the endpoint is a `RouteHealth` route instead, so its wait is the route cooldown: up to 30 minutes, or 1 hour after a broken verdict such as a bad key.

## Acceptance

- With a fallback configured, stopping the local endpoint moves eligible features to the next route after one failed attempt, and restoring it moves traffic back.
- A simulated 48-hour endpoint outage with an injected clock leaves no input given up. After recovery, every pending input is attempted once.
- A feature blocked from cloud routes does not fall back, and its work resumes on the local route.

## Decisions, 2026-11-05 (BK)

- Background work falls back to the cloud while the MacBook is away. The gates in the first section still apply: a feature blocked from cloud routes waits for the local route.
- The default fallback model is `gpt-6-luna` through OpenRouter.
- The ledger and breaker state persist across daemon restarts. Shipped, see above.

## Open questions

- Do the data terms of OpenRouter and its upstream providers for `gpt-6-luna` cover other people's mail?

## Review follow-ups deferred from #883

From CodeRabbit's review of the fallback PR, 2026-11-05. Fixed in the PR: the base and per-feature providers are installed in one update, so a reload can't hand a relationship feature the cloud-capable base provider; a zero `llm.fallback.context_window` is rejected; a 403 from `GET /models` counts as the endpoint answering; three doc corrections. Deferred:

- Draft provenance reports `Cloud` whenever a fallback is configured, even when the local endpoint answered. It should follow the route that answered.
- Compose-model drafts build a fresh `RouteHealth` per call, so their endpoint failures aren't remembered or shown in `mxr llm status`.
- `mxr llm status` builds drafting rows without `llm.compose.model`, so those rows can show the base chain instead of the compose model.
- An unroutable `[llm.fallback]` model makes `sync_llm_from_disk` return early, so a saved `llm.enabled = false` isn't applied until restart. Account and config reloads drop the fallback with only a warning.
- Endpoint route health ignores `api_key_env`, so overrides sharing `base_url` and model share one health entry.
- A successful `GET /models` check resets the endpoint's failure count, so completions failing with 503 behind a working listing never back off past 60 s.
- The reasoning retry on the endpoint gets the full share again instead of the time left.
- `prompt_budget_chars` doesn't reserve the hosted routes' thinking headroom.
- The status chain label shows `base_url:` without the URL, and marks no route `next` when only the foreground forced attempt would run.


## Gaps found in the docs-parity audit, 2026-11-05

The docs now describe what the code does. Fixed alongside: `mxr summarize` sent stored relationship summaries to a cloud model without `allow_cloud_relationship_data`; the web reader showed a `y` key on Summarize that does nothing; `mxr config get`'s key list left out `llm.fallback.model`. Fixed in the follow-up, 2026-11-05:

- A `gpt-*` model on a remote `base_url` now gets OpenAI's request rules: `max_completion_tokens` with thinking room, no temperature, and `reasoning_effort: "none"` on tool calls for Luna and GPT-5.6. Not yet run against the live API.
- The OpenRouter route turns reasoning off for those models' tool calls as `reasoning: {"effort": "none"}`, the shape OpenRouter documents.
- `mxr llm status` shows the background pause and when the endpoint is checked next (`background_paused_until`).
- `llm.overrides.voice_match` and `llm.overrides.expert` are retired keys: ignored, and named once in the log.

Not fixed:

- An `[llm.overrides.*]` table silently ignores keys it doesn't take (`deny_unknown_fields` is off).
