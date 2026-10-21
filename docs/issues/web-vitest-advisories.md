# Web app: npm audit fails on two Vitest advisories until vitest 4.1.12 clears the release-age window

**Status:** open · **Found:** 2026-10-21 on PR #578 (also affects `main` and every PR)

The `Web Unit Tests` CI job runs `npm audit --audit-level=moderate` in
`apps/web/` and fails on two moderate advisories against `vitest` 4.1.11
through `@vitest/mocker`:

- [GHSA-qxg6-89mg-rqv8](https://github.com/advisories/GHSA-qxg6-89mg-rqv8): code injection via unauthenticated manual mock registration.
- [GHSA-xp26-3454-mjgp](https://github.com/advisories/GHSA-xp26-3454-mjgp): Windows backslash traversal bypasses the `@vitest/mocker` redirect allowlist fix.

Vitest only runs the test suite. It is a dev dependency and isn't part of
anything mxr ships.

The audit step fails before the job finishes, so `Web E2E` is skipped on
those runs. Run the affected e2e specs locally until this is fixed.

## Why it isn't fixed yet

The fix is `vitest` 4.1.12. `~/.npmrc` sets `min-release-age=7`, and 4.1.12
was younger than seven days on 2026-10-21, so `npm audit fix` leaves 4.1.11 in
place: "published after the configured release-age cutoff". BK chose to keep
the rule and wait rather than exempt vitest.

## Fix

Once 4.1.12 is seven days old, run `npm install --ignore-scripts
vitest@^4.1.12` in `apps/web/`, so that `@vitest/mocker` follows. Then check
that `npm audit --audit-level=moderate` reports 0 and the unit suite passes,
remove this file, and remove the note in `AGENTS.md` and the comment above the audit step in `.github/workflows/ci.yml`.
