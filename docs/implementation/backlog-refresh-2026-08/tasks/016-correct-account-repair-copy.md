---
id: task-016
title: Correct accounts repair keychain claims
status: ready
phase: issue-216-followups
depends_on: []
risk: { level: low, blast_radius: low }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  final_review_model: gpt-5.6-sol
  skills: [mxr-development, simplify]
scope:
  allowed_paths:
    - crates/daemon/src/cli/**
    - crates/daemon/tests/**
    - site/public/openapi.json
    - site/src/content/docs/**
    - docs/implementation/backlog-refresh-2026-08/**
validation:
  test_commands:
    - scripts/cargo-test -p mxr-daemon --test accounts_repair_cli
    - scripts/cargo-test -p mxr --test cli_help
    - git diff --check
---

# Correct account repair copy

Make help, snapshots, OpenAPI, and directly related docs describe current behavior: credentials are repaired into the permission-restricted local secrets file, with keychain use only where the existing runtime actually mirrors or falls back.

- Do not redesign credential storage; current runtime tests disprove the issue's stale keychain-authoritative assumption.
- Do not claim stronger protection than the implementation provides.
- Keep wording concrete and consistent across generated/user-facing surfaces.
- Use `/simplify` on the final diff. `/typescript-reviewer` applies only if a TypeScript API/type surface is touched.

Report evidence, validation, false assumptions, failed approaches, learnings, and downstream implications.
