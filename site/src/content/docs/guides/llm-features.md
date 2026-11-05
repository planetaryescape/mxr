---
title: LLM features (summarize, draft assist)
description: Configure Ollama, LM Studio, Claude through Anthropic, GPT models through OpenAI (each with OpenRouter as fallback), or any OpenAI-compatible endpoint, with an optional cloud fallback, for thread summarisation and draft assist in mxr.
---

## Configure mxr-native model features

mxr-native model features are disabled by default. The base `[llm]` config
uses a local Ollama endpoint and model; each feature can override enabled
state, endpoint, model, key source, context window, or timeout. Omitted
override values inherit from the base config. Each feature has its own guide:

| Feature | Guide | What the LLM does |
|---|---|---|
| `mxr summarize` | this page | thread → Markdown summary |
| `mxr draft-assist` | this page | thread + instruction → draft body in your voice |
| `mxr compose assist` | [this page](#draft-for-me-builds-on-what-you-typed) | what you typed + the conversation → a proposal that keeps your sentences, fills in what's missing and shows the diff |
| `mxr draft eval` | [this page](#check-how-close-drafts-get) | replay your recent replies (`--set mine`), or grade drafts on the labelled set (`--set quick`) |
| `mxr send --check` answer coverage | [Pre-send safety](/guides/pre-send-safety/#answer-coverage) | extract asks from thread, judge whether the draft addresses each |
| `mxr send --check` commitment candidates | [Forgotten work](/guides/forgotten-work/#track-promises-and-requests) | extract "I'll send the deck Friday" promises from drafts |
| `mxr ask` | [Archive intelligence](/guides/archive-intelligence/) | retrieval-grounded answer over local mail, every claim cited |
| `mxr decisions rebuild` | [Archive intelligence](/guides/archive-intelligence/#the-decision-log--mxr-decisions) | extract explicit decisions from threads |
| `mxr briefing thread` / `recipient` | [Briefings and loop-in](/guides/briefings-and-loop-in/) | dormant-thread / long-gap recap from the local thread transcript or relationship baseline |
| delivery extraction | [Deliveries](/guides/deliveries/) | confirm a shortlisted email is a real shipment, extract merchant / carrier / items / ETA |
| row and reader gists | [Clear the desk](/guides/now/#see-what-each-row-asks) | one line on what a conversation is about and what it asks of you, with the ask quoted from the message |
| `mxr triage` | [CLI: `mxr triage`](/reference/cli/triage/) | sort search results into action, FYI and routine with the summarize model, caching a verdict per message |

Each feature has an explicit disabled path when its effective `enabled`
setting is false. A per-feature override can enable that feature even when
the base `[llm] enabled` is false. Pure LLM commands such as `mxr summarize` and `mxr draft-assist` return
`LLM is disabled`; features with deterministic substrate, such as briefings
or safety checks, return the local fallback instead of pretending synthesis
succeeded.

Enable a native feature by setting `[llm] enabled = true` in your config.
The external agent that calls mxr supplies its own reasoning model and does
not need an mxr-native model configured to run local searches or other
daemon operations. If the agent uses a hosted model, mail results returned
to it leave the machine through that agent's model service. The built-in
[assistant](/guides/assistant/) is a separate client experience; its model
picker is configured through the native `assistant` feature override.

Some model names go to a hosted provider instead of `base_url`: `claude-*`
to Anthropic, then OpenRouter; `gpt-<number>` to OpenAI, then OpenRouter,
unless `base_url` is a server of your own off this machine; and, with the
default `base_url`, OpenRouter-style `vendor/model` ids from known vendors to
OpenRouter. The [routing table](/reference/config/#llm) lists them all. Every
other model name goes to `base_url`. Check `mxr llm status --format json` to
see where each feature goes.

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
| **Claude** (`claude-*` models) | _not used_ | _not used_ | Anthropic's own API, with OpenRouter as fallback. See [Use Claude](#use-claude-through-anthropic-with-openrouter-as-fallback). |
| **GPT** (`gpt-6-luna`, `gpt-5.6-terra` and other `gpt-<number>` models) | _not used_ while `base_url` is on this machine | _not used_ while `base_url` is on this machine | OpenAI's own API with a stored key, with OpenRouter as fallback. See [Use GPT models](#use-gpt-models-through-openai-with-openrouter-as-fallback). |

mxr's local-first stance: model features are off until you set
`[llm] enabled = true`, and the default endpoint is Ollama on
`localhost`, so completions never leave your machine. A cloud endpoint is
opt-in through the same config block, with an API key you supply. mxr ships
no key and runs no relay: requests go straight from the daemon to the
endpoint you name.

## Use Claude through Anthropic, with OpenRouter as fallback

A model whose name starts with `claude-` does not use `base_url` or
`api_key_env`. mxr sends it to Anthropic's own API first and to OpenRouter
only when Anthropic isn't working. Store a key for each, then point every AI
feature at the model:

```bash
op read "op://<vault>/<item>/credential" | mxr llm key set anthropic
op read "op://<vault>/<other-item>/credential" | mxr llm key set openrouter
mxr config set llm.enabled true
mxr config set llm.model claude-haiku-5-5
mxr llm status
```

The key is read from stdin, so it never appears in your shell history or a
process listing, and `mxr llm key set` refuses to read from a terminal. It
goes into `secrets.toml` next to your account passwords (readable only by
you, written only by the daemon), never into `config.toml`. If no key is
stored, mxr reads `ANTHROPIC_API_KEY` and `OPENROUTER_API_KEY`; a stored key
wins. A new key applies to the next request without a restart.

```bash
mxr llm key status                   # which providers have a key, and from where; never the key
mxr llm key set anthropic --dry-run  # check a piped key without storing it
mxr llm key remove anthropic --dry-run
mxr llm key remove anthropic
```

`mxr config set llm.<key>` applies to a running daemon straight away, for
the keys listed in the [config reference](/reference/config/#llm). If the
daemon can't apply it, the command prints `Restart the daemon to apply: mxr
restart`. A hand edit of `config.toml` applies on `mxr restart`, or with the
next `mxr config set llm.<key>`, which reloads all of `[llm]` from the file.

OpenRouter's name for the model is derived from the id: `claude-haiku-5-5`
becomes `anthropic/claude-haiku-5.5`. A model written as `anthropic/...` goes
to OpenRouter only. OpenRouter-style names from known vendors (`openai/...`,
`google/...`, `qwen/...` and the others in the [routing table](/reference/config/#llm))
go to OpenRouter too, but only while `llm.base_url` is still the default;
once you point it at a server of your own, those names are your server's.
Per-feature overrides route the same way.

### What mxr remembers about each provider

Every request goes to the first working provider. When one fails, mxr
remembers it for all callers, so a batch of row gists makes one failed
attempt, not one per row:

| Failure | What mxr does |
|---|---|
| 401, 402 or 403 (bad key, billing) | Skips the provider for an hour, or until you store a new key |
| 404, or a 400 naming an unknown model | Skips it for an hour, or until the daemon applies a config change |
| 408, 429, 529, 5xx, timeout, no connection | Skips it for 1 minute, doubling to 30 minutes on each further failure. A `retry-after` in seconds from the provider replaces the wait, up to an hour |
| A refusal, or an answer that ran out of tokens | Nothing: that request fails, the provider stays in use |

When the wait is over, one request tries the provider again while everything
else keeps using the fallback; if it works, traffic returns. If every
provider is out, background work stops without sending anything, and a
command you run still makes one attempt on whichever recovers soonest and
names each provider's state in its error. This memory is in the daemon only;
a restart forgets it, and so does any config change the daemon applies.

`mxr llm status` shows each feature's model, the providers in order with
their state (`ok`, `cooling until 14:32 (429 rate limited)`,
`broken: 401 invalid key (rechecked 15:32)`, `retry due`, `being retried`,
`no key`), and `[now: ...]` for where a request would go. When every route
is out it says `(no route available; a command you run tries openrouter
once)`: background work waits, and a command makes one attempt on the route
that comes back soonest. A feature privacy settings keep from its model shows
`[blocked: <reason>]`. `--format json` carries the same under `features`, and
the web app's **Settings > Language model** shows the chain read-only. The web app cannot take a key;
keys go in only through the CLI.

Claude 5 models reject temperature and similar settings, so mxr sends none.
Both providers are cloud services: everything under [Know what each request
sends](#know-what-each-request-sends-and-where) applies, including the
`llm.allow_cloud_relationship_data` rule, and drafts and gists are labelled
as written by a cloud model, with the provider that answered
(`claude-haiku-5-5 via Anthropic`).

## Use GPT models through OpenAI, with OpenRouter as fallback

A model whose name is `gpt-` and a number (`gpt-6-luna`, `gpt-5.6-terra`,
`gpt-4o-mini`) is OpenAI's own. It works like a Claude model: mxr sends it to
OpenAI's API first and to OpenRouter, as `openai/<model>`, when OpenAI isn't
working. Store the key the same way:

```bash
op read "op://<vault>/<item>/credential" | mxr llm key set openai
mxr llm key status
```

If no key is stored, mxr reads `OPENAI_API_KEY`; a stored key wins. An
OpenRouter key alone also serves these models. The `openai` row in `mxr llm
status` and `mxr llm key status` shows its state like the others.

`base_url` and `api_key_env` don't apply to these ids while `base_url` is the
default Ollama or a server on this machine, because those servers don't serve
OpenAI's models. If you point `base_url` at a remote server of your own, such
as `https://api.openai.com/v1` with `api_key_env = "OPENAI_API_KEY"`, a `gpt-*`
model keeps going there, with OpenAI's request rules below. Local model names such
as `gpt-oss:20b` never match, since they have no number straight after `gpt-`.

OpenAI's current models reject `max_tokens` and any temperature but the
default, so mxr sends the token cap as `max_completion_tokens`, leaves room in
it for reasoning, and sends no temperature. GPT-6 Luna and the GPT-5.6 models
take tool calls on Chat Completions only with reasoning off, so requests with
tools to those ids carry `reasoning_effort: "none"` through OpenAI and
`reasoning: {"effort": "none"}` through OpenRouter; no other id gets either.

## Fall back to a cloud model when your endpoint is out

A model on a server of your own (Ollama on a laptop that sleeps, a machine
reached over Tailscale) has no second route. Add `[llm.fallback]` and mxr
sends requests to a hosted model while that server is out, then moves back
when it answers:

```toml
[llm]
enabled = true
base_url = "https://ollama.example.net/v1"
model = "gemma4:latest"
context_window = 32768

[llm.fallback]
model = "gpt-6-luna"        # the default when the table is present
# context_window = 128000   # defaults to llm.context_window
```

The fallback goes through the same providers as that model on its own:
`gpt-6-luna` to OpenAI, then to OpenRouter as `openai/gpt-6-luna`, each only
with a key. An OpenRouter key alone is enough:

```bash
op read "op://<vault>/<item>/credential" | mxr llm key set openrouter
mxr config set llm.fallback.model gpt-6-luna
```

`mxr config set llm.fallback.model ''` turns it off. The fallback is off
unless the table is there.

`base_url` is the first route, and mxr remembers its health the way it does
a provider's ([what mxr remembers](#what-mxr-remembers-about-each-provider)):

- The first failure (no connection, a timeout, a 5xx, a rate limit) sends
  that request on to the fallback in the same call, and every request after
  it skips `base_url` for 1 minute, doubling to 30 minutes. A rejected key or
  an unknown model (401, 402, 403, 404) also falls back, and skips `base_url`
  for an hour. Any other error, such as an empty reply or another 400, comes
  back to the caller without trying the fallback. A batch of
  background work makes one failed attempt on `base_url`, not one per item.
- When the wait is over, the next request checks `GET <base_url>/models`
  before sending any mail. If the server answers, the request and the ones
  after it go to `base_url` again; if not, that request goes to the fallback
  and the wait doubles. Other requests keep using the fallback while the one
  check runs.
- Commands you run and background work both fall back. Background work
  shares its `background_request_timeout_secs` among the routes it may still
  try, so with one fallback provider `base_url` gets about 20 seconds of the
  default 45 before it counts as out. If your model is slow to answer
  background work, raise `background_request_timeout_secs` in `config.toml`
  and run `mxr restart`; `mxr config set` doesn't take this key.

It applies to every feature whose model `base_url` serves, including a
per-feature override with its own `base_url`. A `claude-*` model, a `gpt-*`
model on the default or a local endpoint, and a `vendor/model` id that goes to
OpenRouter don't use `base_url` and ignore `[llm.fallback]`; a `gpt-*` model
on a remote server of your own uses it. Answers from
the fallback are labelled with the provider (`gpt-6-luna via OpenAI`,
`openai/gpt-6-luna via OpenRouter`); answers from `base_url` keep the model's
name. Prompts are built to fit the smaller context window of the two.

The fallback is a cloud service, so a configured fallback counts as cloud
for what each request may carry, even while `base_url` answers: drafts carry
your other emails only with `llm.allow_cloud_relationship_data = true`.
Relationship summaries, commitments, answer coverage, briefings, decisions,
`mxr ask` and the assistant use the fallback
only with that setting. Without it they stay on `base_url`, and their
background work [waits for it](#background-work-waits-out-an-endpoint-outage)
and runs when it answers again. With a `base_url` off this machine they are
refused without the setting, as before.

`mxr llm status` shows the chain for each feature:

```text
fallback=gpt-6-luna

features:
  summarize             gemma4:latest  https://ollama.example.net/v1: cooling until 14:32 (unreachable) → openai: no key → openrouter: ok  [now: openrouter]
```

With `base_url` on this machine, a relationship feature shows where it
stays:

```text
  relationship_summary  gemma4:latest  https://ollama.example.net/v1: ok  [now: base_url]  [no fallback: relationship data stays off the cloud fallback; set llm.allow_cloud_relationship_data=true to permit it]
```

`--format json` adds `fallback_model` and, under each feature, `endpoint`
(`base_url`'s `state`, `reason`, `retry_at` and `next`), `fallback` (the
providers, shaped like `routes`) and `fallback_withheld`. A fallback model mxr
can't route, such as a local model name, is refused by `mxr config set`. A
daemon that loads one from a hand edit runs without the fallback, and status
prints `fallback not used:` with the reason (`fallback_error` in JSON).

## Pick the assistant's model

The [assistant](/guides/assistant/#pick-the-model) has a model picker: the web
app's model mark under the message box, `M` in the terminal pane, and `mxr
assistant model set <id>`. It lists the models in `llm.models`, five
by default:

```toml
[llm]
models = ["gemma4:latest", "gpt-6-luna", "gpt-5.6-terra", "claude-haiku-5-5", "claude-sonnet-5-5"]
```

Each entry is a model id, and where it runs follows from the id as above.
Leave the setting out, or empty, for the five. Change it with
`mxr config set llm.models '["gemma4:latest", "gpt-6-luna"]'`, which applies
straight away; a hand edit applies on `mxr restart`. A pick is stored as
`[llm.overrides.assistant] model`, so it changes only the assistant.

## Configuration

In the file printed by `mxr config path`:

```toml
[llm]
enabled = true

# Ollama (recommended local default):
base_url = "http://localhost:11434/v1"
model = "qwen2.5:3b-instruct"
api_key_env = ""

# Common alternatives: uncomment and adjust:
# base_url = "http://localhost:1234/v1"        # LM Studio
# base_url = "https://api.openai.com/v1"       # OpenAI
# base_url = "https://api.groq.com/openai/v1"  # Groq

context_window = 8192
request_timeout_secs = 120
```

The daemon reads the env var named in `api_key_env` from its own environment
when it starts or reloads `[llm]` (empty = no `Authorization` header sent).
Start the daemon from a shell that has the variable set; changing it in
another shell does nothing until `mxr restart`. Keeping the secret out of the
config file is intentional: the config is checked into dotfiles.

Check what the running daemon is using:

```bash
mxr llm status
mxr llm status --format json
```

`mxr config set llm.<key> <value>` reloads `[llm]` in the running daemon, so
the model switches without a restart. Nothing watches `config.toml`: a hand
edit applies on `mxr restart`, or with the next `mxr config set llm.<key>`.

## Background work waits out an endpoint outage

Relationship summaries, commitments, delivery confirmation, row gists and
to-do resolution checks run in the background. When the model's endpoint is out (no connection, a
timeout, a rate limit, a 5xx, a bad key or an unknown model), the email or
contact being processed is not blamed for it:

- After five endpoint failures in a row, background model calls pause for 5
  minutes, doubling to 10 minutes while the endpoint stays out. Commands you run
  are not paused. `mxr llm status` prints `background calls paused after
  repeated endpoint failures until 14:37`, and `--format json` carries the
  time as `background_paused_until`.
- When a pause ends, mxr checks a `base_url` endpoint with `GET
  <base_url>/models`, which sends no mail. One check runs at a time, and a
  failed check starts the next, longer pause.
- Relationship summaries and delivery confirmations skipped while the
  endpoint was out are processed once each soon after a check succeeds,
  without waiting for new mail. Gists are written again the next time a
  client asks for them, and to-do checks wait in their own queue. A parcel
  email seen during the outage keeps its heuristic result until then.

With Claude or GPT through mxr's own routes, or with a
[fallback](#fall-back-to-a-cloud-model-when-your-endpoint-is-out) configured,
there is no separate pause: work skipped while every route was out runs once
one of them is back (see
[what mxr remembers about each provider](#what-mxr-remembers-about-each-provider)).

For relationship summaries, commitments and delivery confirmation, a reply
mxr can't use, such as an empty one, or a 400 about the request, does count
against that input: it is retried after 1 h, 4 h, 16 h and 24 h, and after the
fifth failed try it waits until new mail changes it. `mxr deliveries scan` retries skipped
messages straight away.

The pause, the retry times and the skipped work are saved to
`llm-background.json` next to `mxr.db` in mxr's data directory, readable only
by you, so a restart keeps them. The file holds contact addresses, message
ids and content hashes, never mail text. If it can't be read, mxr logs a
warning and starts fresh. Any config change the daemon applies clears the
pause, the retry times and each input's given-up mark, but keeps the skipped
work.

## Know what each request sends, and where

The endpoint decides where your mail goes. With a local endpoint nothing
leaves your machine. With a cloud endpoint, the text below goes to that
provider, and that includes mail other people sent you, so use a provider
whose API data terms cover it. A key from the provider's API console is
the path mxr supports; mxr has no sign-in with a consumer chat
subscription.

| Work | When it runs | What a cloud endpoint receives |
|---|---|---|
| `mxr summarize`, `mxr draft-assist`, `mxr draft` and `mxr draft refine` | When you run them | The thread the command works on. Stored relationship summaries of the people in it only with `llm.allow_cloud_relationship_data = true` |
| `mxr triage` | When you run it | Each message it sorts |
| Row gists | In the background, for conversations from people that a client shows, and for automated mail while the web app's Updates page or `mxr briefing gists THREAD_ID... --generate --automated` asks | That conversation's messages. Newsletters and mailing lists are never sent. With `llm.allow_cloud_relationship_data = true`, also a line counting your history with the sender (messages, usual reply time, last contact) |
| Delivery confirmation | After sync, for mail the local heuristic shortlisted as shipping | That message's sender, subject and cleaned body |
| Relationship summaries, commitments, answer coverage, briefings, decisions, `mxr ask` | In the background or on demand | Nothing: these are refused for a cloud endpoint unless `llm.allow_cloud_relationship_data = true` |
| Drafts | When you ask for one | The conversation being drafted. Your other emails and habits only with `llm.allow_cloud_relationship_data = true` |
| The [assistant](/guides/assistant/) | When you send it a message | Your message, what you have open, and the mail it reads while answering (search results, threads, drafts). Refused for a cloud endpoint unless `llm.allow_cloud_relationship_data = true`, since it reads across your mailbox |
| Archive's records and answer box | After sync and on demand | Nothing: records are read by rules and schema.org markup on your machine. Only when no record matches does the answer box fall back to `mxr ask`, under that row's rule |

Automatic promises and requests from [stored asks](/guides/forgotten-work/)
use deterministic local rules. The commitments model feature covers optional
draft proposals; it does not decide automatic To do or Waiting on rows.

Prompts that carry mail wrap it as untrusted data and tell the model it is
not an instruction. Check what the running daemon uses:

```bash
mxr llm status --format json
```

`base_url` shows the endpoint. A `localhost`, `127.x.x.x` or `::1` host is
local; anything else is cloud. A `claude-*` model, and a `gpt-*` model
routed to OpenAI, have no `base_url` and are always cloud. With
`[llm.fallback]`, mail goes to the fallback model's providers while `base_url`
is out (OpenAI then OpenRouter for `gpt-*`, Anthropic then OpenRouter for
`claude-*`, OpenRouter alone for a `vendor/model` id),
so each feature that may fall back shows `cloud: true`; use a provider whose
API data terms cover other people's mail.

## Send one feature to a different model

Each feature can override `[llm]` on its own. Fields you leave out
inherit from `[llm]`. This keeps every feature on your local model except
thread summaries, which go to a cloud model:

```toml
[llm.overrides.summarize]
model = "gpt-6-luna"        # key from `mxr llm key set openai`
```

An override takes `enabled`, `base_url`, `model`, `api_key_env`,
`context_window` and `request_timeout_secs`. Other keys in the table, and
feature names mxr doesn't know, are ignored and named once in the daemon log.

The keys are `summarize` (which also covers gists and `mxr triage`),
`relationship_summary`, `commitments`, `draft_assist`, `draft_new`,
`draft_refine`, `humanize_rewrite`, `answer_coverage`, `archive_ask`,
`decision_log`, `briefing`, `delivery_extraction` and `assistant` (which needs
a model that can call tools). Voice matching and expert lookup run locally, so
the old `voice_match` and `expert` keys are retired and ignored. With `[llm.compose] model` set, every draft uses that
model and the `draft_assist`, `draft_new`, `draft_refine` and
`humanize_rewrite` overrides are ignored; `mxr llm status` shows the compose
model on those rows. The relationship rule above still applies to an
override that points at a cloud endpoint.

## Model tiers for email modes are planned

The current mode classifier assigns deterministic placements during sync;
that work does not require an LLM. The `fast` and `smart` model tiers in
[the email modes plan](/guides/email-modes/) are future work. They are not
current configuration keys or shipped model behavior. Current background
model calls, including delivery confirmation and configured gists, are
listed in [what each request sends](#know-what-each-request-sends-and-where).

## Recommended local models

For Ollama (`ollama pull <model>`):

- **`qwen2.5:3b-instruct`**: ~2GB, very fast on a laptop, good summary
  quality. Default in mxr's example config.
- **`qwen2.5:7b-instruct`**: ~4.4GB, noticeably better at draft
  generation for longer threads.
- **`llama3.2:3b`**: comparable to Qwen 3B, slightly different tone.
- **`llama3.1:8b`**: larger but stronger. Good if you have the RAM.

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
voice-match metadata when a relationship profile exists, rewrite iteration
count, and `provenance` (see [See where a draft came from](#see-where-a-draft-came-from)).

In the TUI, open the palette (`:`) → **Summarize thread**. The summary runs in
the background and renders above the message body; opening a long uncached
thread can also start a debounced background summary. In the web reader,
**Summarize** in the thread's More menu, or **Summarize thread** in the palette,
calls the same daemon request and renders the result in the **AI overview**
collapsible above the thread. Summarize has no key; `B` opens the thread
briefing.

## Draft for me builds on what you typed

`mxr compose assist` asks for writing help and prints a proposal. It changes
nothing in your draft until you accept it with `mxr compose apply`. What it
does depends on what you've typed:

| Your text | What happens |
|---|---|
| Nothing | Writes the reply, forward note or new message from the conversation and your instruction. |
| Some text | Finishes it. Your sentences stay word for word. It adds a greeting and sign-off the way you write them, an answer to each question in the message you haven't covered, and a closing line if you'd write one. What only you can decide comes back as a `[[?: …]]` gap. |
| Some text, part of it selected | Rewrites only the selected part and returns the whole text with the rest exactly as it was. |

`--polish` fixes typos and grammar and tightens what you typed without adding
anything. `--shorter`, `--warmer`, `--formal` and `--proofread` adjust
whichever action runs, `--instruction` says what you want said, and
`--write`, `--finish` or `--polish` overrides the choice.

```bash
# Finish a reply you've started. --text is a file, or - for stdin.
mxr compose assist --reply-to MESSAGE_ID --text - <<< "Thursday works. numbers by wed"

# Work on a stored draft; its text (without signature) is read for you.
mxr compose assist --draft DRAFT_ID --instruction "mention the invoice" --model gpt-6-luna

# Change only part of the text, as byte offsets between whole characters.
mxr compose assist --draft DRAFT_ID --select 16..31 --warmer

# Ask again about a proposal you have read: the note is applied to that proposal.
mxr compose assist --draft DRAFT_ID --revise PROPOSAL_ID --instruction "drop the greeting"

# Accept the proposal. Proposal ids are in the output; they last 30 minutes.
mxr compose apply PROPOSAL_ID --draft DRAFT_ID
```

The other targets are `--forward MESSAGE_ID --to ADDRESS` and `--new
--account ACCOUNT --to ADDRESS --subject SUBJECT`. It makes one model call
and takes as long as the model does; the stage lines ("Reading the
conversation", "Writing with gemma4", "Checking") go to stderr.

`--model` must name an id in `llm.models` and applies to this request only.
The web help line and proof, Raycast's **Draft for Me** submenu and the TUI's
pre-proof model list and proof `m` picker use the compose catalogue. Inspect it
with `mxr compose models --format json`. New drafts start from
`[llm.compose] model`; set
it in Settings → Compose or with `mxr config set llm.compose.model <id>`. A
cloud model can draft with the current conversation while relationship
sharing is off; it does not receive past-mail context, and mxr never enables
that sharing setting for you.

The readable output starts with `Proposal <id> · <action> · <model>`, then
shows your words plain, what was added in `{+…+}` and what was removed in
`[-…-]`. After the text come which of your sentences changed or were dropped,
counted from 1 (sentences split at `.`, `!`, `?` and line breaks), the gaps
still to fill, and the `Accept: mxr compose apply …` command. `--format json` prints the whole proposal:
`proposal_id`, `action`, `base` (SHA-256 of the text it was made from, ignoring
blank space at both ends and normalizing CRLF to LF), `text`, `changes`,
`gaps` (each with its byte range in `text`), `provenance`, `yours_changed`
(positions counted from 0), `composer_version`, and the style check's
`style_score`, `style_marks` and `style_version`.

The daemon checks the model's answer before you see it:

- A signature or forwarded-message header the model wrote is removed. Yours
  stay untouched.
- For a selection, any change outside it is put back.
- A number, date or amount absent from the conversation, subject, your
  text, your instruction and today's date becomes a gap that shows the guess,
  as in `[[?: amount, the draft said £40]]`.
- A draft that still has a gap [can't be sent](/guides/pre-send-safety/#unfilled-gap).

`mxr compose apply` writes the proposal into the stored draft's text, keeping
the signature and the forward header, and only while that text still matches
what the proposal was made from. If you kept typing, it refuses and writes
nothing, and you ask again. The daemon keeps proposals in memory for 30
minutes, so a restart loses them. The web app applies a proposal to the text
in its editor itself; `compose apply` is for the CLI, the TUI and agents, and
the MCP tool `mxr_draft_assist` asks for the same proposal.

The privacy rules are the other draft commands'. A cloud model sees your
other emails only with `llm.allow_cloud_relationship_data`, and typed text
that an earlier suggestion wrote from that history is refused for a cloud
model without it. `mxr draft-assist` and `mxr draft refine` still work.

## What the prompts look like

The summarizer asks for concise Markdown that names who said what,
preserves concrete dates, deadlines and asks, and ends with next steps.

Drafts are written as you. Every draft (a reply, a forward, a new email,
or a refine) gives the model:

- **Who you are**: the name you send as, your addresses, and today's date.
- **Real emails you wrote**: your replies to this person paired with what
  they were answering, then your other mail to them, then your replies in
  general when you haven't written to them much. Quoted history and
  signature blocks are stripped first.
- **Your habits in plain sentences**, measured from those emails: how you
  open and sign off, your usual length, contractions, lowercase, exclamation
  marks, emoji.
- **The conversation** as ME and THEM turns with dates, the message being
  answered marked, newest turns kept when it's too long for the model's
  context window.
- **What you've typed**, when there is any, in a block of its own inside
  the untrusted-mail markers (a reply you started can quote the email you're
  answering), and the selected part in a second block. Your instruction
  stays outside the markers.
- **The task**: your instruction, the tone you picked (or "match my
  examples"), a length in words taken from how much you usually write to
  this person, and one line for each of `--shorter`, `--warmer`, `--formal`
  and `--proofread` you asked for.

Your instruction is optional for a reply: the draft answers what the
message asks, and anything only you can decide comes back as a
`[[?: ...]]` gap to fill instead of a guess. The model is told to state
only facts from the conversation or your instruction. Its output is
cleaned before you see it (no "Here's a draft", `Subject:` lines, code
fences or `[Your Name]`), and a draft cut off by the token limit is retried
with more room rather than handed over half-written.

### Check how close drafts get

```bash
mxr draft --format json eval --set mine --limit 20
mxr draft --format json eval --set mine --limit 20 --rewrite
mxr draft --format json eval --set quick
```

`--set mine` is the default. It replays your most recent replies. For each
one it rebuilds the conversation and your voice examples as they were just
before you replied, drafts a reply through the same path the web, TUI and
CLI use, and compares it with what you actually sent: greeting and sign-off
match, length ratio, and numbers the draft invented.

The cutoff excludes the answer being replayed and later message evidence.
Stored relationship summaries and voice profiles are omitted because their
rows do not preserve the source history needed to reconstruct them at that
cutoff. Replay therefore measures drafting from the available earlier mail,
not an exact reconstruction of every historical setting.

Mine skips the humanizer rewrite stage by default, even when normal drafts
have automatic rewriting enabled. Pass `--rewrite` to allow it. A rewrite
model call still requires the humanizer and auto-fix to be enabled and the
draft score to fall below the configured threshold. A skipped rewrite is not
marked in the replay report. `--rewrite` applies only to Mine; Quick rejects it.
An older remote daemon that cannot honor this policy rejects the request;
update the host before running it again.

`--set quick` drafts the labelled drafting set instead: 40 synthetic cases,
each a short thread, a few emails in a sample voice, and sometimes text
already typed (finished the way `mxr compose assist` finishes it) or an
instruction. Every draft is graded against what its case
required. The typed sentences must survive, each question must be answered,
facts nobody has given must be left as `[[?: …]]` gaps, no number may appear
that no input carries, the length must fall in the case's band, and there
must be no subject line, preamble, repeated sign-off or run of the thread
copied back. The output counts how many drafts passed each check. Each draft
also records a style score, reported as a mean; style is not a pass/fail
check.

Both sets save nothing. Each case starts with one LLM call. A draft that hits
the token limit is retried with more room, so a case can make more than one
call, and each retry adds to the time and any model charges. `--set quick`
takes a while on a slow local model. Use `--format jsonl` for one line per
case, and run either again after changing models to compare.

With `--format table`, each case prints its six checks, then the draft, and the
run ends with the counts. This is the shape, from the demo profile's built-in
script, which finishes typed text but echoes its prompt for everything else, so some checks fail:

```text
── price-open · 79 words · A price nobody has given: leave a gap, invent nothing
  pass kept      nothing typed to keep; no typed text
  pass answers   no question to answer; nothing to check
  FAIL gaps      leave a [[?: …]] gap for: price; no gap for: price
  FAIL invented  no number, date or amount that no input carries; not in any input: 23, 2026, 1, 2
  FAIL length    8 to 60 words; 79 words
  pass leftovers no subject line, preamble, fence or placeholder, no repeated sign-off, and no run of 8 words copied from the thread; clean
       draft: Voice-matched draft: / ...

40 cases (0 did not draft) with mxr-demo-canned; style score mean 95 (humanizer score: 100 reads plain, lower reads more like AI; reported, not graded)
kept      40/40 passed
answers   32/40 passed
gaps      36/40 passed
invented  22/40 passed
length    22/40 passed
leftovers 40/40 passed
```

### See where a draft came from

Every draft says which model wrote it, whether your history was used, and
which messages it was written from. Run a draft with the default table
output:

```bash
mxr draft-assist THREAD_ID "say Friday works" --format table
```

The notes after the body go to stderr and name each source with the command
that opens it:

```text
Friday works for me. I'll bring the notes.

[Local model gemma4 · used 4 of your emails to Nora · history used · review before sending]
Your emails it matched the voice of:
  2026-09-29 · you to Nora  mxr cat b5700241-8d68-562a-92d3-ec69db053cea
  2026-09-30 · you to Nora  mxr cat 7294ff6c-2134-5909-8ea2-e2a1f0694ca9
Messages it read from this conversation:
  2026-09-30 · Theo  mxr cat 81fe7849-7927-5257-918d-5dc693765c9b
  2026-09-30 · Nora  mxr cat e0c0e763-0faf-5773-98b9-049d0d90d765
humanizer=92/100
```

`humanizer=` is the draft's style score. `voice_match=` is added when mxr
has a baseline of how you write to that person, and `rewritten Nx` when the
humanizer rewrote the draft.

Run `mxr cat <message-id>` on any line to read that message. The line is
read from what the request actually did, not from your config:

- **Local model** or **Cloud model** comes from the endpoint that answered.
  With `[llm.fallback]`, a draft your local endpoint wrote says local, and one
  the fallback wrote says cloud. If a config reload lands mid-request, the
  draft still describes the model that wrote it.
- The web app shows the model as a mark beside the text, `gemma4 · local` or
  `gemma4 · cloud`, with where it ran in its tooltip. Gists, the AI overview
  and drafts all wear it. The overview knows the model but not where it ran,
  so its mark shows only the model.
- **history used** means your past emails, the habits measured from them,
  or the relationship summary went into the prompt. **history not used**
  means none did, for example a cloud model without
  `llm.allow_cloud_relationship_data`.
- **Your emails it matched the voice of** lists only the emails that fit
  in the prompt, not every one mxr considered.
- **Local** means the endpoint's host is exactly `localhost`, a `127.x.x.x`
  address or `::1`, with no user name in the URL. Anything else, including
  `0.0.0.0`, LAN addresses and `localhost.example.com`, counts as cloud.
  mxr never follows an LLM endpoint's redirect (it reports an error naming
  the new address instead), and calls a local endpoint directly, ignoring
  `HTTP_PROXY`, so a local model's prompts stay on your machine.
- The humanizer's rewrite pass is named by the model that answered it:
  **rewritten by ...** when its text is what you got, **rewrite attempted
  by ..., not used** when the model saw the draft but its text was no
  better, and **rewrite by cloud model ... skipped to keep your history
  local** when the draft was written from your history and the rewrite
  model is a cloud one you haven't opted in to. When a kept rewrite was
  followed by a pass that wasn't used, both models are named.
- A draft body that contains text an earlier draft wrote from your history
  won't be refined or humanized by a cloud model you haven't opted in to.
  mxr says so instead: refine it with a local model, or set
  `llm.allow_cloud_relationship_data = true`. mxr recognises that text for
  7 days, across restarts, by storing a hash of each sentence in its local
  database, never the text itself.

`--format json` carries the same facts under `provenance`:

```json
"provenance": {
  "model": "gemma4",
  "locality": "local",
  "history_used": true,
  "voice_examples": [
    { "message_id": "b5700241-...", "thread_id": "309ae832-...", "date": "2026-09-29T10:54:34Z",
      "from_me": true, "person": "nora@foundry.example", "person_name": "Nora Kim" }
  ],
  "conversation": [ ... ]
}
```

`rewrite` is present only when a rewrite pass ran or was skipped, and its
`outcome` is `applied`, `rejected` or `skipped`. A reply always drafts from
the conversation's own account: naming another account in the request is
refused. `mxr draft`
and `mxr draft refine` print the same notes, and `mxr humanize` names the
model under `rewrite` when it rewrote your text. In the web app the line
sits under the proof in **Draft for me**. Select
**Sources** to list the messages; each one opens in a new tab on that
message, so the draft you're judging stays where it is. The TUI shows the
same line and source list with the draft.

Relationship and profile context is guarded separately for cloud
providers. With `llm.allow_cloud_relationship_data = false` (the default)
and a non-local endpoint, the relationship features are off and a draft
sees only the conversation being drafted: none of your other emails,
habits or relationship summaries leave your machine, and the draft says so.
Set it to `true` only when you want your cloud LLM to draft in your voice.

Every generated draft also runs through a deterministic local humanizer
detector. It flags common AI-writing patterns such as stock vocabulary,
em-dash overuse, sycophantic openers, filler phrases, and rule-of-three
formatting. Detection does not require an LLM.

Compose proposals also carry the local score and marks for patterns the
deterministic checker maps. The web proof shows their context on hover;
Raycast and the TUI list the names and context. Marks are advisory and do not
change the draft. Compose compares supported marks with normalized rates
from up to 200 recent sent messages, computed locally for the request. The
derived rates are not sent to cloud models; source emails still follow
`llm.allow_cloud_relationship_data`.
With `humanizer.auto_fix` enabled, a style excess can trigger one repair call
to the same pinned model with only the marked generated sentences and their
catalogue instructions. A malformed, failed or unsafe repair leaves the
first proposal intact; typed sentences are never sent for repair.

You'll get the best results with models that follow instructions well
and stay close to the source: Qwen 2.5 instruct, Llama 3 instruct,
and the GPT-5 family all do this reliably.

## Limits and what's deferred

- **One response per request**: summaries, drafts and gists arrive whole;
  only the assistant streams its replies.
- **Prompt size**: drafts fit the prompt to `context_window`, between 6,000
  and 28,000 characters, keeping the newest messages. Summaries send the
  whole thread, so a very long thread can overflow a small window.
- **Thinking models**: a model that spends its whole token budget on hidden
  reasoning and returns nothing is asked once more with
  `reasoning_effort: "none"`. If that works, mxr keeps sending the field to
  that endpoint until the daemon restarts or reloads `[llm]`.
- **Summary cache**: unchanged threads reuse the cached summary. The cache
  hash includes weak relationship context, so changed relationship summaries
  or style data invalidate stale summaries. Opening a thread returns a valid
  cached summary with the thread payload when one exists.
- **Humanizer rewrites**: with `humanizer.auto_fix` on (the default), a draft
  scoring below `humanizer.score_threshold` (70) gets up to
  `humanizer.max_rewrite_iterations` (2) rewrite passes on the same model.

## Disabling

Set `[llm] enabled = false` in your config (or remove the section
entirely). Pure LLM commands return `LLM is disabled`; mixed features use
their deterministic fallback when one exists.

## Demo mode: canned offline responses

When `mxr demo` is active, every LLM-backed feature is answered by an
in-process **canned provider** instead of the real backend. The provider
inspects each request's system prompt to classify it (summarize, briefing,
draft-assist, ask, voice, commitments, decisions, …) and returns a realistic
template, so recordings of the demo show real-looking output without
spending tokens or needing an `OPENAI_API_KEY`.

The swap happens inside `build_llm_provider` based on `MXR_INSTANCE ==
mxr-demo`. It supersedes whatever `[llm]` is configured for your real
profile, so even if you have a paid OpenAI key wired up, `mxr demo` will
never call it. Exit demo mode with `mxr demo stop` to return to your
configured backend.

## In real life

- **Catching up after vacation:** `mxr summarize --search
  'is:unread newer_than:7d' --limit 200 | less` turns 200 unread threads
  into 200 short summaries.
- **Replying to legalese:** `mxr summarize THREAD_ID` first, then
  `mxr draft-assist THREAD_ID "ack the request, ask for a 2-week
  extension"` to generate a draft you can polish.
- **Triage rule of thumb:** if a thread has 4+ messages and you're
  about to reply, summarise it first. The cost is 2 seconds with
  local Ollama; the saving is reading the whole chain again.

## Agent prompts that work

```text
"Summarise every thread in my reply-later queue with more than 3
messages. Use `mxr replies --format jsonl | jq -r .thread_id | sort -u |
xargs -n1 mxr summarize`. Group by sender so I can batch responses."
```

```text
"Draft a polite decline to the latest message from acme@example.com.
Use `mxr draft-assist --search 'from:acme@example.com' --first
--instruct "decline politely"`. Show me the draft; don't send."
```

## See also

- [Pre-send safety](/guides/pre-send-safety/): the safety pipeline's LLM-backed answer-coverage check
- [Forgotten work](/guides/forgotten-work/): LLM-confirmed commitment extraction from drafts
- [Archive intelligence](/guides/archive-intelligence/): `mxr ask` and the decision log, citations required
- [Briefings and loop-in](/guides/briefings-and-loop-in/): dormant-thread briefings, deterministic expert lookup, and whois
- [Recipes: talking to your agent](/guides/recipes/#talking-to-your-agent)
- [For agents](/guides/for-agents/)
- [Config: `[llm]`](/reference/config/#llm)
- [CLI: `mxr summarize`](/reference/cli/summarize/), [`mxr draft-assist`](/reference/cli/draft-assist/), and [`mxr llm`](/reference/cli/llm/)
