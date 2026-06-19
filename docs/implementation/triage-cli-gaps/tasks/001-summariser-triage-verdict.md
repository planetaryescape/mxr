---
id: task-001
title: Summariser emits strict triage-verdict first line (Part 2)
status: accepted
phase: phase-001
depends_on: []
blocks:
  - task-002
risk:
  level: medium
  blast_radius: low
execution:
  executor_type: frontier_model
  lane: frontier
  preferred_model: pe-default-frontier
  skills:
    - build-and-fix
    - code-review
    - mxr
scope:
  allowed_paths:
    - crates/daemon/src/handler/summarize.rs
    - crates/daemon/src/commands/summarize.rs
    - crates/llm/**
    - crates/config/src/**
    - crates/tui/src/ui/summary_modal.rs
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
    - scripts/cargo-test -p mxr-llm --tests
  success_criteria:
    - "The summariser prompt is extended (append-only) so every summary's FIRST line is a triage verdict beginning with exactly one of: 'ACTION REQUIRED — ', 'FYI — ', 'ROUTINE — '."
    - "Verdict line is a single token + em dash + one clause, machine-parseable, no hedging, with deadlines surfaced as (by YYYY-MM-DD)."
    - "Tie-breaker rules from the field report are encoded (uncertain->ACTION; security/money/legal/deadline/awaited-reply->ACTION; marketing action-verbs->ROUTINE)."
    - "Calibration examples from the field report produce the expected verdict (manual or snapshot check)."
    - "Existing summary body content is preserved below the verdict line."
    - "SURFACE PARITY: the prompt is daemon-side, so the verdict propagates to every client that shows summaries. Verify the first line is NOT truncated/clipped in the TUI summary modal (crates/tui/src/ui/summary_modal.rs) or the apps/web summary view; fix display if it is."
---

# Summariser emits strict triage-verdict first line

## Goal

Make every email summary lead with an unambiguous triage classification before topic, so a
human (or a `mxr triage` view, task-002) can decide what to do with the mail from line one.

## Work

- Locate the summariser system/instruction prompt (start at `crates/daemon/src/handler/summarize.rs`,
  `crates/daemon/src/commands/summarize.rs`, `crates/llm/`). Confirm where prompt text is assembled.
- Append the exact OUTPUT-FORMAT + TIE-BREAKER snippet from the field report (Part 2).
- Keep it append-only / additive to the existing prompt; do not rewrite the summariser's body behaviour.
- If prompts are user-configurable in `config`, ensure the triage instruction is on by default but documented.
- Add a test (snapshot or assertion against a fake/demo LLM) that the first line matches `^(ACTION REQUIRED|FYI|ROUTINE) — `.

## Out of scope

- No new CLI command (that is task-002).
- No caching/storage of summaries (task-002).

## Source

docs/triage-session-feedback-2026-06-03.md — Part 2. Foundational: unblocks P0-1 (task-002).
