# mxr — Bug Reporting Addendum

> This document covers log storage, the `mxr bug-report` command, and the workflow for users to capture diagnostics and open issues. Builds on top of the logging infrastructure defined in A006 (16-addendum.md).

---

## Log storage recap (from A006)

Logs go to two places:

1. **Text log file**: `$XDG_DATA_HOME/mxr/logs/mxr.log` (default: `~/.local/share/mxr/logs/mxr.log`). Rotated at 50 MB, 5 files kept. Structured tracing output. Configurable level (default: `info`).

2. **event_log SQLite table**: Structured, queryable events (sync completions, errors, rule executions, snooze events). Retained for 90 days by default.

Both are on disk, both survive daemon restarts. The text log captures everything (including debug/trace if configured). The event_log captures significant events at info level and above in a structured, queryable format.

---

## The problem

When a user hits a bug, you need:
- mxr version and build info
- OS and architecture
- Rust version (if built from source)
- Config (sanitized, no credentials)
- Account setup (provider types, not emails)
- Recent logs around the time of the issue
- Daemon status
- Sync history
- Index and store health
- Terminal info (for TUI rendering bugs)

Asking users to manually gather all this is friction that kills bug reports. One command should bundle everything.

---

## `mxr bug-report`

A single command that generates a sanitized diagnostic bundle ready to paste into a GitHub issue or attach as a file.

```bash
mxr bug-report
```

Output:

```
Generating bug report...

⚠ The report has been automatically sanitized:
  - Email addresses replaced with [REDACTED_EMAIL]
  - OAuth tokens and passwords removed
  - API keys removed
  - Message subjects and body content removed
  - File paths anonymized to relative paths

Please review before sharing: /tmp/mxr-bug-report-2026-03-18-a1b2c3.md

Options:
  - Open in $EDITOR to review:  mxr bug-report --edit
  - Copy to clipboard:          mxr bug-report --clipboard
  - Open GitHub issue:          mxr bug-report --github
  - Print to stdout:            mxr bug-report --stdout
```

### What the report contains

```markdown
# mxr Bug Report

## System
- mxr version: 0.1.0 (built 2026-03-15, commit a1b2c3d)
- OS: Linux 6.8.0-45-generic (Ubuntu 24.04)
- Architecture: x86_64
- Terminal: foot 1.18.0 (TERM=foot-extra)
- Shell: zsh 5.9
- $EDITOR: nvim 0.10.2
- Rust: 1.82.0 (if built from source, otherwise "pre-built binary")

## Configuration
- Accounts: 2 configured
  - Account "personal": sync=gmail, send=gmail
  - Account "work": sync=gmail, send=smtp
- Sync interval: 60s
- Reader mode: enabled
- AI features: disabled
- Search engine: tantivy (24,871 docs, 45 MB)
- Store: 52 MB (SQLite WAL mode)

## Daemon Status
- Status: running
- Uptime: 3d 4h 12m
- PID: 12345
- Connected clients: 1

## Account Health
- personal: last sync 2m ago, OK, 12 unread
- work: last sync 2m ago, OK, 3 unread
- Snoozed: 5 messages

## Recent Sync History (last 10)
| Time | Account | Status | Messages | Duration |
|---|---|---|---|---|
| 2026-03-18 09:45 | personal | success | 3 | 1.2s |
| 2026-03-18 09:45 | work | success | 0 | 0.8s |
| 2026-03-18 09:44 | personal | success | 0 | 0.6s |
| ... | ... | ... | ... | ... |

## Recent Errors (last 20)
| Time | Category | Message |
|---|---|---|
| 2026-03-17 14:22 | sync | Rate limited by Gmail API, retrying in 60s |
| 2026-03-16 03:15 | auth | Token refresh failed, re-authenticated |

## Recent Logs (last 100 lines)
```
2026-03-18T09:45:12.345Z INFO  mxr_sync: Sync started account=personal
2026-03-18T09:45:12.890Z INFO  mxr_sync: Delta sync: 3 new, 0 deleted, 1 label change
2026-03-18T09:45:13.012Z INFO  mxr_search: Indexed 3 documents
2026-03-18T09:45:13.045Z INFO  mxr_sync: Sync completed account=personal duration=1.2s
...
```

## User Description
[Please describe the bug here]

## Steps to Reproduce
[Please describe how to reproduce]

## Expected Behavior
[What did you expect to happen?]

## Actual Behavior
[What actually happened?]
```

### Sanitization rules

This is critical. Users should feel safe sharing the report without manually redacting. The sanitizer runs automatically:

| Data | Action |
|---|---|
| Email addresses | Replaced with `[REDACTED_EMAIL]` |
| OAuth tokens | Completely removed |
| Passwords / password_ref values | Completely removed |
| API keys | Completely removed |
| Email subjects in logs | Replaced with `[REDACTED_SUBJECT]` |
| Email body content in logs | Replaced with `[REDACTED_BODY]` |
| Message IDs | Kept (they're opaque UUIDs, not sensitive) |
| File paths | Anonymized to relative paths (`~/...`) |
| Account names | Kept (user-chosen display names, not sensitive) |
| Provider types | Kept (gmail, imap, smtp — not sensitive) |
| Server hostnames | Kept (e.g., `imap.fastmail.com` — needed for debugging) |
| IP addresses | Replaced with `[REDACTED_IP]` |
| Label names | Kept (useful for debugging label sync issues) |

The sanitizer errs on the side of over-redacting. Better to remove something useful than to leak something private. Users can always add back context manually in the issue description.

```rust
pub struct BugReportSanitizer;

impl BugReportSanitizer {
    pub fn sanitize(text: &str) -> String {
        let text = Self::redact_emails(text);
        let text = Self::redact_tokens(text);
        let text = Self::redact_passwords(text);
        let text = Self::redact_api_keys(text);
        let text = Self::redact_subjects(text);
        let text = Self::redact_bodies(text);
        let text = Self::redact_ips(text);
        let text = Self::anonymize_paths(text);
        text
    }
}
```

### CLI flags

```bash
mxr bug-report                    # Generate report, save to temp file, show path
mxr bug-report --edit             # Generate and open in $EDITOR for review before sharing
mxr bug-report --stdout           # Print to stdout (for piping)
mxr bug-report --clipboard        # Copy to clipboard (via xclip/pbcopy)
mxr bug-report --github           # Open browser to new GitHub issue with report pre-filled
mxr bug-report --output ~/report.md  # Save to specific path
mxr bug-report --verbose          # Include debug-level logs (last 500 lines instead of 100)
mxr bug-report --full-logs        # Include ALL logs from today (can be large)
mxr bug-report --no-sanitize      # Skip sanitization (user's choice, warned about risks)
mxr bug-report --since "2h"       # Only include logs from the last 2 hours
```

### `--github` flag

Opens the browser to a pre-filled GitHub issue URL:

```
https://github.com/USER/mxr/issues/new?template=bug_report.md&body=URLENCODED_REPORT
```

There's a URL length limit (~8000 chars for most browsers). If the report exceeds this, fall back to:

1. Save report to temp file
2. Open the new issue page without pre-fill
3. Print: "Report saved to /tmp/mxr-bug-report-xxx.md — please paste it into the issue"

### GitHub issue template

The repo should have an issue template that matches the bug report format:

```yaml
# .github/ISSUE_TEMPLATE/bug_report.yml
name: Bug Report
description: Report a bug in mxr
labels: [bug]
body:
  - type: textarea
    id: report
    attributes:
      label: Bug Report
      description: |
        Paste the output of `mxr bug-report --stdout` below,
        or describe the issue manually.
      placeholder: |
        Run `mxr bug-report --stdout` and paste the output here,
        or describe the bug manually.
    validations:
      required: true

  - type: textarea
    id: description
    attributes:
      label: Description
      description: What happened? What did you expect?
    validations:
      required: true

  - type: textarea
    id: reproduce
    attributes:
      label: Steps to Reproduce
      description: How can we reproduce this?
    validations:
      required: true

  - type: textarea
    id: additional
    attributes:
      label: Additional Context
      description: Screenshots, additional logs, related issues, etc.
    validations:
      required: false
```

---

## Log retention and disk usage

Users should understand how much disk space logs consume and how to manage them.

### Defaults

| Storage | Default retention | Default max size |
|---|---|---|
| Text log file | 5 rotated files | 50 MB each (250 MB max) |
| event_log table | 90 days | No size limit (grows with usage, pruned by age) |
| sync_log table | 90 days | No size limit (pruned by age) |
| ai_usage table | 90 days | No size limit (pruned by age) |

### Pruning

The daemon runs a daily cleanup job (or on startup) that prunes old rows:

```sql
DELETE FROM event_log WHERE timestamp < strftime('%s', 'now', '-90 days');
DELETE FROM sync_log WHERE started_at < strftime('%s', 'now', '-90 days');
DELETE FROM ai_usage WHERE timestamp < strftime('%s', 'now', '-90 days');
```

### User control

```toml
# ~/.config/mxr/config.toml
[logging]
level = "info"
max_size_mb = 50
max_files = 5
event_retention_days = 90
```

```bash
# Check disk usage
mxr doctor --store-stats
# Output includes: log file sizes, event_log row count, total log disk usage

# Manually purge old logs
mxr logs --purge                  # Delete logs older than retention period
mxr logs --purge --before "2026-01-01"  # Delete logs before specific date
mxr logs --purge --all            # Delete all logs (with confirmation)
```

---

## Decision records

**D072: `mxr bug-report` as a single diagnostic capture command**

**Chosen**: One command that generates a sanitized, comprehensive diagnostic bundle: system info, config (redacted), account health, sync history, recent errors, recent logs, and placeholder sections for user description.

**Why**: Bug reports with insufficient information waste everyone's time. Most users won't manually gather version info, check sync logs, and inspect daemon state. One command captures everything needed for diagnosis. Auto-sanitization means users can share without fear of leaking credentials or email content. The `--github` flag reduces friction to the absolute minimum: run command, paste, submit.

**D073: Automatic log sanitization with opt-out**

**Chosen**: All bug reports are auto-sanitized by default. Email addresses, tokens, passwords, API keys, subjects, and body content are redacted. Users can override with `--no-sanitize` if they choose.

**Why**: Trust is critical. If users fear that sharing a bug report might expose their email content or credentials, they won't report bugs. Auto-sanitization makes the safe choice the default. The sanitizer over-redacts intentionally. Users can always add back context they're comfortable sharing in the issue description.

**D074: Log retention defaults — 90 days, 250 MB max for text logs**

**Chosen**: Text logs rotate at 50 MB with 5 files kept (250 MB max). Structured event tables pruned at 90 days.

**Why**: Logs need to survive long enough to capture intermittent issues (an auth bug that happens once a week, a sync failure that occurs on the first of each month). 90 days covers most patterns. 250 MB is a reasonable disk budget for a power-user tool. Both are configurable for users with different constraints (e.g., low-disk-space environments can reduce retention).
