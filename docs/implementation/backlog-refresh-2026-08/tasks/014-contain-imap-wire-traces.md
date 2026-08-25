---
id: task-014
title: Prevent IMAP wire traces from logging credentials or message bodies
status: ready
phase: issue-216-followups
depends_on: []
blocks: [task-017]
risk: { level: high, blast_radius: medium }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  final_review_model: gpt-5.6-sol
  skills: [mxr-development, security-best-practices, simplify]
scope:
  allowed_paths:
    - crates/daemon/**
    - crates/provider-imap/**
    - Cargo.toml
    - Cargo.lock
    - docs/implementation/backlog-refresh-2026-08/**
validation:
  test_commands:
    - scripts/cargo-test -p mxr-daemon --tests
    - scripts/cargo-test -p mxr-provider-imap --tests
    - cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    - cargo build -p mxr
---

# Contain IMAP wire traces

Guarantee that raw IMAP protocol bytes cannot reach mxr logs, even when `RUST_LOG` requests `async_imap=trace`.

- Cover LOGIN passwords, AUTHENTICATE challenge responses, and incoming message bodies.
- Prefer a published dependency fix. Do not restore a vendored/path dependency. If publishing the fork is unavailable, add a narrow application-level hard filter for the unsafe dependency target.
- Preserve safe operational logs needed for diagnosing connection and parser failures.
- Add a regression test with secret/body sentinels and prove they are absent from captured output.
- Use `/simplify` on the final diff. `/typescript-reviewer` does not apply to a pure Rust change.

Report evidence, validation, false assumptions, failed approaches, learnings, and implications for task 017.
