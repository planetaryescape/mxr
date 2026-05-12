# mxr CLI Command Reference

Complete reference for the `mxr` CLI. Pronounced "Mixer".

## Table of contents

- [Conventions](#conventions)
- [Output formats](#output-formats)
- [Search syntax](#search-syntax)
- [Query / read](#query--read)
- [Compose, drafts, send](#compose-drafts-send)
- [Message mutations](#message-mutations)
- [Snooze, reminders, reply-later](#snooze-reminders-reply-later)
- [Screener (sender triage)](#screener-sender-triage)
- [Snippets](#snippets)
- [LLM-assisted](#llm-assisted)
- [Semantic search](#semantic-search)
- [Analytics & relationship intelligence](#analytics--relationship-intelligence)
- [Saved searches](#saved-searches)
- [Attachments](#attachments)
- [Rules](#rules)
- [Labels](#labels)
- [Accounts](#accounts)
- [Daemon, sync, diagnostics](#daemon-sync-diagnostics)
- [Web bridge](#web-bridge)
- [Config](#config)
- [Reset / burn / undo](#reset--burn--undo)
- [Notes for agents](#notes-for-agents)

---

## Conventions

- **Message ID** = UUIDv7 (e.g. `550e8400-e29b-41d4-a716-446655440000`). Get them from `mxr search --format ids`.
- **Thread ID** = UUIDv7. From `mxr search --format json` (`thread_id` field) or `mxr thread --search QUERY --first --format json`.
- **Draft ID** = UUIDv7. From `mxr drafts --format json` or printed by `mxr compose`.
- **Mutation ID** = printed by destructive mutations (`archive`, `trash`, `spam`, `read`, `read-archive`). Pass to `mxr undo` within ~60s.
- **`--dry-run`** = preview without mutating. Exercises the same selection path as the real mutation.
- **`--yes`** = skip confirmation. Required for non-interactive batch mutations.
- **`--search QUERY`** = batch path; resolves to multiple IDs. Mutually exclusive with positional `<id>...`. Most mutations also accept piped IDs on stdin.
- **`--first` / `--limit N`** (on read-shaped commands with `--search`) = cap how many matches the command iterates over.

---

## Output formats

`--format` is honored on read/list/status commands. Default is `table`.

| Format  | Description |
|---------|-------------|
| `table` | Human-readable table |
| `json`  | Single JSON payload |
| `jsonl` | One JSON object per line (good for streams, `events`, `history`, search) |
| `csv`   | CSV |
| `ids`   | One ID per line — pipe straight into mutations |

Available on (non-exhaustive): `search`, `count`, `cat`, `thread`, `status`, `saved run`, `snoozed`, `replies list`, `screener list`, `contacts asymmetry|decay`, `subscriptions`, `storage`, `response-time`, `stale`, `wrapped`, `events`, `history`, `notify`, `logs`, `drafts list`, `sync --status`, `accounts`, `labels`, `rules list`, `summarize`, `draft-assist`, `attachments list`.

---

## Search syntax

BM25 lexical search by default (Tantivy). `--mode hybrid` adds dense recall via RRF when semantic is enabled. `--mode semantic` is dense-only.

### Field prefixes

| Prefix | Example | Notes |
|--------|---------|-------|
| `from:` | `from:alice@x.com` | |
| `to:` | `to:bob@x.com` | |
| `cc:` | `cc:legal@x.com` | |
| `bcc:` | `bcc:archive@x.com` | |
| `subject:` | `subject:invoice` | |
| `body:` | `body:deadline` | |
| `filename:` | `filename:contract.pdf` | Attachment names |
| `list:` | `list:newsletter@x.com` | List-ID header |
| `deliveredto:` | `deliveredto:alias@x.com` | |
| `label:` | `label:Important` | Any label, including user labels |
| `category:` | `category:promotions` | Gmail-style: `promotions`, `social`, `updates`, `forums`, `personal`, `purchases`, `reservations` |

### State filters

| Filter | Meaning |
|--------|---------|
| `is:unread` / `is:read` | |
| `is:starred` | |
| `is:important` | Maps to `IMPORTANT` label |
| `is:muted` | Maps to `MUTED` label |
| `is:draft` / `is:drafts` | |
| `is:sent` | |
| `is:trash` / `is:deleted` | |
| `is:spam` / `is:junk` | |
| `is:answered` / `is:replied` | |
| `is:inbox` | |
| `is:archived` / `is:archive` | |

### Location

| `in:` value | Box |
|-------------|-----|
| `inbox` | Inbox |
| `anywhere` / `all` / `allmail` / `all_mail` | All mail |
| `drafts` | Drafts |
| `sent` | Sent |
| `trash` / `deleted` | Trash |
| `spam` / `junk` | Spam |
| `archived` / `archive` | Archive |
| `snoozed` | Snoozed |

### Has / attachments

| `has:` value | Match |
|--------------|-------|
| `attachment` / `attachments` | Any attachment |
| `drive` | Google Drive link |
| `document` / `spreadsheet` / `presentation` | Specific Drive types |
| `youtube` | YouTube link |
| `inline` / `image` / `inline-image` / `inline-images` | Inline image |
| `userlabels` / `nouserlabels` | Has any / no user labels |
| `yellow-star`, `red-star`, `orange-star`, `green-star`, `blue-star`, `purple-star`, `red-bang`, `yellow-bang`, `green-check`, `blue-info`, `purple-question` | Gmail superstars |

### Time

| Form | Example |
|------|---------|
| `after:` / `before:` | `after:2026-01-01`, `before:2026/03/15` |
| `older:Nd|Nw|Nm|Ny` | `older:30d`, `older:2w`, `older:6m` |
| `newer:` (same units) | `newer:7d` |
| `newer_than:` / `older_than:` | Same as above |
| `date:` | Specific date |
| Presets | `today`, `yesterday`, `this-week`, `this-month` |

### Size

| Form | Example |
|------|---------|
| `larger:` / `smaller:` | `larger:5M smaller:7M` |
| `size:` | `size:10MB` |

Units: `b`, `kb`, `mb`/`m`, `gb`. Time units: `d`, `w`, `m` (months), `y`.

### Combining

- Space = AND: `from:alice subject:meeting`
- `OR`: `subject:meeting OR subject:standup`
- Mix freely: `from:alice subject:invoice is:unread after:2026-01-01 has:attachment`

---

## Query / read

### `mxr search [QUERY] [OPTIONS]`
Search messages.

```
--format <FORMAT>         table | json | jsonl | csv | ids
--limit <N>               Default 50
--mode <MODE>             lexical (default) | hybrid | semantic
--sort <SORT>             date | relevance
--explain                 Print scoring breakdown
```

### `mxr count <QUERY> [OPTIONS]`
Count matching messages. Returns a single number.

```
--mode <MODE>             lexical | hybrid | semantic
--format <FORMAT>
```

### `mxr cat [MESSAGE_ID] [OPTIONS]`
Display a message. ID can be positional, piped on stdin, or resolved via `--search`.

```
--search <QUERY>          Iterate matches with separators
--first                   Most recent match only
--limit <N>               Cap matches (multi-message output)
--view <VIEW>             reader (default) | raw | html | headers
--assets                  Include inline images
--raw                     Shortcut for --view raw
--html                    Shortcut for --view html
--format <FORMAT>
```

### `mxr thread [THREAD_ID] [OPTIONS]`
Display a full thread.

```
--search <QUERY>          Dedupe by thread
--first / --limit <N>
--format <FORMAT>
```

### `mxr headers [MESSAGE_ID] [OPTIONS]`
Raw RFC 5322 headers. Same `--search` / pipe semantics as `cat`.

### `mxr export [THREAD_ID] [OPTIONS]`
Export thread(s) or search results.

```
--search <QUERY>
--format <FORMAT>         markdown (default) | json | mbox | llm-context
--output <PATH>
```

### `mxr labels`
List labels with unread/total counts.

### `mxr status [--watch] [--format <FMT>]`
Daemon status: uptime, accounts, message count, sync state.

---

## Compose, drafts, send

All compose commands open `$EDITOR` (markdown + YAML frontmatter) unless `--body` or `--body-stdin` is provided. Inline `;snippet` expansions are resolved at parse time.

### `mxr compose [OPTIONS]`
```
--to <EMAILS>             Comma-separated
--cc <EMAILS>
--bcc <EMAILS>
--subject <SUBJECT>
--body <STRING>
--body-stdin              Read body from stdin
--attach <PATH>           Repeatable
--from <ACCOUNT_KEY>      Account to send from
--yes
--dry-run
--format <FORMAT>
```

### `mxr reply <MESSAGE_ID> [OPTIONS]`
```
--body <STRING> | --body-stdin
--yes / --dry-run / --format
```

### `mxr reply-all <MESSAGE_ID> [OPTIONS]`
Same flags as `reply`.

### `mxr forward <MESSAGE_ID> [OPTIONS]`
```
--to <EMAILS>
--body <STRING> | --body-stdin
--yes / --dry-run / --format
```

### `mxr drafts [SUBCOMMAND]`
```
mxr drafts                # list (default)
mxr drafts list
mxr drafts recover        # Show orphaned 'sending' drafts (auto-reset after 1h)
mxr drafts resume <id>    # Force-reset orphaned draft to 'draft'
mxr drafts discard <id>   # Permanently delete
```

### `mxr send <DRAFT_ID> [OPTIONS]`
```
--at <TIME>               Schedule (same forms as snooze --until)
--dry-run                 Show what would be sent
--format <FORMAT>
```

### `mxr unsend <DRAFT_ID>`
Cancel a scheduled send. Draft itself is preserved.

---

## Message mutations

All accept a positional `<MESSAGE_ID>...`, piped IDs on stdin, OR `--search <QUERY>`. All support `--yes` and `--dry-run`. Destructive ones print a mutation ID for `mxr undo`.

| Command | Effect |
|---------|--------|
| `mxr archive <id>` | Remove from inbox |
| `mxr read-archive <id>` | Mark read + archive |
| `mxr trash <id>` | Move to trash |
| `mxr spam <id>` | Report as spam |
| `mxr star <id>` / `mxr unstar <id>` | |
| `mxr read <id>` / `mxr unread <id>` | |
| `mxr label "<name>" <id>` / `mxr unlabel "<name>" <id>` | |
| `mxr move "<label>" <id>` | Move to label/folder |
| `mxr unsubscribe <id>` | Use List-Unsubscribe header |

### `mxr open [MESSAGE_ID] [OPTIONS]`
Open in browser (provider web UI).
```
--search <QUERY> --first | --limit <N> --yes   # Multi-tab requires --yes
```

### `mxr undo <MUTATION_ID> [OPTIONS]`
Undo a recent destructive op. ~60s window.
```
--dry-run / --format
```

---

## Snooze, reminders, reply-later

### `mxr snooze [MESSAGE_ID]... --until <TIME> [OPTIONS]`
Time forms: presets (`tomorrow`, `tonight`, `monday`..`sunday`, `weekend`), conversational (`in 2h`, `tomorrow 9am`, `monday 5pm`), RFC3339 (`2026-06-01T15:00:00Z`).
```
--search <QUERY> / --yes / --dry-run / --format
```

### `mxr unsnooze [MESSAGE_ID]... [OPTIONS]`
```
--all                     Unsnooze everything
--dry-run / --format
```

### `mxr snoozed [--format <FMT>]`
List snoozed with snooze + wake times.

### `mxr remind <MESSAGE_ID> [OPTIONS]`
Follow-up reminder on an outbound message — fires only if no reply arrives.
```
--when <TIME>             Same forms as snooze --until
--cancel                  Cancel an existing reminder
```

### `mxr replies [SUBCOMMAND]`
Reply-later queue.
```
mxr replies               # list (default)
mxr replies list
mxr replies add <message_id>
mxr replies remove <message_id>
```

---

## Screener (sender triage)

Local-only consent metadata for unknown senders. Never round-trips to the provider.

```
mxr screener                          # queue (default)
mxr screener queue [--limit N]
mxr screener list
mxr screener allow <email> [--label X]            # Into inbox
mxr screener deny <email>                          # Auto-trash + read on ingest
mxr screener feed <email> [--label X]              # Skip inbox, route to feed
mxr screener paper-trail <email> [--label X]       # Archive on ingest
mxr screener clear <email>                         # Drop the decision

# Global options
--account <NAME>          Restrict to a specific account
--format <FORMAT>
```

---

## Snippets

`;name` expansions resolved during compose.

```
mxr snippets                          # list (default)
mxr snippets list
mxr snippets set <name> "<body>" [--vars var1,var2]   # {var_name} placeholders
mxr snippets remove <name>
```

---

## LLM-assisted

Requires `[llm] enabled = true` in config. Local (Ollama/LM Studio) and cloud (OpenAI, etc.) providers supported.

### `mxr summarize [THREAD_ID] [OPTIONS]`
Summarise a thread. Multi-summary output separated by `--- THREAD_ID ---`.
```
--search <QUERY>
--first                   Only most recent matching thread
--limit <N>               Cap threads summarized (mind metered endpoints)
--format <FORMAT>
```

### `mxr draft-assist [THREAD_ID] [INSTRUCTION] [OPTIONS]`
Generate a draft reply. Output goes to **stdout** — never auto-sends. Pipe into `$EDITOR` or use in `--body`.
```
mxr draft-assist <thread_id> "decline politely"
mxr draft-assist --search "from:acme" --first --instruct "schedule a call"

--search <QUERY>
--first
--limit <N>
--instruct <TEXT>         Long form; required when --search is used
--format <FORMAT>
```

### `mxr llm status [--format <FMT>]`
Show configured LLM provider, model, reachability.

---

## Semantic search

Local dense retrieval layered on top of lexical BM25. Embeddings stay local.

```
mxr semantic status
mxr semantic enable
mxr semantic disable
mxr semantic reindex
mxr semantic profile list
mxr semantic profile install <name>
mxr semantic profile use <name>       # Switch + rebuild index
```

Use modes via `mxr search --mode hybrid` (RRF: lexical + dense) or `--mode semantic`.

Fielded dense queries respect chunk source kinds: `subject:` → header chunks, `body:` → body chunks, `filename:` → attachment-origin chunks. No OCR.

---

## Analytics & relationship intelligence

### `mxr sender <EMAIL> [OPTIONS]`
Per-sender aggregates: volume, response cadence, open threads.
```
--account <NAME>          Restrict to a specific account
--format <FORMAT>
```

### `mxr contacts <SUBCOMMAND>`
Materialized contacts table.
```
mxr contacts asymmetry [--min-inbound 3] [--limit 50] [--account NAME] [--format FMT]
  # Rank by reply imbalance (|inbound - outbound| / max)

mxr contacts decay [--threshold-days 30] [--max-lookback-days 1095] [--limit 50] [--account NAME]
  # Contacts where last inbound is older than last outbound by > threshold-days

mxr contacts refresh
  # Force full refresh of the materialized contacts table
```

### `mxr subscriptions [OPTIONS]`
Senders with `List-Unsubscribe` support.
```
--limit <N>               Default 200
--rank                    Sort by ROI: lowest open-rate first, archived-unread DESC
--format <FORMAT>
```

### `mxr storage [OPTIONS]`
Disk consumption rollup.
```
--by <DIM>                sender (default) | mimetype | label | message
                          (`message` returns biggest emails with IDs — pipe into trash/search)
--limit <N>               Default 50
--account <NAME>
--format <FORMAT>
```

### `mxr response-time [OPTIONS]`
Reply-latency percentiles (clock + business-hours).
```
--theirs                  Their reply latency (default: mine)
--counterparty <EMAIL>    Restrict to one counterparty
--since-days <N>
--account <NAME>
--format <FORMAT>
```

### `mxr stale [OPTIONS]`
Stale threads waiting for a reply.
```
--mine                    Last message is inbound — I owe a reply (default)
--theirs                  Last message is outbound — they owe me
--older-than-days <N>     Default 14 (excludes more recent activity)
--within-days <N>         Default 365 (excludes ancient threads)
--limit <N>               Default 100
--account <NAME>
--format <FORMAT>
```

### `mxr wrapped [OPTIONS]`
Year-in-review: volume, time patterns, top contacts, reply discipline, storage, newsletters, superlatives.
```
--ytd                     Jan 1 → now (default)
--year <YYYY>             Full calendar year UTC
--since-days <N>          Last N days (quarterly / ad-hoc)
--account <NAME>
--format <FORMAT>
```

---

## Saved searches

```
mxr saved                                                   # list (default)
mxr saved list
mxr saved add <name> "<query>" [--mode lexical|hybrid|semantic]
mxr saved run <name>
mxr saved delete <name>
--format <FORMAT>
```

---

## Attachments

```
mxr attachments list [MESSAGE_ID] [--search QUERY] [--first] [--limit N] [--format FMT]
mxr attachments download <MESSAGE_ID> [INDEX] [--dir PATH]
  # INDEX is 1-based; omit for all
mxr attachments open <MESSAGE_ID> [INDEX]
  # Open with system handler
```

---

## Rules

Deterministic-first rule engine. Rules are data: inspectable, replayable, idempotent, dry-runnable.

```
mxr rules                             # list (default)
mxr rules list
mxr rules show <name>
mxr rules add <name> --when '<CONDITION>' --then '<ACTION>' [--priority 100]
mxr rules edit <name>                 # Opens in $EDITOR
mxr rules validate --when '<COND>' --then '<ACTION>'
mxr rules enable <name> / disable <name>
mxr rules delete <name>
mxr rules dry-run [<name>] [--all] [--after <ts>]
mxr rules history
--format <FORMAT>
```

---

## Labels

```
mxr labels                            # list with counts (default)
mxr labels create <name>
mxr labels rename <old> <new>
mxr labels delete <name>
--format <FORMAT>
```

---

## Accounts

Supported providers: `gmail`, `imap`, `imap-smtp`, `smtp`, `outlook`, `outlook-work`.

```
mxr accounts                          # list (default)
mxr accounts add <provider>           # Interactive wizard, or pass flags below
mxr accounts show <account>
mxr accounts test <account>           # Connectivity check
mxr accounts repair <account>         # Re-save passwords into protected keychain
mxr accounts disable <account>
mxr accounts remove <account>         # Cached mail kept unless purged
```

### `accounts add` non-interactive flags
```
--account-name <KEY>
--email <EMAIL>
--display-name <NAME>
--gmail-bundled <true|false>          # Use bundled OAuth or custom
--gmail-client-id <ID>
--gmail-client-secret <SECRET>        # Or set MXR_GMAIL_CLIENT_SECRET
--imap-host <HOST>
--imap-port <PORT>                    # Default 993
--imap-no-auth                        # Default: auth required
--imap-username <USER>
--imap-password <PASS>                # Or set MXR_IMAP_PASSWORD
--smtp-host <HOST>
--smtp-port <PORT>                    # Default 587
--smtp-no-auth
--smtp-username <USER>
--smtp-password <PASS>                # Or set MXR_SMTP_PASSWORD
```

### `mxr accounts addresses <SUBCOMMAND>`
Manage owned addresses (aliases). Drives inbound/outbound direction inference for analytics.
```
mxr accounts addresses list <account>
mxr accounts addresses add <account> <email>
mxr accounts addresses remove <account> <email>
mxr accounts addresses set-primary <account> <email>
```

---

## Daemon, sync, diagnostics

### `mxr setup [OPTIONS]`
First-run wizard for demo / Gmail / IMAP / SMTP.
```
--demo                    Drop a fake-provider account in (legacy; prefer `mxr demo`)
--key <KEY>               Account key for demo (default `demo`)
--force                   Overwrite existing account
```

### `mxr demo`
Isolated 50k-message demo profile. Does not touch your real config.

### `mxr daemon [--foreground]`
Start the daemon explicitly. Usually auto-started by other commands.

### `mxr restart`
Restart the daemon with the current binary.

### `mxr sync [OPTIONS]`
```
--account <NAME>
--status                  Show sync state instead of triggering
--wait                    Block until the triggered sync finishes
--wait-timeout-secs <N>   Default 60
--format <FORMAT>         For --status output
```

### `mxr doctor [OPTIONS]`
```
--reindex                 Drop search index (rebuilds on restart)
--reindex-semantic        Drop semantic index
--check                   Health check only
--semantic-status
--verbose
--index-stats / --store-stats
--rebuild-analytics       Reclassify Unknown directions, backfill list_ids, refresh contacts, business-hours latency
--refresh-contacts        Just the contacts table
--format <FORMAT>
```

### `mxr logs [OPTIONS]`
Tails by default.
```
--no-follow
--level <error|warn|info|debug|trace>
--since <DURATION|TIMESTAMP>
--purge
--format <FORMAT>         json/jsonl parses log lines into {timestamp, level, message}
```

### `mxr events [OPTIONS]`
Stream daemon events.
```
--type <EVENT_TYPE>
--format <FORMAT>
```

### `mxr history [OPTIONS]`
Persisted event history.
```
--category <CATEGORY>
--level <LEVEL>
--limit <N>               Default 50
--format <FORMAT>
```

### `mxr notify [--watch] [--format <FMT>]`
Unread summary tailored for status bars (tmux, polybar, etc.).

### `mxr bug-report [OPTIONS]`
Sanitized diagnostic bundle.
```
--edit                    Open in $EDITOR
--stdout
--clipboard
--github                  Open prefilled GitHub issue
--output <PATH>
--verbose
--full-logs
--no-sanitize
--since <DURATION>
```

---

## Web bridge

### `mxr web [OPTIONS] [COMMAND]`
Open `http://mxr.localhost:42829`, reusing the daemon bridge when healthy or starting a detached local bridge. Auto-authenticates via same-machine handshake.
```
stop                      Stop the detached local bridge
--host <ADDR>             Default 127.0.0.1
--port <N>                Default 42829; fixed local URL port
--auto-port               Try next available port on conflict
--print-url               Print URL instead of opening browser
--no-open                 Print URL, don't launch browser
--strict-port             Explicit fail-fast compatibility flag (default behavior)
--foreground              Run bridge in the current terminal for debugging
--remote-host <HOST[:PORT]>
  # Point browser at a manually configured remote bridge. Prefer tunnels.
  # Reads token from
  # ~/.config/mxr/bridge-tokens/<host>.token
```

---

## Config

```
mxr config                            # show resolved config (default)
mxr config show
mxr config path                       # File path
mxr config edit                       # Open in $EDITOR
mxr config get <key>
mxr config set <key> <value>
```

---

## Reset / burn / undo

### `mxr reset --hard [OPTIONS]`
Destroys local runtime state after stopping the daemon. Preserves config.toml and credentials by default.
```
--hard                                                  Required scope marker
--dry-run                                               Preview
--including-config                                      Also delete config.toml (creds still kept)
--yes-i-understand-this-destroys-local-state            Required for non-interactive
```

### `mxr burn`
Alias for `mxr reset --hard`. Same flags.

### `mxr undo <MUTATION_ID> [--dry-run] [--format <FMT>]`
Reverse a recent destructive mutation. ~60s window.

---

## Misc

```
mxr version
mxr completions <bash|zsh|fish|powershell|elvish>
```

---

## Notes for agents

1. **Plan with `--dry-run`.** Especially for `--search`-driven batches; the dry-run prints the count and a sample so you can sanity-check before committing with `--yes`.
2. **Prefer batch via `--search`.** Don't loop over `mxr search --format ids` piped into per-message mutations when a single `mxr <mutation> --search <q> --yes` will do.
3. **`--format ids` is your friend.** When you do need IDs (e.g., to pass to `cat` or `thread` or `attachments`), this is the cheapest form.
4. **`mxr undo` is short-lived.** Capture the mutation ID from any destructive output if you might need it.
5. **`draft-assist` never sends.** Output is stdout. Pipe into `mxr reply --body "$(...)"` or `--body-stdin`.
6. **Reader mode is default for `cat`.** Use `--raw` or `--html` only if you specifically need them.
7. **`mxr web` for human handoff.** When the user wants to do something visual (lots of images, complex compose), launch the web UI rather than fighting the terminal.
8. **`mxr screener` before mass cleanup.** Triaging unknown senders avoids re-archiving the same noise next week.
9. **Aliases matter for analytics.** If `stale`, `response-time`, `contacts`, or `sender` produce odd results, check `mxr accounts addresses list <account>` — direction inference depends on knowing the user's addresses.
10. **`mxr reset --hard` preserves config and credentials by default.** Only suggest `--including-config` if the user explicitly wants a from-scratch reinstall.
