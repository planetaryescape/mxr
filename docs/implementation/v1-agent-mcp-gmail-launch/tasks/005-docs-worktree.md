---
id: task-005
title: V1 documentation, OAuth guidance, unsigned macOS policy, and worktree cleanup
status: accepted
phase: phase-004
depends_on:
  - task-001
  - task-002
  - task-003
  - task-004
blocks: []
risk:
  level: medium
  blast_radius: medium
execution:
  executor_type: frontier_model
  lane: frontier
  preferred_model: pe-default-frontier
  skills:
    - documentation-refiner
    - writing-docs
    - readme-optimizer
    - mxr
scope:
  allowed_paths:
    - README.md
    - TODO.md
    - SECURITY.md
    - PRIVACY.md
    - docs/**
    - site/src/content/docs/**
    - site/src/pages/**
    - site/public/openapi.json
    - scripts/**
    - .github/workflows/**
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
    - cd site && npm ci && npm run build
  success_criteria:
    - "README and site describe MCP and agent surface as first-class v1, not roadmap."
    - "Gmail setup docs make user-created OAuth clients the official advice; bundled OAuth is documented as unverified fallback only."
    - "Docs explain Gmail-over-IMAP All Mail behavior and what changed for archived mail."
    - "Agent docs describe enforced profiles, account scoping, send approval/gating, activity origins, and MCP usage."
    - "Release docs state unsigned macOS binaries are accepted for v1 and describe Gatekeeper friction honestly."
    - "`SECURITY.md`, privacy/terms pages, blueprint docs, TODO, and docs site are consistent with implemented truth."
    - "Existing dirty docs/site changes are preserved or intentionally reconciled; no unrelated revert."
    - "OpenAPI/CLI generated docs are regenerated only if needed and committed consistently."
---

# V1 docs and worktree hygiene

## Goal

Make the repo and docs tell the same story as v1 product truth.

## User decisions to encode

- MCP and agent surface are first-class.
- Official Gmail OAuth advice: users create their own OAuth client. The bundled client exists but is not verified and should not be the primary recommendation.
- V1 accepts unsigned macOS binaries.
- Docs must reflect implemented agent permission model and MCP command/tooling.

## Worktree caution

There were already dirty docs/site files before this plan. Do not revert them blindly. Inspect diffs and either preserve, reconcile, or explicitly document why a change is superseded.

Primary dirty worktree to inspect as context:

- `/Users/bhekanik/code/planetaryescape/mxr`

Task 005 implementation worktree:

- `/Users/bhekanik/code/planetaryescape/.pe-tasker-worktrees/mxr/v1-agent-mcp-gmail-launch/task-005`

The implementation source of truth is the clean integration branch in the task 005 worktree. Inspect the primary worktree's dirty docs/site/privacy/terms diffs before editing, but do not mutate the primary worktree and do not copy unrelated app/compose/TUI/code changes into this branch.
