---
title: Bug reports
description: Generate a sanitized diagnostic bundle for support and issue filing.
---

## Command

```bash
mxr bug-report
```

By default this writes a Markdown report to a temp file and tells you where it was saved.

## Useful flags

```bash
mxr bug-report --stdout
mxr bug-report --edit
mxr bug-report --clipboard
mxr bug-report --github
mxr bug-report --output ~/report.md
mxr bug-report --verbose
mxr bug-report --full-logs
mxr bug-report --since 2h
mxr bug-report --no-sanitize
```

`--full-logs` includes every log line from today instead of the last 100 (or 500 with `--verbose`). The report is still sanitized.

`--no-sanitize` writes the report without redaction. mxr prints a warning to stderr before it generates anything, and says `(not sanitized)` on the saved-file line. stdout stays clean, so `--stdout` piping still works, and the command never prompts. Read the report before you share it: it can contain email addresses, names, message subjects and body text, and tokens.

## Included data

- System details
- Resolved config summary
- Daemon status
- Account health
- Recent sync events
- Recent errors
- Recent text logs

## Sanitization

Unless `--no-sanitize` is set, mxr redacts:

- Email addresses
- Token and password references
- API keys and authorization material
- Message subjects and body fields
- IP addresses
- Home-directory prefixes in file paths
