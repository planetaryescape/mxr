---
title: LLM features (summarize, draft assist)
description: Configure Ollama, LM Studio, OpenAI, or any OpenAI-compatible endpoint for thread summarisation and draft assist in mxr.
---

## Every model feature uses the backend you configure here

Set up the model backend once, under `[llm]`, and every synthesis feature
in mxr uses it. Each feature has its own guide:

| Feature | Guide | What the LLM does |
|---|---|---|
| `mxr summarize` | this page | thread → Markdown summary |
| `mxr draft-assist` | this page | thread + instruction → draft body in your voice |
| `mxr draft eval` | [this page](#check-how-close-drafts-get) | replay your recent replies and compare drafts with what you sent |
| `mxr send --check` answer coverage | [Pre-send safety](/guides/pre-send-safety/#answer-coverage) | extract asks from thread, judge whether the draft addresses each |
| `mxr send --check` commitment candidates | [Forgotten work](/guides/forgotten-work/#commitments-promises-you-made) | extract "I'll send the deck Friday" promises from drafts |
| `mxr ask` | [Archive intelligence](/guides/archive-intelligence/) | retrieval-grounded answer over local mail, every claim cited |
| `mxr decisions rebuild` | [Archive intelligence](/guides/archive-intelligence/#the-decision-log--mxr-decisions) | extract explicit decisions from threads |
| `mxr briefing thread` / `recipient` | [Briefings and loop-in](/guides/briefings-and-loop-in/) | dormant-thread / long-gap recap from the local thread transcript or relationship baseline |
| delivery extraction | [Deliveries](/guides/deliveries/) | confirm a shortlisted email is a real shipment, extract merchant / carrier / items / ETA |
| row and reader gists | [Clear the desk](/guides/now/#see-what-each-row-asks) | one line on what a conversation is about and what it asks of you, with the ask quoted from the message |
| `mxr triage` | [CLI: `mxr triage`](/reference/cli/triage/) | sort search results into action, FYI and routine, reusing the cached summary verdict |

Every feature has an explicit disabled path when `[llm] enabled = false`.
Pure LLM commands such as `mxr summarize` and `mxr draft-assist` return
`LLM is disabled`; features with deterministic substrate, such as briefings
or safety checks, return the local fallback instead of pretending synthesis
succeeded.

Enable LLM features by setting `[llm] enabled = true` in your config and
pointing at any backend that speaks the **OpenAI Chat Completions**
schema.

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

mxr's local-first stance: model features are off until you set
`[llm] enabled = true`, and the default endpoint is Ollama on
`localhost`, so completions never leave your machine. A cloud endpoint is
opt-in through the same config block, with an API key you supply. mxr ships
no key and runs no relay: requests go straight from the daemon to the
endpoint you name.

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

Config reloads rebuild the runtime provider, so changing `[llm]` and
reloading the daemon account/config runtime switches the model without
restarting the process.

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
| Row gists | In the background, for conversations from people that a client shows | That conversation's messages. Newsletters and automated mail are never sent |
| Delivery confirmation | After sync, for mail the local heuristic shortlisted as shipping | That message's sender, subject and cleaned body |
| Relationship summaries, commitments, voice matching, answer coverage, briefings, decisions, experts, `mxr ask` | In the background or on demand | Nothing: these are refused for a cloud endpoint unless `llm.allow_cloud_relationship_data = true` |
| Drafts | When you ask for one | The conversation being drafted. Your other emails and habits only with `llm.allow_cloud_relationship_data = true` |
| Archive's records and answer box | After sync and on demand | Nothing: records are read by rules and schema.org markup on your machine. Only when no record matches does the answer box fall back to `mxr ask`, under that row's rule |

Prompts that carry mail wrap it as untrusted data and tell the model it is
not an instruction. Check what the running daemon uses:

```bash
mxr llm status --format json
```

`base_url` shows the endpoint. A `localhost`, `127.x.x.x` or `::1` host is
local; anything else is cloud.

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
`archive_ask`, `decision_log`, `briefing`, `expert` and
`delivery_extraction`. The relationship rule above still applies to an
override that points at a cloud endpoint.

## Fast and smart tiers are planned for the email modes

The [email modes](/guides/email-modes/) plan adds model work that reads
more of your mail, so it splits that work into two tiers instead of one
model for everything. None of this is in a release yet.

- `llm.tiers.fast` will place incoming mail in a mode. It reads every new
  message, so it runs on a loopback endpoint unless you point it at a cloud
  endpoint and set `allow_cloud_background_classification = true`. A cloud
  model you set up for drafting will never start classifying all your mail
  on its own.
- `llm.tiers.smart` will pull out fields that need care, such as the amount
  and deadline of a bill. It only sees mail already in To do, Archive or
  Messages. Without a cloud model it runs locally and marks its fields
  unchecked. Archive ships without it: today its record fields come from
  schema.org markup and labelled lines, and the smart tier will read record
  fields the rules miss in a later phase.
- `escalate`, an optional stronger model on the same provider and key, will
  take over when the smart model's answer fails the check that amounts and
  dates appear word for word in the email.
- A cloud tier will need an API key (`api_key_env`), so cloud work stays
  under the provider's API data terms.
- Running both tiers in the cloud, with no local model server, will be a
  supported setup. Turning it on will say plainly that the text of every
  incoming message goes to that provider.

`llm.overrides` keeps working when the tiers land. The design is in
[22. Email is five apps](https://github.com/planetaryescape/mxr/blob/main/docs/blueprint/22-email-modes.md)
and decision D114.

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

In the TUI, press `y` or `Ctrl-p` → **Summarize Thread**. The summary runs in
the background and renders above the message body; opening a long uncached
thread can also start a debounced background summary. In the web reader, the
**Summary** button or `y` calls the same daemon request and renders the result
in the **AI overview** collapsible above the thread.

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
- **The task**: your instruction, the tone you picked (or "match my
  examples"), and a length in words taken from how much you usually write to
  this person.

Your instruction is optional for a reply: the draft answers what the
message asks, and anything only you can decide comes back as a
`[[?: ...]]` gap to fill instead of a guess. The model is told to state
only facts from the conversation or your instruction. Its output is
cleaned before you see it (no "Here's a draft", `Subject:` lines, code
fences or `[Your Name]`), and a draft cut off by the token limit is retried
with more room rather than handed over half-written.

### Check how close drafts get

```bash
mxr draft eval --limit 20
```

`mxr draft eval` replays your most recent replies. For each one it rebuilds
the conversation and your voice examples as they were just before you
replied, drafts a reply through the same path the web, TUI and CLI use, and
compares it with what you actually sent: greeting and sign-off match, length
ratio, and numbers the draft invented. It calls your LLM once per reply and
saves nothing. Use `--format json` or `--format jsonl` to keep the results,
and run it again after changing models to compare.

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
sits under **Draft for me** and under the **Draft assist** panel. Select
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
