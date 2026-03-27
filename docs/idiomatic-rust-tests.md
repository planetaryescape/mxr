# Idiomatic Rust Tests in mxr

This is the project standard for writing, reviewing, and refactoring tests.

## Core principles

1. Test behavior, not implementation details.
2. Prefer exact assertions over broad truthiness checks.
3. Keep one behavior per test, with clear failure localization.
4. Cover happy path, boundaries, and failure paths.
5. Avoid mirroring implementation logic in expected values.
6. Keep default test runs deterministic and offline.
7. Mock only hard boundaries; verify interaction contracts when mocked.
8. Use small fixtures/builders for readable setup.
9. Use snapshot tests only for stable contracts and pair with semantic assertions.
10. Keep tests refactor-resilient when public behavior is unchanged.

## Required style

- Name tests as behavior specs (`does_x_when_y`).
- Use table-driven tests for parser/state matrices.
- Use `#[tokio::test]` for async tests and bound time-sensitive operations.
- Assert on concrete fields/counts/outcomes (`assert_eq!`, explicit enum matches).
- Keep assertions near the action under test.

## Anti-patterns (banned)

- Tautological pass-through tests (asserting mocked values are returned unchanged).
- Vacuous assertions as sole checks (`is_ok`, `is_some`, `!is_empty`, `to_be_truthy` equivalents).
- Snapshot-only tests for critical behavior with no semantic invariants.
- Tests coupled to private internals that fail on harmless refactors.
- Redundant tests covering the same equivalence class repeatedly.

## Delete / merge criteria

Delete or merge tests when they are:

- Redundant with equivalent branch and boundary coverage.
- Tautological or vacuous and not salvageable with small edits.
- Snapshot duplicates of the same behavior branch.
- Legacy live/network smoke checks that can be replaced by deterministic fixtures.

When deleting tests, preserve or improve bug-catching coverage with targeted replacements.

## Test authoring 5-question gate

Before shipping a new or modified test, all answers must be "yes":

1. Would this fail on a realistic bug (boundary/branch flip)?
2. Are expected values requirement-derived (not implementation-derived)?
3. Does this cover more than the happy path where relevant?
4. If function internals are refactored but behavior preserved, does this still pass?
5. If function body is broken/removed, does this fail for the right reason?

## Audit scoring rubric

All test files should be scored on 10 dimensions (0-3 each, total 30):

1. Assertion specificity
2. Behavioral focus
3. Edge-case coverage
4. Mutation resilience
5. Mock hygiene
6. Test independence
7. Readability as specification
8. Single responsibility
9. Redundancy control
10. Failure authenticity

Interpretation:

- `24-30`: high confidence
- `18-23`: decent with targeted gaps
- `12-17`: significant gaps
- `0-11`: ceremony-heavy, low assurance

## Tooling

Use `scripts/test_quality_audit.sh` to generate:

- machine-readable CSV (`target/test-quality/audit.csv`)
- markdown report (`target/test-quality/audit.md`)

This audit is heuristic and intended to prioritize review and remediation.
