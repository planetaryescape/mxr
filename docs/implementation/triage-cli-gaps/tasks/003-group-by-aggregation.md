---
id: task-003
title: Query-scoped sender aggregation across all clients (P0-2)
status: ready
phase: phase-002
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
    - "Daemon computes aggregation server-side over a query result set: per group count, unread, oldest, newest. Supports `from` (ideally also `list`, `category`)."
    - "Reuses existing aggregation (subscriptions --rank / storage --by sender) rather than duplicating it; protocol type shared by all clients."
    - "SURFACE PARITY (AGENTS.md): exposed on CLI (`search/count --group-by from`, table/json/jsonl/csv), TUI (grouped/sender-rollup view), and web (crates/web endpoint + apps/web grouped view). Not CLI-only."
    - "Works for any query (label:, category:, etc.), not just newsletters."
---

# Query-scoped sender aggregation across all clients

## Goal

Give the "survey, don't read in order" step a first-class, cross-client surface. The triage agent
hand-rolled sender tallies ~4x because no client can group an arbitrary query by sender.

## Work

- Daemon: `--group-by <field>` aggregation over the query result set (count/unread/oldest/newest).
  Factor out / reuse the `subscriptions --rank` / `storage --by sender` machinery.
- Protocol: shared aggregation request/response.
- CLI: `search --group-by` / `count --group-by` (table/json/jsonl/csv).
- TUI: a grouped/sender-rollup view in `crates/tui`.
- Web: endpoint in `crates/web` + grouped view in `apps/web`.

## Out of scope

- Raw-result pagination (task-008); per-group triage verdicts (task-002).

## Source

docs/triage-session-feedback-2026-06-03.md — P0-2.
