---
title: Sender view
description: Per-sender relationship aggregates — volume, response cadence, open commitments. The unfair advantage of having SQLite locally.
---

Other email clients reason over messages. mxr also reasons over **people**. The sender view answers questions like:

- Who emails me most?
- Who do I reply to fastest?
- Which senders have I left hanging?
- Did this person ask me anything I haven't responded to?

These questions are cheap when your archive is on disk and indexed. They're impossible when your archive is behind a third-party API and a per-call quota.

## CLI

```bash
mxr sender alice@example.com
```

Prints a profile:

- **Volume** — messages over time, inbound vs outbound
- **Cadence** — median response latency in both directions, both clock-time and business-hours
- **Open threads** — threads where the most recent message is theirs and you haven't replied
- **Open commitments** — questions you've asked them, sorted by age
- **Recent activity** — the last N exchanges

```bash
mxr sender alice@example.com --format json | jq .
```

## Common workflows

### Find people I owe replies to

```bash
mxr stale --mine --older-than-days 7 --format json \
  | jq -r '.threads[] | "\(.days_stale)\t\(.subject)"' \
  | head
```

Then run `mxr sender <email>` on each to confirm before triaging.

### Pick the "biggest" senders interactively

```bash
mxr storage --by sender --format jsonl \
  | jq -r '"\(.size_bytes)\t\(.label // .key)"' \
  | sort -rn \
  | fzf --header='bytes | sender' \
  | awk '{print $2}' \
  | xargs mxr sender
```

`storage` ranks senders by data weight; `sender` opens their profile.

### Daily standup: who emailed me overnight?

```bash
mxr search 'newer_than:1d' --format json \
  | jq -r 'group_by(.from)
           | map({sender: .[0].from, count: length, latest: max_by(.date).subject})
           | sort_by(-.count) | .[]
           | "\(.count)\t\(.sender)\t\(.latest)"'
```

## TUI

Inside the TUI, open the **sender profile modal** for the focused message:

- `Ctrl-p` (palette) → "Sender View"
- The modal shows the same profile sections plus "Open in CLI" — copies the equivalent `mxr sender ...` invocation to the clipboard.

## Why this matters for agents

A sender profile is the most useful single context an LLM can have when drafting a reply or evaluating a triage candidate:

```bash
mxr sender alice@example.com --format json \
  | claude -p "Based on cadence and recent threads, draft a friendly nudge if she's overdue."
```

The agent gets relationship _shape_ as JSON, without needing to read every email.

## See also

- [Triage flow](/guides/triage-flow/) — sender view in the broader triage loop
- [Automated follow-ups](/guides/automated-followups/) — `mxr remind` and reply-later for what sender view surfaces
- [CLI: sender](/reference/cli/sender/) — every flag
