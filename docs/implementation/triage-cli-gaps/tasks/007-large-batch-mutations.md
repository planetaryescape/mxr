---
id: task-007
title: Large-batch mutation chunking / async job surface across all clients (P1-6)
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
    - crates/daemon/src/handler/mutations.rs
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
    - scripts/cargo-test -p mxr --lib
    - scripts/cargo-test -p mxr-tui --tests
    - scripts/cargo-test -p mxr-web --tests
    - cd apps/web && npm ci && npm run typecheck && npm run test
  success_criteria:
    - "A --search batch mutation over 400-500+ messages completes without the 120s IPC timeout (repro: theaibreak 445, franceculture 482)."
    - "Implemented via daemon server-side chunking with progress, and/or an async mode returning a job id; a `jobs` capability polls/inspects."
    - "Clear partial-progress + failure semantics; no silent half-applied batch; undo id(s) surfaced for applied work."
    - "SURFACE PARITY (AGENTS.md): the jobs/progress surface exposed on CLI (`mxr jobs`), TUI (progress/jobs panel), and web (crates/web jobs endpoint + apps/web jobs view). Not CLI-only."
    - "Tests cover a large synthetic batch and the async job lifecycle."
---

# Large-batch mutation chunking / async job surface across all clients

## Goal

Stop big footprint sweeps from timing out (445-482 message archives hit the 120s IPC timeout,
leaving ambiguous state and no undo id). The jobs/progress surface must be visible in every client.

## Work

- Daemon: chunk large `--search` mutations server-side with progress, and/or `--async` returning
  a job id; clear partial-progress + failure semantics; always surface undo id(s).
- Protocol: shared job + progress types.
- CLI: `mxr jobs` poll/inspect.
- TUI: a jobs/progress panel in `crates/tui`.
- Web: jobs endpoint in `crates/web` + jobs view in `apps/web`.
- Tests for a large synthetic batch + async lifecycle.

## Out of scope

- The `--purge` UX (task-005) — but expose batching so task-005 consumes it.

## Source

docs/triage-session-feedback-2026-06-03.md — P1-6. Run before task-005 if both scheduled (shared batch path; no parallel workers).
