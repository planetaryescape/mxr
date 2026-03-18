# mxr — CLI & Shell Integration

## CLI design

The CLI and TUI share the same daemon. CLI commands connect to the daemon via Unix socket, send a command, receive a response, and output to stdout. This means scripts and cron jobs get the same data and capabilities as the TUI.

## Subcommands

```
mxr                         Open TUI (start daemon if needed)
mxr daemon                  Start daemon explicitly
mxr daemon --foreground     Start daemon in foreground (for systemd/launchd/debugging)
mxr sync                    Trigger sync for all accounts
mxr sync --account work     Trigger sync for specific account
mxr search "query"          Search and output results to stdout
mxr search --saved "name"   Run a saved search
mxr search --save "name" "query"  Create a saved search
mxr compose                 Open $EDITOR for new message
mxr reply MESSAGE_ID        Open $EDITOR for reply
mxr forward MESSAGE_ID      Open $EDITOR for forward
mxr drafts                  List drafts
mxr send DRAFT_ID           Send a draft
mxr export THREAD_ID        Export thread (default: markdown)
mxr export THREAD_ID --format json|markdown|mbox|llm
mxr accounts                List configured accounts
mxr accounts add gmail      Interactive Gmail account setup
mxr accounts add smtp       Interactive SMTP account setup
mxr labels                  List all labels with counts
mxr doctor                  Run diagnostics
mxr doctor --reindex        Rebuild Tantivy index from SQLite
mxr config                  Show resolved configuration
mxr version                 Version info
```

## Output format

CLI commands that output data support multiple formats:

```
mxr search "invoice" --format table    # Default: human-readable table
mxr search "invoice" --format json     # Machine-readable JSON
mxr search "invoice" --format csv      # For piping to other tools
```

This makes mxr composable with Unix tools:

```bash
# Find all unread invoices and extract sender emails
mxr search "subject:invoice is:unread" --format json | jq -r '.[].from_email'

# Count messages per sender
mxr search "label:inbox" --format json | jq -r '.[].from_email' | sort | uniq -c | sort -rn

# Export a thread and pipe to an LLM
mxr export abc123 --format llm | llm "Summarize this email thread"

# Batch archive old newsletters
mxr search "label:newsletters before:2025-01-01" --format json | \
  jq -r '.[].id' | \
  xargs -I {} mxr archive {}
```

## Shell integration

### Shell completions

Generate shell completions for bash, zsh, fish:

```
mxr completions bash > /etc/bash_completion.d/mxr
mxr completions zsh > ~/.zfunc/_mxr
mxr completions fish > ~/.config/fish/completions/mxr.fish
```

Use `clap`'s built-in completion generation.

### Exit codes

Standard Unix exit codes:
- 0: success
- 1: general error
- 2: usage error (bad arguments)
- 3: daemon not running
- 4: auth error (need to re-authenticate)

Scripts can check these for conditional logic.
