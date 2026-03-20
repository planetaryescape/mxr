---
title: Bug Reports
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

## Included data

- system details
- resolved config summary
- daemon status
- account health
- recent sync events
- recent errors
- recent text logs

## Sanitization

Unless `--no-sanitize` is set, mxr redacts:

- email addresses
- token and password references
- API keys and authorization material
- message subjects and body fields
- IP addresses
- home-directory prefixes in file paths
