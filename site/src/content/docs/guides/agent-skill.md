---
title: AI agent skill
description: Give your coding agent full control over your email with the mxr CLI skill.
---

## What is the mxr skill?

The mxr skill is a configuration file that teaches AI coding agents (Claude Code, Cursor, Windsurf, etc.) how to use the mxr CLI. Once installed, you can ask your agent to read, search, compose, and triage your email through natural language.

## Install the skill

### Claude Code

Download the skill file and place it in your project or global skills directory:

```bash
# Global (available in all projects)
mkdir -p ~/.claude/skills/mxr
curl -fsSL https://raw.githubusercontent.com/planetaryescape/mxr/main/.claude/skills/mxr/SKILL.md \
  -o ~/.claude/skills/mxr/SKILL.md
```

### Other agents

The skill is a markdown file with structured CLI documentation. It works with any agent that supports custom instructions or tool descriptions. Copy the [SKILL.md](https://github.com/planetaryescape/mxr/blob/main/.claude/skills/mxr/SKILL.md) content into your agent's configuration.

## What your agent can do

With the skill installed, your agent has access to every mxr CLI command. Some examples:

### Triage and summarize

> "Go through my unread emails from the last 24 hours. For each one, tell me who it's from, give a one-line summary, and flag anything that needs a response. Draft replies for the urgent ones."

The agent runs `mxr search "is:unread" --format json`, parses the results, reads individual messages with `mxr cat`, and composes replies with `mxr reply`.

### Automated cleanup

> "Find all notification emails older than 30 days and archive them. But first show me what you'd archive."

The agent uses `mxr archive --search "label:notifications older:30d" --dry-run` to preview, then executes with `--yes` after your confirmation.

### Research and export

> "Export the thread about the Q2 roadmap as markdown, summarize the key decisions, and pull out all action items."

The agent runs `mxr search "subject:Q2 roadmap" --format json` to find the thread, then `mxr export <thread_id> --format markdown` to get the full content.

### Meeting prep

> "Pull up all email threads between me and Sarah from the last two weeks. Summarize open items and draft an agenda for our 1-on-1."

The agent searches with `mxr search "from:sarah" --format json`, reads threads, and synthesizes the information.

### Newsletter management

> "Search my inbox for recurring newsletters I haven't opened in 3 months. Show me the list and unsubscribe from the ones I confirm."

The agent identifies stale subscriptions and uses `mxr unsubscribe <id>` for each confirmed newsletter.

## Key CLI patterns for agents

| Pattern | Purpose |
|---|---|
| `--format json` | Structured output for parsing |
| `--format ids` | Get message IDs for piping |
| `--dry-run` | Preview any mutation safely |
| `--search <query>` | Batch operations on matching messages |
| `--yes` | Skip confirmation for automated workflows |
| `--body` / `--body-stdin` | Compose without opening $EDITOR |

## Publishing the skill

The mxr skill source is at [`/.claude/skills/mxr/SKILL.md`](https://github.com/planetaryescape/mxr/blob/main/.claude/skills/mxr/SKILL.md) in the mxr repository. Contributions welcome.
