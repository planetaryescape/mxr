---
title: LLM features (summarize, draft assist)
description: Configure Ollama, LM Studio, Claude through Anthropic, GPT models through OpenAI (each with OpenRouter as fallback), or any OpenAI-compatible endpoint for thread summarisation and draft assist in mxr.
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
| `mxr send --check` commitment candidates | [Forgotten work](/guides/forgotten-work/#commitments-promises-you-made) | extract "I'll send the deck Friday" promises from drafts |
| `mxr ask` | [Archive intelligence](/guides/archive-intelligence/) | retrieval-grounded answer over local mail, every claim cited |
| `mxr decisions rebuild` | [Archive intelligence](/guides/archive-intelligence/#the-decision-log--mxr-decisions) | extract explicit decisions from threads |
| `mxr briefing thread` / `recipient` | [Briefings and loop-in](/guides/briefings-and-loop-in/) | dormant-thread / long-gap recap from the local thread transcript or relationship baseline |
| delivery extraction | [Deliveries](/guides/deliveries/) | confirm a shortlisted email is a real shipment, extract merchant / carrier / items / ETA |
| row and reader gists | [Clear the desk](/guides/now/#see-what-each-row-asks) | one line on what a conversation is about and what it asks of you, with the ask quoted from the message |
| `mxr triage` | [CLI: `mxr triage`](/reference/cli/triage/) | sort search results into action, FYI and routine, reusing the cached summary verdict |

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

Recognized `claude-*` and `gpt-*` model IDs route to Anthropic or OpenAI,
with OpenRouter as fallback. Other model IDs use the configured compatible
endpoint. Check `mxr llm status --format json` to see the effective native
configuration. Recognized hosted model IDs select their provider regardless
of `base_url`.

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
| **GPT** (`gpt-6-luna`, `gpt-5.6-terra` and other `gpt-<number>` models) | _not used_ | _not used_ | OpenAI's own API with a stored key, with OpenRouter as fallback. See [Use GPT models](#use-gpt-models-through-openai-with-openrouter-as-fallback). |

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

`mxr config set llm.*` applies to a running daemon straight away. If the
daemon can't apply it, the command prints `Restart the daemon to apply: mxr
restart`. A hand edit of `config.toml` needs that restart.

OpenRouter's name for the model is derived from the id: `claude-haiku-5-5`
becomes `anthropic/claude-haiku-5.5`. A model written as `anthropic/...` goes
to OpenRouter only. Other OpenRouter-style names (`openai/...`, `google/...`)
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
| 404, or a 400 naming an unknown model | Skips it for an hour, or until `[llm]` changes |
| 429, 529, 5xx, timeout, no connection | Skips it for 1 minute, doubling to 30 minutes on each further failure. A `retry-after` from the provider replaces the wait |
| A refusal, or an answer that ran out of tokens | Nothing: that request fails, the provider stays in use |

When the wait is over, one request tries the provider again while everything
else keeps using the fallback; if it works, traffic returns. If every
provider is out, background work stops without sending anything, and a
command you run still makes one attempt on whichever recovers soonest and
names each provider's state in its error. This memory is in the daemon only;
a restart forgets it.

`mxr llm status` shows each feature's model, the providers in order with
their state (`ok`, `cooling until 14:32 (429 rate limited)`,
`broken: 401 invalid key`, `no key`), and `[now: ...]` for where a request
would go. `--format json` carries the same under `features`, and the web
app's **Settings > LLM** shows it read-only. The web app cannot take a key;
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
model keeps going there and mxr does nothing different. Local model names such
as `gpt-oss:20b` never match, since they have no number straight after `gpt-`.

OpenAI's current models reject `max_tokens` and any temperature but the
default, so mxr sends the token cap as `max_completion_tokens`, leaves room in
it for reasoning, and sends no temperature. GPT-6 Luna and the GPT-5.6 models
take tool calls on Chat Completions only with reasoning off, so requests with
tools to those ids carry `reasoning_effort: "none"`; no other id gets the
field.

## Pick the assistant's model

The [assistant](/guides/assistant/#pick-the-model) has a model picker: the web
app's model mark under the message box, `M` in the terminal pane, and `mxr
assistant model set <id>`. It lists the models in `llm.assistant_models`, five
by default:

```toml
[llm]
assistant_models = ["gemma4:latest", "gpt-6-luna", "gpt-5.6-terra", "claude-haiku-5-5", "claude-sonnet-5-5"]
```

Each entry is a model id, and where it runs follows from the id as above.
Leave the setting out, or empty, for the five. After editing it, run
`mxr restart`. A pick is stored as
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

The API key is read from the env var named in `api_key_env` at runtime
(empty = no `Authorization` header sent). Keeping the secret out of
the config file is intentional: the config is checked into dotfiles;
the env var lives in your shell init.

Check what the running daemon is using:

```bash
mxr llm status
mxr llm status --format json
```

Config reloads rebuild the runtime provider, so changing `[llm]` switches
the model without restarting the process. `mxr config set llm.<key> <value>`
tells a running daemon to reload; a hand edit of `config.toml` reaches it on
the next account reload or restart.

## Know what each request sends, and where

The endpoint decides where your mail goes. With a local endpoint nothing
leaves your machine. With a cloud endpoint, the text below goes to that
provider, and that includes mail other people sent you, so use a provider
whose API data terms cover it. A key from the provider's API console is
the path mxr supports; mxr has no sign-in with a consumer chat
subscription.

| Work | When it runs | What a cloud endpoint receives |
|---|---|---|
| `mxr summarize`, `mxr triage`, `mxr draft-assist`, `mxr draft` and `mxr draft refine` | When you run them | The thread the command works on |
| Row gists | In the background, for conversations from people that a client shows, and for automated mail while the web app's Updates page or `mxr briefing gists --automated` asks | That conversation's messages. Newsletters and mailing lists are never sent |
| Delivery confirmation | After sync, for mail the local heuristic shortlisted as shipping | That message's sender, subject and cleaned body |
| Relationship summaries, commitments, voice matching, answer coverage, briefings, decisions, experts, `mxr ask` | In the background or on demand | Nothing: these are refused for a cloud endpoint unless `llm.allow_cloud_relationship_data = true` |
| Drafts | When you ask for one | The conversation being drafted. Your other emails and habits only with `llm.allow_cloud_relationship_data = true` |
| The [assistant](/guides/assistant/) | When you send it a message | Your message, what you have open, and the mail it reads while answering (search results, threads, drafts). Refused for a cloud endpoint unless `llm.allow_cloud_relationship_data = true`, since it reads across your mailbox |
| Archive's records and answer box | After sync and on demand | Nothing: records are read by rules and schema.org markup on your machine. Only when no record matches does the answer box fall back to `mxr ask`, under that row's rule |

Prompts that carry mail wrap it as untrusted data and tell the model it is
not an instruction. Check what the running daemon uses:

```bash
mxr llm status --format json
```

`base_url` shows the endpoint. A `localhost`, `127.x.x.x` or `::1` host is
local; anything else is cloud. A `claude-*` model, and a `gpt-*` model
routed to OpenAI, have no `base_url` and are always cloud.

## Send one feature to a different model

Each feature can override `[llm]` on its own. Fields you leave out
inherit from `[llm]`. This keeps every feature on your local model except
thread summaries, which go to a cloud model:

```toml
[llm.overrides.summarize]
base_url = "https://api.openai.com/v1"
model = "gpt-5-mini"
api_key_env = "OPENAI_API_KEY"
```

The keys are `summarize` (which also covers gists and `mxr triage`),
`relationship_summary`, `commitments`, `draft_assist`, `draft_new`,
`draft_refine`, `voice_match`, `humanize_rewrite`, `answer_coverage`,
`archive_ask`, `decision_log`, `briefing`, `expert`,
`delivery_extraction` and `assistant` (which needs a model that can call
tools). The relationship rule above still applies to an override that
points at a cloud endpoint.

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

In the TUI, open the palette (`:`) → **Summarize Thread**. The summary runs in
the background and renders above the message body; opening a long uncached
thread can also start a debounced background summary. In the web reader, the
**Summary** button or **Summarize thread** in the palette calls the same daemon
request and renders the result in the **AI overview** collapsible above the
thread. Summarize lost its `y` key to the thread briefing, `B`; pressing `y`
says so until 0.6.180.

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
mxr compose assist --draft DRAFT_ID --instruction "mention the invoice"

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

The readable output shows your words plain, what was added in `{+…+}` and
what was removed in `[-…-]`, then the gaps still to fill and which of your
sentences changed or were dropped, counted from 1 (sentences split at `.`,
`!`, `?` and line breaks). `--format json` prints the whole proposal:
`proposal_id`, `action`, `base` (SHA-256 of the text it was made from, ignoring
blank space at both ends and normalizing CRLF to LF), `text`, `changes`,
`gaps` (each with its byte range in `text`), `provenance`, `yours_changed`
(positions counted from 0) and
`composer_version`.

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
mxr draft --format json eval --limit 20
mxr draft --format json eval --set quick
```

`--set mine` is the default. It replays your most recent replies. For each
one it rebuilds the conversation and your voice examples as they were just
before you replied, drafts a reply through the same path the web, TUI and
CLI use, and compares it with what you actually sent: greeting and sign-off
match, length ratio, and numbers the draft invented.

`--set quick` drafts the labelled drafting set instead: 40 synthetic cases,
each a short thread, a few emails in a sample voice, and sometimes text
already typed (finished the way `mxr compose assist` finishes it) or an
instruction. Every draft is graded against what its case
required. The typed sentences must survive, each question must be answered,
facts nobody has given must be left as `[[?: …]]` gaps, no number may appear
that no input carries, the length must fall in the case's band, and there
must be no subject line, preamble, repeated sign-off or run of the thread
copied back. The output counts how many drafts passed each check. Each draft
also gets a style score, which is reported and not graded.

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

The notes after the body name each source with the command that opens it:

```text
Friday works for me. I'll bring the notes.

[Local model gemma4 · used 4 of your emails to Nora · history used · review before sending]
Your emails it matched the voice of:
  2026-09-29 · you to Nora  mxr cat b5700241-8d68-562a-92d3-ec69db053cea
  2026-09-30 · you to Nora  mxr cat 7294ff6c-2134-5909-8ea2-e2a1f0694ca9
Messages it read from this conversation:
  2026-09-30 · Theo  mxr cat 81fe7849-7927-5257-918d-5dc693765c9b
  2026-09-30 · Nora  mxr cat e0c0e763-0faf-5773-98b9-049d0d90d765
```

Run `mxr cat <message-id>` on any line to read that message. The line is
read from what the request actually did, not from your config:

- **Local model** or **Cloud model** comes from the endpoint that answered.
  If a config reload lands mid-request, the draft still describes the model
  that wrote it.
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

You'll get the best results with models that follow instructions well
and stay close to the source: Qwen 2.5 instruct, Llama 3 instruct,
and the GPT-5 family all do this reliably.

## Limits and what's deferred

- **Single-shot completions**: no streaming yet. The features are
  short-form (≤2KB outputs); a single round-trip beats streaming for
  this use case.
- **24KB prompt budget**: long threads truncate oldest-first.
- **Semantic grounding is opportunistic**: prior sent examples are included
  only when semantic search is ready and has indexed matching sent messages.
- **Summary cache**: unchanged threads reuse the cached summary. The cache
  hash includes weak relationship context, so changed relationship summaries
  or style data invalidate stale summaries. Opening a thread returns a valid
  cached summary with the thread payload when one exists.
- **Humanizer auto-rewrite is not the core contract**: deterministic scoring
  is available locally; automatic rewrite loops are a separate pipeline layer.

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

- **Catching up after vacation:** `mxr search 'is:unread newer_than:7d'
  --format ids | xargs -n1 mxr summarize | less` turns 200 unread
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
get the thread id, then `mxr draft-assist`. Show me the draft;
don't send."
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
