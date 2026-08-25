---
id: task-015
title: Make IMAP folder sync concurrency configurable per account
status: ready
phase: issue-216-followups
depends_on: []
risk: { level: medium, blast_radius: medium }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  final_review_model: gpt-5.6-sol
  skills: [mxr-development, simplify]
scope:
  allowed_paths:
    - crates/provider-imap/**
    - crates/config/**
    - crates/daemon/**
    - site/src/content/docs/reference/config.md
    - site/public/openapi.json
    - docs/implementation/backlog-refresh-2026-08/**
validation:
  test_commands:
    - scripts/cargo-test -p mxr-provider-imap --tests
    - scripts/cargo-test -p mxr-config --tests
    - cargo build -p mxr
---

# Configure IMAP concurrency

Replace the compile-time-only folder sync cap with an account-level setting while preserving the current default of four.

- Use the existing provider config pattern; provider-specific behavior stays in `mxr-provider-imap`.
- Enforce a minimum of one and avoid an unbounded value. Match existing config validation rather than adding a second mechanism.
- Apply the same selected value to initial and delta folder sync paths.
- Add tests that prove default, configured, and invalid/boundary behavior.
- Update only the config reference/OpenAPI surfaces that expose this field.
- Use `/simplify` on the final diff. `/typescript-reviewer` applies only if a TypeScript API/type surface is touched.

Report evidence, validation, false assumptions, failed approaches, learnings, and downstream implications.
