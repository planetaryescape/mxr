---
id: task-003
title: Document batch-operation reversibility and confirmation coverage
status: ready
phase: phase-001
depends_on: [task-014, task-015, task-016, task-017]
risk: { level: low, blast_radius: low }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  final_review_model: gpt-5.6-sol
  skills: [mxr-development, mxr, typescript-reviewer, simplify]
scope:
  allowed_paths:
    - site/src/content/docs/guides/automation-contract.md
    - docs/implementation/backlog-refresh-2026-08/**
validation:
  test_commands:
    - scripts/cargo-test -p mxr --test cli_help
    - npm run build --prefix site
    - git diff --check
---

# Batch reversibility matrix

Build one code-backed table for destructive/batch operations across CLI, TUI, and web: dry-run or preview, confirmation, undo support, and behavior after the 60-second undo window.

- Audit actual CLI commands/help, TUI bulk/send confirmation modals, and web mutation actions. Do not infer parity from docs.
- Verify the preview selection path matches the real mutation selection path.
- Distinguish reversible local state, provider-reversible mutations, and operations that cannot be undone after send/purge.
- Document gaps as scoped follow-up tasks. Do not fix product behavior inline.
- Use `/typescript-reviewer` when inspecting TypeScript action/type contracts and `/simplify` on the final table/diff.

Report evidence for every row, validation, false assumptions, failed approaches, learnings, and any follow-up task graph.
