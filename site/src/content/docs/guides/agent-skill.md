---
title: AI agent skill
description: Install the mxr skill so a coding agent can drive the current CLI surface.
---

The mxr skill teaches an agent how to use the CLI that already exists.

That is the current agent surface. It is real today, and it is worth stating plainly that it is not the same thing as a first-party MCP server.

## Before you install it

- Read [For agents](/guides/for-agents/) for the current boundaries
- Treat `--dry-run` as the default for batch changes
- Remember that read-only and draft-only agent modes are not shipped yet

## Install the skill

### Claude Code

```bash
mkdir -p ~/.claude/skills/mxr
curl -fsSL https://raw.githubusercontent.com/planetaryescape/mxr/main/.claude/skills/mxr/SKILL.md \
  -o ~/.claude/skills/mxr/SKILL.md
```

### Other agents

The skill is just a markdown file with the CLI surface documented in a form agents can use. If your agent supports custom instructions or tool descriptions, copy the contents of [`SKILL.md`](https://github.com/planetaryescape/mxr/blob/main/.claude/skills/mxr/SKILL.md).

## What the skill gives you

- search and read workflows
- draft and reply workflows
- export workflows
- batch mutations through `--search`
- safer mutation flows through `--dry-run`

## A good starting pattern

```bash
mxr search "is:unread" --format json
mxr archive --search "label:notifications older:30d" --dry-run
mxr history --category mutation
```

The useful thing about the skill is not magic. It is that the agent is using the same CLI surface a human would use, with the same output and the same guardrails.
