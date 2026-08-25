---
id: task-017
title: Make daemon logging.level effective without exposing wire data
status: blocked
phase: issue-216-followups
depends_on: [task-014]
risk: { level: medium, blast_radius: medium }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  final_review_model: gpt-5.6-sol
  skills: [mxr-development, security-best-practices, simplify]
scope:
  allowed_paths:
    - crates/config/**
    - crates/daemon/**
    - site/src/content/docs/reference/config.md
    - site/src/content/docs/troubleshooting.md
    - docs/implementation/backlog-refresh-2026-08/**
validation:
  test_commands:
    - scripts/cargo-test -p mxr-config --tests
    - scripts/cargo-test -p mxr-daemon --tests
    - cargo build -p mxr
---

# Wire logging level safely

Make configured `logging.level` affect daemon logging while preserving the task-014 hard boundary against raw IMAP protocol bytes.

- Define and document precedence between `RUST_LOG` and `logging.level` from existing patterns.
- Keep the configured level scoped to safe mxr targets unless evidence supports a broader filter contract.
- Add tests for defaults, configured levels, environment override, invalid values, and the IMAP trace boundary.
- Do not start until task 014 is deployed and install-verified.
- Use `/simplify` on the final diff. `/typescript-reviewer` does not apply unless a TypeScript API/type surface is touched.

Report evidence, validation, false assumptions, failed approaches, learnings, and downstream implications.
