---
id: task-009
title: CLI output polish bundle - count plain, dry-run parity, unsub preflight (P2-8,9,10)
status: accepted
phase: phase-004
depends_on: []
blocks: []
risk:
  level: low
  blast_radius: low
execution:
  executor_type: frontier_model
  lane: frontier
  preferred_model: pe-default-frontier
  reasoning_effort: low # intent only — low-complexity bundle; PE Tasker has no per-task effort flag yet, so default effort runs unless/until one exists
  skills:
    - build-and-fix
    - code-review
    - mxr
scope:
  allowed_paths:
    - crates/daemon/src/commands/**
    - crates/daemon/src/cli/**
    - crates/daemon/src/handler/**
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
  success_criteria:
    - "P2-8: `mxr count <query> --format plain` (or --quiet) prints a bare integer for easy scripting."
    - "P2-9: mutation dry-run and apply report counts consistently, distinguishing 'N threads / M messages affected' so deltas are not confusing (repro: dry-run 7 vs applied 5 via thread collapse)."
    - "P2-10: `mxr unsubscribe --dry-run <addr>` reports the resolved unsubscribe method or `None` before acting (repro: MedExpress, blockchain.com had no List-Unsubscribe)."
    - "SURFACE PARITY: P2-8 (count --format plain) is a CLI-only output mode — no client surface. P2-9 (thread-vs-message counts) and P2-10 (unsubscribe method preflight) are daemon-level: once the daemon returns these, TUI/web dry-run/preview displays should reflect them too (minor display wiring, not CLI-only)."
    - "Each sub-item has a focused test; changes are additive and do not alter existing default output contracts."
---

# CLI output polish bundle

## Goal

Three small, independent CLI papercuts from the field session, grouped because they all touch
command output/preview and are individually too small to schedule alone.

## Work

- **P2-8** Add a bare-integer output mode to `count` (`--format plain` or `--quiet`).
- **P2-9** Make dry-run vs apply counts consistent and label threads vs messages affected.
- **P2-10** Add method preflight to `unsubscribe --dry-run <addr>` (report method or None).
- One focused test per sub-item.

## Out of scope

- The `--purge` flow (task-005) and async batching (task-007).

## Source

docs/triage-session-feedback-2026-06-03.md — P2-8, P2-9, P2-10.
