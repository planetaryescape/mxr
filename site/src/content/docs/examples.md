---
title: Examples
description: Everything mxr can do, as copy-pasteable one-liners — search, triage, compose, clean up, follow up, analyze, and drive it from an agent.
---

mxr is a single binary with a deep command set. This page is the fast tour: one-liners grouped by what you're trying to do, each linking to its full guide. Most commands take `--format json|jsonl|ids|csv|table`, and every mutation takes `--dry-run` — so anything here composes with your shell or an agent.

For multi-step pipelines (fzf, jq, xargs, cron) see [Recipes](/guides/recipes/); for the per-command automation guarantees see the [Automation Contract](/guides/automation-contract/).

## Get started

Try it on seeded data before connecting anything real.

```bash
mxr demo                 # explore a seeded two-account inbox, safely
mxr accounts add gmail   # connect Gmail (OAuth) when you're ready
mxr accounts add imap    # or any IMAP server
mxr sync --wait          # pull mail, then build the local index
mxr                      # open the TUI
```

Full guide → [Quick Start](/getting-started/quick-start/) · [First Sync](/getting-started/first-sync/)

## Search your mailbox

Search is the primary way you navigate — instant, local, exact.

```bash
mxr search "from:alice is:unread"
mxr search "subject:\"quarterly review\" after:2026-01-01"
mxr search "label:work has:attachment"
mxr search "has:calendar newer_than:30d"
mxr search "{from:amy from:david} subject:(dinner movie)"   # grouped terms
mxr search "holiday AROUND 10 vacation"                      # proximity
mxr search "house of cards" --mode semantic                 # meaning, not keywords
mxr search "from:github.com is:unread" --account work --format json
```

Full guide → [Search](/guides/search/) · [Semantic Search](/guides/semantic-search/)

## Read and triage

Open in reader mode, then clear the inbox a key at a time.

```bash
mxr cat MESSAGE_ID --view reader            # rendered, trackers stripped
mxr snooze --until "tomorrow 9am" MESSAGE_ID
mxr snooze --until "friday 5pm" MESSAGE_ID
mxr star MESSAGE_ID
mxr archive MESSAGE_ID --dry-run            # preview first
mxr screener allow alice@example.com        # first-time-sender screening
mxr screener feed newsletter@example.com --label "Newsletters"
```

Full guide → [Triage Flow](/guides/triage-flow/) · [Mailbox Workflow](/guides/mailbox/)

## Compose, reply, forward

Compose opens `$EDITOR`; the daemon handles parsing, validation, and send.

```bash
mxr compose --to alice@example.com --subject "hello"
mxr compose --attach ./invoice.pdf --attach ./notes.txt
mxr reply MESSAGE_ID --body "Thanks." --dry-run
mxr reply-all MESSAGE_ID
mxr forward MESSAGE_ID --to team@example.com
mxr send DRAFT_ID --check                   # pre-send safety checks
mxr snippets set decline "Can't take this on right now." --vars ""
```

Full guide → [Compose](/guides/compose/) · [Pre-send Safety](/guides/pre-send-safety/) · [Snippets](/guides/snippets/)

## Clean up and unsubscribe

Rank newsletters by how little you read them, then leave — preview, then confirm.

```bash
mxr subscriptions --rank --format json                       # who you never open
mxr unsubscribe newsletter@example.com --dry-run
mxr unsubscribe newsletter@example.com --yes
mxr archive --search "from:noreply older_than:30d" --dry-run
mxr archive --search "from:noreply older_than:30d" --yes
```

Full guide → [Unsubscribe](/guides/unsubscribe/)

## Never drop a thread

Surface what you owe, what's going cold, and what you promised.

```bash
mxr owed --since 7 --format json                 # you owe a reply
mxr stale --mine --older-than-days 7             # cooling on your side
mxr commitments --status open --format json      # promises made in email
mxr remind MESSAGE_ID --when "in 5d"             # nudge if no reply
mxr replies                                      # your reply-later queue
```

Full guide → [Forgotten Work](/guides/forgotten-work/) · [Automated Follow-ups](/guides/automated-followups/)

## Labels and saved searches

Saved searches are programmable lenses; labels are queues.

```bash
mxr labels create FollowUp --color "#ff6600"
mxr label FollowUp --search "from:recruiter@example.com"
mxr saved add owe-replies "is:unread label:inbox older_than:3d"
mxr saved run owe-replies
mxr move Done --search "label:inbox from:billing@example.com"
```

Full guide → [Labels and Saved Searches](/guides/labels-and-saved-searches/)

## Schedule and follow up

Send later, set send-and-remind, and unsend within the window.

```bash
mxr send DRAFT_ID --at "tomorrow 9am"
mxr reply MESSAGE_ID --body "On it." --yes --remind-after "in 5d"
mxr unsend DRAFT_ID
```

Full guide → [Timing and Cadence](/guides/timing-and-cadence/) · [Automated Follow-ups](/guides/automated-followups/)

## Understand your mail

Analytics over the local corpus — no dashboard, no upload.

```bash
mxr response-time --since-days 90                          # how fast you reply
mxr response-time --counterparty client@example.com --theirs
mxr contacts asymmetry --min-inbound 5                     # one-sided relationships
mxr contacts decay --threshold-days 60                     # contacts going quiet
mxr storage --by label --limit 20                          # what's eating disk
mxr wrapped                                                # your year in email
```

Full guide → [Analytics](/guides/analytics/)

## Ask, summarize, extract

Optional LLM features that read your mail and hand back structure.

```bash
mxr ask "what did Alice and I decide about pricing in Q2?"
mxr summarize --search "from:team newer_than:7d" --limit 5
mxr expert --query "DKIM setup" --format json             # answer from your archive
mxr briefing recipient alice@example.com --format json    # prep before a meeting
mxr draft-assist THREAD_ID "Build a 1:1 agenda, grouped by open question."
```

Full guide → [LLM Features](/guides/llm-features/) · [Briefings and Loop-in](/guides/briefings-and-loop-in/)

## Automate and pipe

Reads emit JSON; mutations take IDs on stdin. It's a Unix citizen.

```bash
# archive everything from a sender — reviewed first
mxr search "from:no-reply older_than:30d" --format ids | xargs -I{} mxr archive {} --dry-run

# export a thread as markdown for your notes (or an agent)
mxr export THREAD_ID --format markdown > thread.md

# every unread from a domain, as JSONL
mxr search "is:unread from:acme" --format jsonl
```

Full guide → [Recipes](/guides/recipes/) · [Automation Contract](/guides/automation-contract/)

## Drive it from an agent

The CLI, MCP server, and HTTP bridge all call the same daemon — JSON in, JSON out, dry-run on every mutation.

```bash
mxr mcp serve                                              # typed tools over stdio
mxr search "from:sarah after:2026-04-23" --format json | jq '.results[0]'
mxr archive --search "from:newsletter@example.com" --dry-run
mxr history --category mutation --limit 3 --format json   # audit trail
```

Full guide → [For Agents](/guides/for-agents/) · [MCP Server](/reference/mcp/)

## Rules

Server-side automations, validated and dry-run before they touch mail.

```bash
mxr rules add "Archive newsletters" --when "label:newsletters unread" --then archive
mxr rules dry-run --all
mxr rules validate --when "from:billing@example.com" --then "add-label:finance"
```

Full guide → [Rules](/guides/rules/)

## Multiple accounts

One mailbox, one search, many providers.

```bash
mxr accounts add gmail
mxr accounts add imap
mxr search "is:unread" --account work --format json
mxr accounts addresses add alias@example.com   # aliases you send and receive as
```

Full guide → [Accounts](/guides/accounts/)
