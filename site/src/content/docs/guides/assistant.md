---
title: Talk to Your Mail with the Assistant
description: Ask questions about your email, or have the assistant sort, archive and draft. Every change waits for your approval.
---

The assistant is a chat you have with your mailbox. Ask it what needs a reply this week, what a thread decided, or which newsletters you never open. Ask it to archive every shipping notice older than a month, move a sender to Reading, or draft a reply. It works through the same tools mxr's MCP server offers agents, so anything mxr can do, it can do.

It reads and searches on its own. Every change it wants to make stops and waits for you, with a preview of exactly what it will touch. When it needs you to choose (which account, which of three matching emails, how wide a cleanup) it asks with a card of options.

## Open it

In the web app, press `⌘.` (`Ctrl+.` on Linux and Windows) or the assistant button next to Search. It opens on the right, beside whatever page you're on, and stays as you move between pages. Drag its edge to resize it. On narrow windows it slides over the page instead. It is closed until you open it, and it remembers whether you left it open.

In the TUI, press `Ctrl-a`; see [In the terminal](#in-the-terminal).

From a terminal:

```sh
mxr assistant send "Which newsletters haven't I opened in a month?"
```

## In the terminal

In the TUI, press `Ctrl-a` on any screen. The assistant opens as a pane on the right, or over the right of the screen when the terminal is narrower than 110 columns. It is hidden until you open it. `Ctrl-a` hides it again, or brings the keys back to it after you've moved away. The palette (`Ctrl-p`) has the same commands under "Assistant".

While the pane has the keys, everything you type goes to it, not to the screen behind it:

| Key | Does |
|-----|------|
| `Enter` | Send the message (`Alt-Enter` or `Ctrl-j` for a new line) |
| `Esc` | Leave the message box for the pane's keys; `Esc` again returns the keys to the screen |
| `i` | Back to the message box |
| `y` / `n` | Approve or reject the waiting change; `n` asks why (optional), `Enter` rejects |
| `x` | Show or hide the waiting change's exact call: the tool and its arguments |
| `1`-`9`, `o`, `Enter`, `s` | Choose an option, write your own answer, answer, or skip the question |
| `1`-`9` | With no question waiting, open a numbered link in the latest reply with links: an email opens in the reader, a person in Messages |
| `Ctrl-c` | Stop a running turn |
| `N` / `L` | New conversation / past conversations; in the list, `Enter` opens one and `D` deletes it after saying what goes |
| `T` | Let reversible changes run without asking in this conversation, or stop; the title says "auto-approve on" |
| `D` | Delete the conversation, after saying what goes |
| `e` | Open or fold the steps |
| `j` / `k` | Scroll the conversation |

The assistant also knows what you have open: the screen, the thread in the reader and any messages you selected.

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

## Let reversible changes run on their own

For a long cleanup, approving every archive gets old. The shield button in the sidebar's header (or `mxr assistant trust <id>`) lets that one conversation run reversible changes without asking: archiving, marking read or unread, stars, moves between modes, Later, categories and to-do state. A line under the header says it is on. Sending, deleting, trash, spam, rules and anything that talks to another service still wait for you. Steps that ran this way say "ran without asking". Turn it off with the same button, or `mxr assistant trust <id> --off`.

Only you can switch this on. The assistant can't ask for it, and nothing in an email can turn it on.

## See what it did

Replies appear as the model writes them. Each step the assistant takes shows as a line with its reason. Once a turn ends the steps fold into one line such as "5 steps · 1 changed · 1 failed", and a reply after steps that failed is followed by a note naming them, "This step failed, whatever the reply says", so a reply that claims more than happened doesn't stand alone. A step a later retry redid isn't named. A change mxr refused to make, such as one sent without its confirmation, counts as failed. Open a step to see what it looked for and what it found: the first few emails or people, each of which opens in the app, and how many more there were. **Raw call** under that shows the exact call and what mxr returned. **Stop** ends a running turn; calls it hadn't made yet are not made. Its changes appear in [Activity](/guides/activity-log/) with the source `assistant`. Changes that mxr can undo by id (archiving, read state and stars made in one step) show an **Undo** button on their step for about a minute (in the terminal, `mxr assistant` prints the `mxr undo` command); undoing doesn't tell the assistant, so mention it if it matters for what comes next.

Replies link the emails and people they name. A subject opens that email in All Mail and a name opens the person's row in Messages, with the assistant still open beside it, and the address in the browser points at that email, so you can copy it. A link only goes to an email or person the assistant's tools returned in that conversation; anything else it names stays plain text. In the TUI, the latest reply with links numbers them, "Q1 report [1]", and `1`-`9` opens one with the pane still open beside it; other replies show their links as words. Opening the steps with `e` adds a line under each one saying what it found.

Past conversations are under the history button, under `L` in the TUI, and `mxr assistant list` shows them in the terminal. Delete one there or with `mxr assistant delete <id>` (add `--dry-run` to see what goes).

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

Long conversations are fine on small models too. When a conversation would crowd the model's context window (`context_window` in `[llm]`), or the model says the request is too long, the assistant summarises the older messages and carries on; a line in the conversation shows where the summary starts, and the full history stays in your database.

A model that can call tools is needed: Claude, OpenAI-compatible APIs, or a local Ollama model with tool support. Large requests go faster on hosted models; a local model works but can take half a minute a turn. `mxr assistant status` says whether it is ready and on what. In `mxr demo` the assistant follows a short script, so you can try an approval without a model.
