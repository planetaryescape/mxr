---
id: task-002
title: Gmail-over-IMAP All Mail sync without archived-mail gaps or duplicates
status: accepted
phase: phase-001
depends_on: []
blocks:
  - task-004
  - task-005
risk:
  level: high
  blast_radius: high
execution:
  executor_type: frontier_model
  lane: frontier
  preferred_model: pe-default-frontier
  skills:
    - build-and-fix
    - code-review
    - tdd
    - mxr
scope:
  allowed_paths:
    - Cargo.toml
    - Cargo.lock
    - crates/provider-imap/**
    - crates/core/**
    - crates/sync/**
    - crates/store/**
    - crates/test-support/**
    - docs/implementation/v1-agent-mcp-gmail-launch/**
  blocked_paths:
    - .env*
    - node_modules/**
    - .git/**
    - target/**
    - "*.db"
validation:
  test_commands:
    - cargo build -p mxr
    - scripts/cargo-test -p mxr-provider-imap --tests
    - scripts/cargo-test -p mxr-sync --tests
    - scripts/cargo-test -p mxr --test cli_journey
  success_criteria:
    - "IMAP servers advertising X-GM-EXT-1 use a Gmail-specific sync path that includes archived-only mail."
    - "Gmail-over-IMAP does not duplicate rows by syncing the same logical Gmail message through multiple folders."
    - "X-GM-LABELS are mapped into mxr label_provider_ids so INBOX/SENT/STARRED/user labels remain visible."
    - "Initial Gmail All Mail sync is paginated/bounded-memory and resumable through existing has_more/backfill cursor mechanics."
    - "Non-Gmail IMAP servers keep the existing folder sync behavior."
    - "Tests cover Gmail All Mail selection, label mapping, duplicate avoidance, pagination/resume, and fallback for servers without X-GM-EXT-1."
    - "Beta users upgrading from old per-folder Gmail IMAP cursors do not skip archived-only messages; the first All Mail run must not trust an arbitrary non-All-Mail cursor."
---

# Gmail-over-IMAP All Mail sync

## Goal

Fix the correctness bug from GitHub issue #50: Gmail-over-IMAP currently risks missing archived-only mail unless All Mail is synced, but naive All Mail plus per-folder sync creates duplicates.

## Expected approach

Use Gmail IMAP extensions when available:

- detect `X-GM-EXT-1`;
- sync Gmail All Mail as the canonical source for Gmail-over-IMAP;
- fetch Gmail labels/message metadata through `X-GM-LABELS` and related FETCH data;
- keep non-Gmail IMAP behavior unchanged;
- make initial backfill paginated and resumable.

Research the current `mxr-async-imap` API before coding. If needed, bump to a published version with Gmail FETCH accessors or use the least invasive parser-supported path. Do not fork or vendor unless there is no viable dependency path.

## Source

- GitHub issue #50: Gmail archived mail without duplicates via X-GM-LABELS.
- Existing provider code: `crates/provider-imap/src/lib.rs`, `folders.rs`, `parse.rs`, `session.rs`.

## Host review feedback after first worker pass

The first worker pass fixed initial Gmail All Mail sync, pagination, label mapping, and non-Gmail fallback. Host validation passed:

- `git diff --check`
- `scripts/cargo-test -p mxr-provider-imap --tests`
- `scripts/cargo-test -p mxr-sync --tests`
- `cargo build -p mxr`

But the delta Gmail path needs one more migration guard before acceptance:

- `delta_gmail_all_mail_sync` currently falls back to `old_mailboxes.first()` when there is no cursor for the All Mail mailbox.
- Existing beta users can have old per-folder IMAP cursors (`INBOX`, `Sent`, custom folders) before the Gmail All Mail path exists.
- The first Gmail All Mail delta after upgrade must not use an arbitrary folder cursor as if it represented All Mail. If no exact All Mail cursor is present, force a Gmail All Mail canonical backfill/full sync path so archived-only mail cannot be skipped.
- Add focused tests for this upgrade case. Non-Gmail delta behavior should remain unchanged.
