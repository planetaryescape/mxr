# Web E2E Test Suite (Playwright)

This directory contains Playwright end-to-end tests for the mxr web application.

## Test Pyramid & Invariants

1. **Vitest first**: Unit and component tests in `apps/web/src/**/*.test.tsx` are the default test surface. Component state, keypress reducers, modal forms, date parsing, and visual formatters run in jsdom in milliseconds.
2. **Playwright for full-stack integration only**: Playwright tests run against a real running `mxr` daemon (started by `scripts/e2e-server.mjs`) with SQLite storage. Write E2E tests only for:
   - Full user journeys that cross daemon boundaries (e.g., triage action mutating backend state, waking up a snoozed email, sending compose drafts).
   - Real browser engine rendering differences (Chromium vs WebKit / Safari).
3. **What NOT to put in E2E**:
   - Date parsing permutations or string formatters (test in Vitest).
   - Component internal state or keyboard reducer logic (test in Vitest).
   - CSS animation keyframe counts / polling `document.getAnimations()` (test in Vitest or lint rules).
   - Milestone or sprint relics (`w6-*.spec.ts`, `milestone-*.spec.ts`, `spike-*.spec.ts`).
4. **PR vs Main Tiering**:
   - `npm run e2e:engine` and `npm run e2e:webkit:engine`: Focused suite running ~15 core journey specs on pull requests.
   - `npm run e2e` and `npm run e2e:webkit`: Full regression matrix including multi-theme visual gates, running on `main` post-merge.
