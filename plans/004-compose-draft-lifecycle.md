# Plan 004: Close compose draft temp-file leak paths

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat a57a53f5..HEAD -- crates/tui/src/compose_flow.rs crates/tui/src/runner.rs`
> On any drift, compare the "Current state" excerpts against the live code;
> on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (coordinate with 005, which changes where these files live)
- **Category**: bug
- **Planned at**: commit `a57a53f5`, 2026-07-05

## Why this matters

Compose writes the draft to a temp file and opens `$EDITOR` on it (by design).
The cleanup paths have holes: if the editor fails to launch, or the draft file
can't be read back after creation, the file — containing recipients, subject,
quoted thread context, and body — is stranded in the temp directory forever.
There is also a dead `Ok(None)` match arm masking the actual contract of
`pending_send_from_edited_draft`. Small fix, real privacy/disk hygiene payoff;
005 hardens where these files live, this plan makes sure they get deleted.

## Current state

- Draft creation: `crates/tui/src/compose_flow.rs:209-224`
  (`handle_compose_action` tail):

  ```rust
  let (path, cursor_line) =
      mxr_compose::create_draft_file_async_with_signature(kind, &from, signature.as_ref())
          .await
          .map_err(|e| MxrError::Ipc(e.to_string()))?;

  Ok(ComposeReadyData {
      account_id,
      intent,
      draft_path: path.clone(),
      cursor_line,
      initial_content: mxr_compose::read_draft_file_async(&path)
          .await
          .map_err(|e| MxrError::Ipc(e.to_string()))?,   // ← Err leaks `path`
      invite_reply,
  })
  ```

- Editor result handling: `crates/tui/src/compose_flow.rs:436-470`
  (`handle_compose_editor_status`):

  ```rust
  match status {
      Ok(s) if s.success() => match pending_send_from_edited_draft(data).await {
          Ok(Some(mut pending)) => { /* opens send-confirm modal */ }
          Ok(None) => {}                                   // ← dead arm (see below)
          Err(error) => {
              app.report_error(error.modal_title(), error.modal_detail());
          }                                                 // ← intentionally keeps the file (user content)
      },
      Ok(_) => {
          app.status_message = Some("Draft discarded".into());
          let _ = mxr_compose::delete_draft_file_async(&data.draft_path).await;
      }
      Err(error) => {
          app.report_error("Compose Failed", format!("Failed to launch editor: {error}"));
      }                                                     // ← editor never launched; file leaks
  }
  ```

- `pending_send_from_edited_draft` (`compose_flow.rs:366-434`) returns
  `Result<Option<PendingSend>, ComposeValidationError>` but **every** success
  path returns `Ok(Some(..))` (line 417) — `Ok(None)` is unreachable.

- Cleanup infrastructure that already exists (use it, don't invent another):
  - `App::schedule_draft_cleanup(path)` — `crates/tui/src/app/compose_helpers.rs:5-9`
    (dedup-pushes onto `compose.pending_draft_cleanup`).
  - The queue is drained in `crates/tui/src/local_io.rs:182-189` via
    `mxr_compose::delete_draft_file_async`, with failures surfaced as a status
    message.
  - `delete_draft_file_async` treats NotFound as success
    (`crates/compose/src/lib.rs:124-131`).

- Caller of `handle_compose_editor_status`: `crates/tui/src/runner.rs:2586-2594`
  (`AsyncResult::ComposeReady(Ok(data))` arm — suspends terminal, runs
  `$EDITOR`, then calls the handler). `handle_compose_action` runs in a
  background task without `&mut App`, so it cannot call
  `schedule_draft_cleanup` — its error path must delete the file directly.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `scripts/cargo-test -p mxr-tui --tests` | all pass, exit 0 |
| Build | `cargo build -p mxr` | exit 0 |

## Scope

**In scope**:
- `crates/tui/src/compose_flow.rs`

**Out of scope**:
- The `Err` arm of `pending_send_from_edited_draft` (validation failure):
  keeping the file there is **deliberate** — deleting it would destroy the
  user's just-written content. Do not add deletion there.
- `crates/compose/**` — file location/permissions are Plan 005's scope.
- The send/confirm modal flow and where *it* schedules cleanup after a
  successful send — out of scope unless a test reveals it never cleans up
  (then STOP and report).

## Git workflow

- Branch: `advisor/004-compose-draft-lifecycle`
- Conventional commit, e.g. `fix: delete compose draft file on editor-launch and read failures`
- No AI attribution lines. Do not push/PR unless instructed.

## Steps

### Step 1: Delete the draft when the editor fails to launch

In `handle_compose_editor_status`, `Err(error)` arm, schedule cleanup (the
handler has `&mut App`):

```rust
Err(error) => {
    app.schedule_draft_cleanup(data.draft_path.clone());
    app.report_error("Compose Failed", format!("Failed to launch editor: {error}"));
}
```

**Verify**: `cargo build -p mxr` → exit 0.

### Step 2: Don't leak the file when read-back fails during creation

In `handle_compose_action`, restructure the tail so a `read_draft_file_async`
error deletes the just-created file before returning `Err`:

```rust
let initial_content = match mxr_compose::read_draft_file_async(&path).await {
    Ok(content) => content,
    Err(e) => {
        let _ = mxr_compose::delete_draft_file_async(&path).await;
        return Err(MxrError::Ipc(e.to_string()));
    }
};
Ok(ComposeReadyData { account_id, intent, draft_path: path, cursor_line, initial_content, invite_reply })
```

(Note `path.clone()` becomes unnecessary.)

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → pass.

### Step 3: Remove the impossible `Ok(None)` contract

Change `pending_send_from_edited_draft` to return
`Result<PendingSend, ComposeValidationError>` (drop the `Option`), return
`Ok(PendingSend { .. })` at line 417, and delete the `Ok(None) => {}` arm in
`handle_compose_editor_status`. Fix any other callers the compiler finds
(check `crates/tui/src/compose_flow.rs` tests and `runner/tests/`).

**Verify**: `cargo build -p mxr` → exit 0; `scripts/cargo-test -p mxr-tui --tests` → pass.

### Step 4: Tests

In `compose_flow.rs`'s existing `#[cfg(test)] mod tests` (see the test at
lines ~655-693 for the fixture pattern — it drives `handle_compose_action`
with a fake IPC channel):

1. Read-back failure cleanup: hard to force `read_draft_file_async` to fail
   without filesystem tricks — instead test the editor-launch failure path:
   build a `ComposeReadyData` pointing at a real temp file, call
   `handle_compose_editor_status` with `status = Err(io::Error::new(...))`,
   then assert the path appears in `app.take_pending_draft_cleanup()`.
2. Discard path still deletes: call with `Ok(exit_status_failure)` (construct
   via `std::process::ExitStatus` from a real failed command, e.g.
   `Command::new("false").status()`) and assert the file is gone.

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → all pass with new tests.

## Test plan

As Step 4; model fixtures after the existing
`new_compose_draft_includes_tui_recipient_and_subject_frontmatter` test in the
same file. The editor-launch-failure test is the regression test for the main
leak.

## Done criteria

- [ ] `scripts/cargo-test -p mxr-tui --tests` exits 0 with new tests
- [ ] `cargo build -p mxr` exits 0
- [ ] `grep -n "Ok(None)" crates/tui/src/compose_flow.rs` returns no matches
- [ ] No files outside scope modified
- [ ] `plans/README.md` status row updated

## STOP conditions

- Other callers of `pending_send_from_edited_draft` exist outside
  `crates/tui/src/compose_flow.rs` (grep first) — report before changing the
  signature.
- You discover the successful-send path also never deletes the draft file —
  report it as a new finding; do not widen scope.
- Excerpt drift or verification fails twice.

## Maintenance notes

- The validation-error path (`Err` from `pending_send_from_edited_draft`)
  intentionally strands the file to preserve user content; the error modal
  does not currently tell the user where it is. Follow-up worth considering: a
  "draft preserved at <path>" line in that modal, or a recover-draft flow.
- Plan 005 moves these files into a private directory; land in either order,
  but re-run this plan's tests after 005.
