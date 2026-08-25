---
id: task-005
title: Remove the UDS stop_accepting test race
status: ready
phase: phase-001
depends_on: [task-014, task-015, task-016, task-017]
risk: { level: low, blast_radius: low }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  final_review_model: gpt-5.6-sol
  skills: [mxr-development, rust-async-patterns, typescript-reviewer, simplify]
scope:
  allowed_paths:
    - crates/transport/src/uds.rs
    - crates/transport/tests/**
    - docs/blueprint/20-transports.md
    - docs/implementation/backlog-refresh-2026-08/**
validation:
  test_commands:
    - scripts/cargo-test -p mxr-transport --tests
    - cargo clippy -p mxr-transport --all-targets -- -D warnings
    - cargo build -p mxr
---

# Fix UDS stop_accepting race

Replace the single-attempt connection-refused assertion with deterministic, bounded synchronization that proves new connections are refused while socket unlink remains deferred.

- Observe the current failure path before choosing retry/polling mechanics.
- Keep the retry short and bounded. Assert both listener shutdown and path-lifetime semantics.
- Run the focused suite repeatedly and under test-thread pressure.
- Remove only the resolved follow-up from the transport blueprint.
- Use `/typescript-reviewer` scope gating; it should be skipped for a Rust-only diff. Use `/simplify` on the final code/diff.

Report reproduction evidence, validation counts, false assumptions, failed approaches, learnings, and downstream implications.
