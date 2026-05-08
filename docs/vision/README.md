# mxr — Vision & Delight Plan

This directory holds the long-arc product plan for taking mxr from "well-built operator tool" to "the email client people are delighted to use."

## Index

| File | Purpose |
|------|---------|
| [01-delight-plan.md](./01-delight-plan.md) | The canonical plan. Thesis, methodology (TDD + 5-question quality gate), 17 features across 4 phases, schema migrations, IPC additions, CLI surface, decisions log. |
| [phase-1-feel.md](./phase-1-feel.md) | Tracker for Phase 1 (Make it feel right) — optimistic mutations, command palette, inbox row richness, type-ahead search, saved-search tabs. |
| [phase-2-triage.md](./phase-2-triage.md) | Tracker for Phase 2 (Triage that scales) — reply-later walk, custom snooze, auto-reminders, send-later, screener, bulk unsubscribe. |
| [phase-3-sender-as-unit.md](./phase-3-sender-as-unit.md) | Tracker for Phase 3 (the unique bet) — snippets, sender view, LLM provider trait, thread summarize, draft assist. |
| [phase-4-onboarding.md](./phase-4-onboarding.md) | Tracker for Phase 4 (Onboarding & resilience) — crash-safe drafts, doctor 2.0, setup wizard. |

## Status

- **Plan version**: 1.0 (2026-05-07)
- **Approved**: yes
- **Active phase**: not started

Trackers carry the per-feature checklist. Update them as features land. The plan itself is the source of truth for design intent; trackers are the source of truth for what's actually shipped.

## Methodology reminder

Every behavior change goes RED → GREEN → REFACTOR with vertical tracer bullets (one test, one impl, repeat). No horizontal slicing. Run the [5-question test quality gate](./01-delight-plan.md#test-quality-gate-mandatory-per-test) before writing any implementation for a new test. Tests verify behavior through public APIs and must survive an implementation swap.
