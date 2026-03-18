# mxr — Blueprint Addendum

> This document captures decisions and refinements made AFTER the main blueprint (docs/blueprint/) was finalized. The coding agent should treat these as amendments that override or extend the corresponding blueprint sections.

---

## A001: CLI compose without $EDITOR

**Affects**: 06-compose.md, 09-cli.md

**What was missing**: The blueprint only described the `$EDITOR` flow for composing. It did not account for fully inline CLI compose, which is essential for scripting, automation, and Unix composability.

**The rule**: If `--to` and `--body` (or `--body-stdin`) are both provided, skip `$EDITOR` entirely and go straight to send (with confirmation prompt unless `--yes` is passed). If anything is missing, fall back to opening `$EDITOR` with whatever was provided pre-populated in the frontmatter.

### Full CLI compose syntax

```bash
# Full inline compose (no editor opens)
mxr compose --to "alice@example.com" --subject "Quick update" --body "Deployment is done."

# Multiple recipients
mxr compose --to "alice@example.com,bob@example.com" --cc "carol@example.com" --subject "Update" --body "All good."

# Attach files
mxr compose --to "alice@example.com" --subject "Invoice" --body "Attached." --attach ~/invoice.pdf --attach ~/receipt.png

# Specify which account to send from
mxr compose --from work --to "boss@company.com" --subject "Status" --body "On track."

# Pipe body from stdin
echo "Here are the logs" | mxr compose --to "alice@example.com" --subject "Logs" --body-stdin

# Pipe a file as body
cat report.md | mxr compose --to "alice@example.com" --subject "Weekly report" --body-stdin

# Skip confirmation prompt (for scripts/cron)
mxr compose --to "alice@example.com" --subject "Automated alert" --body "Disk usage at 90%" --yes

# Dry run (show what would be sent without sending)
mxr compose --to "alice@example.com" --subject "Test" --body "Hello" --dry-run

# Partial: pre-populate frontmatter, open editor for the rest
mxr compose --to "alice@example.com"
# Opens $EDITOR with 'to' already filled in, user writes subject + body

# Reply inline without editor
mxr reply MESSAGE_ID --body "Sounds good, let's do it."

# Forward inline without editor
mxr forward MESSAGE_ID --to "bob@example.com" --body "FYI, see below."
```

### Flags

| Flag | Description |
|---|---|
| `--to` | Recipient(s), comma-separated |
| `--cc` | CC recipient(s), comma-separated |
| `--bcc` | BCC recipient(s), comma-separated |
| `--subject` | Subject line |
| `--body` | Message body as string argument |
| `--body-stdin` | Read message body from stdin |
| `--attach` | File path to attach (repeatable) |
| `--from` | Account name to send from (uses default if omitted) |
| `--yes` | Skip confirmation prompt |
| `--dry-run` | Show what would be sent without sending |

### Behavior logic

```
if --to AND (--body OR --body-stdin):
    → Build message from flags
    → If --dry-run: print message summary, exit
    → If --yes: send immediately
    → Else: prompt "Send to alice@example.com? [y/n]"
else:
    → Open $EDITOR with whatever flags were provided
      pre-populated in YAML frontmatter
    → Normal editor compose flow from 06-compose.md
```

### Why this matters

Without inline compose, `mxr compose` is only useful interactively. With it, mxr becomes scriptable:

```bash
# Cron job: daily digest
mxr search "label:alerts date:today" --format json | \
  jq -r '[.[].subject] | join("\n- ")' | \
  mxr compose --to "me@example.com" --subject "Today's alerts" --body-stdin --yes

# CI/CD: notify on deploy
mxr compose --from work --to "team@company.com" \
  --subject "v2.3 deployed" \
  --body "Deployment completed at $(date). All health checks passing." \
  --yes

# Batch: send file to multiple people
for email in alice@ex.com bob@ex.com carol@ex.com; do
  mxr compose --to "$email" --subject "Q1 Report" \
    --body "Please find attached." --attach ~/q1-report.pdf --yes
done
```

This aligns directly with principle #8 (shell hooks over premature plugin systems) and the Unix philosophy of composable tools.

### Decision record

**D025: CLI compose without editor**

**Chosen**: Support fully inline compose via CLI flags, skipping $EDITOR when sufficient flags are provided.

**Why**: A CLI tool that always requires an interactive editor isn't scriptable. Unix tools should work in pipelines, cron jobs, and shell scripts. The $EDITOR flow is the default for interactive use. Flags are the override for scripted use. Both paths produce the same Draft and go through the same send pipeline.

---

## A002: Markdown rendering is invisible to recipients

**Affects**: 06-compose.md

**Clarification needed because**: It was unclear from the blueprint whether recipients would see raw markdown. They do not.

**How it works**: The send pipeline converts the markdown body into a standard multipart email:

- **text/html**: Markdown rendered to proper HTML via comrak. `**bold**` becomes `<strong>bold</strong>`, lists become `<ol>`/`<ul>`, links become `<a>` tags. The recipient's email client renders this as a normal formatted email.
- **text/plain**: The raw markdown as the plain text fallback. Markdown is readable as plain text (lists, paragraphs, links all make sense), so this is fine for plain-text-only clients.

Recipients see a normal email. They have no idea it was written in markdown. This is the same approach used by Fastmail's compose and several other email clients.

---

## A003: Web client feasibility via daemon architecture

**Affects**: 01-architecture.md

**Context**: The question was raised whether the daemon architecture would support a future web-based client. The answer is yes, trivially.

**How it works**:

```
Browser (React/Svelte/whatever)
    ↓ HTTP / WebSocket
Thin HTTP server (axum)
    ↓ Unix socket (existing JSON IPC protocol)
mxr daemon (unchanged)
```

The HTTP server is a dumb proxy: receives REST requests, converts them to the same IPC commands the TUI uses, forwards to daemon, returns JSON response. Every endpoint maps 1:1 to an existing daemon Command. For real-time updates, a WebSocket connection subscribes to the same DaemonEvent stream the TUI listens to.

Estimated work: one new crate (`crates/web/`) depending on `mxr-core`, `mxr-protocol`, and `axum`. 500-1000 lines for a full REST API.

**NOT on the roadmap.** But the architecture makes it trivial when the time comes. This is explicitly why daemon-backed architecture was chosen over monolithic (see Decision D002).

---

## A004: Complete CLI command surface for scriptability

**Affects**: 09-cli.md, 08-tui.md, 14-roadmap.md

**What was missing**: The blueprint's CLI command list only covered read operations (search, export, sync, doctor) and compose. Every mutation that exists as a TUI keybinding was missing from the CLI. If you can do it in the TUI, you should be able to script it from the shell. Otherwise mxr is an interactive tool, not a Unix citizen.

### Complete CLI command reference

This supersedes the command list in 09-cli.md.

#### System & daemon

```bash
mxr                                  # Open TUI (start daemon if needed)
mxr daemon                           # Start daemon explicitly
mxr daemon --foreground              # Foreground mode (systemd/launchd/debugging)
mxr daemon stop                      # Stop running daemon gracefully
mxr daemon status                    # Show daemon status (running/stopped, uptime, connected clients)
mxr doctor                           # Run diagnostics (config, auth, connectivity, index health)
mxr doctor --reindex                 # Rebuild Tantivy index from SQLite
mxr config                           # Show fully resolved configuration
mxr config path                      # Print config file path
mxr version                          # Version info (binary version, build info, data dir)
mxr completions bash|zsh|fish        # Generate shell completions
```

#### Accounts

```bash
mxr accounts                         # List configured accounts with status
mxr accounts show NAME               # Show account details
mxr accounts add gmail               # Interactive Gmail account setup (OAuth2 flow)
mxr accounts add imap                # Interactive IMAP account setup
mxr accounts add smtp                # Interactive SMTP account setup
mxr accounts remove NAME             # Remove an account (with confirmation)
mxr accounts default NAME            # Set default account for compose
mxr accounts reauth NAME             # Re-authenticate (refresh OAuth2 tokens)
mxr accounts test NAME               # Test connectivity (sync + send)
mxr accounts --format json           # Machine-readable account list
```

#### Sync

```bash
mxr sync                             # Sync all enabled accounts
mxr sync --account NAME              # Sync specific account
mxr sync --status                    # Show sync status per account
mxr sync --history                   # Recent sync log
mxr sync --watch                     # Live sync output
```

#### Reading messages

```bash
mxr cat MESSAGE_ID                   # Print message body (reader mode applied)
mxr cat MESSAGE_ID --raw             # Print body without reader mode
mxr cat MESSAGE_ID --html            # Print original HTML body
mxr cat MESSAGE_ID --headers         # Print full headers + body
mxr cat MESSAGE_ID --all             # Print everything
mxr cat MESSAGE_ID --format json     # Full message as structured JSON
mxr thread THREAD_ID                 # Print full thread (chronological, reader mode)
mxr thread THREAD_ID --format json   # Thread as structured JSON
mxr headers MESSAGE_ID               # Print raw email headers only
```

#### Search

```bash
mxr search "query"                   # Search, output to stdout
mxr search "query" --format table|json|csv|ids
mxr search "query" --limit 50
mxr search "query" --sort relevance|date_asc|date_desc
mxr search "query" --account NAME
mxr search --saved "name"            # Run a saved search
mxr count "query"                    # Count matching messages
```

#### Saved searches

```bash
mxr saved                            # List all saved searches
mxr saved add "name" "query"         # Create
mxr saved delete "name"              # Delete
mxr saved run "name"                 # Execute
mxr saved --format json
```

#### Compose / Reply / Reply-All / Forward

```bash
# Interactive (opens $EDITOR)
mxr compose
mxr reply MESSAGE_ID
mxr reply-all MESSAGE_ID
mxr forward MESSAGE_ID

# Inline (skip $EDITOR — see A001)
mxr compose --to "X" --subject "Y" --body "Z" [--yes] [--dry-run]
mxr reply MESSAGE_ID --body "LGTM" [--yes]
mxr reply-all MESSAGE_ID --body "Agreed" [--yes]
mxr forward MESSAGE_ID --to "X" --body "FYI" [--yes]
```

#### Drafts

```bash
mxr drafts                           # List all drafts
mxr drafts show DRAFT_ID
mxr drafts edit DRAFT_ID
mxr drafts delete DRAFT_ID
mxr send DRAFT_ID [--yes]
```

#### Single message mutations

```bash
mxr archive MESSAGE_ID
mxr trash MESSAGE_ID
mxr spam MESSAGE_ID
mxr star MESSAGE_ID
mxr unstar MESSAGE_ID
mxr read MESSAGE_ID
mxr unread MESSAGE_ID
mxr label MESSAGE_ID "work"
mxr unlabel MESSAGE_ID "work"
mxr move MESSAGE_ID "archive"
mxr snooze MESSAGE_ID --until tomorrow|monday|weekend|tonight|"2026-03-20"|"2026-03-20 14:00"
mxr unsnooze MESSAGE_ID
mxr unsubscribe MESSAGE_ID [--yes]
mxr open MESSAGE_ID                  # Open HTML in system browser
```

#### Batch mutations via --search

Every single-message mutation also accepts `--search` to operate on all matching messages:

```bash
mxr archive --search "label:newsletters is:read" [--yes] [--dry-run]
mxr trash --search "from:spam@junk.com" --yes
mxr read --search "label:notifications" --yes
mxr label --search "from:boss@work.com" "important"
mxr snooze --search "label:todo is:unread" --until monday
mxr unsubscribe --search "has:unsubscribe label:newsletters -label:keep" --yes
```

Batch operations require `--yes` or interactive confirmation showing count. `--dry-run` shows what would happen.

#### Snooze management

```bash
mxr snoozed                          # List snoozed messages with wake times
mxr unsnooze MESSAGE_ID
mxr unsnooze --all
```

#### Attachments

```bash
mxr attachments MESSAGE_ID
mxr attachments download MESSAGE_ID [INDEX] [--name FILE] [--dir PATH]
mxr attachments open MESSAGE_ID INDEX
```

#### Labels management

```bash
mxr labels
mxr labels create "name" [--color "#hex"]
mxr labels delete "name"
mxr labels rename "old" "new"
```

#### Export

```bash
mxr export THREAD_ID [--format markdown|json|mbox|llm] [--output PATH]
mxr export --search "query" --format mbox > archive.mbox
```

#### Rules

```bash
mxr rules
mxr rules show RULE_ID
mxr rules add "name" --when "query" --then action
mxr rules enable|disable RULE_ID
mxr rules delete RULE_ID
mxr rules dry-run RULE_ID [--after DATE]
mxr rules dry-run --all
mxr rules history [RULE_ID]
```

#### Notification / status

```bash
mxr notify                           # Unread summary (for status bars)
mxr notify --format json
mxr notify --watch                   # Continuous output on new messages
mxr count "query"                    # Just a number
```

### Universal flags

Output commands: `--format table|json|csv|ids`, `--account NAME`, `--limit N`, `--quiet`, `--verbose`

Mutation commands: `--yes`, `--dry-run`, `--search "query"`

Auto-format detection: TTY → table, piped → json. Override with explicit `--format`.

### TUI-to-CLI cross-reference

| TUI | Action | CLI |
|---|---|---|
| `c` | Compose | `mxr compose` |
| `r` | Reply | `mxr reply MESSAGE_ID` |
| `a` | Reply all | `mxr reply-all MESSAGE_ID` |
| `f` | Forward | `mxr forward MESSAGE_ID` |
| `e` | Archive | `mxr archive MESSAGE_ID` |
| `#` | Trash | `mxr trash MESSAGE_ID` |
| `!` | Spam | `mxr spam MESSAGE_ID` |
| `s` | Star | `mxr star/unstar MESSAGE_ID` |
| `I` | Mark read | `mxr read MESSAGE_ID` |
| `U` | Mark unread | `mxr unread MESSAGE_ID` |
| `l` | Apply label | `mxr label MESSAGE_ID "name"` |
| `v` | Move to label | `mxr move MESSAGE_ID "label"` |
| `D` | Unsubscribe | `mxr unsubscribe MESSAGE_ID` |
| `Z` | Snooze | `mxr snooze MESSAGE_ID --until ...` |
| `O` | Open in browser | `mxr open MESSAGE_ID` |
| `E` | Export | `mxr export THREAD_ID` |
| `R` | Reader mode | `mxr cat --raw` vs `mxr cat` |
| `/` | Search | `mxr search "query"` |
| `Enter`/`o` | View | `mxr cat MESSAGE_ID` |

### Decision records

**D026**: Every TUI action has a CLI equivalent. **D027**: `mxr cat` for reading messages. **D028**: `reply` and `reply-all` are separate commands. **D029**: `mxr attachments` subcommand tree. **D030**: `mxr notify` for status bars. **D031**: `mxr count` for quick counts. **D032**: Auto-format detection (TTY vs pipe). **D033**: `mxr spam` command. **D034**: `mxr open` for browser viewing.

---

## A005: Keybinding scheme — vim-native first, Gmail for email actions

**Affects**: 08-tui.md, 12-config.md

**The hierarchy**:
1. **Vim-native first**: Navigation uses vim conventions.
2. **Gmail second**: Email actions use Gmail keyboard shortcuts.
3. **Custom last**: Only invent a keybinding if neither applies.

### Complete revised keybinding map

#### Navigation (vim-native)

```
j / ↓           Move down in list
k / ↑           Move up in list
gg              Jump to top
G               Jump to bottom
Ctrl-d          Half-page down
Ctrl-u          Half-page up
H               Top of visible area
M               Middle of visible area
L               Bottom of visible area
zz              Center current item
Enter / o       Open selected message
Escape          Back / close / cancel
q               Quit current view
/               Search
n               Next search result
N               Previous search result
?               Help
```

#### Email actions (Gmail-native)

```
c               Compose                (Gmail: c)
r               Reply                  (Gmail: r)
a               Reply all              (Gmail: a)
f               Forward                (Gmail: f)
e               Archive                (Gmail: e)
#               Trash                  (Gmail: #)
!               Spam                   (Gmail: !)
s               Star/unstar            (Gmail: s)
I               Mark read              (Gmail: Shift+I)
U               Mark unread            (Gmail: Shift+U)
v               Move to label          (Gmail: v)
l               Apply label            (Gmail: l)
x               Select/check message   (Gmail: x)
```

#### Gmail go-to navigation (`g` prefix)

```
gi              Go to inbox
gs              Go to starred
gt              Go to sent
gd              Go to drafts
ga              Go to all mail
gl              Go to label (picker)
```

Uses same multi-key state machine as `gg`.

#### mxr-specific actions

```
Z               Snooze menu
Ctrl-p          Command palette
R               Toggle reader mode
O               Open HTML in browser
E               Export thread
D               Unsubscribe ("don't subscribe")
Tab             Switch panes
F               Toggle fullscreen
```

#### Attachment handling (in message view)

```
A               Show attachment list
1-9             Select by number
Enter           Download
O               Open with xdg-open
```

### Key changes from original blueprint

| Action | Old | New | Reason |
|---|---|---|---|
| Archive | `a` | `e` | Gmail uses `e` |
| Reply all | `A` | `a` | Gmail uses `a` |
| Trash | `d` | `#` | Gmail uses `#` |
| Unsubscribe | `U` | `D` | `U` is now mark-unread (Gmail) |
| Mark unread | `u` | `U` | Gmail: Shift+U |
| Mark read | implicit | `I` | Gmail: Shift+I |
| Open browser | `o` | `O` | `o` is now open/enter (vim) |

### Multi-select with `x`

`x` toggles selection. When messages are selected, action keys apply to ALL selected. Status bar shows "N selected". `Escape` clears selection.

### Decision records

**D035**: Vim first, Gmail second, custom last. **D036**: Gmail `g` prefix for navigation. **D037**: Multi-select with `x`. **D038**: Archive is `e` not `a`.

---

## A006: Daemon observability — logs, monitoring, diagnostics

**Affects**: 01-architecture.md, 04-sync.md, 09-cli.md, 14-roadmap.md

### Structured logging

Daemon uses `tracing` crate. Output goes to:
1. Log file: `$XDG_DATA_HOME/mxr/logs/mxr.log` (rotated)
2. Stdout (foreground mode)
3. Connected clients (subscribe to log streams)

### CLI observability commands

```bash
mxr logs                             # Tail daemon logs (live)
mxr logs --level warn                # Filter by level
mxr logs --since "1h"                # Time filter
mxr logs --grep "gmail"              # Text filter
mxr logs --category sync|rule|send   # Category filter
mxr logs --format json
mxr status                           # Single-command overview of everything
mxr status --format json
mxr status --watch                   # Live dashboard
mxr events                           # Watch daemon event stream
mxr events --type sync|message|snooze|rule|error|send
mxr events --format json             # JSONL for piping
mxr doctor --check                   # Health check (exit 0 = healthy)
mxr doctor --check --format json
mxr doctor --index-stats
mxr doctor --store-stats
```

### Schema addition: event_log table

```sql
CREATE TABLE IF NOT EXISTS event_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   INTEGER NOT NULL,
    level       TEXT NOT NULL CHECK (level IN ('error', 'warn', 'info')),
    category    TEXT NOT NULL,
    account_id  TEXT,
    message_id  TEXT,
    rule_id     TEXT,
    summary     TEXT NOT NULL,
    details     TEXT,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX idx_event_log_time ON event_log(timestamp DESC);
CREATE INDEX idx_event_log_category ON event_log(category, timestamp DESC);
CREATE INDEX idx_event_log_level ON event_log(level, timestamp DESC);
```

### Logging config

```toml
[logging]
level = "info"
max_size_mb = 50
max_files = 5
stderr = true
event_retention_days = 90
```

### TUI status bar enhancement

```
Normal:  [INBOX] 12 unread | personal: synced 2m ago | work: synced 2m ago | reader
Syncing: [INBOX] 12 unread | personal: syncing (47/200)... | reader
Error:   [INBOX] 12 unread | ⚠ work: auth expired | reader
```

### Decision records

**D039**: Structured logging with tracing + event_log table. **D040**: `mxr logs` with filtering. **D041**: `mxr status` single-command overview. **D042**: `mxr events` for real-time stream. **D043**: `mxr doctor --check` for monitoring.

---

## A007: TUI batch operations — vim Visual mode + Gmail select patterns

**Affects**: 08-tui.md, A005 keybinding scheme

### Three selection modes

#### 1. Toggle select (Gmail `x`)
```
x               Toggle select on current message
```

#### 2. Visual line mode (vim `V`)
```
V               Enter visual line mode
j/k             Extend selection
G/gg            Extend to bottom/top
Escape          Cancel, clear selection
```

#### 3. Pattern select (`*` prefix)
```
*a              Select all in current view
*n              Select none (clear)
*r              Select all read
*u              Select all unread
*s              Select all starred
*t              Select all in current thread
```

### Vim count support

`5j` = move down 5. `V 10j` = select 11 messages. Digit presses accumulate before motion key.

### How actions apply to selections

If messages are selected, action keys apply to selection. If none selected, applies to cursor position. Same pattern as vim.

### Configurable batch confirmation

```toml
[behavior]
batch_confirm = "destructive"  # "always" | "destructive" | "never"
```

### Decision records

**D044**: Vim Visual Line mode. **D045**: Pattern select with `*` prefix. **D046**: Vim count support. **D047**: Configurable batch confirmation.

---

## A008: IMAP support promoted to first-party in v1

**Affects**: 03-providers.md, 04-sync.md, 14-roadmap.md, 15-decision-log.md (overrides D015)

### Why this changed

Shipping an open-source, local-first email client where the only sync backend is a proprietary Google API is a contradiction. Target audience disproportionately uses IMAP (Fastmail, Proton Bridge, self-hosted Dovecot, Migadu, etc.). IMAP also validates the provider-agnostic architecture against a genuinely different protocol.

### Implementation details

New crate: `crates/providers/imap/` — implements `MailSyncProvider` only. Send uses existing SMTP adapter.

**Connection management**: `async-imap` crate. Persistent connections with reconnection logic.

**Sync strategy** (layered):
1. **CONDSTORE/QRESYNC (RFC 7162)**: Delta sync via MODSEQ (Fastmail, Dovecot support this)
2. **UID-based polling**: Fallback. Track UIDVALIDITY + UIDNEXT per mailbox.
3. **IDLE (RFC 2177)**: Real-time push notifications.

**Threading**: JWZ algorithm from `In-Reply-To` + `References` headers. Lives in sync crate as shared module.

**Folder → label mapping**: INBOX/Sent/Drafts/Trash → system labels. Custom folders → Label { kind: Folder }. IMAP flags → MessageFlags. RFC 6154 SPECIAL-USE for folder detection.

**Mutations**: Archive = COPY to Archive + DELETE from source. Star = `\Flagged` flag. Read = `\Seen` flag. Move = COPY + DELETE (or MOVE if RFC 6851 supported).

**Labels vs folders**: Documented honestly. IMAP is folder-based. Applying multiple labels creates copies. Don't pretend IMAP has Gmail-style labels.

**Config**:
```toml
[accounts.fastmail]
name = "Fastmail"
email = "bk@fastmail.com"

[accounts.fastmail.sync]
provider = "imap"
host = "imap.fastmail.com"
port = 993
username = "bk@fastmail.com"
password_ref = "mxr/fastmail-imap"
use_tls = true

[accounts.fastmail.send]
provider = "smtp"
host = "smtp.fastmail.com"
port = 587
username = "bk@fastmail.com"
password_ref = "mxr/fastmail-smtp"
use_tls = true
```

**Roadmap placement**: Phase 2, after Gmail sync is proven. IMAP validates the provider-agnostic model.

### Decision records

**D048**: IMAP promoted to first-party (overrides D015). **D049**: CONDSTORE first, UID fallback, IDLE for push. **D050**: JWZ threading. **D051**: Honest labels vs folders documentation.

---

## End of addendum

Any future refinements should be appended below as A009, A010, etc. following the same format.
