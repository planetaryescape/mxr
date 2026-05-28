# mxr Security Audit Findings

Date: 2026-05-27
Rubric: [`security_audit_rubric.md`](./security_audit_rubric.md)
Scope: HEAD on `main`, full Rust workspace + `site/` (Astro docs) + `apps/web/` (Vite SPA) + `install.sh` + `.github/workflows/*` + packaging.
Method: Manual review + parallel category agents + `gitleaks` (git history), `cargo audit`, `cargo deny`, `npm audit` (site + apps/web).

> Resolution note, 2026-05-28: the original audit below is kept as the
> historical finding record. P1, P2, and P3 findings have since been
> remediated or explicitly tracked with code/docs/CI evidence listed in
> "Resolution Status".

## Headline

**No P0 findings. mxr is safe to publish.** The code is structurally sound: local-first by design, no telemetry, parameterised SQL throughout, OS keychain for credentials, TLS-mandatory provider transport, loopback-only web bridge with bearer auth + Host/CORS gating, attachment filenames sanitised, HTML emails rendered through `html2text` (no JS surface), reset/burn flows preserve credentials by default.

The findings below are real but none are exploitable-without-prior-local-access, and none should flag mxr as malicious during a third-party scan. Distribution-trust hardening (install.sh checksum, release artifact attestation) is the main thing to tighten before broad publication — these are what supply-chain reviewers will look at.

## Severity Summary

| Severity | Count | Categories with findings |
| --- | --- | --- |
| P0 Critical | 0 | — |
| P1 High | 3 | Distribution (install.sh), CI/CD (release), Dependencies (site JS) |
| P2 Medium | 9 | IPC, Web bridge, Storage, Shell hook, Email parsing, CI/CD, JS deps |
| P3 Low | 5 | Filename portability, file perms, Swagger exposure, Rust deps, release-please fallback |

## Resolution Status

As of 2026-05-28, all P1/P2/P3 findings are resolved or explicitly tracked:

| ID | Status | Evidence |
| --- | --- | --- |
| P1-1 | Fixed | `install.sh` downloads and verifies the `.sha256` file before extraction. |
| P1-2 | Fixed | `.github/workflows/release.yml` verifies downloaded release artifacts with `sha256sum -c ./*.sha256` before upload. |
| P1-3 | Fixed | `site/package-lock.json` updated; CI runs `npm audit --audit-level=high` for the docs site. |
| P2-1 | Fixed | `crates/daemon/src/server.rs` sets Unix socket permissions to `0600` after bind. |
| P2-2 | Documented | `SECURITY.md` documents the local IPC trust boundary and same-OS-user authority model. |
| P2-3 | Fixed | `crates/web/src/middleware.rs` injects frame, no-sniff, and referrer-policy headers. |
| P2-4 | Fixed | `safe_attachment_destination` constrains user-supplied attachment destinations to allowed roots and rejects symlinks / parent traversal. |
| P2-5 | Fixed | Remote HTML assets use a 25 MB cap from both `Content-Length` and chunked reads. |
| P2-6 | Fixed/documented | `SECURITY.md` documents shell-hook trust; rule upsert emits a one-time warning for enabled shell hooks. |
| P2-7 | Documented | `SECURITY.md` explains bundled OAuth identifiers are public app identifiers, not user secrets, and that BYOC overrides exist. |
| P2-8 | Fixed | Dependabot auto-merge now fetches metadata and refuses major-version auto-merge. |
| P2-9 | Fixed | `apps/web/package-lock.json` updated; CI runs `npm audit --audit-level=moderate`. |
| P3-1 | Fixed | Attachment filename sanitizer falls back for Windows reserved device names; focused test added. |
| P3-2 | Fixed | Saved attachments, exports, and HTML assets get Unix `0600` permissions; focused download test added. |
| P3-3 | Fixed | Swagger UI and OpenAPI JSON are now bridge-auth gated; web tests assert unauthenticated requests fail. |
| P3-4 | Tracked | `deny.toml` contains explicit advisory ignores with reachability reasons for the current RustSec advisories. |
| P3-5 | Fixed | `release-please.yml` fails loudly when `RELEASE_PLEASE_TOKEN` is absent and no longer falls back to `GITHUB_TOKEN`. |

## P1 Findings (Fix Before Broad Distribution)

### P1-1 — `install.sh` does not verify download integrity
- Category: 1 (Distribution Trust)
- File: `install.sh:45`
- Evidence: `curl -fsSL "$url" -o "$tmp_dir/$archive"` followed directly by `tar -xzf … && install …`. No `sha256sum -c`, no `gpg --verify`, no cosign/sigstore. The release pipeline already generates `.sha256` artifacts; the installer just doesn't consume them.
- Impact: Anyone who can MITM the download (compromised CDN edge, hostile WiFi without TLS pinning, future GitHub asset compromise) can substitute a malicious binary. This is the single most likely thing a third-party security reviewer will flag.
- Fix: Download `<archive>.sha256` next to the archive and verify before extracting. Bonus: publish SLSA provenance via `actions/attest-build-provenance@v1` and have `install.sh` verify with `gh attestation verify` when available, falling back to checksum.

### P1-2 — Release workflow uploads artifacts without inter-job verification
- Category: 12 (CI/CD)
- File: `.github/workflows/release.yml:264-300` (github-release job)
- Evidence: `github-release` job pulls artifacts from build matrix jobs and uploads via `softprops/action-gh-release` without re-computing or comparing the `.sha256` files generated upstream.
- Impact: A compromised build runner could swap the binary between the build step and the upload step; nothing in the workflow detects it.
- Fix: After `actions/download-artifact`, run `sha256sum -c *.sha256` over the downloaded set and fail the job on mismatch. Pair with `actions/attest-build-provenance` to publish SLSA L3 attestations.

### P1-3 — Four unpatched high-severity JS vulnerabilities in `site/`
- Category: 2 (Dependency Hygiene)
- File: `site/package-lock.json`
- Evidence (`npm audit` in `site/`):
  - `defu ≤ 6.1.4` — GHSA-737v-mqg7-c878 prototype pollution, CVSS 7.5
  - `devalue 5.6.3–5.8.0` — GHSA-77vg-94rm-hx3p sparse-array DoS, CVSS 7.5
  - `picomatch ≤ 2.3.1` — glob-bypass via POSIX char classes
  - `vite ≤ 6.4.1` — path traversal in `.map` handling
- Impact: The docs site is statically built and served via Vercel; impact is constrained to build-time and dev-server. Not user-data-exposing for end users of the desktop binary, but reviewers ranking the project on `npm audit` will see these.
- Fix: Bump `@astrojs/starlight` (drags in the defu/devalue chain), then `npm install` in `site/` to take the patched picomatch + vite. Add `npm audit --audit-level=high` to CI for `site/`.

## P2 Findings (Fix Soon)

### P2-1 — Unix socket created without explicit `chmod 0600`
- Category: 5 (IPC)
- File: `crates/daemon/src/server.rs:101` (`UnixListener::bind(&sock_path)`)
- Evidence: Socket inherits the process umask. On macOS the parent (`~/Library/Application Support/mxr/`) is 0700 by default. On Linux `$XDG_RUNTIME_DIR` is 0700 per spec. So in practice it's fine — but the daemon relies on parent-dir permissions, not on socket permissions, which is fragile (a misconfigured XDG var or a different platform breaks the assumption).
- Fix: After `bind`, call `std::fs::set_permissions(&sock_path, fs::Permissions::from_mode(0o600))` on `cfg(unix)`. Defence-in-depth.

### P2-2 — No IPC-layer authentication; sole gate is filesystem permissions
- Category: 5 (IPC)
- File: `crates/daemon/src/ipc_client.rs:22-33`
- Evidence: Anyone who can `connect(2)` the socket can issue any IPC request. This is by design and matches Gmail-CLI/IMAP-CLI conventions, but on shared/multi-user hosts a sibling-uid attacker who can read the socket can drive the daemon.
- Fix: Optional — gate IPC behind `SO_PEERCRED` (Linux) / `LOCAL_PEERCRED` (macOS) check that the connecting peer has the same UID as the daemon. Or accept the current model and document it explicitly in `SECURITY.md`. Either path works; pick one.

### P2-3 — Web-bridge API responses missing standard security headers
- Category: 6 (Web Bridge)
- File: `crates/web/src/spa.rs:26-34` (SPA-only CSP) and middleware in `crates/web/src/middleware.rs`
- Evidence: The SPA HTML response sets a strict CSP, but JSON API responses don't carry `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, or `Referrer-Policy: no-referrer`.
- Impact: Low given loopback-only binding + bearer auth, but reviewers running OWASP ZAP / Mozilla Observatory will dock points.
- Fix: Add a tower middleware that injects these three headers on every response.

### P2-4 — Attachment write destination not constrained to safe directories
- Category: 9 (Storage/Filesystem)
- File: `crates/daemon/src/handler/mod.rs` (`materialize_attachment_to_path`)
- Evidence: Accepts `destination: &std::path::Path` from IPC and calls `create_dir_all(parent)` + `tokio::fs::write` without checking whether the path escapes a designated downloads directory. A rogue or buggy client (web UI bug, malicious script using bridge token) can write under `~/.ssh/`, `~/.config/`, etc.
- Impact: Requires either (a) bridge-token theft, (b) a TUI/CLI bug, or (c) a sibling-uid attacker on the socket. Not exploitable from email content alone.
- Fix: Canonicalise `destination`, then require it to be within an allowlist of dirs (configured downloads dir, `$TMPDIR`, current working dir).

### P2-5 — Remote image / asset fetch has no body-size limit
- Category: 7 (Email Parsing)
- File: `crates/daemon/src/handler/mailbox.rs` (`materialize_remote_asset` path)
- Evidence: `response.bytes().await` followed by `tokio::fs::write`. No `Content-Length` check, no `take(N)`.
- Impact: Only triggers when the user explicitly opts into `allow_remote=true` for a given message. Then a hostile email pointing to a multi-GB asset can exhaust disk or RAM.
- Fix: Read with `response.bytes_stream()` into a `take()`-capped buffer (e.g. 25 MB), or check `Content-Length` first and refuse if it exceeds a configurable cap.

### P2-6 — Shell-hook design routes JSON through `sh -c <cmd>`
- Category: 8 (Shell Execution)
- File: `crates/rules/src/shell_hook.rs:48-50`
- Evidence: `Command::new("sh").arg("-c").arg(command)` where `command` is the user's hook string. The piped stdin (containing email fields) is safe — it's JSON. The risk is that the *hook command itself* is user config, so anyone who can write the config (or trick the user into pasting one) gets RCE.
- Impact: This is the intended trust model — hooks are explicit user-authored escape hatches. The risk reviewers flag is documentation, not behaviour.
- Fix: Document the hook trust boundary in `SECURITY.md` and emit a warning the first time a hook is enabled. Consider an `--allow-shell-hooks` daemon flag that must be set explicitly.

### P2-7 — Build-time OAuth client credentials embedded in release binary
- Category: 3 (Secrets) / 12 (CI/CD)
- File: `crates/provider-gmail/src/auth.rs:93-94` and `crates/provider-outlook/src/auth.rs:7`
- Evidence: `option_env!("GMAIL_CLIENT_ID")` / `option_env!("GMAIL_CLIENT_SECRET")` are baked at compile time, fed from `.github/workflows/release.yml:132-143` secrets.
- Impact: Google explicitly states "Desktop app" client secrets are NOT confidential — they're identifiers, and PKCE protects the flow. Same for Microsoft public clients. So this is the same pattern `gh`, `gcloud`, `aws` CLI use. But the binary becomes fingerprintable and the secrets *will* be extractable via `strings`.
- Fix: No code change needed. Document in `SECURITY.md` that bundled OAuth credentials are public app-level identifiers, not user secrets. Mention BYOC (config.toml override) for users who want their own.

### P2-8 — Dependabot auto-merge workflow uses `pull_request_target` with write perms
- Category: 12 (CI/CD)
- File: `.github/workflows/dependabot-automerge.yml`
- Evidence: `pull_request_target` + `permissions: { contents: write, pull-requests: write }`. The conditional gates merging to actor=`dependabot[bot]`, which is the correct pattern — but if the condition ever regresses, arbitrary PR code could land on `main`.
- Fix: Narrow `permissions` to the minimum each step needs. Consider requiring human approval for major-version Dependabot PRs.

### P2-9 — Two moderate JS vulnerabilities in `apps/web/`
- Category: 2 (Dependency Hygiene)
- File: `apps/web/package-lock.json`
- Evidence: `brace-expansion 5.0.2-5.0.5` (GHSA-jxxr-4gwj-5jf2, CVSS 6.5 DoS) and `ws 8.0.0-8.20.0` (GHSA-58qx-3vcg-4xpx, memory disclosure, CVSS 4.4).
- Impact: `apps/web` is the bridge SPA — bundled into the daemon binary. End-user reachable only via the loopback bridge.
- Fix: `npm install` in `apps/web/` to pick up patches.

## P3 Findings (Track, Don't Block)

### P3-1 — Attachment filenames don't reject Windows reserved names
- File: `crates/daemon/src/handler/mod.rs:2485-2535`
- Evidence: Sanitiser strips path separators, control chars, NULs, but doesn't reject `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`.
- Impact: Currently macOS/Linux only. Becomes a real bug on Windows.
- Fix: Add `is_reserved_windows_name(stem)` check, fall back to `attachment-{id}` if matched.

### P3-2 — Saved attachments inherit umask file mode
- File: `crates/daemon/src/handler/mod.rs` and `crates/daemon/src/handler/export.rs`
- Evidence: `tokio::fs::write(path, bytes)` without explicit `set_permissions`. Typical umask gives `0644`.
- Impact: On single-user systems, fine. On shared hosts, sibling users could read attachments. Email content can already be subject to local-fs disclosure via `mxr.db`, so this is mostly consistency.
- Fix: After write on `cfg(unix)`, `set_permissions(path, Permissions::from_mode(0o600))`.

### P3-3 — Swagger UI and OpenAPI JSON exposed unauthenticated on the bridge
- File: `crates/web/src/router.rs:192-194`
- Evidence: `SwaggerUi::new("/api/v1/docs").url("/api/v1/openapi.json", …)` is mounted without auth gating.
- Impact: Loopback-only by default, so only the local user can read it. Reveals API surface but not data.
- Fix: Either gate behind bearer-auth like the rest of `/api/v1/*`, or compile out unless `--dev` is set.

### P3-4 — Two RustSec advisories with current low reachability
- File: `Cargo.lock`
- Evidence:
  - `RUSTSEC-2023-0071` (rsa 0.9.10, Marvin Attack timing sidechannel) — transitive via `sqlx`; mxr does not perform externally observable RSA crypto operations. No upstream patch available.
  - `RUSTSEC-2026-0097` (rand 0.9.2 unsound under custom logger + log trace) — mxr does not install a custom rand-using logger.
  - `async-std 1.13.2` unmaintained — used by `mxr-async-imap`; tracked in `deny.toml` already.
- Fix: Monitor for upstream patches; upgrade when available. No immediate action.

### P3-5 — `release-please.yml` falls back silently if `RELEASE_PLEASE_TOKEN` is unset
- File: `.github/workflows/release-please.yml:54`
- Evidence: Falls back to `GITHUB_TOKEN`, which (by GitHub policy) does not trigger downstream workflow runs. So a missing secret would silently disable the actual release pipeline.
- Fix: Add a guard step that fails loudly when `RELEASE_PLEASE_TOKEN` is unset.

## Pass Categories (Strengths to Mention Publicly)

These are worth surfacing in `SECURITY.md` / privacy page — they're the things a careful reviewer will want to see.

- **Local-first by default.** Zero telemetry/analytics/crash reporters. No phone-home on startup or update check. Activity log is local-only with a `MXR_ACTIVITY=off` kill switch, redaction support, retention pruning, and a structural test (`crates/daemon/tests/activity_invariants.rs`) that forbids credential keys in `context_json` and forbids new writers outside `crates/daemon/src/activity/`.
- **Credentials in OS keychain.** macOS Keychain and Linux Secret Service via `crates/keychain/`. Disk fallback writes with explicit `0o600` (e.g. `crates/provider-gmail/src/auth_storage.rs:119`).
- **Minimal OAuth scopes.** Gmail: `readonly`, `modify`, `labels`. Outlook: `IMAP.AccessAsUser.All`, `SMTP.Send`, `offline_access`. No calendar, contacts, drive, or admin scopes.
- **TLS-mandatory transport.** IMAP via `async_native_tls` with hostname verification; no `danger_accept_invalid_certs`. SMTP picks implicit TLS on 465 or STARTTLS otherwise; plaintext only when explicitly opted in via config.
- **Parameterised SQL throughout.** `sqlx::query!` / `sqlx::query_as!` dominate. No `query_unchecked` or `format!`-built SQL with user data. Dynamic queries (`crates/store/src/thread_summary.rs:76`) build placeholder skeletons and `.bind()` values.
- **HTML email is rendered to plain text** via `html2text`. No JS surface, no DOM. Remote image fetch is opt-in per message. CID handling is local-only lookup. The SPA does not use unsafe React innerHTML escapes, `innerHTML` direct DOM writes, or `eval`.
- **Web bridge fails closed on non-loopback bind** without TLS (`crates/daemon/src/bridge.rs:119-129`). CORS allowlist is loopback-only (`crates/web/src/middleware.rs:78-107`). Host header validation defends against DNS-rebinding (`middleware.rs:41-64`). Bearer auth on all sensitive endpoints (`crates/web/src/auth.rs:44-89`). Bridge token file enforced `0o600` and re-validated on load (`crates/daemon/src/bridge.rs:156-185`).
- **Bounded resources.** Length-delimited IPC frames capped at 16 MB (`crates/protocol/src/codec.rs:14`). Request semaphores 64/8 (hot/bulk). Activity recorder bounded mpsc(1024) with `try_send` backpressure. Tantivy writer buffer capped at 50 MB. Gmail sync batches capped.
- **`mxr reset --hard` / `mxr burn` preserve credentials and config by default** (`crates/daemon/src/commands/reset.rs`); interactive confirmation phrase or explicit non-interactive flag required.
- **Bug reports redact secrets by default**, regex set covers `client_secret|token|password|access_token|refresh_token|api_key|authorization` (`crates/daemon/src/commands/bug_report.rs:404-443`).
- **No `curl | sh`, no sudo in `install.sh`.** Installs to `$HOME/.local/bin`. Pinned actions in CI (no `@main` / `@latest`). `Cargo.lock` committed; both JS `package-lock.json` files committed. `deny.toml` enforces license, banned-crate, and source policy.
- **gitleaks history scan: 14 hits, all benign.** Every hit is in a PII/secret-detector test fixture (`crates/safety/src/pii.rs`, `crates/safety/tests/no_raw_secrets.rs`, `crates/daemon/src/handler/mod.rs` test data) or in `site/src/content/docs/guides/pre-send-safety.md` documentation showing what the detector catches. No real secrets in history. The repo's local `.env` (real-looking `GMAIL_CLIENT_SECRET`) is correctly gitignored and never committed.

## Original Suggested Order Of Operations (Historical)

1. P1-1: Patch `install.sh` to verify checksums. ~30 min.
2. P1-2: Add checksum verification step in `release.yml github-release` job. ~30 min.
3. P1-3 + P2-9: `npm install` in `site/` and `apps/web/`, re-run `npm audit`. ~10 min.
4. P2-7: Add a `SECURITY.md` section explaining bundled OAuth credentials are public app-level identifiers. ~15 min.
5. P2-1: `chmod 0600` on the daemon socket after bind. ~10 min.
6. P2-3: Tower middleware for security headers on bridge responses. ~20 min.
7. P2-4: Constrain attachment destinations to an allowlist of dirs. ~30 min.
8. P2-5: Cap remote-asset body size. ~15 min.
9. P3 items: track in issues; ship as time allows.

Total: roughly half a day of focused work to clear all P1s and most P2s.

## What Reviewers / Scanners Were Likely To Flag Before The Fixes

Based on what AV / supply-chain reviewers typically focus on:

- **No installer signature** (P1-1) — clamav and macOS Gatekeeper both look for this. Add notarisation for the macOS binary at minimum if you're shipping signed releases.
- **`option_env!` baked OAuth client secret** (P2-7) — secret scanners may flag the released binary. Pre-empt with a `SECURITY.md` note.
- **`npm audit` HIGH in `site/`** (P1-3) — every quality dashboard runs this.
- **Lack of SBOM / provenance** — not in the rubric explicitly, but `actions/attest-build-provenance` is becoming a baseline expectation. Worth doing.

Nothing in the codebase looks like malware. Behaviour is local-first, network calls go only to user-configured providers + the release-check URL the installer uses. No persistence beyond the documented daemon socket + sqlite. No unexpected background services.
