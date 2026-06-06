---
id: task-001
title: First-class agent surface and enforced permission model
status: accepted
phase: phase-001
depends_on: []
blocks:
  - task-003
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
    - security-best-practices
scope:
  allowed_paths:
    - Cargo.toml
    - Cargo.lock
    - crates/config/**
    - crates/protocol/**
    - crates/daemon/src/**
    - crates/daemon/tests/**
    - crates/store/**
    - crates/safety/**
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
    - scripts/cargo-test -p mxr --lib
    - scripts/cargo-test -p mxr --test activity_invariants
    - scripts/cargo-test -p mxr --test cli_journey
  success_criteria:
    - "IPC source identity has first-class Agent and MCP/Mcp variants, and activity logging preserves the origin without storing secrets/full bodies."
    - "Agents/MCP callers can be scoped by configured profile: safety policy, allowed accounts, and send/destructive capability."
    - "Account scoping is enforced in daemon request handling, not only by CLI convention."
    - "Agent/MCP send requires explicit approval or an equivalent first-class send gate; draft-only/read-only profiles cannot send."
    - "`MXR_ACTIVITY=off` still disables writes, and safety policy behavior remains covered by tests."
    - "Existing CLI/TUI/web behavior remains unchanged for human clients unless configured otherwise."
---

# First-class agent surface and enforced permission model

## Goal

Make agent use a first-class daemon concept, not a loose CLI convention.

## Product decisions to preserve

- Agent surface is v1.
- MCP is also v1, and will depend on this task.
- Local-first and no-telemetry remain non-negotiable.

## Expected shape

Implement the narrowest v1 permission model that solves the real gap:

- distinguish human/script/web/tui/daemon/agent/mcp origins in IPC and activity;
- support configured agent profiles with safety policy + account allowlist;
- enforce profile/account/safety in the daemon before provider mutations;
- send/destructive actions must require explicit approval or a clear configured capability, with tests proving blocked paths.

Do not invent remote auth or hosted control-plane behavior. This is local runtime authority, enforced before daemon handlers touch providers.

## Notes

Existing references:

- `crates/config/src/types.rs` has `SafetyPolicy`.
- `crates/daemon/src/handler/mod.rs` enforces safety policy.
- `docs/activity-log.md` documents `ClientKind` source tagging.
- `site/src/content/docs/guides/for-agents.md` currently says account scoping is only CLI convention; task 005 will update docs after this implementation.
