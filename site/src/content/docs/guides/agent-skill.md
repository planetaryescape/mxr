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
mkdir -p ~/.claude/skills/mxr
curl -fsSL https://raw.githubusercontent.com/planetaryescape/mxr/main/.claude/skills/mxr/SKILL.md \
  -o ~/.claude/skills/mxr/SKILL.md
```

Restart Claude Code. The skill is now invoked when the user asks anything email-shaped — "check email", "search for...", "archive that thread", etc.

## What the skill teaches

The skill is intentionally short — it documents the CLI surface in a form an agent can map prompts onto. Key sections:

### Quick reference (excerpt)

```bash
# Read
mxr search "is:unread"                    # Find unread messages
mxr search "from:alice subject:meeting"   # Search with field prefixes
mxr cat <id>                              # Read message body
mxr thread <id>                           # Read full thread

# Compose
mxr compose --to a@x.com --subject "Hi" --body "Hello"
mxr reply <id> --body "Thanks!"

# Mutate (single or batch via --search)
mxr archive <id>
mxr star --search "subject:urgent" --yes
mxr label "todo" --search "from:boss" --yes

# Snooze
mxr snooze <id> --until tomorrow

# Status
mxr status                                # Daemon status
mxr count "is:unread"                     # Count unread
```

### Important patterns the skill enforces

1. **Message IDs are UUIDs** — get them from `mxr search --format ids` or `mxr search --format json`.
2. **Batch mutations** — use `--search <query>` instead of `<id>` for bulk operations. Always add `--yes` to skip confirmation when running non-interactively.
3. **`--dry-run`** — available on all mutations. Use to preview before executing.
4. **`--format json`** — for machine-readable output on search, cat, thread, status.
5. **`--format ids`** — for piping into `xargs` or other mutations.
6. **Daemon auto-starts** — no need to manually start; commands that need it launch it.

### Typical workflows the skill seeds

**Check inbox:** `mxr search "is:unread" --format json`

**Read and reply:**
```bash
mxr search "from:alice is:unread" --format json --limit 5
mxr cat <message_id>
mxr reply <message_id> --body "Got it, thanks!"
```

**Bulk cleanup with preview:**
```bash
mxr archive --search "older:30d label:notifications" --dry-run
mxr archive --search "older:30d label:notifications" --yes
```

**Triage with labels:** `mxr label "review" <id>; mxr star <id>; mxr snooze <id> --until monday`

For the full skill content, see [SKILL.md on GitHub](https://github.com/planetaryescape/mxr/blob/main/.claude/skills/mxr/SKILL.md). For the exhaustive command reference the agent has access to, see [the CLI overview](/reference/cli/) and the [automation contract](/guides/automation-contract/).

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
