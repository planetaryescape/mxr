# Draft revisions preserve the losing editor's work

S02 shipped and passed installed acceptance in v0.6.65. [Feature PR341](https://github.com/planetaryescape/mxr/pull/341) integrated the changes at `a631583c8fd8438abc6892fdf10fb49dc09e3748`. The merge tree `3a903ddea1843b4c380cd81adf344882e7d39c42` matches reviewed candidate `3d338bebfa9e6b714b6f9503ed5546c037d5eef1`. Current-head CI and OpenAPI validation passed. Release PR344 merged as `7cfb0c38cf10b975eb76232a3e53ad6eb3fc2f54`, tree `d8b8463eebbd1eaa4e5f47ed75d69b30cbadc531`, with manifest version 0.6.65. The [merge proof](evidence/s02-release-pr344-gate.json) records four version/changelog files; the version proof records 36 internal Cargo version bumps and unchanged external dependencies. Tag v0.6.65 and its published artifacts are verified. The installer and Homebrew binaries report `mxr 0.6.65` and share SHA256 `86096ad9c869173c3fa6d69fcf0cac1547c94dc36ecd90e1f2ede2a1dea03e26`.

## Installed checks and browser acceptance

The released CLI passed the draft-revision smoke through both install paths. Each isolated fake profile saved, restarted and read a draft; rejected a stale revision while preserving its unsent text; preserved malformed editor text; and left the draft unchanged during delete preview. The shutdown smoke also passed through both install paths, retaining four fake message IDs through active SIGTERM and idle SIGINT restart. The [installed proof](evidence/s02-installed-v0.6.65.json) records source, tree, archive and binary hashes, workflow status, and smoke timings. No real mail was used.

The embedded-browser journey against the published installer passed. Two tabs observed an actual stale-revision 409, and the losing editor recovered its latest text and attachment after close/reopen. Recovery-only Discard removed the current private copy while preserving four older recovery files and their attachment bytes; the canonical winner remained unchanged. On a second draft, an actual IPC 503 preserved edits while two canonical reads seven seconds apart remained identical. After reconnect, explicit Retry accepted the draft. Offline and stopped-bridge reopen preserved the remembered recovery entry; availability guards handled both before the loader catch ran. Two typed outage regressions in the five-test `activeDrafts` suite cover the loader failure paths. The [sealed browser packet](evidence/s02-installed-v0.6.65/README.md) records the journey, selection manifest, receipts and four selected screenshots. It contains no HAR; [HAR provenance](evidence/s02-installed-v0.6.65/raw-har-provenance.json) records checksums for captures retained outside the repository. The recorded browser requests contain no compose send/schedule or explicit provider push/sync route; the isolated fake daemon did run its startup sync. Its complete run through cleanup took 382.91 seconds. S02 is accepted; S03 is the next stage. Run the CLI smoke against each install path with:

```sh
scripts/smoke-draft-revisions /absolute/path/to/released/mxr
```

The earlier local Mac journey passed at candidate `6c9a22e8f1eab335e7912f346188f1251efe2e49`, tree `2146fa1edf700945375d2efa2c9aafa424e748da`, but predates the final five-file correction and is not installed-release evidence. The [curated local summary](evidence/s02-local-mac-6c9a22e8.json) preserves its scope and source candidate; the sealed release packet records the accepted installed journey.

## Review and verification

Whole-diff Claude Opus 5.5/max review of `010ae9518c5ee9bf96c194628483e10a0f0ac4ce` returned GREEN with six should-fix findings and no blockers. Triage adopted three narrow fixes: preserve a reopened draft's sender, correct the orphaned-draft delete hint, and retain recovery paths through daemon transport failure. Three verified recovery limits remain in [draft recovery follow-ups](../../issues/draft-recovery-followups.md). The final bounded five-file correction used the recorded trivial post-triage exception. The [review report](evidence/s02-claude-010ae951-pinned.md), [metadata](evidence/s02-claude-010ae951-pinned-metadata.json), [triage](evidence/s02-claude-010ae951-triage.md) and [reproducible delta proof](evidence/s02-trivial-delta-3d338beb-proof.json) preserve its provenance.

[Final-head CI](https://github.com/planetaryescape/mxr/actions/runs/38130635937) passed on the exact candidate: 3,983 fast tests, 4,029 semantic tests, 885 web unit tests, 381 browser tests with no retries, and four speed checks. The run took 15m46s; its browser job took 15m08s, including 12m04s for the suite. [OpenAPI validation](https://github.com/planetaryescape/mxr/actions/runs/38130635939) passed 232 schema checks and six bridge integration tests. The [merge gate record](evidence/s02-pr341-gate.json), [CI summary](evidence/s02-ci-38130635937-3d338beb.json), [release version proof](evidence/s02-release-pr344-version-proof.json) and review records above pin the evidence. An earlier CI browser run retried one unchanged pane-width test; the final run had zero retries. Its unexplained one-pixel mismatch remains in [the pane follow-up](../../issues/resizable-pane-first-frame-flake.md).

## Timing

| Phase | Route or model | Elapsed | Evidence |
| --- | --- | ---: | --- |
| Implementation | GPT-6.1-Sol/high; no escalation | — | revision and recovery ordering |
| Whole-diff review | Claude Opus 5.5/max | 20m56.479s | final production review |
| Final local correction | focused Rust checks/build and web checks | compile 19.40s; test 0.96s; Clippy 47.14s; `cargo build -p mxr` 19.52s | restored-sender regression, five recovery reload tests, typecheck and lint passed |
| Exact-candidate CI | final candidate | 15m46s | browser suite 12m04s; 381 tests, no retries |
| Release workflow | v0.6.65 | 24m20s | Mac native build 19m22s |
| Installed CLI draft smoke | installer / Homebrew | 2.159s / 2.002s | both passed |
| Installed shutdown smoke | installer / Homebrew | 5.705s / 5.557s | both passed |
| Installed browser journey | published installer | 382.91s including cleanup | accepted; offline and bridge guards ran before loader catch |

The release proof notes that the embedded-SPA release-binary check passed on Linux and was conditionally skipped on macOS; the separate installed-browser journey passed on the published macOS binary. See [orchestration](../../implementation/daemon-portability/orchestration.md) for delivery order and the current handover.
