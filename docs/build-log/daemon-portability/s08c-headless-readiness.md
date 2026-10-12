# Supervised clients preserve daemon ownership and report dependency failures

Status on 2026-10-12: S08c source merged in PR398 as `b6b88deff57e0ab1b6f523844e1708e63d2121a7` (tree `764486c7068745e07e263843e31ca0b1d82edb5b`). Release metadata PR410 merged as `98bc13f574da5cfc93fec1dfc1dddab4d6e996b9` (tree `b4814c3c3631a1d83ee85369b47a81760843d132`), producing v0.6.84. Release workflow 38192614021 passed, all four GitHub asset digests and both archive checksums matched, and the installer and Homebrew binaries matched. Published supervised acceptance passed on macOS, Linux and the native TUI. S08c acceptance is complete; publication of this final documentation closeout remains pending.

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

The final reviewed candidate was `4b2d19d621d0fba90eb9c5f0dbef570dd68ad28f`
(tree `764486c7068745e07e263843e31ca0b1d82edb5b`). The reviewed PR patch
matched the merge tree; PR398 merged as `b6b88deff57e0ab1b6f523844e1708e63d2121a7`.
The release-only metadata update is `98bc13f574da5cfc93fec1dfc1dddab4d6e996b9`.
The candidate's root review, CI summary, merge proof, release metadata gate,
and sanitized runtime receipts are indexed in
[`evidence/s08c-candidate-4b2d19d6/`](evidence/s08c-candidate-4b2d19d6/).
Published v0.6.84 release and supervised Mac/Linux acceptance are recorded in
[`evidence/s08c-installed-v0.6.84/`](evidence/s08c-installed-v0.6.84/).

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

For the final candidate, PR398 CI run 38190844230 completed successfully,
including the Linux supervised-daemon smoke, Docs Build, workspace tests,
Clippy, SQLx Offline, and Rustdoc. The merge gate confirms the reviewed patch
and integration tree match. The captured macOS setup smoke returned four
persisted search results and three controlled model requests. The supervised
TUI fixture accepted a compatible daemon, rejected an incompatible replacement,
and left the replacement alive until owned cleanup. It also exposed a
nonblocking TUI issue: repeated reconnect attempts reset the outage timestamp,
so the watchdog does not open its error modal during that loop, and the
130-column status line clips the longer guidance. The frame receipt and current
source path are recorded in the packet; the issue is tracked in
[`docs/issues/tui-reconnect-watchdog-guidance.md`](../../issues/tui-reconnect-watchdog-guidance.md).

Published release acceptance passed for distribution, the supervised CLI
smokes, and the native TUI reconnect journey. Timing evidence is summarized
below; workflow jobs overlap, so their durations do not sum to the total.

| Work | Observed duration |
| --- | ---: |
| Release workflow, created to completed | 25m40s |
| Plan Release job | 8s |
| Build macOS job | 22m51s |
| Build Linux job | 18m38s |
| Release CLI Smoke job | 9m18s |
| GitHub Release job | 23s |
| Homebrew Formula job | 1m42s |
| Published supervised macOS smoke | 8.590s |
| Published supervised Linux/amd64 smoke | 14.478s |
| Prior docs build, queued job 6032 | about 1s queue wait; about 2m total |
| S08c worker assignment to source checkpoint | 41m wall clock; no separate agent-time total captured |

Homebrew v0.6.84 resolved to
`/opt/homebrew/Cellar/mxr/0.6.84/bin/mxr`; its binary SHA-256 matched the
installer binary. The production headless guide is live at
[`mxr.sh/guides/headless-linux`](https://mxr.sh/guides/headless-linux/) from
source `98bc13f5`; the browser check found no JavaScript errors and the
rendered command tokens match the source guide. The screenshot and compact
deployment receipt are in the installed evidence packet. The native TUI ran
under a 150×36 PTY: search against the compatible daemon returned one result;
after reconnecting to a frozen v0.6.82 replacement on the same socket, v0.6.84
rejected it, showed service-manager guidance and left it alive and unchanged
until owned cleanup. The TUI exited successfully, and its owned processes and
socket were cleaned up. The compact receipt records the frame hash.

Linux ran in a network-disabled Docker container with the fixed Debian image,
two CPUs and 2 GiB memory. It proves the published Linux binary under the
supervised fixture, not operation under an installed systemd service; systemd
was not installed. Analytics queue recovery and SMTP failure aggregation remain
outside this acceptance. The final S08c documentation closeout PR has not yet
been deployed.

Root review found that a supervised TUI could reconnect without checking the
replacement daemon identity. Commit `c5144ab5` adds a shared identity policy and
validates the connection before returning it to the TUI. Four focused client
tests passed, including a compatible connection followed by an incompatible
replacement and no normal request sent to the rejected peer. The existing
daemon build-identity test passed. Scoped all-target Clippy for client, TUI and
daemon passed in 1m19s, as did formatting and diff checks. The smoke script now
also triggers the Rust CI path when changed alone. The final CI and merge gate
results are recorded above; published release acceptance is recorded in the installed evidence packet.

## Effort and follow-up

The worker was assigned on 2026-10-11 at 22:27 UTC. Source was checkpointed
at 23:08 UTC, a 41-minute wall-clock interval. This is the recorded worker
window, not a separate estimate of agent execution time. The timing table above
separates observed work and queue waiting. No aggregate agent-time estimate is
available. One nonblocking
TUI follow-up is tracked above; all S08c published consumer acceptance passed.
