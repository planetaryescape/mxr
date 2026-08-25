---
id: task-004
title: Decide whether existing history surfaces satisfy action history
status: ready
phase: phase-001
depends_on: [task-014, task-015, task-016, task-017]
risk: { level: low, blast_radius: low }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  final_review_model: gpt-5.6-sol
  skills: [mxr-development, typescript-reviewer, simplify]
scope:
  allowed_paths:
    - docs/blueprint/15-decision-log.md
    - docs/implementation/backlog-refresh-2026-08/**
validation:
  test_commands:
    - git diff --check
---

# Decide action history

Compare `mxr history`, `mxr activity`, the web `/activity` route, and the observability guide against the requested user-visible, per-mutation, undo-linked history.

- Verify current behavior in code and CLI help; docs are leads, not proof.
- If existing surfaces satisfy the need, add one concise evidence-backed decision to the decision log.
- If a concrete gap remains, write a scoped task with affected surfaces and acceptance criteria. Do not implement it here.
- Use `/typescript-reviewer` for any inspected web type/API contract and `/simplify` on the decision/diff.

Report evidence, validation, false assumptions, failed approaches, learnings, and downstream implications.
