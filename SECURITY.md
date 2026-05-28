# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.5.x   | Yes                |
| < 0.5   | No                 |

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
- Provider credentials live in the OS keychain where supported, with documented mode-`0600` local fallbacks for flows that need them. mxr does not operate a token relay service.
- The daemon IPC surface listens on a local Unix domain socket. The optional HTTP/WebSocket bridge binds to loopback by default, requires a bearer token for every route except `/api/v1/health` and the same-machine token handshake, and uses Host/CORS checks plus standard security headers.

## Local IPC Trust Boundary

The daemon IPC socket is a local user boundary. mxr creates the socket
with mode `0600`; any process that can connect as the same OS user can
drive the daemon with that user's authority. Do not place the socket in a
shared directory or relax its permissions.

## Shell Hooks

Rules may run explicit user-configured shell hooks through `sh -c`.
The message data is passed as JSON on stdin, but the hook command itself
is trusted local configuration and has the same authority as the user
running mxr. Only enable shell hooks you wrote or fully reviewed.

## Bundled OAuth Client Identifiers

Release binaries may include public desktop-app OAuth client identifiers
for Gmail or Outlook. These identifiers are not user secrets; PKCE and
the provider-issued user tokens protect the actual account access.
Users who prefer bring-your-own-client setups can override the bundled
identifiers in `config.toml`.
