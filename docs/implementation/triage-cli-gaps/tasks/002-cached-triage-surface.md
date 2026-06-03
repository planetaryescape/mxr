---
id: task-002
title: Surface a cached triage signal across all clients (P0-1)
status: pending
phase: phase-002
depends_on:
  - task-001
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
    - crates/daemon/src/handler/**
    - crates/daemon/src/commands/**
    - crates/daemon/src/cli/**
    - crates/protocol/**
    - crates/store/**
    - crates/llm/**
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
    - "Daemon handler returns per-message triage verdicts for a search query, reusing the task-001 summariser first line (parsed)."
    - "Verdicts are CACHED in store keyed by message id + summariser/prompt version; re-runs do not re-call the LLM; cache invalidates on message/prompt change."
    - "SURFACE PARITY (AGENTS.md): exposed on all clients off the one daemon capability — CLI (`mxr triage` / `search --triage`, table/json/jsonl/ids), TUI (a triage view/column in crates/tui), and web (crates/web endpoint + apps/web UI). Not CLI-only."
    - "Verdict token is greppable (^ACTION/^FYI/^ROUTINE) and sortable in CLI; TUI/web can sort/filter by verdict."
    - "Metered-LLM safety: only un-cached messages call the model; --limit cap respected; run shows count of LLM calls about to fire."
---

# Surface a cached triage signal across all clients

## Goal

Turn "read 200 bodies to classify" into "scan 200 triage lines" — the biggest time sink in the
field report. Per the surface-parity invariant this lands in every client, not just the CLI.

## Work

- Daemon handler: for a search query, return per-message triage verdicts, reusing task-001's
  strict first line. Cache in `store` keyed by message id + summariser/prompt version.
- Protocol: add the triage request/response type (shared by all clients).
- CLI: `mxr triage` (or `search --triage`) with table/json/jsonl/ids.
- TUI: a triage view/column in `crates/tui`, sortable/filterable by verdict.
- Web: endpoint in `crates/web` + UI in `apps/web` to show and sort by verdict.
- Guard metered LLM use (only un-cached call the model; print call count; respect --limit).

## Out of scope

- The verdict format itself (task-001).

## Source

docs/triage-session-feedback-2026-06-03.md — P0-1. Depends on task-001's verdict format.
