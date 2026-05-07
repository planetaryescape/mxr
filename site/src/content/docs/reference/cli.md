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
| `mxr restart` | Reap the running daemon and start a fresh one against the current binary |
| `mxr sync [--account NAME] [--status] [--history]` | Trigger or inspect sync |
| `mxr status [--watch]` | Daemon health |
| `mxr doctor` | Diagnostics and index/store checks |
| `mxr web [--host HOST] [--port PORT]` | HTTP/WebSocket bridge over the daemon socket |
| `mxr reset --hard` / `mxr burn` | Destroy local runtime state only |

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

- `mxr search --format table|json|jsonl|csv|ids`
- `mxr search --limit N`
- `mxr search --mode lexical|hybrid|semantic`
- `mxr search --explain`
- `mxr cat --view reader|raw|html|headers`
- `mxr cat --assets` (extracts inline images alongside the body)
- `mxr thread --format json`
- `mxr export --output PATH`

### Query operators

The query parser accepts Gmail-style operators in any search query
(`mxr search`, `mxr count`, `mxr saved add`, the TUI `/`, the
`--search` flag on every batch mutation):

| Operator | Example | Notes |
|---|---|---|
| `from:` / `to:` / `cc:` / `bcc:` | `from:alice@example.com` | substring + display-name match |
| `subject:` | `subject:"quarterly review"` | quoted phrase, exact within tokens |
| `body:` | `body:reimbursement` | full-text body |
| `label:` | `label:inbox` | matches by `provider_id` (case-insensitive) |
| `is:` | `is:unread`, `is:starred`, `is:answered` | flags |
| `has:` | `has:attachment` | |
| `before:` / `after:` | `after:2025-01-01` | YYYY-MM-DD |
| `older_than:` / `newer_than:` | `older_than:30d`, `newer_than:7d` | days |
| `filename:` | `filename:invoice.pdf` | attachment names |
| `OR`, `AND`, `NOT`, `(...)` | `from:vendor AND (label:bills OR label:travel)` | AND is implicit between bare terms |

Search modes:

- `lexical`: Tantivy BM25 only
- `hybrid`: lexical + dense retrieval + RRF
- `semantic`: dense retrieval only

Fielded hybrid examples:

```bash
mxr search "body:house of cards" --mode hybrid --explain
mxr search "subject:quarterly report" --mode hybrid --explain
mxr search "filename:roadmap" --mode hybrid --explain
```

Dense side intent:

- `subject:` -> header chunks
- `body:` -> body chunks
- `filename:` -> attachment-origin chunks

## Saved searches

```bash
mxr saved
mxr saved list
mxr saved add owe-replies "is:unread label:inbox older_than:3d"
mxr saved add hot-clients "from:client@bigcorp.com" --mode hybrid
mxr saved delete owe-replies
mxr saved run owe-replies
mxr saved run owe-replies --format ids | xargs -n1 mxr archive --yes
```

`--mode` accepts `lexical`, `hybrid`, or `semantic` (defaults to whatever
`config.search.default_mode` is set to). Saved searches show up in the
TUI sidebar as one-key lenses.

## Compose and drafts

```bash
mxr compose
mxr compose --to alice@example.com --subject "hello"
mxr reply MESSAGE_ID
mxr reply-all MESSAGE_ID
mxr forward MESSAGE_ID --to team@example.com
mxr drafts
mxr send DRAFT_ID
mxr send DRAFT_ID --dry-run     # account, recipients, subject, body bytes; no provider call
```

Useful flags on the compose verbs:

- `--body`
- `--body-stdin`
- `--attach PATH` repeatable
- `--from ACCOUNT`
- `--yes`
- `--dry-run` (compose verbs render a preview; `mxr send --dry-run` shows what would be sent without touching the provider)

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

Undoable mutations (`archive`, `trash`, `spam`, `read`, `read-archive`)
print a `mutation_id` after they land. Pass it to `mxr undo` within
~60 seconds to reverse the operation.

## Undo

```bash
mxr archive abc123             # prints: mutation_id=mut_01HW...
mxr undo mut_01HW...           # restores the original labels and read state
```

The undo log is local: the daemon snapshots the message's labels and
flags before each undoable mutation and replays the inverse on `mxr
undo`. Tantivy is reindexed after the restore. Window is fixed at ~60
seconds; older mutation IDs return `not found`.

The TUI binds `u` to the same operation against the most recent
mutation — handy for "oops, didn't mean to archive that."

## Snooze

```bash
mxr snooze MESSAGE_ID --until tomorrow
mxr snooze --search "label:inbox from:alerts@example.com" --until monday
mxr unsnooze MESSAGE_ID
mxr unsnooze --all
mxr unsnooze --all --dry-run     # lists which messages would wake; no mutation
mxr snoozed
```

Accepted `--until` preset values include `tomorrow`, `monday`,
`weekend`, `tonight`, or an ISO8601 timestamp. The TUI binds `Z` to
snooze the focused message via the same path.

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
mxr accounts                                  # list accounts
mxr accounts add gmail                        # interactive OAuth (asks: bundled or BYO client)
mxr accounts add gmail --gmail-bundled true   # non-interactive: use mxr's bundled OAuth client
mxr accounts add imap                         # interactive: prompts for host, port, password
mxr accounts add imap --imap-host imap.fastmail.com --imap-username me@fm --imap-password ENV:IMAP_PW
mxr accounts add smtp --smtp-host smtp.fastmail.com --smtp-username me@fm --smtp-password ENV:SMTP_PW
mxr accounts add outlook                      # device-code OAuth for personal Outlook (alpha; via IMAP)
mxr accounts show ACCOUNT
mxr accounts test ACCOUNT                     # round-trips auth without writing anything

# Maintenance
mxr accounts repair ACCOUNT                   # re-prompts for the credential and overwrites the keychain entry
mxr accounts disable ACCOUNT                  # stop syncing, keep config + local data
mxr accounts remove ACCOUNT                   # remove from config; preserves local data
mxr accounts remove ACCOUNT --purge-local-data  # also drops messages, labels, semantic chunks for that account

# Owned addresses (for inbound/outbound classification + analytics)
mxr accounts addresses list
mxr accounts addresses add me@example.com --primary
mxr accounts addresses add alias@example.com
mxr accounts addresses set-primary alias@example.com
mxr accounts addresses remove old@example.com
```

OAuth refresh tokens (Gmail, Outlook personal) live in the OS keychain
(macOS Keychain, Linux Secret Service). IMAP and SMTP passwords live in
the same place. `mxr accounts repair` is the unified re-prompt path
when a credential goes stale.

## Semantic

```bash
mxr semantic status
mxr semantic enable
mxr semantic disable
mxr semantic reindex

mxr semantic profile list
mxr semantic profile install bge-small-en-v1.5
mxr semantic profile use multilingual-e5-small
```

Notes:

- semantic search is an optional local platform feature
- embeddings stay local
- sync may prepare semantic chunks even while semantic retrieval is disabled
- OCR is not used for semantic indexing

## Analytics

```bash
mxr storage --by sender
mxr storage --by mimetype
mxr storage --by label
mxr storage --by message            # individual messages by size
mxr storage --by message --format ids | xargs -n1 mxr trash    # bulk-trash the biggest

mxr wrapped                          # year-to-date inbox stats (default)
mxr wrapped --year 2025
mxr wrapped --since-days 90

mxr subscriptions --rank          # alias: mxr unsub --rank
mxr subscriptions --rank --format json

mxr stale --mine                  # threads where I owe a reply (14d–365d window)
mxr stale --theirs                # threads where they owe a reply
mxr stale --mine --older-than-days 7 --within-days 90

mxr contacts asymmetry --min-inbound 3
mxr contacts decay --threshold-days 30
mxr contacts decay --threshold-days 90 --max-lookback-days 1095
mxr contacts refresh

mxr response-time
mxr response-time --theirs
mxr response-time --counterparty alice@example.com --since-days 90
mxr response-time --since-days 30 --format json   # both clock and business-hours rows

mxr doctor --rebuild-analytics    # 6-step rebuild with live progress
                                  # (rarely needed: daemon self-heals post-sync)
mxr doctor --refresh-contacts     # contacts table only
```

`response-time` always emits both clock-time and business-hours
percentiles per direction; there is no `--working-hours` toggle. Owned
addresses (used to classify inbound vs. outbound) are managed under
`mxr accounts addresses` — see the Accounts section above.

See the [Analytics guide](/guides/analytics/) for what each command means
and how to bootstrap a fresh install.

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

## Local reset

```bash
mxr reset --hard --dry-run
mxr reset --hard --including-config --dry-run
mxr burn --dry-run
mxr burn --including-config
mxr reset --hard --yes-i-understand-this-destroys-local-state
```

Notes:

- destroys local runtime state only: database, indexes, semantic model cache, attachments under `MXR_DATA_DIR`, logs, source temp artifacts, and other rebuildable data-dir state
- stops the daemon first, then removes the planned paths
- preserves `config.toml` and system keychain/keyring credentials by default
- `--including-config` also deletes `config.toml`, but still preserves system keychain/keyring credentials
- attachment dirs outside `MXR_DATA_DIR` stay preserved even with `--including-config`
- interactive destructive runs require typing `DELETE MY MXR DATA`
- interactive `--including-config` runs require typing `DELETE MY MXR DATA AND CONFIG`
- non-interactive destructive runs require `--yes-i-understand-this-destroys-local-state`

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
