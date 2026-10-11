# Draft revisions preserve the losing editor's work

S02 is integrated in [PR341](https://github.com/planetaryescape/mxr/pull/341) at `a631583c8fd8438abc6892fdf10fb49dc09e3748`. The merge tree `3a903ddea1843b4c380cd81adf344882e7d39c42` matches reviewed candidate `3d338bebfa9e6b714b6f9503ed5546c037d5eef1`. Current-head CI and OpenAPI validation passed. Release PR344 merged as `7cfb0c38cf10b975eb76232a3e53ad6eb3fc2f54`, tree `d8b8463eebbd1eaa4e5f47ed75d69b30cbadc531`, with manifest version 0.6.65. The [merge proof](evidence/s02-release-pr344-gate.json) records four version/changelog files; the version proof records 36 internal Cargo version bumps and unchanged external dependencies. Tag v0.6.65 exists. [Release run 38132041238](https://github.com/planetaryescape/mxr/actions/runs/38132041238) passed its plan job; Linux and macOS builds were in progress at last check, the installer worker verification had passed, and no assets were published.

## Acceptance and evidence

A draft update uses a compare-and-set revision. On conflict, the losing editor keeps its private recovery text and attachments. The browser exposes explicit Retry and Open saved draft actions; recovery-only Discard leaves the canonical draft untouched. Run the CLI smoke on the installed binary:

```sh
scripts/smoke-draft-revisions /absolute/path/to/released/mxr
```

Then run the browser journey against the release's embedded production SPA. Use isolated fake profiles. Run the CLI smoke through both install paths. After verifying the installer and Homebrew binaries have identical hashes, run the embedded-browser journey once against either binary. All three runs must pass before S03 implementation starts.

The local Mac CLI and browser journeys passed at earlier candidate `6c9a22e8f1eab335e7912f346188f1251efe2e49`, tree `2146fa1edf700945375d2efa2c9aafa424e748da`. That run covered save/restart/read, two-editor conflict and retained text, owned attachment recovery, Close/reopen/Discard, an actual IPC outage and explicit Retry after reconnect. The local focused Rust checks, scoped Clippy, binary build, frontend tests, typecheck, lint, motion checks and production build passed. The browser journey used a synthetic account and made no send, schedule or provider-save calls. These results predate the final five-file correction; they are local candidate evidence, not installed-release evidence. The [curated local summary](evidence/s02-local-mac-6c9a22e8.json) preserves its scope and source candidate.

Whole-diff Claude Opus 5.5/max review of `010ae9518c5ee9bf96c194628483e10a0f0ac4ce` returned GREEN with six should-fix findings and no blockers. Triage adopted three narrow fixes: preserve a reopened draft's sender, correct the orphaned-draft delete hint, and retain recovery paths through daemon transport failure. Three verified recovery limits remain in [draft recovery follow-ups](../../issues/draft-recovery-followups.md). The final bounded five-file correction used the recorded trivial post-triage exception. The [review report](evidence/s02-claude-010ae951-pinned.md), [metadata](evidence/s02-claude-010ae951-pinned-metadata.json), triage and [reproducible delta proof](evidence/s02-trivial-delta-3d338beb-proof.json) preserve its provenance.

[Final-head CI](https://github.com/planetaryescape/mxr/actions/runs/38130635937) passed on the exact candidate: 3,983 fast tests, 4,029 semantic tests, 885 web unit tests, 381 browser tests with no retries, and four speed checks. The run took 15m46s; the browser job took 15m08s, including 12m04s for the suite. [OpenAPI validation](https://github.com/planetaryescape/mxr/actions/runs/38130635939) passed 232 schema checks and six bridge integration tests. The [merge gate record](evidence/s02-pr341-gate.json), [CI summary](evidence/s02-ci-38130635937-3d338beb.json), [release version proof](evidence/s02-release-pr344-version-proof.json), [triage](evidence/s02-claude-010ae951-triage.md), and [reproducible final-delta proof](evidence/s02-trivial-delta-3d338beb-proof.json) pin the evidence.

The earlier CI browser run had one retry of an unchanged pane-width test; the final run had zero flaky tests. The pane test's unresolved measurement question stays in [its follow-up](../../issues/resizable-pane-first-frame-flake.md).

## Review and timing

The implementation worker used GPT-6.1-Sol/high for revision and recovery ordering; no escalation occurred. Claude Opus 5.5/max reviewed the whole diff in 20m56.479s. The final local correction passed its restored-sender test (19.40s compile, 0.96s test), scoped Clippy (47.14s), and `cargo build -p mxr` (19.52s). The alias restoration regression and five recovery reload tests passed with typecheck and lint. Final-head CI took 15m46s, including 12m04s for browser tests. The current release run has no completion time yet.

## Release completion

When release run 38132041238 finishes, verify archive checksums and both installer and Homebrew binaries, including matching binary hashes. Run the CLI smoke above through both install paths. Then run the embedded-browser journey once against the verified identical bytes and record the version, source SHA/tree, and observed results. Until those checks pass, S02 remains pending and S03 must wait. See [orchestration](../../implementation/daemon-portability/orchestration.md) for the delivery order.
