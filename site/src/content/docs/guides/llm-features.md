---
title: LLM features (summarize, draft assist)
description: Configure Ollama, LM Studio, OpenAI, or any OpenAI-compatible endpoint for thread summarisation and draft assist in mxr.
---

## What's in scope

mxr ships two LLM-driven features today:

- `mxr summarize <thread-id>` — concise Markdown summary with per-message
  bullets and concrete next steps.
- `mxr draft-assist <thread-id> "<instruction>"` — generate a draft
  reply grounded on the thread context, your instruction, and similar
  prior sent mail when semantic search is ready. Output goes to stdout;
  **never auto-sends**.

Both are off by default. Enable them by setting `[llm] enabled = true`
in your config and pointing at any backend that speaks the
**OpenAI Chat Completions** schema.

## Backends supported

Because the wire format is OpenAI-compatible, the same client covers
every major option:

| Backend | `base_url` | `api_key_env` | Notes |
|---------|------------|---------------|-------|
| **Ollama** (local) | `http://localhost:11434/v1` | _(empty)_ | No auth header. Default in mxr's config. |
| **LM Studio** (local) | `http://localhost:1234/v1` | _(empty)_ | No auth header. |
| **OpenAI** | `https://api.openai.com/v1` | `OPENAI_API_KEY` | |
| **Groq** | `https://api.groq.com/openai/v1` | `GROQ_API_KEY` | Very fast, tight context windows. |
| **OpenRouter** | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` | Single key, many models. |
| **Together AI** | `https://api.together.xyz/v1` | `TOGETHER_API_KEY` | |
| **Mistral La Plateforme** | `https://api.mistral.ai/v1` | `MISTRAL_API_KEY` | |
| **Anthropic via OpenAI-compatible proxy** | depends | depends | |

mxr's local-first stance: the recommended config uses Ollama or LM
Studio so completions never leave your machine. Cloud endpoints are
opt-in via the same single config block.

## Configuration

In `~/.config/mxr/config.toml`:

```toml
[llm]
enabled = true

# Ollama (recommended local default):
base_url = "http://localhost:11434/v1"
model = "qwen2.5:3b-instruct"
api_key_env = ""

# Common alternatives — uncomment and adjust:
# base_url = "http://localhost:1234/v1"        # LM Studio
# base_url = "https://api.openai.com/v1"       # OpenAI
# base_url = "https://api.groq.com/openai/v1"  # Groq

context_window = 8192
request_timeout_secs = 120
```

The API key is read from the env var named in `api_key_env` at runtime
(empty = no `Authorization` header sent). Keeping the secret out of
the config file is intentional — the config is checked into dotfiles;
the env var lives in your shell init.

Check what the running daemon is using:

```bash
mxr llm status
mxr llm status --format json
```

Config reloads rebuild the runtime provider, so changing `[llm]` and
reloading the daemon account/config runtime switches the model without
restarting the process.

## Recommended local models

For Ollama (`ollama pull <model>`):

- **`qwen2.5:3b-instruct`** — ~2GB, very fast on a laptop, good summary
  quality. Default in mxr's example config.
- **`qwen2.5:7b-instruct`** — ~4.4GB, noticeably better at draft
  generation for longer threads.
- **`llama3.2:3b`** — comparable to Qwen 3B, slightly different tone.
- **`llama3.1:8b`** — larger but stronger. Good if you have the RAM.

For LM Studio: any GGUF model loaded via the LM Studio UI works. Use
the model identifier shown in LM Studio's "Local Server" tab as `model`
in the config.

## Usage

```bash
# Summarize a long thread:
mxr summarize THREAD_ID

# Generate a reply draft:
mxr draft-assist THREAD_ID "decline politely, suggest next month"
mxr draft-assist THREAD_ID "ack and ask for the deadline"
```

`mxr draft-assist` writes the body to stdout. Pipe it into your editor:

```bash
mxr draft-assist THREAD_ID "decline politely, suggest next month" \
  | $EDITOR -
```

Or use `--format json` for structured output:

```bash
mxr summarize THREAD_ID --format json
mxr draft-assist THREAD_ID "..." --format json
```

Draft JSON includes the generated body, model id, humanizer score summary,
voice-match metadata when a relationship profile exists, and rewrite iteration
count.

## What the prompts look like

Both features use a tuned system prompt followed by the thread
context. The summarizer asks for concise Markdown that names who said
what, preserves concrete dates/deadlines/asks, and ends with next
steps. The draft assistant asks for **just the reply body, no greeting
line if the thread is mid-conversation, no signature, plain prose,
matching the formality and length of the thread**.

When semantic search is enabled and indexed, draft assist first looks
for similar prior outbound messages, filters out inbound mail and the
current thread, and includes up to three examples as voice grounding.
If semantic search is disabled or unavailable, draft assist still works
with only the current thread and instruction.

When relationship data exists for a contact, mxr injects it as weak
background guidance. The current thread and your explicit instruction
override it, and the prompt tells the model not to invent familiarity
outside stored known topics, commitments, or summaries.

Every generated draft also runs through a deterministic local humanizer
detector. It flags common AI-writing patterns such as stock vocabulary,
em-dash overuse, sycophantic openers, filler phrases, and rule-of-three
formatting. Detection does not require an LLM.

You'll get the best results with models that follow instructions well
and stay close to the source — Qwen 2.5 instruct, Llama 3 instruct,
and the GPT-4o family all do this reliably.

## Limits and what's deferred

- **Single-shot completions** — no streaming yet. The features are
  short-form (≤2KB outputs); a single round-trip beats streaming for
  this use case.
- **24KB prompt budget** — long threads truncate oldest-first.
- **Semantic grounding is opportunistic** — prior sent examples are included
  only when semantic search is ready and has indexed matching sent messages.
- **Summary cache** — unchanged threads reuse the cached summary. The cache
  hash includes weak relationship context, so changed relationship summaries
  or style data invalidate stale summaries.
- **Humanizer auto-rewrite is not the core contract** — deterministic scoring
  is available locally; automatic rewrite loops are a separate pipeline layer.

## Disabling

Set `[llm] enabled = false` in your config (or remove the section
entirely). All LLM-backed commands then return `LLM is disabled`
errors and graceful degradation kicks in everywhere — no client code
needs to know whether the feature is on.

## Demo mode: canned offline responses

When `mxr demo` is active, every LLM-backed feature is answered by an
in-process **canned provider** instead of the real backend. The provider
inspects each request's system prompt to classify it (summarize, briefing,
draft-assist, ask, voice, commitments, decisions, …) and returns a realistic
template — so recordings of the demo show real-looking output without
spending tokens or needing an `OPENAI_API_KEY`.

The swap happens inside `build_llm_provider` based on `MXR_INSTANCE ==
mxr-demo`. It supersedes whatever `[llm]` is configured for your real
profile, so even if you have a paid OpenAI key wired up, `mxr demo` will
never call it. Exit demo mode with `mxr demo stop` to return to your
configured backend.

## In real life

- **Catching up after vacation:** `mxr search 'is:unread newer_than:7d'
  --format ids | xargs -n1 mxr summarize | less` — turn 200 unread
  threads into 200 short summaries.
- **Replying to legalese:** `mxr summarize THREAD_ID` first, then
  `mxr draft-assist THREAD_ID "ack the request, ask for a 2-week
  extension"` to generate a draft you can polish.
- **Triage rule of thumb:** if a thread has 4+ messages and you're
  about to reply, summarise it first. The cost is 2 seconds with
  local Ollama; the saving is reading the whole chain again.

## Agent prompts that work

```text
"Summarise every thread in my reply-later queue with more than 3
messages. Use `mxr replies --format jsonl | jq -r .id | xargs -I{}
mxr summarize {}`. Group by sender so I can batch responses."
```

```text
"Draft a polite decline to the latest message from acme@example.com.
Use `mxr search 'from:acme@example.com' --format ids | head -1` to
get the thread id, then `mxr draft-assist`. Show me the draft —
don't send."
```

## See also

- [Recipes — talking to your agent](/guides/recipes/#talking-to-your-agent)
- [For agents](/guides/for-agents/)
- [Config — `[llm]`](/reference/config/#llm)
- [CLI — LLM features](/reference/cli/#llm-features)
