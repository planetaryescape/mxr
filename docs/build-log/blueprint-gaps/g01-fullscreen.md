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
a persisted agenda search result. One failed harness assertion expected an
unseen body while split view correctly showed thread `#487`; the corrected
harness checks the expected sender and subject. Accepted evidence is under
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

## Handover

Parent owns final candidate review, exact-SHA acceptance, PR, release, and
published install verification. Release is not complete until those checks
pass. Keep this registered worktree, target cache, and ledger evidence until
the parent authorizes cleanup.
