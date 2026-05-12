---
title: AI agent skill
description: Install the mxr skill into Claude Code, Cursor, Continue, Aider, or any LLM agent that takes custom instructions. The agent then drives the same CLI a human would.
---

The mxr skill teaches an agent how to use the CLI that already exists. It is a markdown file with frontmatter, structured exactly like a Claude Code skill, but it works in any agent that supports custom instructions or tool descriptions.

> Pronounced "Mixer". Once you install the skill, your agent should say "Mixer" too.

## Before you install it

- Read [For agents](/guides/for-agents/) for the boundaries — what's safe, what's not yet enforced.
- Treat `--dry-run` as the default for batch changes.
- Remember: a first-party MCP server isn't shipped yet. The skill teaches the CLI; the CLI is the surface.

## Supported agents

| Agent | Status | Install path |
|---|---|---|
| **Claude Code** | First-class | `mkdir -p ~/.claude/skills/mxr && curl … SKILL.md` (below) |
| **Cursor** | Via custom instructions | Copy SKILL.md content into Settings → Rules for AI |
| **Continue** | Via slash command | Add `~/.claude/skills/mxr/SKILL.md` to `~/.continue/config.json` `customCommands` |
| **Aider** | Via system prompt | `aider --read-only ~/.claude/skills/mxr/SKILL.md ...` |
| **Generic LLM** | Copy-paste | Paste SKILL.md content into the agent's system prompt |

## Install — Claude Code

```bash
mkdir -p ~/.claude/skills/mxr/references
curl -fsSL https://raw.githubusercontent.com/planetaryescape/mxr/main/.claude/skills/mxr/SKILL.md \
  -o ~/.claude/skills/mxr/SKILL.md
curl -fsSL https://raw.githubusercontent.com/planetaryescape/mxr/main/.claude/skills/mxr/references/commands.md \
  -o ~/.claude/skills/mxr/references/commands.md
```

Restart Claude Code. The skill is now invoked when the user asks anything email-shaped — "check email", "search for...", "archive that thread", "summarize this thread", "remind me if no reply by Monday", etc. The skill loads `SKILL.md` into context on invoke; the agent reads `references/commands.md` on demand for exhaustive flag detail.

## What the skill teaches

The skill is intentionally short — it documents the CLI surface in a form an agent can map prompts onto, with a longer reference file (`references/commands.md`) for the exhaustive flag set.

### Quick reference (excerpt)

```bash
# Read / search
mxr search "is:unread"                       # Lexical BM25
mxr search "from:alice" --mode hybrid        # Lexical + dense when semantic is ready
mxr cat <id>                                 # Reader mode
mxr thread <id>                              # Whole thread
mxr export <thread_id>                       # Markdown export

# Compose / send / undo
mxr compose --to a@x.com --subject "Hi" --body "Hello"
mxr reply <id> --body "Thanks!"
mxr send <draft_id> --at "monday 9am"        # Scheduled send
mxr unsend <draft_id>                        # Cancel a scheduled send
mxr undo <mutation_id>                       # ~60s window on destructive ops

# Mutate (positional, stdin pipe, or --search batch — all with --dry-run / --yes)
mxr archive <id>
mxr read-archive --search "from:noreply older:7d" --yes
mxr star --search "subject:urgent" --yes
mxr label "todo" --search "from:boss" --yes

# Snooze / remind / reply-later
mxr snooze <id> --until tomorrow
mxr remind <id> --when "monday 9am"          # Follow-up if no reply
mxr replies add <id>                         # Reply-later queue

# Triage unknown senders (local consent)
mxr screener                                 # Queue of undecided senders
mxr screener allow|deny|feed|paper-trail <email>

# LLM-assisted (never auto-sends)
mxr summarize <thread_id>
mxr draft-assist <thread_id> "decline politely"  # Relationship-aware when profile data exists

# Analytics (local SQLite)
mxr sender alice@x.com
mxr stale --mine --older-than-days 7
mxr response-time
mxr subscriptions --rank
mxr wrapped --ytd

# Daemon
mxr status                                   # Daemon status
mxr sync --status
mxr web                                      # Open the web app via local bridge
```

### Important patterns the skill enforces

1. **Message / thread / draft / mutation IDs are UUIDs** — get them from `mxr search --format ids` (one per line), `--format json`, or printed inline by mutations.
2. **Batch via `--search`** — most mutations accept `<id>` positionals, piped stdin IDs, OR `--search <query>`. Always add `--yes` for non-interactive batches.
3. **`--dry-run`** — available on every mutation, compose flow, `rules dry-run`, `reset --dry-run`, and `undo --dry-run`. Preview the count and sample before committing.
4. **Output formats** — `--format table|json|jsonl|csv|ids`. `ids` is the cheapest form to pipe into other commands; `jsonl` is best for streams (`events`, `history`, search).
5. **`mxr undo` window is ~60s** — destructive ops (`archive`, `trash`, `spam`, `read`, `read-archive`) print a mutation ID; capture it if you might need to reverse.
6. **`draft-assist` never sends** — output goes to stdout. JSON output includes model, humanizer, and voice-match metadata when available. Pipe the body into `mxr reply --body "$(...)"`.
7. **Daemon auto-starts** — no need to launch it manually.

### Typical workflows the skill seeds

**Triage inbox:**
```bash
mxr screener                                                # Decide on unknown senders
mxr search "is:unread label:inbox" --format json --limit 20
mxr read-archive --search "from:noreply older:7d" --yes     # Bulk newsletter sweep
mxr replies add <id>                                        # Interesting → reply-later
```

**Reply with LLM scaffold:**
```bash
mxr draft-assist <thread_id> "agree, propose Tuesday 2pm"   # → stdout
mxr reply <message_id> --body "..." --dry-run
mxr reply <message_id> --body "..."
```

**Bulk cleanup with preview + undo:**
```bash
mxr archive --search "label:notifications older:30d" --dry-run
mxr archive --search "label:notifications older:30d" --yes
mxr undo <mutation_id>                                      # If wrong (~60s)
```

**Find what's slipping:**
```bash
mxr stale --mine --older-than-days 7        # I owe a reply
mxr contacts decay --threshold-days 60       # Going-cold relationships
mxr response-time                            # My reply percentiles
```

**Schedule and unsend:**
```bash
mxr compose --to a@x.com --subject "..." --body "..."       # Becomes draft
mxr send <draft_id> --at "monday 9am"
mxr unsend <draft_id>                                       # Before it fires
```

For the full skill content, see [SKILL.md on GitHub](https://github.com/planetaryescape/mxr/blob/main/.claude/skills/mxr/SKILL.md) and [references/commands.md](https://github.com/planetaryescape/mxr/blob/main/.claude/skills/mxr/references/commands.md). For the human-facing surface, see [the CLI overview](/reference/cli/) and the [automation contract](/guides/automation-contract/).

## Install — Cursor

1. Copy the contents of [SKILL.md](https://github.com/planetaryescape/mxr/blob/main/.claude/skills/mxr/SKILL.md).
2. Open Cursor → Settings → Rules for AI.
3. Paste the SKILL.md body into the rules field.
4. Save.

The agent now invokes mxr commands when the user asks email-shaped questions.

## Install — Continue

Add to `~/.continue/config.json`:

```json
{
  "customCommands": [
    {
      "name": "mxr",
      "prompt": "$(cat ~/.claude/skills/mxr/SKILL.md)\n\nThe user request: ",
      "description": "Drive mxr to operate on email"
    }
  ]
}
```

Then `/mxr archive newsletters older than 30 days` invokes the skill.

## Install — Aider

```bash
curl -fsSL https://raw.githubusercontent.com/planetaryescape/mxr/main/.claude/skills/mxr/SKILL.md \
  -o ~/.aider/mxr-skill.md

aider --read-only ~/.aider/mxr-skill.md
```

Aider's read-only file becomes part of the system prompt — the agent treats mxr commands as first-class actions.

## Install — generic LLM

Copy the SKILL.md content into the system prompt. The agent now knows the mxr CLI surface and will invoke commands when prompts map onto them.

## Token budgets

The skill itself is ~3KB. The full command reference (auto-generated from `--help` snapshots, see [/reference/cli/](/reference/cli/)) is ~50KB across all commands; agents shouldn't ingest all of it. Pull only the pages relevant to the active task — `mxr archive --help` plus `mxr search --help` is usually enough for triage.

## What this is _not_

- Not an MCP server. Each tool call is a `mxr ...` shell invocation, not an MCP message.
- Not a permissioned agent platform. The agent has whatever access the user has.
- Not a bypass of the safety primitives — the agent uses `--dry-run`, `--yes`, and `mxr undo` like a human would.

The point of the skill is not magic. It's that the agent uses the same CLI a human would, with the same output and the same guardrails.

## See also

- [For agents](/guides/for-agents/) — three worked examples and the safety primitives
- [Automation contract](/guides/automation-contract/) — what's safe to script
- [HTTP bridge](/reference/bridge/) — same surface over HTTP for non-shell agents
- [API explorer](/api/bridge/) — try requests interactively
