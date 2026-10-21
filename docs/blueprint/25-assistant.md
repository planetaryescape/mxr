# 25. The assistant

The assistant is a chat agent you talk to about your mail. It answers questions, analyses, and carries out what you ask (archive, move, label, to-dos, drafts, sending) through mxr's own tools. Reading and searching run straight away; every change waits for your approval, and when it needs a choice from you it asks with a question card. It is most useful for analysis ("who am I waiting on?") and bulk work ("archive every shipping notice older than a month"), where doing it by hand means many clicks.

Decision: [D122](15-decision-log.md#d122-the-assistant-is-a-daemon-agent-over-the-mcp-tools-with-approval-enforced-in-code). How it should look and feel, in web and TUI, is in [29](29-assistant-design.md) (D135).

## The agent runs in the daemon and calls the MCP tools in-process

```
web sidebar ─┐                         ┌─ mxr-assistant: loop, gate, ask_user, prompt
mxr assistant┼─ IPC ─▶ daemon ─ turn ──┤
             │   (AssistantSend,       └─ tools: MxrMcpServer over an in-memory pipe
             │    AssistantDecide, …)        └─ requests dispatched as ClientKind::Assistant
             └─ AssistantUpdated events ◀─ store: assistant_conversations / _messages
```

- **Loop** (`crates/assistant/src/turn.rs`): call the model, run what it asks for, stop when a call needs the user, at most 30 model calls a turn. Turns run in a daemon task, so closing the sidebar or reloading the page doesn't stop one.
- **Tools**: every MCP tool, unchanged. `mxr_mcp::InProcessTools` runs `MxrMcpServer` over a `tokio::io::duplex` pipe with a requester that calls `handler::handle_request` with `ClientKind::Assistant`. The assistant therefore has exactly MCP's capability and preview/confirm flows, and its calls pass through the global safety policy, workspace ownership checks and the activity log like any client's. Adding an MCP tool adds an assistant tool.
- **Model**: `LlmFeature::Assistant`, configurable with `[llm.overrides.assistant]`. `mxr-llm` gained a tool-calling chat turn for OpenAI-compatible endpoints and Claude, streamed (`chat_stream_as`, `crates/llm/src/stream.rs`) and sharing routing, route health and observations with completions. Claude's streamed blocks, thinking signatures included, are rebuilt into the same content an unstreamed reply has, so replay stays byte-exact. The reply so far reaches clients as `AssistantProgressData::Writing`, at most every 120ms. The demo profile answers with a short script (`demo_assistant.rs`).
- **Model choice** (D145): one model for the assistant, not per conversation. `GetAssistantModels` lists the ids in `llm.assistant_models` (default `gemma4:latest`, `gpt-6-luna`, `gpt-5.6-terra`, `claude-haiku-5-5`, `claude-sonnet-5-5`) with `{id, label, cloud, available, unavailable_reason}`, and `SetAssistantModel { model, dry_run }` writes `[llm.overrides.assistant] model` through `AppState::mutate_config`, which rebuilds the provider, so the next turn of any conversation uses it (`crates/daemon/src/assistant/models.rs`). A choice is unavailable when AI features are off, when none of its routes has a key (the reason is `mxr llm key set <provider>`), or when it is a cloud model and `llm.allow_cloud_relationship_data` is off (the reason names that setting). The daemon refuses an id outside the list, and any unavailable one, with the same sentence; it never flips the setting. Agent and MCP clients can't call either request. Each reply stores the model and `leaves_machine` that wrote it in the message's meta, so `AssistantItemData::Assistant` still marks an old reply with its own model after a switch.
- **Storage**: `assistant_conversations` and `assistant_messages` (migration 88). The transcript is append-only and stored as the model saw it.

## Approval is enforced in code, never by the prompt

`crates/assistant/src/gate.rs` classifies every call before it runs, from the tool name and arguments alone:

| Rule | Runs straight away when | Otherwise |
|---|---|---|
| Read | always | |
| Confirm | `confirm` is absent or `false` | waits; preview is the same call with `confirm: false` |
| Confirm, previewed by another tool | `confirm` is absent or `false` | waits; preview is `mxr_mutation_preview` (for `mxr_mutate`), `mxr_get_draft` (for `mxr_send_draft`) or `mxr_archive_outcome` (for `mxr_archive_submit`), because those tools only answer "blocked" without `confirm` |
| Dry run | `dry_run` is `true` | waits (a missing `dry_run` is the real change) |
| Preview token | no `preview_token` | waits; no fresh preview, since the token binds what an earlier preview showed |
| Always | never | waits |

A tool with no entry waits, and `every_mcp_tool_has_a_gate_rule` fails until it is classified.

A conversation the user sets to trust (`SetAssistantAutoApprove`, the sidebar's shield, `mxr assistant trust`) runs reversible changes without waiting: `gate::auto_approvable` allows `mxr_mutate` archive, read/unread and star/unstar, plus `mxr_archive_submit`, `mxr_move`, `mxr_reading_later`, `mxr_category_assignment_apply` and `mxr_todo_set_state`. Everything else still waits, including trash, spam, sending, drafts, rules and anything that reaches another service. The flag is stored on the conversation and read by the daemon at the start of each turn; no tool, model output or email can set it. Such calls are stored with their own result prefix and shown as "ran without asking". Approving runs exactly the call stored in the transcript; the daemon never takes the arguments from the client. A decision for a call that isn't waiting is refused, so a stale client can't approve twice. A preview that names messages only by id is joined with `mxr_mutation_preview` so the card lists subjects and senders.

Waiting calls are cleared from the store before an approved call runs: a crash mid-turn leaves the call unanswered (the model sees "interrupted, check before assuming it ran") rather than an approval that could run twice.

Agent and MCP clients can't send assistant requests. The assistant's tools run as a client kind no agent profile governs, so an agent driving the assistant could approve its own changes; agents call the tools directly instead.

## Email is data, and only the user grants authority

- Every tool result is wrapped in `wrap_untrusted_mail` before the model reads it, and the system prompt carries `UNTRUSTED_MAIL_GUARD` plus the rule that only the user's messages can ask for anything.
- Results mxr writes itself (approvals, rejections, answers) are plain JSON with `"source": "user"`; mail can't forge one because mail always arrives fenced.
- The gate, not the prompt, is the boundary: an injected instruction can at most make the model propose a change, which then waits for the user with a preview.
- Replies render as sanitized markdown: no scripts, no images (a remote image is a tracking pixel), links open outside the app without a referrer.
- Replies link the emails and people they name as `mxr:thread/<id>` and `mxr:person/<id>`. The daemon keeps a link only when its id came back in a tool result in that conversation, and links a subject or name the reply quotes exactly when a result gave that text to one email or person, since small models rarely write the links themselves. Clients open them in the app: the web goes to the email in All Mail (`/m/archive/<id>`) or the person's row in Messages; the TUI numbers the links of the latest reply that has any, "Q1 report [1]", and `1`-`9` opens the email in the reader or the person in Messages.
- Each step carries a readout the daemon reads from the whole result, not the 4,000-character preview: how many things it lists, the first five with where to open each, or its plain fields when it lists nothing. On the web, "and N more" opens every row the result lists, up to 500, from `GET /api/v1/mail/assistant/conversations/{id}/steps/{call_id}/rows` (`GetAssistantStepRows` over IPC); a total larger than the rows the result listed ends the list with a line saying so. A step whose result is still running, or failed, has no rows to open.
- Each reply carries `failed_call_ids`: the steps right before it that failed and that no later step among them redid (same tool and arguments apart from `confirm`). Small models sometimes say they did what failed, so every client names those steps under the reply, "whatever the reply says". The daemon decides once; clients only look the steps up.

## Questions are a tool the turn waits on

`ask_user` takes one to four questions with up to six options each, single or multi-select, and an "Other" answer unless the model turns it off. The turn waits like an approval; the answer is validated against the options and returned as the tool result. Writing a new message instead of answering declines whatever is waiting, and nothing waiting runs.

## Steps, changes and failures are worded by the daemon

Clients don't phrase what the assistant does. `crates/daemon/src/assistant/titles.rs` gives every MCP tool its words from the tool and its arguments: a step carries `title: {running, done}` ("Searching your mail for "shipped"", then "Searched your mail: 14 found"), a running turn's `Calling` progress carries the running title, and a waiting approval carries `title` ("Archive 14 emails") and, for a change that can't be put back, `warning` ("Sends an email. Can't be undone.", fixed daemon text that no model or email text can reach). A change's done title says only "Ran: archive 3 emails", the request, unless the tool is on a verified list (mutate, to-do state, sync, archive submit, send, search) and its real result confirms the outcome; titles read the whole result, not the copy cut for clients. A failed step's done title starts "Couldn't" and gives one of a few fixed reasons ("it needs your confirmation first"); results and email text are never repeated into a title, and a title is one line of at most 80 characters. The model's own reason stays on the step but is never its title. A test fails until each new MCP tool has titles, the same way it does for gate rules. The other live-line phrases ("Reading your request", "Writing the answer") are fixed text in `AssistantProgressData::live_line`, not on the wire. When a reply links two or more emails or people that one step of its turn listed, the conversation view attaches those rows to the reply (`rows`, at most 8, in the order the reply names them). A turn that fails is worded as a sentence with what to do next in `last_error`, with the raw error in `last_error_detail`. Every field is optional, so older clients ignore them and newer clients fall back to the reason and their own wording when a daemon sends none.

## History is append-only

Anthropic replays thinking blocks only in the exact transcript that produced them and rejects edited history, so the transcript is never rewritten. Each Claude reply's content blocks are stored and replayed verbatim; other providers (a local model, OpenAI) get the rebuilt text and tool calls, so a conversation can move between models and back. Tool results are capped (40,000 characters, with a note telling the model to narrow the query) when written, not trimmed later. A call with no stored result gets an "interrupted" result in what is sent, never in storage. When the next request would fill three quarters of the model's context window (`context_window`, only when the system prompt and tools alone leave room), or the provider answers that it is too long, the loop summarises (`crates/assistant/src/compact.rs`): it cuts before the current turn's user message, or inside the turn if that isn't enough, asks the model with no tools for a summary of everything before the cut, and from then on sends the summary (fenced as untrusted, since it quotes mail) in place of those messages. The summary and its cut are stored on the conversation (`summary`, `summary_through_seq`); the transcript keeps everything and shows where the summary starts. A summary replaces history in what is sent, so on Claude it starts a new cached prefix once. The system prompt and the tool list are byte-identical across a conversation; the time and what the user has open travel in each user message's `[context]` block. Claude requests use automatic prompt caching.

## Clients

- **Web**: a right-hand pane, closed until opened with the toolbar button or `Mod+.`, beside every page and kept as you navigate; an overlay drawer below 1024px. Steps fold into "N steps" once a turn ends; each opens to what it looked for and what it found, rows linking to the email or person, with the exact call and result under "Raw call". Approval cards show the change in words, the reason, the messages or draft it touches, and for a change that can't be put back (a send, trash, spam, a rule) the daemon's line above Apply ("Sends an email. Can't be undone."). A click on the toolbar button slides the pane in over 180ms; `Mod+.` opens it at once, and reduced motion only fades it. Email chips in a reply carry the glyph of the mode that holds the email. Question cards offer the options, an "Other" box and Skip.
- **CLI**: `mxr assistant status | send | approve | reject | answer | stop | list | show | delete`. `send` and the decisions wait for the turn and print what waits with the command that answers it; `--format json` prints the conversation.
- **TUI**: a pane opened with `Ctrl-a` on any screen, beside the content or over its right on narrow terminals, with the same approval and question cards, the same daemon warning in the danger colour, and "trusted" in the title when the conversation is; `x` unfolds an approval's exact call, `1`-`9` opens the numbered links of the latest reply with links, open steps add a line of what each found, and `L` lists past conversations, where `D` deletes one after a dry run. `Mod+.` is listed as a web-only key.
- **MCP**: deliberately absent. An external agent already has the tools; driving the assistant would let it approve its own changes.

## Privacy

Conversations live in the local database and are deleted with `mxr assistant delete`. They are not in portable backups (like the activity log). The assistant reads across the mailbox, so it is a relationship-data feature: on a model off this machine it runs only with `llm.allow_cloud_relationship_data = true`, and the sidebar says where conversations go. Its mailbox actions are in the activity log with source `assistant`; chat text never is.

## Not built yet

Known limits and their status are in [docs/issues/assistant-followups.md](../issues/assistant-followups.md).
