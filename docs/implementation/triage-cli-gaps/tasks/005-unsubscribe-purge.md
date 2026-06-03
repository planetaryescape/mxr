---
id: task-005
title: One-shot unsubscribe + footprint clear across all clients (P0-4)
status: ready
phase: phase-002
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
    - crates/core/src/**
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
    - "Daemon exposes an unsubscribe-and-purge capability: unsubscribe + read-archive the sender's whole footprint as one logical, undoable op (single undo id)."
    - "Has a dry-run/preview using the SAME selection path as the real mutation (AGENTS.md); shows method + message count before acting."
    - "No usable List-Unsubscribe method -> reported clearly; still offers archive (or aborts per flag); never silent-fails."
    - "Destructive guardrails: read+archive not delete, reversible, undo id surfaced."
    - "SURFACE PARITY (AGENTS.md): exposed on CLI (`unsubscribe <addr> --purge`), TUI (an unsubscribe-and-clear action), and web (crates/web endpoint + apps/web action). Not CLI-only."
    - "Coordinate with task-007: large footprints must not hit the 120s IPC timeout (consume its chunking/async if landed)."
---

# One-shot unsubscribe + footprint clear across all clients

## Goal

Collapse the most-repeated triage sequence (unsubscribe, then `read-archive --search from:<addr>`)
into one undoable capability, exposed in every client. Used ~13x in the field session.

## Work

- Daemon: unsubscribe + read-archive full `from:<addr>` footprint as one undoable op; dry-run via
  the identical selection path; surface method (or None) + count up front.
- Protocol: shared purge request/response.
- CLI: `unsubscribe <addr> --purge` (or `--archive-all`).
- TUI: an "unsubscribe & clear sender" action in `crates/tui`.
- Web: endpoint in `crates/web` + action in `apps/web`.
- Keep archive-not-delete + undo-id guarantees across all surfaces.

## Out of scope

- General batch chunking/async (task-007) — consume it if available to avoid timeouts.

## Source

docs/triage-session-feedback-2026-06-03.md — P0-4. Shares the batch path with task-007 — no parallel workers.
