# 25. The assistant

The assistant is a chat agent you talk to about your mail. It answers questions, analyses, and carries out what you ask (archive, move, label, to-dos, drafts, sending) through mxr's own tools. Reading and searching run straight away; every change waits for your approval, and when it needs a choice from you it asks with a question card. It is most useful for analysis ("who am I waiting on?") and bulk work ("archive every shipping notice older than a month"), where doing it by hand means many clicks.

Decision: [D122](15-decision-log.md#d122-the-assistant-is-a-daemon-agent-over-the-mcp-tools-with-approval-enforced-in-code).

## The agent runs in the daemon and calls the MCP tools in-process

```
web sidebar ─┐                         ┌─ mxr-assistant: loop, gate, ask_user, prompt
mxr assistant┼─ IPC ─▶ daemon ─ turn ──┤
             │   (AssistantSend,       └─ tools: MxrMcpServer over an in-memory pipe
             │    AssistantDecide, …)        └─ requests dispatched as ClientKind::Assistant
             └─ AssistantUpdated events ◀─ store: assistant_conversations / _messages
```

- **Loop** (`crates/assistant/src/turn.rs`): call the model, run what it asks for, stop when a call needs the user, at most 30 model calls a turn. Turns run in a daemon task, so closing the sidebar or reloading the page doesn't stop one.
- **Tools**: the 72 MCP tools, unchanged. `mxr_mcp::InProcessTools` runs `MxrMcpServer` over a `tokio::io::duplex` pipe with a requester that calls `handler::handle_request` with `ClientKind::Assistant`. The assistant therefore has exactly MCP's capability and preview/confirm flows, and its calls pass through the global safety policy, workspace ownership checks and the activity log like any client's. Adding an MCP tool adds an assistant tool.
- **Model**: `LlmFeature::Assistant`, configurable with `[llm.overrides.assistant]`. `mxr-llm` gained a tool-calling chat turn for OpenAI-compatible endpoints and Claude, streamed (`chat_stream_as`, `crates/llm/src/stream.rs`) and sharing routing, route health and observations with completions. Claude's streamed blocks, thinking signatures included, are rebuilt into the same content an unstreamed reply has, so replay stays byte-exact. The reply so far reaches clients as `AssistantProgressData::Writing`, at most every 120ms. The demo profile answers with a short script (`demo_assistant.rs`).
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

## Questions are a tool the turn waits on

`ask_user` takes one to four questions with up to six options each, single or multi-select, and an "Other" answer unless the model turns it off. The turn waits like an approval; the answer is validated against the options and returned as the tool result. Writing a new message instead of answering declines whatever is waiting, and nothing waiting runs.

## History is append-only

Anthropic replays thinking blocks only in the exact transcript that produced them and rejects edited history, so the transcript is never rewritten. Each Claude reply's content blocks are stored and replayed verbatim; other providers get the rebuilt text and tool calls. Tool results are capped (40,000 characters, with a note telling the model to narrow the query) when written, not trimmed later. A call with no stored result gets an "interrupted" result in what is sent, never in storage. When the next request would fill three quarters of the model's context window (`context_window`, only when the system prompt and tools alone leave room), or the provider answers that it is too long, the loop summarises (`crates/assistant/src/compact.rs`): it cuts before the current turn's user message, or inside the turn if that isn't enough, asks the model with no tools for a summary of everything before the cut, and from then on sends the summary (fenced as untrusted, since it quotes mail) in place of those messages. The summary and its cut are stored on the conversation (`summary`, `summary_through_seq`); the transcript keeps everything and shows where the summary starts. A summary replaces history in what is sent, so on Claude it starts a new cached prefix once. The system prompt and the tool list are byte-identical across a conversation; the time and what the user has open travel in each user message's `[context]` block. Claude requests use automatic prompt caching.

## Clients

- **Web**: a right-hand pane, closed until opened with the toolbar button or `Mod+.`, beside every page and kept as you navigate; an overlay drawer below 1024px. Steps fold into "N steps" once a turn ends and each opens to the exact call and result. Approval cards show the change in words, the reason, and the messages or draft it touches. Question cards offer the options, an "Other" box and Skip.
- **CLI**: `mxr assistant status | send | approve | reject | answer | stop | list | show | delete`. `send` and the decisions wait for the turn and print what waits with the command that answers it; `--format json` prints the conversation.
- **TUI**: not yet; see [docs/issues/assistant-followups.md](../issues/assistant-followups.md). `Mod+.` is listed as a web-only key.
- **MCP**: deliberately absent. An external agent already has the tools; driving the assistant would let it approve its own changes.

## Privacy

Conversations live in the local database and are deleted with `mxr assistant delete`. They are not in portable backups (like the activity log). The assistant reads across the mailbox, so it is a relationship-data feature: on a model off this machine it runs only with `llm.allow_cloud_relationship_data = true`, and the sidebar says where conversations go. Its mailbox actions are in the activity log with source `assistant`; chat text never is.

## Not built yet

What is still open is in [docs/issues/assistant-followups.md](../issues/assistant-followups.md).
