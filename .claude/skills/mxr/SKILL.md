---
name: mxr
description: "Use the mxr terminal email client CLI to read, search, compose, mutate, and manage email. Invoke when the user asks to check email, search messages, compose/reply/forward emails, archive/trash/star/label messages, manage accounts, check sync status, or perform any email operations via mxr. Triggers: 'check email', 'search email', 'compose email', 'reply to', 'forward to', 'archive', 'trash', 'star', 'label', 'snooze', 'unsubscribe', 'mxr', 'email', 'inbox', 'unread messages', 'send email', 'drafts', 'sync email', 'saved search'."
---

# mxr CLI

Terminal email client. Daemon-backed, local-first. All commands go through `mxr <subcommand>`.

## Quick Reference

```bash
# Read
mxr search "is:unread"                    # Find unread messages
mxr search "from:alice subject:meeting"   # Search with field prefixes
mxr cat <id>                              # Read message body
mxr thread <id>                           # Read full thread
mxr labels                                # List labels with counts

# Compose
mxr compose --to a@x.com --subject "Hi" --body "Hello"
mxr reply <id> --body "Thanks!"
mxr forward <id> --to b@x.com

# Mutate (single or batch via --search)
mxr archive <id>
mxr star --search "subject:urgent" --yes
mxr label "todo" --search "from:boss" --yes
mxr read --search "is:unread" --yes       # Mark all unread as read

# Snooze
mxr snooze <id> --until tomorrow
mxr snoozed                               # List snoozed

# Status
mxr status                                # Daemon status
mxr sync --status                         # Sync status
mxr count "is:unread"                     # Count unread
```

## Important Patterns

1. **Message IDs are UUIDs** -- get them from `mxr search --format ids` or `mxr search --format json`
2. **Batch mutations** -- use `--search <query>` instead of `<id>` for bulk operations. Always add `--yes` to skip confirmation.
3. **`--dry-run`** -- available on all mutations and compose commands. Use to preview before executing.
4. **`--format json`** -- use for machine-readable output on search, cat, thread, status, saved commands.
5. **`--format ids`** -- use to get message IDs for piping into other commands.
6. **Daemon auto-starts** -- no need to manually start. Commands that need it will launch it.

## Typical Workflows

### Check inbox
```bash
mxr search "is:unread" --format json
```

### Read and reply to a message
```bash
mxr search "from:alice is:unread" --format json --limit 5
mxr cat <message_id>
mxr reply <message_id> --body "Got it, thanks!"
```

### Bulk cleanup
```bash
mxr archive --search "older:30d label:notifications" --yes --dry-run  # Preview
mxr archive --search "older:30d label:notifications" --yes             # Execute
```

### Triage with labels
```bash
mxr label "review" <id>
mxr star <id>
mxr snooze <id> --until monday
```

## Full Command Reference

See [references/commands.md](references/commands.md) for complete documentation of every command, flag, and option.
