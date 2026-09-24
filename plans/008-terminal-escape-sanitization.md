# Plan 008: Verify and harden the TUI against terminal escape sequences in mail content

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat a57a53f5..HEAD -- crates/tui/src/ui/message_view.rs crates/tui/src/ui/mail_list.rs`
> On any drift, compare the "Current state" excerpts against the live code;
> on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: S–M (starts as an investigation; fix size depends on the result)
- **Risk**: LOW
- **Depends on**: none
- **Category**: security (investigate-then-fix)
- **Planned at**: commit `a57a53f5`, 2026-07-05

## Why this matters

Email is attacker-controlled input rendered into a terminal. Subjects,
sender names, and plain-text bodies flow into ratatui `Span`s with no
control-character stripping anywhere in the pipeline (verified by grep across
`crates/reader/src` and `crates/tui/src/ui`). If raw ESC/OSC bytes survive to
the terminal, a crafted email could manipulate terminal state (title changes,
clipboard writes via OSC 52 on some terminals, display corruption). ratatui
*probably* neutralizes zero-width control characters in its buffer — that is
the thing this plan first proves or disproves, then locks in with an explicit
sanitization layer and regression tests either way. Defense-in-depth for a
mail client, not a confirmed exploit.

## Current state

- `crates/tui/src/ui/message_view.rs:~1159-1204` — `style_line_with_links()`
  turns body lines into `Span::raw(..)` segments. No filtering of C0/C1
  control characters or ESC sequences.
- `crates/tui/src/ui/mail_list.rs` — subject/sender cells rendered as spans;
  same absence of filtering.
- `grep -rn "\\x1b\|strip\|sanitiz\|control" crates/reader/src crates/tui/src/ui` →
  no sanitization hits (as of the planned-at commit).
- Snapshot-test infra exists: `crates/tui/tests/snapshots.rs` and
  `mxr_test_support::render_to_string` (used across ui tests) render through a
  ratatui test backend — the right harness to observe what actually lands in
  the buffer.
- Design constraint (repo doctrine): reader path is plain-text-first; any
  sanitization must not alter legitimate content (tabs, newlines, wide chars,
  emoji must render unchanged).

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `scripts/cargo-test -p mxr-tui --tests` | pass, exit 0 |
| Build | `cargo build -p mxr` | exit 0 |

## Scope

**In scope**:
- `crates/tui/src/ui/message_view.rs`
- `crates/tui/src/ui/mail_list.rs`
- A new shared helper module, e.g. `crates/tui/src/ui/sanitize.rs`
- New tests (in the files above or `crates/tui/tests/`)

**Out of scope**:
- `crates/reader/**` — the reader crate is shared with export/CLI paths where
  raw fidelity may be wanted; sanitize at the TUI render boundary only.
- HTML rendering path (`html2text` output) — HTML entities are decoded to
  text and flow through the same span path; covered by the same helper, but do
  not modify the html pipeline itself.
- Toasts/status-bar (daemon-controlled, not mail-controlled) — skip.

## Git workflow

- Branch: `advisor/008-escape-sanitization`
- Conventional commit, e.g. `fix: strip control sequences from mail content at the tui render boundary`
- No AI attribution lines. Do not push/PR unless instructed.

## Steps

### Step 1: Establish what ratatui does today (the verdict step)

Write a test (in `message_view.rs`'s test module, using the existing
`render_to_string`-style harness) that renders a message whose body contains:
`"before \x1b[31mred\x1b[0m \x1b]0;title\x07 \x1b]8;;http://x\x1b\\ after"`.
Inspect the rendered buffer string: does any `\x1b` byte appear?

- If **no** ESC survives: ratatui neutralizes; proceed anyway (Step 2) — the
  sanitizer becomes cheap insurance against ratatui behavior changes and
  future non-ratatui sinks, and the test from this step becomes the
  regression lock. Record the verdict in the commit message.
- If **yes** ESC survives: this is a live injection vector; continue with
  urgency and note it in `plans/README.md`.

**Verify**: the test compiles and its assertion documents the observed
behavior (start by asserting the buffer contains no `\x1b`; if that fails,
you have the "yes" verdict — keep the failing assertion and make it pass via
Steps 2-3).

### Step 2: Add the sanitizer

`crates/tui/src/ui/sanitize.rs`:

```rust
/// Remove terminal-dangerous control characters from mail-controlled text.
/// Keeps \n and \t; drops other C0 controls, DEL, and C1 controls
/// (U+0080-U+009F). Legitimate printable content — wide chars, emoji,
/// combining marks — passes through untouched.
pub(crate) fn strip_control_chars(input: &str) -> Cow<'_, str> { ... }
```

Return `Cow::Borrowed` when nothing needs stripping (the overwhelmingly common
case — keep the render path allocation-free). Unit-test the helper directly:
ANSI CSI, OSC with BEL and ST terminators, bare ESC, C1 bytes, and a
passthrough case with CJK + emoji + tabs.

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → helper tests pass.

### Step 3: Apply at the render boundary

- `message_view.rs`: run each body line through `strip_control_chars` before
  `style_line_with_links` builds spans (one call site at the line-iteration
  level — find where lines are prepared, not per-span).
- `mail_list.rs`: apply to subject and sender display strings once, where the
  row cells are built.

**Verify**: `scripts/cargo-test -p mxr-tui --tests` → Step 1's test passes;
existing snapshot tests in `crates/tui/tests/snapshots.rs` unchanged (if a
snapshot changes, a legitimate character was stripped — STOP condition).

## Test plan

- Step 1's end-to-end render test (message with escape payloads → buffer
  contains no `\x1b`, visible text preserved).
- Step 2's unit tests for the helper (list above).
- A mail_list test: subject containing `\x1b[2J` renders with the sequence
  removed, rest of the subject intact.

## Done criteria

- [ ] `scripts/cargo-test -p mxr-tui --tests` exits 0, new tests present
- [ ] `cargo build -p mxr` exits 0
- [ ] Step 1 verdict (ratatui neutralizes: yes/no) recorded in the commit message and `plans/README.md`
- [ ] Existing snapshot tests unchanged
- [ ] `plans/README.md` status row updated

## STOP conditions

- Applying the sanitizer changes an existing snapshot — a legitimate
  character class is being stripped; report which test and character.
- The line-preparation call site in `message_view.rs` can't be found within
  the file (structure drifted) — report.
- Step 1 shows ESC survives AND also renders through some path other than the
  two in-scope files (e.g. help/preview panes) — report the extra paths, fix
  only the in-scope ones.

## Maintenance notes

- Any new UI surface rendering mail-controlled strings (new lens, new modal)
  must call `strip_control_chars` — add to review checklist.
- If mxr ever prints mail content outside ratatui (e.g. `--stdout` preview,
  clipboard integration), that sink needs the same treatment; the helper is
  deliberately dependency-free so it can move to a shared crate then.
