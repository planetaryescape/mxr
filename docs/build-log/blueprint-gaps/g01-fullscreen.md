# The fullscreen reader uses the whole mail area

The mailbox `F` action now gives the message reader the usable main mail area.
The sidebar and message list are hidden; Tab, h, and Left cannot focus the
hidden list. A second `F` restores split view with the message focused, and
Escape returns to the list. Search keeps its own pane switching even when the
mailbox's fullscreen layout state is stale.

The implementation is in commit `1c7352c2af8968c556ef40e72e721ca84450f9e6`,
based on `5581afb3cb1a8767c87fea0bc807c3a4e5fc4184`. The TUI change is in
`crates/tui/src/app/draw.rs`, `input.rs`, `mailbox_actions.rs`, and
`message_actions.rs`; regression coverage is in
`crates/tui/src/runner/tests/input_and_compose.rs`. User-facing documentation
is in `docs/blueprint/08-tui.md` and `site/src/content/docs/reference/tui.md`.
The precise TLS lockfile repair is commit `94b2348b5`.

## Verification

The published v0.6.115 baseline showed the `From:` header at column 19 in a
120-column terminal, with the sidebar visible. Parent acceptance confirmed
the candidate's header at column 1 after `ga` then `F`, then ran 12 PTY checks
in 17.778 seconds on the final rebuilt binary. These covered Search pane
navigation, fullscreen and split restore, a `gm`→`j`→`o`→`F` mode-origin
message (`From: CI Bot` at column 1), doctor health, activity status/list, and
a persisted agenda search result. Accepted evidence is under
`/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-blueprint-gaps-20261021/g01-accepted-candidate-evidence/`.
No real mail or personal daemon was used. Owned processes were reaped and the
synthetic socket was absent after the run.

The installed Cargo wrapper is broken because `~/.cargo/bin/cargo` points to a
missing rustup executable. Checks used Rust/Cargo 1.99.1 directly through
`agent-work`, with the worktree target at
`/Users/bhekanik/code/planetaryescape/.worktrees/mxr-blueprint-fullscreen/target`.
After primary RustSec advisories were verified, only these lock entries were
updated: `rustls-webpki` 0.103.15→0.103.16, `webpki-roots` 1.0.7→1.0.10, and
`webpki-root-certs` 1.0.7→1.0.10. No other package versions changed. The
advisory-only `cargo-deny` failure before this update reported RUSTSEC-2026-0381,
0382, and 0383; all three cleared after the precise updates.

After the lock update, `cargo test -p mxr-tui --tests` passed 1,088 tests,
`cargo test -p mxr-client -p mxr-provider-gmail -p mxr-provider-outlook -p mxr-provider-smtp --tests`
passed 166 tests, and `cargo build -p mxr` passed. The rebuilt binary is at
`target/debug/mxr` with SHA-256
`e6f5709bfdf0d46882580f9453919961aa7025e32c2a22179766249e1c7f89fd`.
The standalone `cargo-deny check --hide-inclusion-graph` and scoped
`scripts/pre-pr-rust-gate --base FETCH_HEAD` passed. The gate checked both
`mxr` and `mxr-tui` after Cargo.lock mapped the dependency update to the root
binary. Deny reports existing duplicate-version, yanked-crate, and
unmatched-skip warnings, but advisories, bans, licenses, and sources all pass.

The first PR Docs Build (`39514396385`) then failed at site `npm audit` with
three `launch-editor` advisories in Vite 8.1.5. `site/package-lock.json` had
Vite under the affected range. A package-lock-only npm update advanced to
8.3.4 and changed the rolldown dependency graph, so that generated lockfile
change was discarded. npm rejects versions on `npm update`, while
`npm install --no-save --package-lock-only` did not update the lock. The
minimal exact `site` override to Vite 8.1.6 let npm update only the Vite lock
entry. No other package versions changed. The site audit and docs validator
passed locally. The full Docs Build passed on final PR CI
`39515274566`, alongside the required Rust checks.
The later release-harness PR enabled the required web audit, which failed
on `apps/web` Vite 7.3.6 in CI `39522071231`. Commit
`72f77c20fb56b4feb3eb452b31218202457e4f17` pins Vite 7.3.7; only its package
record and root constraint changed. The patched network audit reports zero
vulnerabilities. Required web checks remain pending on the final candidate;
receipts are in `docs/issues/apps-web-vite-launch-editor-advisories.md`.

## Review notes

The first review caught Search pane handling behind the mailbox fullscreen
guard. Since opening Search preserves mailbox layout state, Tab could otherwise
stop switching Search panes. Search dispatch now precedes the mailbox guard,
with a regression test. A 40×12 render initially appeared to fail the new
fullscreen test, but the TUI intentionally rejects terminals below 80×20; the
test now uses that supported minimum.

The inline code-simplifier pass found no additional changes: the existing
message-view draw path and mailbox/input action handlers already own the
respective layout and focus behavior. Anti-slop and TypeScript review were not
applicable because no JavaScript or TypeScript changed. The documentation
review corrected the blueprint's stale lowercase `f` spelling and added the
fullscreen workflow to the TUI reference. Site docs validation passed; the
documentation clean-text pass reported zero removals and replacements.

## Release verification is pending

[PR 543](https://github.com/planetaryescape/mxr/pull/543) merged the reviewed
candidate `1e4853d865c27ee78d4d43757de845c16631881a` as main commit
`36c88ac87c130862d463a7a10ee5f7a9267d97d2`. Their trees match:
`c25925031fb6780002471194f098414f2e04afb8`.
[CI 39515274566](https://github.com/planetaryescape/mxr/actions/runs/39515274566)
passed on the exact candidate. Independent second-opinion review was waived
by BK; worker checks and parent review passed.

Release-please [PR 544](https://github.com/planetaryescape/mxr/pull/544)
passed its required CI and merged as
`748d2e9d6b5cba4374d1cd5fbd8ed035b4563d9f`, the source of `v0.6.116`.
Its tree matches the reviewed release metadata tree
`e728dc6903cf182ea1e50422363ef7d62376cf5a`.

The [tagged release workflow](https://github.com/planetaryescape/mxr/actions/runs/39518033868)
completed both CLI builds and the signed desktop build. Both attempts failed
at launch-proof temporary-directory cleanup after all 13 proof rows passed;
publication did not run. The cleanup ordering and observed error are tracked
in [the launch-proof issue](../../issues/launch-proof-daemon-cleanup.md).
The failed-job retry reproduced the error. Cleanup repair
`2957da9aa759dc297e50fea48a282d88272eaf70` now passes its delayed-writer and
failure-boundary regression tests plus all 13 rows of an isolated launch
proof. A further inherited-socket review finding is fixed in `283d3e897`
and `4ed210f10`: the proof chooses its own socket and explicit temp-directory
template. Final verification passed 13 proof rows with an untouched foreign
sentinel, then confirmed all owned processes and sockets stopped. Release-please must
create a new version for the repair; v0.6.116 remains unpublished.

Next: require successful release publication and Homebrew update, verify the
published install.sh download and the Homebrew binary, and run the isolated
13-check PTY acceptance against the released binary. Preserve the existing
personal daemon. Keep the registered worktree, build cache and evidence until
scope-bound cleanup approval is given.
