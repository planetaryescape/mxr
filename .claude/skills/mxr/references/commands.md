# mxr CLI Command Reference

## Table of Contents
- [Account Management](#account-management)
- [Query Commands](#query-commands)
- [Compose & Send](#compose--send)
- [Message Mutations](#message-mutations)
- [Snooze](#snooze)
- [Saved Searches](#saved-searches)
- [Attachments](#attachments)
- [Daemon & Diagnostics](#daemon--diagnostics)
- [Output Formats](#output-formats)
- [Search Syntax](#search-syntax)

---

## Account Management

### `accounts`
List, add, show, or test accounts.

```bash
mxr accounts                    # List all accounts
mxr accounts add gmail          # Add Gmail account (OAuth flow)
mxr accounts show personal      # Show account details
mxr accounts test work          # Test account connectivity
```

---

## Query Commands

### `search [<query>] [--format <fmt>] [--limit <n>]`
Search messages using BM25 full-text search.

```bash
mxr search "from:alice@example.com"
mxr search "is:unread" --format json --limit 100
mxr search "subject:invoice" --format ids
mxr search "from:boss label:important" --format csv
```

### `count <query>`
Count matching messages. Returns single number.

```bash
mxr count "is:unread"
mxr count "from:alice@example.com is:starred"
```

### `cat <message_id> [--html] [--format <fmt>]`
Display a single message body.

```bash
mxr cat 550e8400-e29b-41d4-a716-446655440000
mxr cat 550e8400-e29b-41d4-a716-446655440000 --html
mxr cat 550e8400-e29b-41d4-a716-446655440000 --format json
```

### `thread <thread_id> [--format <fmt>]`
Display full thread with all messages.

```bash
mxr thread 550e8400-e29b-41d4-a716-446655440000
mxr thread 550e8400-e29b-41d4-a716-446655440000 --format json
```

### `headers <message_id>`
Show raw RFC headers.

```bash
mxr headers 550e8400-e29b-41d4-a716-446655440000
```

### `labels`
List all labels with unread/total counts.

```bash
mxr labels
```

### `status [--format <fmt>]`
Show daemon status: uptime, accounts, message count.

```bash
mxr status
mxr status --format json
```

---

## Compose & Send

All compose commands open `$EDITOR` with markdown + YAML frontmatter unless `--body` or `--body-stdin` is provided.

### `compose [options]`
Compose a new email.

```bash
mxr compose --to alice@example.com --subject "Hello"
mxr compose --to alice@example.com --subject "Test" --body "Hi there"
mxr compose --to a@x.com,b@x.com --cc c@x.com --attach ~/file.pdf
echo "Body" | mxr compose --to alice@example.com --subject "Test" --body-stdin
mxr compose --to alice@example.com --subject "Test" --dry-run
```

Flags: `--to`, `--cc`, `--bcc`, `--subject`, `--body`, `--body-stdin`, `--attach <path>` (repeatable), `--from <account>`, `--yes`, `--dry-run`

### `reply <message_id> [options]`
Reply to sender only.

```bash
mxr reply 550e8400-e29b-41d4-a716-446655440000
mxr reply 550e8400-e29b-41d4-a716-446655440000 --body "Thanks!"
```

### `reply-all <message_id> [options]`
Reply to all recipients.

```bash
mxr reply-all 550e8400-e29b-41d4-a716-446655440000
```

### `forward <message_id> [--to <email>] [options]`
Forward a message.

```bash
mxr forward 550e8400-e29b-41d4-a716-446655440000 --to bob@example.com
```

Reply/reply-all/forward share flags: `--body`, `--body-stdin`, `--yes`, `--dry-run`

### `drafts`
List saved drafts.

### `send <draft_id>`
Send a draft by ID.

---

## Message Mutations

All mutations accept either `<message_id>` (single) OR `--search <query>` (batch). Mutually exclusive. All support `--yes` and `--dry-run`.

### Single message

```bash
mxr archive <id>
mxr trash <id>
mxr spam <id>
mxr star <id>
mxr unstar <id>
mxr read <id>
mxr unread <id>
mxr label "project-x" <id>
mxr unlabel "project-x" <id>
mxr move "Finance" <id>
mxr unsubscribe <id>
```

### Batch via search

```bash
mxr archive --search "older:30d" --yes
mxr trash --search "from:spam@example.com" --yes
mxr read --search "is:unread from:noreply" --yes
mxr label "todo" --search "from:boss@example.com" --yes
mxr star --search "subject:urgent" --yes --dry-run
```

### `open <message_id>`
Open message in browser (Gmail web UI).

```bash
mxr open 550e8400-e29b-41d4-a716-446655440000
```

---

## Snooze

### `snooze [<message_id>] --until <time> [--search <query>] [--yes] [--dry-run]`
Snooze until specified time.

Shortcuts: `tomorrow`, `tonight`, `monday`-`sunday`, `weekend`/`saturday`
ISO 8601: `2026-03-20T14:30:00` or `2026-03-20T14:30:00Z`
Shortcuts default to 9 AM.

```bash
mxr snooze <id> --until tomorrow
mxr snooze <id> --until monday
mxr snooze --search "is:unread" --until "2026-03-25T09:00:00" --yes
```

### `unsnooze [<message_id>] [--all]`
Unsnooze a message or all snoozed messages.

```bash
mxr unsnooze <id>
mxr unsnooze --all
```

### `snoozed`
List all snoozed messages with snooze/wake times.

---

## Saved Searches

### `saved [action] [--format <fmt>]`

```bash
mxr saved                  # List all saved searches
mxr saved list             # Same as above
mxr saved add inbox-unread "is:unread label:inbox"
mxr saved run inbox-unread --format ids
mxr saved delete inbox-unread
```

---

## Attachments

### `attachments <action>`

```bash
mxr attachments list <message_id>
mxr attachments download <message_id>                     # All attachments
mxr attachments download <message_id> 1 --dir ~/Downloads # Specific (1-based index)
mxr attachments open <message_id> 2                       # Open with system handler
```

---

## Daemon & Diagnostics

### `daemon [--foreground]`
Start daemon explicitly. Usually auto-started.

### `sync [--account <name>] [--status]`
Trigger sync or check sync status.

```bash
mxr sync                        # Sync all accounts
mxr sync --account personal     # Sync specific account
mxr sync --status               # Show sync status
```

### `doctor [--reindex]`
Run diagnostics. `--reindex` removes search index (rebuilds on restart).

### `logs [--no-follow] [--level <level>]`
View daemon logs. Tails by default.

```bash
mxr logs
mxr logs --no-follow --level error
```

### `version`
Show version.

### `completions <shell>`
Generate shell completions. Shells: `bash`, `zsh`, `fish`, `powershell`, `elvish`.

### `config [path]`
Show config file path.

---

## Output Formats

Available on: `search`, `cat`, `thread`, `status`, `saved run`

| Format | Description |
|--------|-------------|
| `table` | Human-readable table (default) |
| `json` | JSON output |
| `csv` | CSV output |
| `ids` | One ID per line (search/saved only) |

---

## Search Syntax

Tantivy BM25 with field boosts. Key prefixes:

| Prefix | Example |
|--------|---------|
| `from:` | `from:alice@example.com` |
| `to:` | `to:bob@example.com` |
| `subject:` | `subject:invoice` |
| `body:` | `body:deadline` |
| `is:unread` | Unread messages |
| `is:starred` | Starred messages |
| `is:read` | Read messages |
| `label:` | `label:important` |

Combine with spaces (AND) or `OR`: `from:alice subject:meeting OR subject:standup`

## Key Notes

- **Message IDs** are UUIDs: `550e8400-e29b-41d4-a716-446655440000`
- **Daemon auto-starts** when any command needs it
- **$EDITOR** used for compose/reply/forward (falls back to detected editors)
- **`--dry-run`** shows what would happen without executing
- **Batch mutations** via `--search` replace per-message operations for bulk actions
