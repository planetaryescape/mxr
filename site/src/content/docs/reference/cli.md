---
title: CLI
description: Exit codes and error output for the mxr command.
---

## Exit codes

| Code | Meaning | Error kinds |
| --- | --- | --- |
| `0` | Success | |
| `1` | General error | |
| `2` | Usage error: bad flags or arguments | |
| `3` | Daemon not running or unusable | `daemon_unavailable`, `daemon_incompatible` |
| `4` | Auth error | `daemon_auth`, `auth` |

Exit `3` means no request reached a usable daemon. The daemon is not running, could not be reached after an autostart attempt, or speaks a protocol this build cannot use. With `MXR_DAEMON_AUTOSTART=off`, the daemon is never started for you, so a missing daemon always exits `3`.

Exit `4` means the daemon rejected the token the client presented (`daemon_auth`), or answered a request with an auth error (`auth`).

Other failures exit `1`.

## Error output for scripts

When a command prints JSON or JSONL, either because you passed `--format json` or `--format jsonl`, or because stdout is not a terminal, a failure with exit code `3` or `4` also writes one line to stderr:

```json
{"error":{"kind":"daemon_unavailable","message":"MXR_DAEMON_AUTOSTART=off and no compatible daemon is available; ...","exit_code":3}}
```

Stdout only ever carries results. The human-readable error on stderr is the same text as without JSON output.
