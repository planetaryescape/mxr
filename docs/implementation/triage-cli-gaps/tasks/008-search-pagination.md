---
id: task-008
title: Search result pagination / lift discovery ceiling across all clients (P1-7)
status: pending
phase: phase-003
depends_on: []
blocks: []
risk:
  level: medium
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
    - crates/search/**
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
    - scripts/cargo-test -p mxr-search --tests
    - scripts/cargo-test -p mxr --lib
    - scripts/cargo-test -p mxr-tui --tests
    - scripts/cargo-test -p mxr-web --tests
    - cd apps/web && npm ci && npm run typecheck && npm run test
  success_criteria:
    - "Full-inbox discovery no longer silently truncates: a large limit returns all matches (repro: `search --limit 1080` returned ~755)."
    - "Daemon/search supports `--offset` or a cursor for paging; protocol type shared by clients; behaviour documented."
    - "If a hard cap is retained, it is explicit and truncation is signalled in output, not silent."
    - "SURFACE PARITY (AGENTS.md): paging exposed on CLI (`--limit`/`--offset`), TUI (paged/lazy list in crates/tui), and web (apps/web paging/infinite scroll over the crates/web endpoint). Not CLI-only."
    - "Tests cover large-limit and offset/cursor paging."
---

# Search result pagination / lift discovery ceiling across all clients

## Goal

Make full-inbox discovery reliable everywhere. `search --limit 1080` returned only ~755, silently
truncating the survey the whole triage method depends on; clients need consistent paging.

## Work

- Daemon/search: honor large `--limit`, or add `--offset`/cursor paging; explicit + signalled cap
  if one is kept. (Server-side `--search` mutations are unaffected — discovery/listing only.)
- Protocol: shared paging params.
- CLI: `--limit`/`--offset`.
- TUI: paged/lazy message list in `crates/tui`.
- Web: paging/infinite scroll in `apps/web` over the `crates/web` endpoint.
- Tests for large-limit + paged retrieval.

## Out of scope

- Aggregation (task-003).

## Source

docs/triage-session-feedback-2026-06-03.md — P1-7.
