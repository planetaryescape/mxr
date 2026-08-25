---
id: task-001
title: Refresh TODO.md against current code truth
status: accepted
phase: phase-000
depends_on: []
risk: { level: low, blast_radius: low }
execution:
  executor_type: frontier_model
  preferred_model: gpt-5.5
  skills: [mxr-development, simplify]
scope:
  allowed_paths: [TODO.md]
validation:
  test_commands: [git diff --check]
---

# Refresh TODO.md

- Update the triage date/release.
- Mark the stale security wording audit, exact provider-claim decision, and release-assets item complete with current file evidence. Keep history; do not delete items.
- Do not mark live IMAP/SMTP proof complete. `unavailable_no_live_smoke` remains.
- Add a concise deferred-follow-ups index for the still-open T1/T2/T3 items, pointing at this plan.
- Use `/simplify` as a final scoped prose/diff pass. `/typescript-reviewer` does not apply.

Report changed lines, validation, false assumptions, failed checks, learnings, and downstream implications.
