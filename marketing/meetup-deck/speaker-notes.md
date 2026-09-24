# Speaker notes - I talk to my email now

Target about 22-23 minutes, then questions. Pacing per slide is a guide, not a script.

---

## 1 · I talk to my email now  (~0.5 min)

mxr, say "Mixer." I built a terminal email client, and the first goal was a
keyboard-native one. The slightly funny outcome is that I now spend most of my
email time talking to an agent instead of opening it.

## 2 · Which emails should I be careful with?  (~1.2 min)

This is a recorded run against the demo mailbox, so there's no personal mail on
screen. Watch the commands the agent picks. It's running `mxr` and `jq` - there's
no browser automation and no Gmail SDK in the loop. I'll let most of the clip
play, it's about half a minute.

## 3 · What the agent runs  (~1.5 min)

For a drafting request, the loop is usually some version of this: locate the
thread, read it, ask how I write to this person, then preview a reply without
sending. The agent might draft with its own model after reading the profile, or
it can call `draft-assist`.

## 4 · All my accounts, in one local mailbox  (~1.4 min)

Those commands don't fan out to three providers when I ask. Gmail comes in through
the Gmail API, other accounts come in over IMAP, and it all normalizes into one
local model in SQLite. Sending is a separate capability, so SMTP handles outbound
where it's configured. An account can sync one way and send another.

## 5 · Sync continues after I close the TUI  (~1.5 min)

I can close the TUI and sync and indexing keep going in the daemon. SQLite is the
canonical store. Tantivy is a lexical index I can rebuild from SQLite if I need
to. Relationship profiles and rules run there too, whether or not a client is
open. So when I ask the agent something, the mail is already synced and indexed.

## 6 · Search stays on the machine  (~1.5 min)

By the time I ask a question, mxr is searching local state. Opening a message is a
SQLite read, not a network round trip. There is local process overhead, so I'm
not claiming zero. But I don't notice the search. The only thing I wait for is the
model.

## 7 · Every client uses the same mail operations  (~1.4 min)

The TUI, CLI, web app, scripts, skill, and MCP all reach the same daemon and
local state. The web app goes through an HTTP and WebSocket bridge because a
browser can't open a Unix socket. MCP tools call the daemon too, not the provider
internals.

## 8 · The agent uses the same CLI I do  (~1.5 min)

A coding agent already knows how to run commands, so there's nothing to invent
here. It reads the flags from `--help`, asks for JSON, keeps the account on every
step, and previews a write before committing. IDs are the cheapest thing to pipe
into the next command.

## 9 · One short skill, then command help  (~1.1 min)

The integration is mostly instructions. The skill is short: use structured output,
keep the account explicit, and preview writes. For anything else, the agent reads
the relevant `--help` page instead of me shipping the whole manual into context.

## 10 · Drafting for one person  (~1.8 min)

The request I care about most is: look at how I actually talk to this person
before drafting. The current thread and my instruction weigh the most. Prior
outbound examples come in when semantic search is available, and the contact
style is local. And it's told not to invent familiarity it can't see.

## 11 · What mxr learns from sent mail  (~1.3 min)

When I say tone, I mean ordinary signals you can inspect. Formality, sentence
length, how often I use contractions, how often I ask a question, how I open and
sign off. Recent messages count for more. There's a lower-case-opener signal and
an emoji rate in there too. These signals do not capture a voice perfectly. But
the model uses them while drafting, and if the tone is way off, I can see it.

## 12 · A draft is still a draft  (~1.3 min)

Matching my voice is useful. Sending as me is a separate decision. `draft-assist`
returns a body plus some metadata - a voice match and a humanizer report - and it
never sends. The agent can show me the draft, save it, or ask for approval. Over
MCP a send needs `confirm=true`, and the daemon can still deny it.

## 13 · The daemon checks the request again  (~1.5 min)

The agent can ask for an action. The daemon still decides whether that action is
allowed. It checks the allowed accounts, the safety policy, whether sends are
allowed, and which destructive actions are allowed, before it touches a provider.
If there's no MCP profile, the request is denied. `confirm=true` doesn't get you
past a profile. The profile limits what the daemon will do for that origin.

## 14 · Email content cannot authorize an action  (~1.4 min)

There's another source of instructions in the room: the email itself. mxr treats
all of it as untrusted data. Text inside a message that reads like a command is
inert. It can be quoted or summarized, but it can't expand permissions or redirect
the task. Sender reputation doesn't change that, and a summary of the mail
inherits the same trust level.

## 15 · Shell skill or MCP  (~1.2 min)

The entry point depends on the agent you already use. Shell-capable agents use the
CLI and the skill. Clients that speak MCP get typed tools. Both reach the same
daemon, the same accounts, and the same gates. MCP is stdio-only right now. Use
the CLI if the agent already has a shell. Use MCP if the client expects tools with
schemas.

## 16 · What this setup costs  (~1.2 min)

There's real machinery under the smooth part. A full local copy of your mail and
its indexes use disk. The daemon needs maintenance - sync, repair, lifecycle. A
remote LLM sees the context you send it, though you can point it at a local
endpoint if you want generation to stay on the machine. And mxr doesn't sandbox
the agent's other shell access. New mail still depends on the upstream providers.

## 17 · What I'd build into the next app  (~1.4 min)

If I were adding agent access to another app, these are the parts I'd insist on
first. Keep the data ready before the agent asks. Give the agent the same
operations the UI has, over a CLI or MCP. Return structured, bounded output.
Enforce account and action policy in the core, below the model. And make writes
previewable, reviewable, and reversible. This works best when the agent keeps
coming back to the same user-owned data and can actually change things.

## 18 · What did Maya ask me for?  (~0.7 min)

That now feels like a normal way for me to use email. The source and docs are
here if you want to try the demo mailbox or see how it works. Happy to take
questions.
