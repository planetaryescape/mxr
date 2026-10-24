# Messages offers Done on archived conversation history, then refuses it

Status: resolved in [v0.6.160](https://github.com/planetaryescape/mxr/releases/tag/v0.6.160), installed and verified on 2026-10-24.

BK reported two failures pressing `e` in the web app but could not recall the
error text. The mismatch below was reproduced independently; whether it explains
both original attempts remains unconfirmed.

The live Homebrew 0.6.159 daemon on port 42829 returned an unfinished Quiet
topic in `GET /api/v1/mail/people`. A dry-run request to
`POST /api/v1/mail/modes/messages/done` for that topic returned
`error: "not in Messages"`. Membership confirmed `in_inbox: false` and
`modes: []`. No mail was mutated during investigation.

At fetched `origin/main` b3c2f3b00aca85c2c7375030e86fb7f2380ba4fc,
`crates/daemon/src/handler/messages.rs` selects conversation history without
requiring inbox membership. `crates/daemon/src/handler/mode_done.rs` requires
active mode membership, which excludes those archived topics.

Fix merged in [PR #703](https://github.com/planetaryescape/mxr/pull/703) at
`5213b493b7e7be40584b7f0bdf8d6a76158cc458` on `main`.
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

## The published binary passes the same Done path

[Release PR #704](https://github.com/planetaryescape/mxr/pull/704) merged at
`673b3fdc526ee9f0d6340f8349737304f474f19c`, also the target of `v0.6.160`.
The manifest on `main` agrees with the tag. The
[release workflow](https://github.com/planetaryescape/mxr/actions/runs/40043527814)
passed, published all six assets, and updated and verified the Homebrew formula
and cask. Its initial Docs Build failure was resolved by updating only the two
site lockfile entries for undici to 6.29.1 and 8.12.2; audit and full site build passed.

- Homebrew and `install.sh` both report `mxr 0.6.160`; the installer checksum passed.
- Homebrew, the published installer binary and the CI macOS archive contain the
  same executable bytes. That release binary passed HTTP preview, Done and undo
  against an isolated fake-account daemon, preserving the archived state.
- The default live daemon restarted onto the Homebrew executable and reported
  healthy. The exact archived-topic dry run that returned `not in Messages` on
  0.6.159 returned `marked: true`, no error, and `Done in Messages.` on 0.6.160.
- No real mail was mutated to verify the fix. Messages was opened in Dia.

No release or installation steps remain. Investigation artifacts and the detailed
local handover remain in `/tmp/mxr-done-clones/mxr`; the site audit fix was prepared
in `/tmp/mxr-release-audit-clones/mxr`.
