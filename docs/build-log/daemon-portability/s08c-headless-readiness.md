# S08c headless readiness

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
python3 scripts/s08c_headless_smoke.py target/debug/mxr
```

It uses temporary config/data/socket paths, fake mail, a loopback controlled
OpenAI-compatible endpoint, and `MXR_KEYCHAIN=off`; its parent process stands in
for systemd without installing or enabling a service. CI runs it on Ubuntu after
the workspace tests. A macOS run is setup validation only and does not establish
the Linux ownership result.

## Code and baseline

The implementation was prepared from `f56ac1527787ca312bda7b52935947ee94495251`
(`main` at S08c start). Root owns the source checkpoint SHA and integration
ledger; this entry does not claim a committed implementation SHA.

- `crates/client/src/lib.rs` owns the default-compatible
  `MXR_DAEMON_AUTOSTART=off` policy and shared remediation.
- `crates/daemon/src/server.rs` applies the policy before stale-socket recovery
  or daemon replacement; `crates/tui/src/ipc.rs` applies it to reconnects.
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

The fixture proves supervised restart and search over persisted fake mail. It
does not promise recovery of pending analytics work; the existing
process-local-queue limitation remains. No service is installed and no real
mail, keychain, personal daemon, or VPS is used.

## Verification

Passed on macOS: `mxr-llm --lib` (36 tests), `mxr-client --lib` (24 tests),
`mxr-protocol --lib` (63 tests), `mxr-web --lib openapi_spec_snapshot` (1 test),
the full `mxr --lib` suite (1,284 passed, 6 ignored), the focused DraftAssist
route-health test (1 test), and CLI help snapshots (1 test). `cargo fmt --all
-- --check`, `git diff --check`, and Python syntax validation passed. The
isolated runtime smoke remains pending a build from the committed checkpoint;
Linux acceptance is established only by the Ubuntu CI run, not this macOS
worktree.

## Effort and follow-up

Implementation began around 2026-10-11 23:35 UTC; exact completion and waiting
times will be recorded at the source-ready checkpoint. No follow-up ticket is
currently required; Linux CI remains the runtime acceptance evidence.
