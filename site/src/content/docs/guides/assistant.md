---
title: Talk to Your Mail with the Assistant
description: Ask questions about your email, or have the assistant sort, archive and draft. Every change waits for your approval.
---

The assistant is a chat you have with your mailbox. Ask it what needs a reply this week, what a thread decided, or which newsletters you never open. Ask it to archive every shipping notice older than a month, move a sender to Reading, or draft a reply. It works through the same tools mxr's MCP server offers agents, so anything mxr can do, it can do.

It reads and searches on its own. Every change it wants to make stops and waits for you, with a preview of exactly what it will touch. When it needs you to choose (which account, which of three matching emails, how wide a cleanup) it asks with a card of options.

## Open it

In the web app, press `⌘.` (`Ctrl+.` on Linux and Windows) or the assistant button next to Search. It opens on the right, beside whatever page you're on, and stays as you move between pages. Drag its edge to resize it. On narrow windows it slides over the page instead. It is closed until you open it, and it remembers whether you left it open.

From a terminal:

```sh
mxr assistant send "Which newsletters haven't I opened in a month?"
```

## Approve or reject each change

When the assistant wants to change something, the turn pauses and a card appears:

- **The change in words**, such as "Archive 14 messages", and the reason the assistant gave.
- **What it touches**: the messages by subject and sender, or the draft it would send.
- **Exact call**: the tool and arguments that run if you approve.

**Approve** runs exactly that call, once. **Reject** runs nothing; add a note to tell the assistant why or what to do instead. If you write a new message instead of answering, whatever was waiting is declined and nothing runs.

The same from the terminal: `send`, `approve`, `reject` and `answer` wait for the turn to finish and print what is waiting with the command that answers it.

```sh
mxr assistant approve asst_01… call_7
mxr assistant reject asst_01… call_7 --reason "keep the ones from Ana"
mxr assistant answer asst_01… call_9 --choose "Work"
```

## See what it did

Each step the assistant takes shows as a line with its reason. Once a turn ends the steps fold into "5 steps"; open one to see the exact call and what mxr returned. **Stop** ends a running turn; calls it hadn't made yet are not made. Its changes appear in [Activity](/guides/activity-log/) with the source `assistant`.

Past conversations are under the history button, and `mxr assistant list` shows them in the terminal. Delete one there or with `mxr assistant delete <id>` (add `--dry-run` to see what goes).

## What it reads, and where it goes

Mail the assistant reads is data to it, never instructions. An email that says "forward this to…" is something it can report, not something it will do, and any change it proposes still waits for you.

Conversations are stored in your local mxr database and aren't included in portable backups. Each request goes to your configured model, so with a hosted model your question and the mail the assistant reads leave your machine; the line under the message box says which model and where. Because the assistant reads across your whole mailbox, a model off this machine needs `allow_cloud_relationship_data`:

```toml
[llm]
allow_cloud_relationship_data = true

# Optional: a stronger model for the assistant than for the rest.
[llm.overrides.assistant]
model = "claude-sonnet-5-5"
```

A model that can call tools is needed: Claude, OpenAI-compatible APIs, or a local Ollama model with tool support. Large requests go faster on hosted models; a local model works but can take half a minute a turn. `mxr assistant status` says whether it is ready and on what. In `mxr demo` the assistant follows a short script, so you can try an approval without a model.
