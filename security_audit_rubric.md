# mxr Security Audit Rubric

Date: 2026-05-27

Scope: `mxr` repository before public distribution. Covers the Rust binary, daemon, IPC, local web bridge, packaged website/frontends, install scripts, release workflows, and third-party dependency surface.

## Severity Scale

| Severity | Meaning | Required response before public distribution |
| --- | --- | --- |
| P0 Critical | Plausible remote code execution, credential theft, silent data exfiltration, destructive action without consent, or supply-chain compromise. | Block release until fixed. |
| P1 High | Public-facing or local-network exploitable flaw, broad local privilege/data exposure, unsafe installer behavior, known vulnerable dependency in reachable shipped code. | Fix before broad distribution, or document compensating control if not reachable. |
| P2 Medium | Defense-in-depth gap, local-only issue requiring user interaction, weak defaults that could become unsafe when publicly exposed. | Fix soon; acceptable for limited release only with clear caveat. |
| P3 Low | Hardening/documentation issue, scanner noise, low-reachability dependency warning, or missing assurance artifact. | Track; does not block release by itself. |

## Assessment Status

| Status | Meaning |
| --- | --- |
| Pass | Evidence shows the control is present. |
| Concern | Some evidence is good, but there is a gap or incomplete assurance. |
| Fail | Evidence shows the control is absent or unsafe. |
| Not assessed | Not enough local evidence in this audit pass. |

## Audit Categories

### 1. Distribution Trust And Malware-Scanner Risk

Goal: public package/install surfaces look transparent, predictable, and non-malicious.

Checks:
- Installer scripts avoid hidden persistence, privilege escalation, arbitrary shell eval, silent network calls beyond documented artifact downloads.
- Release artifacts are reproducible enough to inspect: checksums, clear versioning, locked dependencies, release workflow provenance.
- Package metadata accurately describes the app; no obfuscation, misleading names, hidden binaries, or unexpected background services.
- Auto-start behavior is explicit and user-owned; daemon/socket files are local runtime state.
- Public docs explain local-first data handling and network behavior.

### 2. Dependency And Supply-Chain Hygiene

Goal: shipped code does not include known vulnerable or suspicious dependencies.

Checks:
- Rust dependencies pass RustSec/cargo-audit or equivalent advisory review.
- JS dependencies pass `npm audit` or equivalent advisory review for shipped frontend/site code.
- Lockfiles are committed for distributed applications where reproducibility matters.
- GitHub Dependabot or equivalent keeps dependency alerts visible.
- Build scripts, vendored/forked crates, and generated artifacts are reviewed for unexpected execution.

### 3. Secrets, Tokens, And Credential Handling

Goal: credentials are never committed, logged, synced, or exposed to local clients unnecessarily.

Checks:
- No hardcoded OAuth tokens, refresh tokens, passwords, API keys, private keys, or realistic secrets.
- OAuth and provider credentials are stored in system keychain or an explicitly protected local store.
- Logs, activity rows, bug reports, and diagnostics redact secrets and message bodies where appropriate.
- Config examples use placeholders, not live-looking credentials.
- Environment variables and CLI flags that accept secrets avoid accidental echoing/logging.

### 4. Local-First Privacy And Data Egress

Goal: private email data stays local unless the user explicitly invokes a provider or configured external model.

Checks:
- No telemetry, analytics beaconing, crash upload, or third-party tracking in default flows.
- Activity logs remain local-only, retention-bound, and redaction-capable.
- LLM/semantic features are opt-in where external providers could receive content.
- Bug reports and exports require explicit user action and redact sensitive fields by default.
- Network calls are limited to configured mail providers, release/install checks, or user-requested services.

### 5. IPC And Daemon Boundary

Goal: local clients cannot abuse the daemon beyond intended user authority.

Checks:
- Unix socket path is in a user-owned runtime directory and has restrictive permissions.
- IPC requests are typed, validated, and do not expose client-specific screen payloads as privileged daemon operations.
- Mutations require explicit commands and dry-run/preview for destructive or batch operations.
- Daemon failures do not leak secrets through errors or panic traces.
- Long-running jobs and event streams cannot trivially cause unbounded memory/disk growth.

### 6. Web Bridge And Browser Surface

Goal: web UI is safe when exposed locally and fails closed if users bind it publicly.

Checks:
- Default bind is loopback-only; remote binding is explicit and warned.
- CORS, WebSocket origin checks, and CSRF posture match the daemon authority exposed.
- Security headers are present where meaningful: CSP, frame protection, content type sniffing, referrer policy.
- No inline untrusted HTML rendering without sanitization.
- OpenAPI/Swagger/dev tools are not exposed by default in risky modes.

### 7. Untrusted Email Parsing, Rendering, And Attachments

Goal: hostile email content cannot execute code, escape paths, or silently load remote content.

Checks:
- HTML email is rendered to safe plain text by default; images/scripts are not executed.
- Attachment filenames are sanitized; downloads prevent path traversal and accidental overwrite surprises.
- MIME parsing failures are contained and do not panic the daemon.
- Large, malformed, or deeply nested messages have bounded memory/CPU behavior where practical.
- Search indexing and semantic chunking do not treat untrusted content as code or shell input.

### 8. Shell, Editor, And External Process Execution

Goal: command execution is explicit, parameterized, and not injectable.

Checks:
- Uses `Command::new` with explicit args rather than shell string interpolation for untrusted input.
- `$EDITOR`, opener, browser, and hook execution are documented trust boundaries.
- User-controlled filenames, URLs, email headers, and search queries are not passed to shells unsafely.
- Scripts/hooks are opt-in and previewable where destructive.
- Temporary files use safe creation and reasonable permissions.

### 9. Storage, SQL, And Filesystem Safety

Goal: local state is durable, private, and resistant to injection/path issues.

Checks:
- SQL uses parameterized queries or compile-time checked macros; no string-built SQL from user input.
- SQLite files, search indexes, sockets, and temp files live under user-owned paths.
- Reset/burn commands preserve credentials/config by default and require clear destructive confirmation.
- Migrations preserve data integrity and avoid unsafe `INSERT OR REPLACE` cascade traps.
- File writes avoid symlink/path traversal issues for user-supplied paths.

### 10. Provider, Network, And Protocol Safety

Goal: provider integrations use secure transport and minimal authority.

Checks:
- Gmail/Outlook/IMAP/SMTP use TLS-capable clients and provider-appropriate auth.
- OAuth scopes are minimal and documented.
- Provider-specific quirks stay in adapters and do not leak into core policy decisions.
- HTTP clients set sane timeouts/retry behavior for external calls.
- Errors from providers do not log credentials or full message bodies.

### 11. Rust Safety, Denial Of Service, And Robustness

Goal: Rust safety properties are preserved and input-driven failure is contained.

Checks:
- `unsafe` blocks are absent or justified with tight invariants.
- `unwrap`, `expect`, panics, and unchecked indexing are not reachable from hostile input in daemon/provider/web paths.
- Parsers and background tasks handle malformed input with typed errors.
- Resource usage is bounded for sync, search, indexing, and web requests.
- Tests cover cross-component security-sensitive flows, not only units with fakes.

### 12. CI/CD, Release, And Maintainer Operations

Goal: public release machinery is difficult to tamper with and easy to audit.

Checks:
- Workflows use least-privilege permissions and pinned/trusted actions where practical.
- Release tags are immutable; existing artifacts are not overwritten.
- Dependabot/security alerts are enabled for Rust and JS ecosystems.
- CI runs build/test plus dependency/security checks before release.
- Secrets are scoped to release jobs and never printed.

## Evidence Expectations

For each finding, the report should include:
- ID, severity, category, status, and one-sentence impact.
- File and line references for code evidence.
- Reachability notes: default path, opt-in path, test/dev-only path, or not shipped.
- Recommended fix and distribution-blocking decision.

## Automated Checks To Run

- `cargo audit` or equivalent RustSec advisory scan.
- `npm audit` for each committed JS lockfile/package surface.
- Secret scan with a pattern scanner and manual review of suspicious hits.
- `rg` scans for `unsafe`, `Command`, `shell`, `unwrap`, `expect`, `password`, `token`, `secret`, network binds, CORS, and security headers.
- Build/test smoke checks relevant to touched/audited code when fixes are made.
