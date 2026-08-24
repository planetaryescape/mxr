---
id: task-002
title: Reconcile stale addendum follow-ups
status: ready
phase: phase-000
depends_on: []
risk: { level: low, blast_radius: low }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  skills: [mxr-development, simplify]
scope:
  allowed_paths: [docs/blueprint/16-addendum.md]
validation:
  test_commands: [git diff --check]
---

# Reconcile stale addendum follow-ups

- Mark IMAP IDLE and Reply-To preference as shipped with code references.
- Correct the Gmail provider-thread note: native threading is already recovered through provider lookup and cached after draft push. Do not propose an Envelope schema migration without failure/cost evidence.
- Keep `--format-version` exit codes explicitly deferred.
- Use `/simplify` as a final scoped prose/diff pass. `/typescript-reviewer` does not apply.

Report changed lines, validation, false assumptions, failed checks, learnings, and downstream implications.
