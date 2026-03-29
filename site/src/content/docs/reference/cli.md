---
title: CLI commands
description: Daemon-backed CLI reference for mxr.
---

## Overview

mxr is a single binary with subcommands. Running `mxr` without arguments launches the TUI.

Conceptually, the CLI spans three daemon IPC buckets:

- `core-mail`
- `mxr-platform`
- `admin-maintenance`

It does not get special screen-only payloads from the daemon. Formatting stays in the CLI/output layer.

## Core commands

| Command | Purpose |
|---------|---------|
| `mxr` | Launch TUI |
| `mxr daemon [--foreground]` | Start daemon |
| `mxr sync [--account NAME] [--status] [--history]` | Trigger or inspect sync |
| `mxr status [--watch]` | Daemon health |
| `mxr doctor` | Diagnostics and index/store checks |

## Mail retrieval and inspection

```bash
mxr search QUERY
mxr count QUERY
mxr cat MESSAGE_ID
mxr thread THREAD_ID
mxr headers MESSAGE_ID
mxr export THREAD_ID --format markdown|json|mbox|llm
mxr export --search QUERY --format mbox
```

Useful flags:

- `mxr search --format table|json|csv|ids`
- `mxr search --limit N`
- `mxr cat --raw`
- `mxr cat --html`
- `mxr thread --format json`
- `mxr export --output PATH`

## Saved searches

```bash
mxr saved
mxr saved list
mxr saved add NAME QUERY
mxr saved delete NAME
mxr saved run NAME
```

## Compose and drafts

```bash
mxr compose
mxr compose --to alice@example.com --subject "hello"
mxr reply MESSAGE_ID
mxr reply-all MESSAGE_ID
mxr forward MESSAGE_ID --to team@example.com
mxr drafts
mxr send DRAFT_ID
```

Useful flags:

- `--body`
- `--body-stdin`
- `--attach PATH` repeatable
- `--from ACCOUNT`
- `--yes`
- `--dry-run`

## Mail mutations

Single-message or search-scoped:

```bash
mxr archive MESSAGE_ID
mxr trash MESSAGE_ID
mxr spam MESSAGE_ID
mxr star MESSAGE_ID
mxr unstar MESSAGE_ID
mxr read MESSAGE_ID
mxr unread MESSAGE_ID
mxr label LABEL MESSAGE_ID
mxr unlabel LABEL MESSAGE_ID
mxr move LABEL MESSAGE_ID
mxr unsubscribe MESSAGE_ID
mxr open MESSAGE_ID
```

Search-scoped examples:

```bash
mxr archive --search "label:inbox older_than:30d" --yes
mxr label FollowUp --search "from:recruiter@example.com" --yes
mxr move Done --search "label:inbox from:billing@example.com" --dry-run
```

Shared mutation flags:

- `--search QUERY`
- `--yes`
- `--dry-run`

## Snooze

```bash
mxr snooze MESSAGE_ID --until tomorrow
mxr snooze --search "label:inbox from:alerts@example.com" --until monday
mxr unsnooze MESSAGE_ID
mxr unsnooze --all
mxr snoozed
```

Accepted preset values include `tomorrow`, `monday`, `weekend`, `tonight`, or an ISO8601 timestamp.

## Attachments

```bash
mxr attachments list MESSAGE_ID
mxr attachments download MESSAGE_ID
mxr attachments download MESSAGE_ID 1 --dir ~/Downloads
mxr attachments open MESSAGE_ID 1
```

## Labels

```bash
mxr labels
mxr labels create NAME --color "#ff6600"
mxr labels rename OLD NEW
mxr labels delete NAME
```

## Rules

```bash
mxr rules
mxr rules show RULE_ID
mxr rules add NAME --when QUERY --then ACTION
mxr rules edit RULE_ID --then ACTION --disable
mxr rules validate --when QUERY --then ACTION
mxr rules enable RULE_ID
mxr rules disable RULE_ID
mxr rules delete RULE_ID
mxr rules dry-run RULE_ID
mxr rules dry-run --all
mxr rules history
```

## Accounts

```bash
mxr accounts
mxr accounts add gmail
mxr accounts add imap
mxr accounts add smtp
mxr accounts show ACCOUNT
mxr accounts test ACCOUNT
```

## Observability

```bash
mxr events
mxr events --type sync
mxr notify
mxr notify --watch
mxr logs
mxr logs --level error
mxr logs --purge
mxr bug-report
mxr bug-report --stdout
mxr bug-report --github
```

## Config and shell integration

```bash
mxr config path
mxr version
mxr completions bash
mxr completions zsh
mxr completions fish
```

## Output formats

Many commands support `--format`:

- `table`
- `json`
- `csv`
- `ids`

`ids` is especially useful for shell pipelines:

```bash
mxr search "label:inbox unread" --format ids
```
