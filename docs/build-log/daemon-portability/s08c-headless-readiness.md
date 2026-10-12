# Supervised clients preserve daemon ownership and report dependency failures

S08c adds an explicit client opt-out from daemon autostart, process-local model
outcomes by provider route, actionable missing-IMAP-sync-credential doctor
findings, and a Linux supervised-daemon acceptance fixture. The fixture keeps
one foreground daemon under an external owner while clients remain unable to
replace it, verifies persisted fake mail after restart, and observes optional
model failure and recovery without diagnostic endpoint probes.

## Run

The focused Linux acceptance is:

```sh
cargo build -p mxr
python3 scripts/s08c_headless_smoke.py target-cli/debug/mxr
```

It uses temporary config/data/socket paths, fake mail, a loopback controlled
OpenAI-compatible endpoint, and `MXR_KEYCHAIN=off`; its parent process stands in
for systemd without installing or enabling a service. CI runs it on Ubuntu after
the workspace tests. A macOS run is setup validation only and does not establish
the Linux ownership result.

## Code and baseline

The implementation was prepared from `f56ac1527787ca312bda7b52935947ee94495251`
(`main` at S08c start). The full candidate `166f308b0dc722e8b6df47bb27af91e8685773ca` passed
CI38187946732 and OpenAPI38187946716. Its Linux supervised smoke passed
in 63 seconds; its macOS fixture preserved four search results across restart
and recorded three controlled model requests.

- `crates/client/src/lib.rs` owns the default-compatible
  `MXR_DAEMON_AUTOSTART=off` policy and shared remediation.
- `crates/daemon/src/server.rs` applies the policy before stale-socket recovery
  or daemon replacement. The central TUI client checks the same exact protocol,
  version and build identity on supervised initial, dedicated and reconnect
  connections before normal requests; the status handshake is bounded to five
  seconds.
- `crates/llm/src/observations.rs` stores typed process-local outcomes and
  provider generations; `crates/llm/src/observation_tests.rs` covers route
  isolation, sanitization, timeout, reset, and stale completion.
- `crates/daemon/src/handler/diagnostics/mod.rs` and
  `crates/daemon/src/handler/status_helpers.rs` expose no-probe outcomes and
  missing-credential guidance through existing status/doctor surfaces.
- `scripts/s08c_headless_smoke.py` drives the supervised lifecycle and
  optional-model acceptance. `.github/workflows/ci.yml` runs it on Ubuntu PR CI;
  `.github/workflows/release.yml` retains it in release smoke.
- `site/src/content/docs/guides/headless-linux.md` contains a user-service
  recipe; reference docs describe autostart, diagnostics, and credential setup.

## Decisions and limits

Autostart stays enabled by default for existing installations. Explicit `off`
lets a service manager remain the sole daemon owner. LLM outcomes are typed and
memory-only; disabled, privacy-blocked, and breaker-open results do not create
endpoint observations. A provider replacement clears route outcomes, and an
older in-flight request cannot repopulate them. Mail readiness does not depend
on an optional model. After an observed sync failure, doctor reports a missing
IMAP sync credential with disk-secret/keychain remediation without returning
the configured credential reference. SMTP send failures are not aggregated by
doctor and remain an existing limitation.

The smoke injects the credential-resolution error through the fake provider's
spool after the model checks. It verifies the observed sync-error doctor path
and shell-safe remediation without opening an IMAP connection; disk-only
credential resolution is covered by separate unit tests. Search JSON omits
thread IDs, so the fixture resolves drain targets with `mxr thread --search
QUERY --first --format ids` and keeps search message IDs for restart comparison.

The fixture proves supervised restart and search over persisted fake mail. It
does not promise recovery of pending analytics work; the existing
process-local-queue limitation remains. No service is installed and no real
mail, keychain, personal daemon, or VPS is used.

## Verification

Passed on macOS: `mxr-llm --lib` (36 tests), `mxr-client --lib` (24 tests),
`mxr-protocol --lib` (63 tests), `mxr-web --lib openapi_spec_snapshot` (1 test),
the full `mxr --lib` suite (1,285 passed, 6 ignored), the focused DraftAssist
route-health test (1 test), and CLI help snapshots (1 test). `cargo fmt --all
-- --check`, `git diff --check`, and Python syntax validation passed. The
focused quoted-remediation and merged OpenAPI snapshot checks pass on the
integrated candidate. Scoped Clippy, architecture-boundary, and cargo-deny
checks passed for the integrated candidate. The first queued boundary attempt
selected a Python interpreter without `tomllib`; the retry used Python 3.14.8.
Cargo deny reported existing duplicate/yanked dependency warnings but exited
successfully. The earlier
Linux smoke stopped when the fixture expected `thread_id` in search JSON; the
fixture now uses the thread-ID CLI surface.

Root review found that a supervised TUI could reconnect without checking the
replacement daemon identity. Commit `c5144ab5` adds a shared identity policy and
validates the connection before returning it to the TUI. Four focused client
tests passed, including a compatible connection followed by an incompatible
replacement and no normal request sent to the rejected peer. The existing
daemon build-identity test passed. Scoped all-target Clippy for client, TUI and
daemon passed in 1m19s, as did formatting and diff checks. The smoke script now
also triggers the Rust CI path when changed alone. This corrected candidate
still needs its final CI, build and published acceptance.

## Effort and follow-up

The worker was assigned on 2026-10-11 at 22:27 UTC. Source was checkpointed
at 23:08 UTC. Candidate runtime validation passed on macOS and Ubuntu; the
reconnect correction and release are pending final checks. No follow-up ticket
is currently required; Linux CI remains the Linux runtime evidence.
