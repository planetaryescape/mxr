# Plan 001: Make TUI display truncation multibyte-safe (fix two panic sites)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat a57a53f5..HEAD -- crates/tui/src/ui/analytics_page.rs crates/tui/src/ui/briefing_modal.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `a57a53f5`, 2026-07-05

## Why this matters

Two rendering helpers slice strings by **byte index** on data that can contain
multibyte UTF-8 (email addresses and message/citation ids sourced from mail
content). Slicing a `&str` at a non-boundary byte panics in Rust, and a panic
in a draw path crashes the whole TUI. A single contact with a non-ASCII email
(internationalized addresses are legal) makes the analytics Wrapped view
unusable; a non-ASCII citation id crashes the pre-send briefing modal.

## Current state

- `crates/tui/src/ui/analytics_page.rs:737-749` — `short_email` truncates an
  email for stat cards:

  ```rust
  fn short_email(email: &str) -> &str {
      if email.len() <= 18 {
          email
      } else {
          // Find first '@' and keep that bit if short.
          if let Some(at) = email.find('@') {
              if at <= 14 {
                  return &email[..at];
              }
          }
          &email[..18]          // ← panics if byte 18 is not a char boundary
      }
  }
  ```

  (`&email[..at]` is safe — `find` returns a boundary — but `&email[..18]` is
  not, and `email.len()` counts bytes, so an 18-char CJK address takes the
  slicing branch.)

- `crates/tui/src/ui/briefing_modal.rs:101-107` — `short_id` renders citation
  ids:

  ```rust
  fn short_id(id: &str) -> String {
      if id.len() > 12 {
          format!("{}…{}", &id[..4], &id[id.len() - 4..])   // ← both slices can panic
      } else {
          id.to_string()
      }
  }
  ```

- Bonus cleanup in the same file (fold into this plan, same commit is fine):
  `crates/tui/src/ui/analytics_page.rs:1660,1671` builds a `bar` String that is
  never rendered (`let _ = bar;` on line 1671). Delete both lines — the
  styled `line` built at 1661-1670 is what gets rendered.

- Repo conventions: helpers in these files are private free functions with
  `#[cfg(test)] mod tests` in the same file. `briefing_modal.rs` already has a
  test module (see `crates/tui/src/ui/briefing_modal.rs:109+`) using
  `mxr_test_support::render_to_string` — match that structure for unit tests,
  or plain assert-based unit tests for pure string helpers.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `scripts/cargo-test -p mxr-tui --tests` | all pass, exit 0 |
| Build | `cargo build -p mxr` | exit 0 |
| Lint | `cargo clippy -p mxr-tui --all-targets` | no new warnings |

## Scope

**In scope** (the only files you should modify):
- `crates/tui/src/ui/analytics_page.rs`
- `crates/tui/src/ui/briefing_modal.rs`

**Out of scope** (do NOT touch, even though they look related):
- `crates/tui/src/ui/message_view.rs` — its slice sites (lines ~1170-1191) use
  offsets returned by `str::find`, which are always char boundaries; they are
  safe. Do not "fix" them.
- Any shared-truncation-helper refactor across the ui/ directory — out of this
  plan's blast radius.

## Git workflow

- Branch: `advisor/001-multibyte-safe-truncation`
- Conventional commit, e.g. `fix: make analytics and briefing truncation multibyte-safe`
- No Claude/AI attribution lines in the commit message. Do not push or open a
  PR unless the operator instructed it.

## Steps

### Step 1: Fix `short_email`

Rewrite so all truncation is by characters, never raw byte offsets. Target
shape (return type changes to `String` — check the call sites in the same file
and adjust; they only format it into stat-card text):

```rust
fn short_email(email: &str) -> String {
    if email.chars().count() <= 18 {
        return email.to_string();
    }
    if let Some(at) = email.find('@') {
        if email[..at].chars().count() <= 14 {
            return email[..at].to_string();
        }
    }
    email.chars().take(18).collect()
}
```

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → compiles, existing tests pass.

### Step 2: Fix `short_id`

```rust
fn short_id(id: &str) -> String {
    let chars: Vec<char> = id.chars().collect();
    if chars.len() > 12 {
        let head: String = chars[..4].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}…{tail}")
    } else {
        id.to_string()
    }
}
```

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → pass.

### Step 3: Delete the dead `bar` allocation

In `render_split_bar` (analytics_page.rs), remove the `let bar = format!(...)`
line and the `let _ = bar;` line.

**Verify**: `cargo clippy -p mxr-tui --all-targets` → no unused-variable warnings.

### Step 4: Add regression tests

In each file's `#[cfg(test)] mod tests`:

- `short_email` with: an 18+-char ASCII address (truncates to 18), an address
  whose 18th byte falls inside a multibyte char (e.g. repeat `"é"` or CJK to
  >18 chars) — must not panic, a short address (unchanged), a long local-part
  with `@` beyond position 14.
- `short_id` with: a 13+-char ASCII id, an id of multibyte chars (must not
  panic, returns 4-char head/tail), a short id (unchanged).

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → all pass including the new tests.

## Test plan

New unit tests as in Step 4, colocated in each file's existing/new
`#[cfg(test)] mod tests`. Model after the existing tests at the bottom of
`crates/tui/src/ui/briefing_modal.rs`. The multibyte cases are the regression
tests for this bug; before the fix they panic.

## Done criteria

- [ ] `scripts/cargo-test -p mxr-tui --tests` exits 0, new multibyte tests present and passing
- [ ] `cargo build -p mxr` exits 0
- [ ] `grep -n '&email\[\.\.18\]' crates/tui/src/ui/analytics_page.rs` returns no matches
- [ ] `grep -n 'let _ = bar' crates/tui/src/ui/analytics_page.rs` returns no matches
- [ ] No files outside the in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- The excerpts above don't match the live code (drift).
- Changing `short_email`'s return type to `String` breaks more than ~3 call
  sites or a call site relies on borrowing — report instead of restructuring.
- A verification fails twice after a reasonable fix attempt.

## Maintenance notes

- Any future truncation of user/mail-controlled strings in the TUI must be
  char-aware (or width-aware via `unicode-width`, which ratatui already pulls
  in). Reviewers: grep new UI code for `[..n]` slices on `&str`.
- Deferred: a shared `truncate_chars` helper for the ui/ directory (several
  files do width-based truncation their own way). Not done here to keep the
  diff minimal.
