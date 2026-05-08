---
title: LLM features (summarize, draft assist)
description: Configure Ollama, LM Studio, OpenAI, or any OpenAI-compatible endpoint for thread summarisation and draft assist in mxr.
---

## What's in scope

mxr ships two LLM-driven features today:

- `mxr summarize <thread-id>` — 2 to 3 sentence summary focused on
  what's actionable for a busy reader.
- `mxr draft-assist <thread-id> "<instruction>"` — generate a draft
  reply grounded on the thread context plus your instruction. Output
  goes to stdout; **never auto-sends**.

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

## What the prompts look like

Both features use a tuned system prompt followed by the thread
context. The summariser asks for **2 to 3 short sentences focused on
what's actionable for a busy reader**, no pleasantries. The draft
assistant asks for **just the reply body, no greeting line if the
thread is mid-conversation, no signature, plain prose, matching the
formality and length of the thread**.

You'll get the best results with models that follow instructions well
and stay close to the source — Qwen 2.5 instruct, Llama 3 instruct,
and the GPT-4o family all do this reliably.

## Limits and what's deferred

- **Single-shot completions** — no streaming yet. The features are
  short-form (≤2KB outputs); a single round-trip beats streaming for
  this use case.
- **24KB prompt budget** — long threads truncate oldest-first.
- **No retrieval-grounded draft assist yet** — the current draft
  assistant uses thread context + instruction only. The future
  extension is to retrieve top-K similar prior sent messages from
  your own corpus (the `crates/semantic/` infrastructure is in place)
  and inject them as few-shot examples to ground the generated voice.
- **No cache for summaries** — re-running `mxr summarize` on the same
  thread re-prompts the model. Inexpensive for local Ollama; worth
  thinking about for cloud APIs with metered costs.

## Disabling

Set `[llm] enabled = false` in your config (or remove the section
entirely). All LLM-backed commands then return `LLM is disabled`
errors and graceful degradation kicks in everywhere — no client code
needs to know whether the feature is on.

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
