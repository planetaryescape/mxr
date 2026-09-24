# Plan 005: Write mail-content temp files into a private 0700 directory with 0600 permissions

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat a57a53f5..HEAD -- crates/compose/src/lib.rs crates/tui/src/editor.rs crates/tui/src/local_io.rs`
> On any drift, compare the "Current state" excerpts against the live code;
> on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2 (security)
- **Effort**: M
- **Risk**: MED (touches compose file paths used by TUI, CLI, and tests)
- **Depends on**: none (coordinate with 004 — different files, but both touch draft lifecycle; re-run each other's tests)
- **Category**: security
- **Planned at**: commit `a57a53f5`, 2026-07-05

## Why this matters

Draft emails, diagnostics dumps (logs/events/activity — personal data under
the project's own privacy invariant), and bug reports are written to the
**shared** system temp directory with umask-default permissions (typically
0644) and predictable names. On multi-user Linux hosts and containers, other
local users can read unsent email bodies and activity data. Additionally,
diagnostics buffers from `open_temp_text_buffer` are **never deleted**, so
this content accumulates indefinitely. The daemon already sets its socket to
0600 (`crates/daemon/src/server.rs:276-281`) — client-side temp files should
meet the same bar.

## Current state

Three write sites, all `std::env::temp_dir()` + default perms:

1. `crates/compose/src/lib.rs:135-137` (`build_draft_file`) — used by every
   compose path (TUI and CLI):

   ```rust
   let draft_id = Uuid::now_v7();
   let path = std::env::temp_dir().join(format!("mxr-draft-{draft_id}.md"));
   ```

   Written via `std::fs::write` / `tokio::fs::write`
   (`create_draft_file_with_signature` lib.rs:56-64,
   `create_draft_file_async_with_signature` lib.rs:89-97).

2. `crates/tui/src/editor.rs:60-89` (`open_temp_text_buffer`) — diagnostics
   pane details (doctor/storage/sync-health/events/jobs/activity text):

   ```rust
   let path = std::env::temp_dir().join(format!(
       "mxr-{}-{}.txt",
       name,
       chrono::Utc::now().format("%Y%m%d-%H%M%S")
   ));
   std::fs::write(&path, content) ...
   ```

   The file is never removed — not on success, not on cancel.

3. `crates/tui/src/local_io.rs:192-208` (`submit_bug_report_write`):

   ```rust
   let filename = format!("mxr-bug-report-{}.md", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
   let path = std::env::temp_dir().join(filename);
   let result = tokio::fs::write(&path, &content).await ...
   ```

   The user is told the path afterwards (`local_io.rs:228-230`), so this file
   must survive — but should still be 0600 in a private dir.

Facts for the design:

- `tempfile = "3"` is already a workspace dependency and is already declared
  in `crates/compose/Cargo.toml:37` and `crates/tui/Cargo.toml:49`
  (currently dev/test use — check which section; move/add to `[dependencies]`
  as needed). You may not need it at all (see Step 1 design).
- Crate boundaries (from `.agents/skills/mxr-development/SKILL.md`): `tui`
  already depends on `mxr-compose`; putting the helper in `mxr-compose` keeps
  boundaries intact. Do NOT add a tui→daemon or compose→daemon dependency.
- `delete_draft_file_async` (`crates/compose/src/lib.rs:124-131`) treats
  NotFound as success — path changes don't break cleanup.
- Repo runs on macOS + Linux; Windows is not a supported target for perms —
  gate Unix-only permission code with `#[cfg(unix)]` and keep non-Unix
  behavior compiling (plain create).

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Compose tests | `scripts/cargo-test -p mxr-compose --tests` | pass, exit 0 |
| TUI tests | `scripts/cargo-test -p mxr-tui --tests` | pass, exit 0 |
| Build | `cargo build -p mxr` | exit 0 |

## Scope

**In scope**:
- `crates/compose/src/lib.rs` (+ a new `crates/compose/src/private_tmp.rs` module)
- `crates/compose/Cargo.toml` (only if a dependency section move is needed)
- `crates/tui/src/editor.rs`
- `crates/tui/src/local_io.rs`

**Out of scope**:
- `crates/tui/src/local_io.rs` browser-cache HTML files (`local_io.rs:24-120`)
  — separate feature with its own cleanup logic; note in report if it has the
  same perms issue, don't fix here.
- Daemon-side attachment cache — already hardened (commit `c0780377`).
- Draft *cleanup* logic paths — Plan 004's scope.

## Git workflow

- Branch: `advisor/005-private-temp-files`
- Conventional commit, e.g. `fix: write drafts and diagnostics dumps to a private 0700 dir with 0600 perms`
- No AI attribution lines. Do not push/PR unless instructed.

## Steps

### Step 1: Add a private-dir helper to `mxr-compose`

New module `crates/compose/src/private_tmp.rs` (wired in `lib.rs` as
`pub mod private_tmp;`):

```rust
/// Per-user private scratch dir for mail-content temp files.
/// $XDG_RUNTIME_DIR/mxr (already 0700 by spec) when set, else
/// $TMPDIR/mxr-$UID created 0700. Never the shared temp root directly.
pub fn private_scratch_dir() -> std::io::Result<std::path::PathBuf> { ... }

/// Create `dir/name` with 0600 perms (O_EXCL) and write `content`.
pub fn write_private(path: &std::path::Path, content: &[u8]) -> std::io::Result<()> { ... }
pub async fn write_private_async(path: &std::path::Path, content: &[u8]) -> std::io::Result<()> { ... }
```

Implementation notes:
- Unix: `std::fs::DirBuilder::new().mode(0o700).recursive(true)` via
  `std::os::unix::fs::DirBuilderExt`; if the dir pre-exists, verify ownership
  (`uid == geteuid`, via metadata `st_uid` — `std::os::unix::fs::MetadataExt`)
  and mode 0700, else return an error (defends against a squatter dir in
  shared /tmp).
- File create: `OpenOptions::new().write(true).create_new(true).mode(0o600)`
  (`std::os::unix::fs::OpenOptionsExt`). `create_new` (O_EXCL) also removes
  symlink-follow risk.
- Async variant may wrap the sync one in `tokio::task::spawn_blocking` or use
  `tokio::fs::OpenOptions` with the same flags (it re-exports `mode` on Unix).
- Non-Unix (`#[cfg(not(unix))]`): fall back to plain create in
  `temp_dir().join("mxr")`.
- No new dependencies needed; do not add any without checking `deny.toml`.

**Verify**: `scripts/cargo-test -p mxr-compose --tests` → compiles, pass
(add a unit test here in Step 4).

### Step 2: Move the three write sites onto the helper

1. `build_draft_file` (compose lib.rs:136-137):
   `private_scratch_dir()?.join(format!("mxr-draft-{draft_id}.md"))`, and the
   two create functions write via `write_private` / `write_private_async`.
   Map `io::Error` into `ComposeError` the same way existing code does.
2. `open_temp_text_buffer` (tui editor.rs): same dir + `write_private`; ALSO
   delete the file after the editor returns (both success and cancel arms) —
   the content is regenerable diagnostics text:

   ```rust
   let result = /* existing status handling producing the message */;
   let _ = std::fs::remove_file(&path);
   result
   ```

3. `submit_bug_report_write` (tui local_io.rs): same dir + async write; keep
   the file (the user is shown the path) — 0600 in the private dir is the fix.

**Verify**: `cargo build -p mxr` → exit 0;
`scripts/cargo-test -p mxr-tui --tests` → pass (some tests assert on draft
paths — update fixtures that assume `temp_dir()` directly).

### Step 3: Sweep for missed sites

`grep -rn "env::temp_dir" crates/compose/src crates/tui/src --include='*.rs'`
— remaining legitimate hits should be tests and
`crates/compose/src/editor.rs:94,105` (editor *shell-string* fixtures — check
what they are; if they write mail content, migrate them too, if they're
test-only stubs leave them and note it).

**Verify**: the grep output contains no non-test production write of mail
content to the shared temp root.

### Step 4: Tests

- `crates/compose/src/private_tmp.rs` unit tests (Unix-gated):
  dir created with mode 0700; file created 0600 (`metadata.permissions().mode() & 0o777 == 0o600`);
  `create_new` refuses to overwrite an existing file.
- `crates/compose` draft test: `create_draft_file_with_signature` produces a
  file under the private dir with 0600.
- `crates/tui` editor test: `open_temp_text_buffer` removes the file after a
  cancelled editor (set `$EDITOR` to `false` via the existing
  `resolve_editor` config-or-env path — see how other tests fake the editor,
  e.g. in `crates/tui/src/runner/tests/input_and_compose.rs`).

**Verify**: `scripts/cargo-test -p mxr-compose --tests` and
`scripts/cargo-test -p mxr-tui --tests` → all pass.

## Test plan

As Step 4. Permission asserts use `std::os::unix::fs::PermissionsExt` and are
`#[cfg(unix)]`. Model compose-crate tests on the existing test module in
`crates/compose/src/lib.rs` (if none exists, create `#[cfg(test)] mod tests`
in `private_tmp.rs` only).

## Done criteria

- [ ] `scripts/cargo-test -p mxr-compose --tests` exits 0
- [ ] `scripts/cargo-test -p mxr-tui --tests` exits 0
- [ ] `cargo build -p mxr` exits 0
- [ ] `grep -rn "env::temp_dir" crates/compose/src crates/tui/src --include='*.rs' | grep -v test | grep -v "private_tmp"` shows no production mail-content writes
- [ ] New files verified 0600, parent dir 0700 (unit tests)
- [ ] `plans/README.md` status row updated

## STOP conditions

- The CLI (`crates/daemon/src/cli` or `apps/`) constructs draft paths itself
  expecting the old location (grep `mxr-draft-` across the repo first) —
  report before changing the path scheme.
- `XDG_RUNTIME_DIR` handling conflicts with an existing helper in
  `mxr-config` (check `crates/config/src` for a runtime/cache dir helper — if
  one exists, use it instead of inventing one, and report the substitution).
- Permission tests fail on macOS CI due to tmpdir semantics — report rather
  than loosening the assertions.

## Maintenance notes

- Anyone adding a new temp-file write containing mail/activity content must
  use `mxr_compose::private_tmp` — reviewers should reject raw
  `env::temp_dir()` in tui/compose code.
- Follow-up (deferred): a startup sweep deleting stale `mxr-draft-*` files
  older than N days from the private dir; plus the browser-cache perms check
  noted in Scope.
- Ties into Plan 004 (deletion paths) — together they give create-private +
  delete-reliably.
