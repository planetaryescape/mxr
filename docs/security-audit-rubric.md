# mxr Security Audit Rubric

Date: 2026-05-27

Scope: `mxr` before public distribution. Covers the Rust CLI/daemon, IPC, local web bridge, web app/site package surfaces, installers, release workflows, and dependency supply chain.

## Severity

| Severity | Meaning | Public release decision |
| --- | --- | --- |
| P0 Critical | Plausible remote code execution, credential theft, silent data exfiltration, destructive action without consent, or supply-chain compromise. | Block release. |
| P1 High | Public-facing or local-network exploitable flaw, broad local data exposure, unsafe installer behavior, or known vulnerable dependency in reachable shipped code. | Fix before broad distribution, or document why unreachable. |
| P2 Medium | Defense-in-depth gap, user-interaction/local-only issue, weak default, or incomplete assurance likely to concern auditors. | Fix soon; acceptable only with caveat. |
| P3 Low | Hardening, documentation, low-reachability scanner noise, or missing assurance artifact. | Track; not release-blocking alone. |

## Status

| Status | Meaning |
| --- | --- |
| Pass | Evidence shows the control is present. |
| Concern | Partial evidence, incomplete assurance, or acceptable behavior that needs hardening/docs. |
| Fail | Evidence shows the control is absent or unsafe. |
| Not assessed | Not enough evidence in this pass. |

## Rubric Categories

### 1. Distribution Trust And Malware-Scanner Risk

Goal: public install surfaces look transparent, predictable, and non-malicious.

Checks:
- Install scripts avoid hidden persistence, privilege escalation, arbitrary shell eval, and undocumented network calls.
- Release artifacts have clear versioning, checksums, and locked dependencies.
- Package metadata accurately describes the app; no obfuscation, hidden binaries, or misleading names.
- Daemon/background behavior is explicit and user-owned.
- Docs explain local-first data handling and expected network behavior.

### 2. Dependency And Supply-Chain Hygiene

Goal: shipped code avoids known vulnerable or suspicious dependencies.

Checks:
- Rust dependencies pass RustSec or equivalent advisory review.
- JS package surfaces pass `npm audit` or equivalent.
- Lockfiles are committed for reproducible builds.
- GitHub/dependency alerting is configured.
- Build scripts, forked crates, generated assets, and release tooling are reviewed for unexpected execution.

### 3. Secrets, Tokens, And Credential Handling

Goal: credentials are never committed, logged, synced, or exposed to local clients unnecessarily.

Checks:
- No hardcoded OAuth tokens, refresh tokens, passwords, API keys, private keys, or realistic secrets.
- OAuth/provider credentials use keychain or a documented protected local fallback.
- Logs, activity rows, bug reports, and diagnostics redact secrets and full message bodies.
- Config examples use placeholders.
- Secret-bearing CLI flags/env vars avoid accidental echoing/logging.

### 4. Local-First Privacy And Data Egress

Goal: private email data stays local unless the user explicitly invokes a provider or configured external service.

Checks:
- No telemetry, analytics beaconing, crash upload, or third-party tracking in default flows.
- Activity logs are local-only, retention-bound, and redaction-capable.
- LLM/semantic features are opt-in when external providers could receive content.
- Bug reports and exports require explicit user action and redact sensitive fields by default.
- Network calls are limited to configured mail providers, release/install checks, or user-requested services.

### 5. IPC And Daemon Boundary

Goal: local clients cannot abuse daemon authority beyond intended user actions.

Checks:
- Unix socket path is user-owned and restrictive.
- IPC request/response types are structured and validated.
- Destructive/batch mutations are explicit and dry-runnable.
- Daemon errors do not leak secrets through logs, responses, or panic traces.
- Event streams and long-running jobs avoid unbounded memory/disk growth.

### 6. Web Bridge And Browser Surface

Goal: web UI is safe by default and fails closed if users bind it publicly.

Checks:
- Default bind is loopback-only; remote binding is explicit and warned.
- CORS, WebSocket origin checks, and CSRF posture match exposed daemon authority.
- Useful security headers are set: CSP, frame policy, no-sniff, referrer policy.
- Untrusted HTML is sanitized or rendered inert.
- Dev tools/API docs are not exposed by default in risky modes.

### 7. Untrusted Email Parsing, Rendering, And Attachments

Goal: hostile email content cannot execute code, escape paths, or silently load remote content.

Checks:
- HTML email is converted to safe text by default; scripts/images are not executed.
- Attachment filenames are sanitized; downloads prevent traversal and overwrite surprises.
- MIME parsing failures are contained.
- Large, malformed, or deeply nested messages have bounded resource behavior where practical.
- Search/semantic indexing treats email as data, never code/shell input.

### 8. Shell, Editor, And External Process Execution

Goal: command execution is explicit, parameterized, and not injectable.

Checks:
- `Command::new` with explicit args is used instead of shell interpolation for untrusted data.
- `$EDITOR`, browser/opener, and hook execution are documented trust boundaries.
- User-controlled filenames, URLs, headers, and queries are not passed to a shell unsafely.
- Scripts/hooks are opt-in and previewable where destructive.
- Temp files use safe creation and reasonable permissions.

### 9. Storage, SQL, And Filesystem Safety

Goal: local state is durable, private, and resistant to injection/path issues.

Checks:
- SQL uses parameterized queries or compile-time checked macros.
- SQLite, search indexes, sockets, and temp files live under user-owned paths.
- Reset/burn commands preserve credentials/config by default and require clear confirmation.
- Migrations avoid `INSERT OR REPLACE` cascade traps.
- File writes avoid symlink/path traversal issues for user-supplied paths.

### 10. Provider, Network, And Protocol Safety

Goal: provider integrations use secure transport and minimal authority.

Checks:
- Gmail/Outlook/IMAP/SMTP use TLS-capable clients and provider-appropriate auth.
- OAuth scopes are minimal and documented.
- Provider-specific behavior stays in adapters.
- HTTP clients use sane timeouts/retry behavior.
- Provider errors do not log credentials or full message bodies.

### 11. Rust Safety, Denial Of Service, And Robustness

Goal: Rust safety properties are preserved and input-driven failure is contained.

Checks:
- `unsafe` is absent or tightly justified.
- `unwrap`, `expect`, panics, and unchecked indexing are not reachable from hostile input in daemon/provider/web paths.
- Parsers and background tasks handle malformed input with typed errors.
- Sync, search, indexing, and web requests have practical resource bounds.
- Tests cover cross-component security-sensitive flows.

### 12. CI/CD, Release, And Maintainer Operations

Goal: release machinery is difficult to tamper with and easy to audit.

Checks:
- Workflows use least-privilege permissions and trusted actions.
- Release tags/artifacts are immutable; existing releases are not overwritten.
- Dependency/security checks run before release.
- Secrets are scoped to release jobs and never printed.
- Publishing instructions and incident/security policy are clear.

## Evidence Expectations

Each finding should include:
- ID, severity, category, status, and one-sentence impact.
- File and line references.
- Reachability: default, opt-in, test/dev-only, or not shipped.
- Recommended fix and release-blocking decision.

## Automated Checks

Run where available:
- `cargo audit` or `cargo deny check advisories`
- `npm audit` for each committed JS package surface
- secret scan with patterns plus manual triage
- `rg` scans for `unsafe`, `Command`, shell use, network binds, CORS, security headers, secrets, `unwrap`, `expect`, and panic paths
- relevant build/test smoke checks when fixes are made
