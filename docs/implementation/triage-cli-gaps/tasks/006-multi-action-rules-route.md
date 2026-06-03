---
id: task-006
title: Multi-action rules + atomic queue route across all clients (P1-5)
status: pending
phase: phase-003
depends_on: []
blocks: []
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
    - crates/rules/**
    - crates/daemon/src/handler/**
    - crates/daemon/src/commands/**
    - crates/daemon/src/cli/**
    - crates/protocol/**
    - crates/tui/**
    - crates/web/**
    - apps/web/**
    - docs/implementation/triage-cli-gaps/**
  blocked_paths:
    - .env*
    - node_modules/**
    - .git/**
    - target/**
    - "*.db"
validation:
  test_commands:
    - cargo build -p mxr
    - scripts/cargo-test -p mxr-rules --tests
    - scripts/cargo-test -p mxr --lib
    - scripts/cargo-test -p mxr-tui --tests
    - scripts/cargo-test -p mxr-web --tests
    - cd apps/web && npm ci && npm run typecheck && npm run test
  success_criteria:
    - "Rules accept chained actions, e.g. `mark-read,archive` (currently rejected as Unsupported action); ordering defined + validated; rules dry-run renders the full sequence."
    - "Daemon `route` capability performs label + unlabel + read-archive as one atomic mutation, single undo id, with dry-run (home / Notto / Follow Up pattern)."
    - "SURFACE PARITY (AGENTS.md): chained-action rules editor AND the route action exposed on CLI (`mxr route ...`), TUI (rules editor + route action), and web (crates/web + apps/web rules UI + route action). Not CLI-only."
    - "Tests cover multi-action rule execution and route atomicity + dry-run parity across the daemon surface."
---

# Multi-action rules + atomic queue route across all clients

## Goal

(1) Rules allow only ONE action, so auto-archive rules cannot also mark-read; (2) queue routing
(home/Notto/Follow Up) took three CLI calls. Both fixes must reach every client.

## Work

- Rules engine: ordered list of supported actions (`archive`, `mark-read`, `trash`, label ops),
  deterministic ordering + validation; dry-run renders the full sequence.
- Daemon `route` mutation: apply target label, remove queue label, optionally read-archive — one
  atomic op, single undo id, dry-run.
- Protocol: shared route + multi-action rule types.
- CLI: `mxr route --to <label> --from-queue <label> [--archive]`; rules accept chained actions.
- TUI: rules editor supports chained actions; a route action in `crates/tui`.
- Web: rules UI + route action in `crates/web` + `apps/web`.

## Out of scope

- New rule condition types.

## Source

docs/triage-session-feedback-2026-06-03.md — P1-5.
