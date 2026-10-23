# Messages offers Done on archived conversation history, then refuses it

Status: validated locally; shipping authorized on 2026-10-24; release in progress.

BK reported two failures pressing `e` in the web app. The exact error text is
still needed to establish whether the reproduction below explains those attempts.

The live Homebrew 0.6.159 daemon on port 42829 returned an unfinished Quiet
topic in `GET /api/v1/mail/people`. A dry-run request to
`POST /api/v1/mail/modes/messages/done` for that topic returned
`error: "not in Messages"`. Membership confirmed `in_inbox: false` and
`modes: []`. No mail was mutated during investigation.

At fetched `origin/main` b3c2f3b00aca85c2c7375030e86fb7f2380ba4fc,
`crates/daemon/src/handler/messages.rs` selects conversation history without
requiring inbox membership. `crates/daemon/src/handler/mode_done.rs` requires
active mode membership, which excludes those archived topics.

Fix commit: `60391fe6dd0a0bdbea1b6ced084e001964884ca7` on `fix/done-errors`.
Done accepts archived topics selected by the same gather path as the person page,
records a local watermark, and retains the existing provider mutation selection.
Failed Done outcomes now emit a structured warning. The regression test covers
the listed archived topic, preview, completion and undo without restoring inbox mail.

Validation after memory pressure returned to normal:

- `scripts/cargo-test -p mxr --lib handler::tests::modes`: 24 passed, one ignored.
- `scripts/cargo-test -p mxr --lib handler::tests::messages`: 26 passed, one ignored.
- `cargo build -p mxr`: passed. The macOS debug linker warned about compact
  unwind table size; the build and tests succeeded.
- `cargo fmt --all`, `git diff --check`, and architecture boundaries: passed.
- `python3 target-cli/done-http-smoke.py`: passed against an isolated fake-account
  daemon. The same HTTP endpoint the web app calls returned matching preview and
  commit outcomes: `marked: true`, `archived: 0`, `copy: "Done in Messages."`.
  HTTP undo removed the mark and preserved the archived state. The smoke daemon
  was stopped. Its script and logs remain under the ignored `target-cli/` directory.

Next: finish `scripts/pre-pr-rust-gate`, then push a draft PR, mark it ready and
merge after CI passes. Continue through release-please, the tag release workflow,
Homebrew upgrade and install.sh verification. The user authorized the full chain
with “Ship it”. The fix is rebased onto fetched `origin/main`
544da5355fbaa11a7cf64e0fecbd844ca119623a. Release manifest and latest tag agree on
0.6.159; no release-please PR was open at preflight. No real mail was mutated.

Working tree: `/tmp/mxr-done-clones/mxr`. The running web app still uses Homebrew
0.6.159. Do not replace its binary with a debug build; install the release artifacts.
