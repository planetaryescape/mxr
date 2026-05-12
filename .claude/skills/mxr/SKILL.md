---
name: mxr
description: "Use the mxr (pronounced 'Mixer') terminal email client CLI to read, search, compose, mutate, triage, and analyze email. Daemon-backed, local-first, fully scriptable. Invoke for any email-shaped task: check/read/search messages, compose/reply/forward, archive/trash/star/label/snooze, screen unknown senders, manage drafts, sender/contact analytics, follow-up reminders, reply-later queue, snippets, LLM-summarize, draft-assist, semantic search, year-in-review, account management, sync, undo. Triggers: 'check email', 'search email', 'compose', 'reply', 'forward', 'archive', 'trash', 'star', 'label', 'snooze', 'unsubscribe', 'screener', 'snippet', 'remind me', 'follow up', 'reply later', 'summarize thread', 'draft a reply', 'semantic search', 'stale threads', 'wrapped', 'response time', 'top senders', 'mxr', 'mixer', 'inbox', 'unread', 'drafts', 'sync email', 'saved search', 'undo'."
---

# mxr CLI

Terminal email client. Daemon-backed, local-first. Every action goes through `mxr <subcommand>`.

> Pronounced "Mixer". Write `mxr` in code; say "Mixer" out loud.

## Core surfaces

```bash
# Read / search
mxr search "is:unread"                          # Find messages (lexical BM25)
mxr search "from:alice" --mode hybrid           # Lexical + semantic (if enabled)
mxr cat <id>                                    # Message body (reader mode)
mxr thread <id>                                 # Whole thread
mxr headers <id>                                # Raw RFC headers
mxr labels                                      # Labels + counts
mxr count "is:unread"                           # Just a number
mxr export <thread_id>                          # Markdown export of thread

# Compose / send
mxr compose --to a@x.com --subject "Hi" --body "Hello"
mxr reply <id> --body "Thanks!"
mxr reply-all <id>
mxr forward <id> --to b@x.com
mxr drafts                                      # List drafts
mxr send <draft_id>                             # Send a draft
mxr send <draft_id> --at "tomorrow 9am"         # Schedule
mxr unsend <draft_id>                           # Cancel a scheduled send
mxr undo <mutation_id>                          # Undo last destructive op (~60s)

# Mutate (positional <id>, stdin pipe, or batch via --search; all support --dry-run, --yes)
mxr archive <id>
mxr read-archive <id>                           # Mark read + archive in one shot
mxr trash <id>                                  # Move to trash
mxr spam <id>                                   # Report as spam
mxr star <id> / mxr unstar <id>
mxr read <id> / mxr unread <id>
mxr label "todo" <id>                           # Apply label
mxr unlabel "todo" <id>
mxr move "Finance" <id>                         # Move to label/folder
mxr unsubscribe <id>                            # Use List-Unsubscribe header
mxr open <id>                                   # Open in Gmail web UI

# Snooze / reminders / reply-later
mxr snooze <id> --until tomorrow                # Or: monday, weekend, "in 2h", RFC3339
mxr unsnooze <id> / mxr unsnooze --all
mxr snoozed                                     # List snoozed
mxr remind <id> --when "monday 9am"             # Fires if no reply by then
mxr remind <id> --cancel
mxr replies add <id>                            # Mark for reply-later
mxr replies                                     # Reply-later queue

# Triage unknown senders (local consent — never round-trips)
mxr screener                                    # Queue of undecided senders (default)
mxr screener allow alice@x.com [--label X]
mxr screener deny noise@x.com                   # Auto-trash + read on ingest
mxr screener feed news@x.com                    # Route to feed (skip inbox)
mxr screener paper-trail receipts@x.com         # Archive on ingest
mxr screener clear alice@x.com

# Snippets (compose-time ;name expansions)
mxr snippets                                    # List
mxr snippets set sig "— Best, {name}" --vars name
mxr snippets remove sig

# LLM-assisted
mxr summarize <thread_id>                       # Or --search QUERY --first / --limit N
mxr draft-assist <thread_id> "decline politely" # Outputs to stdout, never sends
mxr draft-assist --search "from:acme" --first --instruct "schedule a call"
mxr llm status                                  # Provider config status

# Semantic search
mxr semantic status
mxr semantic enable / disable
mxr semantic reindex
mxr semantic profile list / install <name> / use <name>

# Analytics (the unfair advantage of local SQLite)
mxr sender alice@x.com                          # Per-sender aggregates
mxr contacts asymmetry                          # Reply imbalance ranking
mxr contacts decay                              # "Going cold" relationships
mxr subscriptions --rank                        # Newsletters worth dropping (lowest open-rate first)
mxr storage --by sender                         # Disk usage rollup; also: mimetype, label, message
mxr response-time                               # Reply latency percentiles (mine; --theirs flips)
mxr stale                                       # Threads where I owe a reply; --theirs flips
mxr wrapped --ytd                               # Year-in-review (also --year N, --since-days N)

# Attachments
mxr attachments list <id>
mxr attachments download <id> [INDEX] [--dir ~/Downloads]
mxr attachments open <id> [INDEX]

# Saved searches
mxr saved                                       # List
mxr saved add inbox-unread "is:unread label:inbox" [--mode hybrid]
mxr saved run inbox-unread
mxr saved delete inbox-unread

# Rules (deterministic, dry-runnable)
mxr rules                                       # list/show/add/edit/validate/enable/disable/delete/dry-run/history
mxr rules add my-rule --when 'from:boss' --then 'star;label:urgent'
mxr rules dry-run my-rule [--after <ts>]
mxr rules validate --when '...' --then '...'

# Accounts (Gmail, IMAP, IMAP+SMTP, SMTP, Outlook, Outlook-work)
mxr accounts                                    # List
mxr accounts add gmail                          # Interactive wizard; or pass flags
mxr accounts show personal
mxr accounts test work
mxr accounts addresses list <account>           # Manage aliases (affects in/outbound classification)
mxr accounts addresses add <account> alias@x.com
mxr accounts addresses set-primary <account> alias@x.com

# Daemon, sync, diagnostics
mxr setup                                       # First-run wizard (demo|gmail|imap/smtp)
mxr demo                                        # Isolated 50k-message demo inbox
mxr sync [--account NAME] [--status] [--wait]
mxr status [--watch] [--format json]
mxr notify [--watch]                            # Unread summary for status bars
mxr events [--type X]                           # Stream daemon events (JSONL)
mxr history [--category X] [--level X]          # Persisted event history
mxr logs [--level error] [--since 1h] [--no-follow]
mxr doctor [--reindex|--reindex-semantic|--check|--rebuild-analytics|--refresh-contacts]
mxr bug-report [--edit|--stdout|--clipboard|--github|--output FILE]
mxr web [--port N] [--auto-port] [--no-open] [--remote-host HOST] # Open browser UI at mxr.localhost
mxr web stop                                          # Stop detached local web bridge
mxr restart                                     # Restart the daemon
mxr reset --hard [--dry-run] [--including-config]     # Wipe local state; preserves config+creds by default
mxr burn                                        # Alias for reset --hard
mxr config show|path|edit|get <key>|set <k> <v>
mxr version
mxr completions <bash|zsh|fish|powershell|elvish>
```

## Patterns the agent must follow

1. **Message IDs are UUIDs.** Get them from `mxr search --format ids` (one per line) or `--format json`.
2. **Batch mutations.** Most mutations accept either positional `<id>...`, piped stdin IDs, OR `--search <query>`. Use `--search` for bulk operations. Always pair with `--yes` in non-interactive contexts. Mutually exclusive with positional IDs.
3. **`--dry-run` first.** Available on every mutation, compose flow, `rules dry-run`, `reset --dry-run`, `undo --dry-run`. Use it to preview before committing — especially for `--search`-driven batches.
4. **Output formats.** `--format` accepts `table` (default, human), `json` (single payload), `jsonl` (one obj per line, great for streams), `csv`, `ids` (one ID per line — pipe into other commands). Honored across search, count, cat, thread, status, saved run, snoozed, replies list, screener list, contacts \*, subscriptions, storage, response-time, stale, wrapped, events, history, notify, logs (parses log lines), drafts list, sync --status.
5. **Daemon auto-starts.** No need to `mxr daemon` first; commands launch it on demand.
6. **Compose uses `$EDITOR`** unless `--body` or `--body-stdin` is given. YAML frontmatter + markdown body. Snippets expand via `;name` in the editor.
7. **Undo window is ~60s.** Destructive mutations (`archive`, `trash`, `spam`, `read`, `read-archive`) print a mutation ID; pass it to `mxr undo <mutation_id>`.
8. **Reader mode.** `mxr cat <id>` defaults to plain-text reader view. `--view raw|html|headers` or shortcuts `--raw` / `--html` to override. `--assets` includes inline images.
9. **`accounts addresses`** controls direction inference. If a user has aliases, register them so inbound/outbound classification (and `stale`, `response-time`, `contacts`) is correct.
10. **`reset --hard` and `burn` wipe local runtime state only** — they preserve config and credentials unless `--including-config` is passed. For non-interactive: also add `--yes-i-understand-this-destroys-local-state`.

## Typical workflows

### Triage inbox
```bash
mxr screener                                              # Decide on unknown senders first
mxr search "is:unread label:inbox" --format json --limit 20
mxr read-archive --search "from:noreply older:7d" --yes   # Bulk newsletter sweep
mxr replies add <id>                                      # Mark interesting ones for later
```

### Reply with LLM scaffold (never auto-sends)
```bash
mxr search "from:alice is:unread" --format ids --limit 1 | head -1 | xargs mxr thread
mxr draft-assist <thread_id> "agree to the meeting, propose Tuesday 2pm"   # → stdout
mxr reply <message_id> --body "$(...)" --dry-run
mxr reply <message_id> --body "..."
```

### Bulk cleanup with preview
```bash
mxr archive --search "label:notifications older:30d" --dry-run    # Preview count + sample
mxr archive --search "label:notifications older:30d" --yes        # Execute
mxr undo <mutation_id>                                            # If wrong (~60s window)
```

### Find what's slipping
```bash
mxr stale --mine --older-than-days 7          # Threads waiting on me
mxr stale --theirs --older-than-days 14       # Threads where they owe a reply
mxr contacts decay --threshold-days 60         # Going-cold relationships
mxr response-time                              # My reply percentiles
```

### Schedule a send and unsend
```bash
mxr compose --to a@x.com --subject "..." --body "..." --dry-run
mxr compose --to a@x.com --subject "..." --body "..."   # Becomes draft
mxr drafts                                              # Get the draft ID
mxr send <draft_id> --at "monday 9am"
mxr unsend <draft_id>                                   # Before it fires
```

### Year-in-review
```bash
mxr wrapped --ytd
mxr wrapped --year 2025 --format json
```

## Full command reference

See [references/commands.md](references/commands.md) for every flag and subcommand, including search syntax (`from:`, `to:`, `subject:`, `body:`, `label:`, `is:unread|starred|sent|trash|spam|draft|inbox|archived|answered|important|muted`, `in:<box>`, `has:attachment|drive|document|spreadsheet|presentation|youtube|inline|userlabels|nouserlabels`, `after:`, `before:`, `older:Nd|Nw|Nm|Ny`, `newer:`, `larger:`, `smaller:`, `size:`, `filename:`, `list:`, `deliveredto:`, `category:promotions|social|updates|forums|personal|purchases|reservations`, `OR`, plus presets `today|yesterday|this-week|this-month`).
