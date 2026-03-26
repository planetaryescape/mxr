# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.4.x   | Yes                |
| < 0.4   | No                 |

## Reporting a Vulnerability

If you discover a security vulnerability in mxr, please report it responsibly.

**Do not open a public GitHub issue for security vulnerabilities.**

Instead, use one of these channels:

1. **GitHub Security Advisory** (preferred): [Report a vulnerability](https://github.com/planetaryescape/mxr/security/advisories/new)
2. **Email**: security@planetaryescape.com

### What to include

- Description of the vulnerability
- Steps to reproduce
- Impact assessment
- Suggested fix (if any)

### Response timeline

- **Acknowledgment**: Within 72 hours
- **Initial assessment**: Within 1 week
- **Fix or mitigation**: Depends on severity, but we aim for:
  - Critical: 48 hours
  - High: 1 week
  - Medium/Low: Next release cycle

## Scope

The following components are in scope:

- **Daemon**: IPC socket server, request handling, sync loops
- **OAuth tokens**: Storage, refresh, and transmission
- **SQLite database**: Local mail storage, query handling
- **IPC protocol**: JSON message parsing over Unix domain socket
- **Config files**: TOML parsing, credential references
- **Search index**: Tantivy query parsing

## Design Principles

- All mail data stays local after sync. No telemetry, no phone-home.
- OAuth tokens are stored on the local filesystem. mxr does not operate a token relay service.
- The daemon listens only on a Unix domain socket, not on network interfaces.
