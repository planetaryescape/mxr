---
id: task-010
title: Document subscriptions --rank opened_count semantics (P2-11)
status: pending
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
  reasoning_effort: low # intent only — docs clarification; PE Tasker has no per-task effort flag yet, so default effort runs unless/until one exists
  skills:
    - documentation-refiner
    - mxr
scope:
  allowed_paths:
    - site/src/content/docs/**
    - crates/daemon/src/handler/**
    - crates/relationship/**
    - docs/implementation/triage-cli-gaps/**
  blocked_paths:
    - .env*
    - node_modules/**
    - .git/**
    - target/**
    - site/src/content/docs/reference/cli/**
validation:
  test_commands:
    - cargo build -p mxr
  success_criteria:
    - "The meaning of `opened_count` and the stable-zero `replied_count` field in `subscriptions --rank` is documented accurately, verified against the code that emits them."
    - "Docs explain why opened_count can equal message_count for some senders (the field session found this ambiguous when judging engagement)."
    - "Docs land in the user-facing site content (site/src/content/docs/), NOT the generated CLI reference under reference/cli/ (that is auto-generated)."
    - "Follows docs/guides/writing-docs.md conventions."
---

# Document subscriptions --rank opened_count semantics

## Goal

`subscriptions --rank` was the MVP of the field session, but `opened_count` semantics were
unclear (it equalled message_count for several senders), making engagement-based keep/cut
calls fuzzy. Document what it actually measures.

## Work

- Read the code that computes `opened_count` and emits `replied_count` (start in the daemon
  subscriptions handler and the store query). Establish ground truth.
- Document the metric (proxied pixel opens? distinct opens? counted how?) in the user-facing docs,
  including the equal-to-message-count case.
- Respect writing-docs conventions; do not hand-edit the generated CLI reference.

## Out of scope

- Changing how the metric is computed.

## Source

docs/triage-session-feedback-2026-06-03.md — P2-11.
