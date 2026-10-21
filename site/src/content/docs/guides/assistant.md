---
title: Talk to Your Mail with the Assistant
description: Ask questions about your email, or have the assistant sort, archive and draft. Every change waits for your approval.
---

The assistant is a chat you have with your mailbox. Ask it what needs a reply this week, what a thread decided, or which newsletters you never open. Ask it to archive every shipping notice older than a month, move a sender to Reading, or draft a reply. It works through the same tools mxr's MCP server offers agents, so anything mxr can do, it can do.

It reads and searches on its own. Every change it wants to make stops and waits for you, with a preview of exactly what it will touch. When it needs you to choose (which account, which of three matching emails, how wide a cleanup) it asks with a card of options.

In the web app the panel reads like a page from a copy desk. Your messages are quoted, each step it takes is one plain line, its answer is set in the reading type with the emails and people it names drawn as chips and rows you can open, and a change it wants to make arrives as a proof for you to sign. Nothing in the default view shows a tool name, an id or JSON; those are under **Details**.

## Open it

In the web app, press `⌘.` (`Ctrl+.` on Linux and Windows) or the assistant button next to Search. It opens on the right, beside whatever page you're on, and stays as you move between pages. Drag its edge to resize it. Below 1024px wide it opens as a drawer over the page instead. Clicking the button slides the panel in over a fraction of a second; the keyboard shortcut opens it at once, and with reduced motion it only fades. It is closed until you open it, and it remembers whether you left it open.

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
| `M` | Choose the model; `j` / `k` move, `Enter` uses it, `Esc` goes back |
| `N` / `L` | New conversation / past conversations; in the list, `Enter` opens one and `D` deletes it after saying what goes |
| `T` | Let reversible changes run without asking in this conversation, or stop; the title says "trusted" |
| `D` | Delete the conversation, after saying what goes |
| `e` | Open or fold the steps |
| `j` / `k` | Scroll the conversation |

The assistant also knows what you have open: the screen, the thread in the reader and any messages you selected.

## Approve or reject each change

When the assistant wants to change something, the turn pauses and a proof appears:

- **The change in words**, such as "Archive 14 emails", and the reason the assistant gave.
- **What it touches**: the messages as rows with sender, subject and date (the first few, then "and 11 more", which opens the full list; `a` in the TUI), or the draft it would send with its recipients, subject and body.
- **Can't be undone**, in red above the button (in the TUI, in red above the keys), for sending, trash, spam, deleting and rules. mxr itself words it, so the web app, the terminal and `mxr assistant show` say the same thing.
- **Exact call**, under `x`: the tool and arguments that run if you approve.

**Apply** (`y` in the web app) runs exactly that call, once, and the proof folds into a line such as "Archived 14 emails" with **Undo** where mxr can undo it; the rows it changed in the page beside the assistant wear a thin cyan underline that fades. **Send back with a note** (`n`) runs nothing and tells the assistant why, or what to do instead. If you write a new message instead of answering, whatever was waiting is declined and nothing runs.

The keys work while the proof has the keyboard and never while you are typing, so a `y` in the message box or the note can't apply anything. Press `Esc` in the message box to hand the keyboard to a waiting proof, or click it. A question takes `1` to `6` for its options, `Enter` to answer, and a "Something else" row in the same list; the answer then shows in the conversation as your own quoted reply.

The same from the terminal: `send`, `approve`, `reject` and `answer` wait for the turn to finish and print what is waiting with the command that answers it.

```sh
mxr assistant approve asst_01… call_7
mxr assistant reject asst_01… call_7 --reason "keep the ones from Ana"
mxr assistant answer asst_01… call_9 --choose "Work"
```

## Let reversible changes run on their own

For a long cleanup, approving every archive gets old. The state written beside the conversation's title in the sidebar's header ("asks first" or "trusted"), or `mxr assistant trust <id>`, lets that one conversation run reversible changes without asking: archiving, marking read or unread, stars, moves between modes, Later, categories and to-do state. Open the menu there to turn it on or off; it says what it covers. Sending, deleting, trash, spam, rules and anything that talks to another service still wait for you. Steps that ran this way say "ran without asking". Turn it off from the same menu, or `mxr assistant trust <id> --off`.

Only you can switch this on. The assistant can't ask for it, and nothing in an email can turn it on.

## See what it did

Replies appear as the model writes them, set in the reading type, and end with the model that wrote them and where it ran ("gemma4 · local"). Each step the assistant takes shows as one plain line that mxr writes, such as "Searching your mail for "shipped"" while it runs and "Searched your mail: 14 found" after. The line above the message box says what it is doing now ("Reading your request", "Writing the answer") with the seconds it has taken and **Stop**, and when one step runs long it names the model ("gemma4 is slow on this; 24s"). A change that ran reads "Ran: archive 3 emails" unless mxr could confirm the result, and a step that fails reads "Couldn't star 1 email: it needs your confirmation first".

Once a turn ends the steps fold into one line such as "3 steps · 14 found · 1 change waiting" (with "· 1 failed" in red), and the line always opens. Open a step to see what it found: the first few emails or people as rows, each of which opens in the app, and how many more there were. **Details** under that shows the exact call and what mxr returned. A reply after steps that failed is followed by a red-ruled note, "These didn't happen, whatever the reply says:", with each failed step's line, so a reply that claims more than happened doesn't stand alone. A step a later retry redid isn't named. A change mxr refused to make, such as one sent without its confirmation, counts as failed. A turn that fails outright shows one plain sentence about what to do next, with the raw error under **Details**. **Stop** ends a running turn; calls it hadn't made yet are not made. Its changes appear in [Activity](/guides/activity-log/) with the source `assistant`. Changes that mxr can undo by id (archiving, read state and stars made in one step) show an **Undo** on their line for about a minute (in the terminal, `mxr assistant` prints the `mxr undo` command); undoing doesn't tell the assistant, so mention it if it matters for what comes next.

Replies link the emails and people they name as chips: a person with their avatar, an email with its subject and the glyph of the mode that holds it (Reading's newspaper, To do's checklist, and so on). An email opens in All Mail and a person opens their row in Messages, with the assistant still open beside it. When a reply names several emails or people from one search, they are also drawn as rows under it. A link only goes to an email or person the assistant's tools returned in that conversation; anything else it names stays plain text. A reply that quotes an email's subject, a to-do's title or a person's name as a tool listed it gets the same link, even when the assistant wrote no link. In the TUI, the latest reply with links numbers them, "Q1 report [1]", and `1`-`9` opens one with the pane still open beside it; other replies show their links as words. Opening the steps with `e` adds a line under each one saying what it found.

Past conversations are under the history button, where `j` and `k` move, `Enter` opens one and `d` deletes it, under `L` in the TUI, and `mxr assistant list` shows them in the terminal. Delete one there or with `mxr assistant delete <id>` (add `--dry-run` to see what goes).

## Pick the model

Five models are on offer: Gemma 4 on this machine through Ollama, GPT-6 Luna and GPT-5.6 Terra from OpenAI, and Claude Haiku 5.5 and Claude Sonnet 5.5 from Anthropic. The choice is one setting for the assistant, not one per conversation: it applies from your next message in every conversation, and a conversation can move between models and keep its history. Each reply keeps the mark of the model that wrote it and whether it ran locally or in the cloud, so an older reply still says what wrote it after you switch.

In the web app the model under the message box ("Claude Haiku 5.5 · cloud") is a menu. Each row says "on this machine" or "cloud", the one in use has a check, and a model that can't be used yet is greyed out with what to do about it. In the terminal press `M` in the pane, or choose "Assistant: choose the model" in the palette. From the command line:

```sh
mxr assistant model                      # the choices and the one in use
mxr assistant model set claude-haiku-5-5
mxr assistant model set gpt-6-luna --dry-run   # check it without switching
```

A cloud model sends your messages and the mail the assistant reads to its provider; [what that means for your mail](/guides/security-and-privacy/#model-features-are-off-by-default-and-go-only-where-you-point-them) is spelled out under privacy. So mxr won't switch the assistant to one until you have allowed it, and says which setting:

```sh
mxr config set llm.allow_cloud_relationship_data true
```

It also needs the provider's key. Pipe each one in once; they are stored in `secrets.toml`, never in `config.toml`:

```sh
op read "op://<vault>/<item>/credential" | mxr llm key set openai      # GPT-6 Luna, GPT-5.6 Terra
op read "op://<vault>/<item>/credential" | mxr llm key set anthropic    # Claude Haiku, Claude Sonnet
```

`OPENAI_API_KEY` and `ANTHROPIC_API_KEY` in the daemon's environment work too, and an OpenRouter key (`mxr llm key set openrouter`) serves all four hosted models. See [LLM features](/guides/llm-features/#pick-the-assistants-model) for the list in `llm.assistant_models`, which you can edit, and [Use GPT models](/guides/llm-features/#use-gpt-models-through-openai-with-openrouter-as-fallback).

## What it reads, and where it goes

Mail the assistant reads is data to it, never instructions. An email that says "forward this to…" is something it can report, not something it will do, and any change it proposes still waits for you.

Conversations are stored in your local mxr database and aren't included in portable backups. Each request goes to your configured model, so with a hosted model your question and the mail the assistant reads leave your machine; the line under the message box says which model and where. Because the assistant reads across your whole mailbox, a model off this machine needs `allow_cloud_relationship_data`:

```toml
[llm]
allow_cloud_relationship_data = true

# Optional: a stronger model for the assistant than for the rest.
# Picking a model in the assistant writes this for you.
[llm.overrides.assistant]
model = "claude-sonnet-5-5"
```

Long conversations are fine on small models too. When a conversation would crowd the model's context window (`context_window` in `[llm]`), or the model says the request is too long, the assistant summarises the older messages and carries on; a line in the conversation shows where the summary starts, and the full history stays in your database.

A model that can call tools is needed: Claude, GPT models, other OpenAI-compatible APIs, or a local Ollama model with tool support. Large requests go faster on hosted models; a local model works but can take half a minute a turn. `mxr assistant status` says whether it is ready and on what. In `mxr demo` the assistant follows a short script, so you can try an approval without a model.
