# Selected-daemon diagnostics preserve host ownership

Status: done in [v0.6.62](https://github.com/planetaryescape/mxr/releases/tag/v0.6.62), source `8e81648a48a726aa22f0c210dd2407b003a9331e`. [PR 331](https://github.com/planetaryescape/mxr/pull/331) and [version PR 332](https://github.com/planetaryescape/mxr/pull/332) are merged. Both platform archives match their published checksums. The pinned installer and Homebrew installations each passed all six isolated selected-daemon checks.

Run the consuming-client smoke against a built or downloaded binary:

```sh
python3 docs/build-log/daemon-portability/s01_smoke.py /absolute/path/to/mxr
```

It creates separate client and target profiles, starts a real daemon, and checks target PID, doctor database/index ownership, JSONL framing, local repair rejection and missing-target isolation. It uses synthetic empty profiles and disables the HTTP bridge, semantic model downloads, LLM and activity. It does not use personal mail. Its six checks passed against the exact reviewed candidate and both installed release paths.

## The selected host supplies the facts

Previously, `status` mixed selected-daemon values with local paths, `doctor` could inspect a different local database, and missing explicit Unix targets could trigger local autostart. The candidate uses existing status/doctor IPC requests, separates `client_local` metadata, rejects local-only reindex before connecting, and redacts command arguments from client-generated connection errors. The local-default JSON contract remains intact.

The implementation is in `crates/daemon/src/commands/{diagnostics,status,doctor}.rs`, the command dispatcher in `crates/daemon/src/lib.rs`, and `IpcClient::connect`. Focused integration coverage lives in `crates/daemon/tests/selected_daemon_diagnostics.rs`. These references describe final reviewed head `25f70c7d297d67ec32220e927f607ba8d195093e` / tree `f8944bdd539696dc1b9651e69161bf3a56bbd2e0`, proven equivalent to the integrated tree at `f783d937`.

## Independent checks found no blocking defect

The orchestrator exercised a real daemon and found four defects during implementation: client socket metadata used the target resolver; repair advice pointed to the wrong command context; the status dispatcher still autostarted for a missing Unix target; and a missing command executable exposed its arguments. All four were corrected before independent review.

Claude Opus 5.5/max reviewed the whole diff through OpenCode 1.18.35 in a fresh read-only session and found no blockers. [Review](evidence/s01-claude-5de44f93.md), [model metadata](evidence/s01-claude-5de44f93-metadata.json), [CI results](evidence/s01-ci-38013781257.json) and [merge-tree check](evidence/s01-pr331-merge-check.json) retain provenance. GitHub's authoritative PR diff, applied at the base, produces the reviewed tree. CI passed both whole-workspace test variants and all applicable checks on the exact candidate.

The final wording/test pass clarified local repair, watch-test timing, test environment isolation and documentation. The [exact delta](evidence/s01-trivial-delta-25f70c7d.diff) against Claude-reviewed `5de44f93` received parent review under the orchestration skill's trivial-fix exception. Focused diagnostics tests (3/3), status tests (4/4), the build, and all six real-daemon smoke checks passed on final `25f70c7d`. [Final CI](evidence/s01-ci-38014927704.json) passed again. The [integration proof](evidence/s01-integrated.json) and [version PR proof](evidence/s01-release-pr332-gate.json) record the actual squash commits and tree equality.

## The review gate took longer than compilation

| Measured phase | Wall time | Evidence |
| --- | --- | --- |
| Initial focused package suite | 8m12.5s | Pueue job 4783; before final dispatch/redaction/advice edits |
| Exact reviewed-head build | 5.84s | Pueue job 4811 |
| Ready-PR CI | 7m18s | Run 38013781257, 01:35:44–01:43:02 UTC |
| Independent review | 16m36.84s | Fresh session in model metadata |
| Release workflow, including Homebrew | 29m06s | Run 38016043149, 02:11:26–02:40:32 UTC |
| macOS artifact build | 25m26s | Build job, 02:11:49–02:37:15 UTC |

The worker used GPT-6-Luna/high; the orchestrator used GPT-6-Astra/xhigh; the independent reviewer used Anthropic Opus 5.5/max. No worker escalation occurred. Local builds and checks shared the two-job queue with independent shutdown/token work. Recorded agent time and waiting are lower bounds; no human delivery estimate is inferred from them.

The large review included the requested blueprint and research alongside code. Use narrow subsequent stage diffs and preserve this shared context instead of repeatedly rediscovering it. Export large OpenCode sessions directly to a file: piped export output truncated in this run, while file-directed export produced valid JSON and confirmed the requested model and read-only tools.

## Both installation paths passed the released-client journey

Tag `v0.6.62` points to `8e81648a`. The [release workflow](evidence/s01-release-workflow-38016043149.json) passed Linux/macOS builds, CLI smoke, publication and Homebrew. [Installer evidence](evidence/s01-installed-v0.6.62.json) records both archive checksums and the consuming-client smoke. [Homebrew evidence](evidence/s01-homebrew-v0.6.62.json) records `/opt/homebrew/Cellar/mxr/0.6.62/bin/mxr` and the same six passing checks. The known-good v0.6.61 binary remains available for comparison; the user's personal daemon and primary checkout were not used for testing.

The largest release wait was the macOS artifact build. Independent token, shutdown and draft work ran during publication. Version releases remain serial to preserve Latest/Homebrew order; independent code review and the next version PR's CI may overlap publication. Atomic token persistence subsequently shipped in v0.6.63. Next: finish shutdown and durable draft delivery. Broader send recovery remains dependent on released draft revisions.

TCP and actual off-machine SSH were not run in the S01 smoke; command transport used disjoint local profiles. Broader target-aware command failure guidance is recorded in [diagnostic follow-ups](../../issues/selected-daemon-diagnostic-followups.md). Future drafts, send recovery and native iOS/VPS deployment are not part of S01.
