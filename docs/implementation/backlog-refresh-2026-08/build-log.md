# Build log — backlog-refresh-2026-08

## 2026-08-24 — Mapping and routing

- Base audited: `origin/main` at `f4bed17b` (`v0.6.26`).
- Three read-only GPT-5.5 workers audited Phase 0/1, Phase 2, and Phase 3/4.
- PE Tasker skill selected, but its CLI is not installed in this environment. The host is preserving its task graph, worktree, review, routing, and learning-log rules manually.
- Requested Fable 5 / Opus 5 models are unavailable. Runtime substitution: GPT-5.5 workers; this GPT-5.6-sol thread orchestrates; fresh-context GPT-5.6-sol performs the final review.
- `/simplify` is required on every scoped diff. `/typescript-reviewer` applies only to TypeScript API/type/tooling work, per its own scope gate.

### Corrected plan assumptions

- Security “Not shipped yet” wording and the exact tested-provider landing claim are already gone. IMAP/SMTP live proof is still absent.
- IMAP IDLE and Reply-To preference shipped; the addendum is stale.
- Gmail provider thread lookup plus draft caching already preserves native threading; T3.2 is docs reconciliation, not a schema feature.
- VIPs are still browser-local. The documented `useVips()` abstraction does not exist, so adapter preparation must precede any daemon migration.
- `v1_launch_proof.sh` proves fake-provider behavior, not installation or real authentication.
- `install.sh` installs `mxr` and `mxr-mailmerge`; release archives also contain `mxr-chime-player`. Clean-install verification must resolve whether this is intentional.
- T3.3, T3.4, and Phase 4 remain evidence-gated; do not build them because they are easy.

### Existing work and deployment gate

- Issue #216 implementation: `efb68d17` on `fix/216-domino-fetch-recovery`.
- Primary fix scope: bounded UID batches; discard desynchronised sessions; retry individual UIDs; skip persistently malformed messages.
- Separate issue concerns remain out of scope: per-account connection cap and credential/logging/keychain behavior.
- SSH push works. GitHub CLI/browser authentication was absent when the branch was pushed. PR/release operations may require login.

### Worker knowledge protocol

Every worker reports false assumptions, failed approaches, and transferable learnings. The host sends applicable findings to concurrent and downstream workers before they continue.

## 2026-08-24 — Issue 216 adversarial review, round 1

- Initial review raised cursor advance as possible silent loss and noted Gmail All Mail bypasses the recovery helper.
- Adversarial follow-up rejected cursor flooring: Domino's isolated UIDs remain unframeable, so flooring would retry and refetch from the first bad UID forever. Advancing is intentional quarantine-by-skip for this failure class.
- Gmail All Mail remains a documented residual gap, not a blocker: the reported Domino server has no `X-GM-EXT-1`, and widening the patch lacks reproduction evidence.
- Final GPT-5.5 recommendation: approve for the scoped Domino initial-sync failure.
- Transferable rule: distinguish “response framed, local mapping failed” (floor cursor and retry) from “server response cannot be framed even in isolation” (skip to contain blast radius unless a persisted retry/quarantine policy exists).

## 2026-08-24 — Issue 216 GPT-5.6-sol review

- Reviewed all three changed files plus fetch callers, cursor persistence, QRESYNC/CONDSTORE paths, mock changes, and the real async-imap parser regression.
- No material findings for the scoped Domino path.
- Residual gap: Gmail All Mail initial/backfill still has a separate direct fetch path. It is intentionally unchanged because the reported server has no Gmail extension and no reproduction supports widening this patch.
- Validation: 115 unit + 8 integration + 3 smoke tests passed; provider clippy passed with `-D warnings`; `cargo build -p mxr` passed with the existing `process_probe.rs` unfulfilled-lint warning; diff check passed.
- Review state: green. Merge, versioned release, and install-channel verification remain.

## 2026-08-24 — Phase 0 implementation and adversarial review

- Task 001 refreshed `TODO.md` at worker commit `076283bd`; integrated as `233a26b5`.
- Both review rounds found no material issue. Shipped items have code/file evidence; live IMAP/SMTP proof remains open.
- Task 002 reconciled shipped addendum items at worker commit `b573437b`.
- The first adversarial pass found that the format-version bullet was only implicitly deferred under a mixed-status heading.
- The worker made the status explicit at `82c46bb4`; integrated as `efa3a129` plus `dc5b5432`.
- The GPT-5.6-sol pass checked IDLE, Reply-To, and Gmail reply-thread claims against provider, parser, daemon-loop, and draft-cache code. No further findings.
- `/typescript-reviewer` was correctly skipped: both task diffs are Markdown-only. Each worker ran `/simplify` and `git diff --check`.
- Transferable rule: a mixed shipped/deferred list needs status on every bullet; heading context is not enough.
- Phase 0 remains in progress until the commits reach `main`; repository docs have no separate Vercel artifact.

## 2026-08-25 — Preflight and Phase 0 deployment

- `v0.6.27` shipped the issue 216 Domino recovery plus Phase 0 docs. Release assets passed, but main CI exposed a stale `dead_code` expectation in `process_probe.rs`.
- Hotfix `936f86a4` removed only that stale expectation. Both review rounds and local validation passed. `v0.6.28` then shipped successfully.
- The `v0.6.28` CI Clippy job still failed while the same command passed locally. Updating local stable from Rust 1.95 to the CI runner's Rust 1.98 reproduced the actual remaining lints. This disproved the cache-corruption theory.
- Hotfix `b05b46d4` replaced fixed-size `chunks_exact(4)` decoding with `as_chunks::<4>()` and a test-only full-vector drain with `std::mem::take`. GPT-5.5 and GPT-5.6-sol reviews found no behavior change.
- Main CI for `b05b46d4` passed all jobs, including Clippy on Rust 1.98. Release commit `66281449` shipped as `v0.6.29`.
- `v0.6.29` CI passed in 7m21s. Release passed in 24m19s: Linux, macOS, CLI smoke, GitHub Release, and Homebrew all green.
- Install verification passed for Homebrew, `install.sh`, and `cargo install --git ... --tag v0.6.29 --locked`. Each returned `mxr 0.6.29`; temporary installs were removed; PATH resolves only `/home/linuxbrew/.linuxbrew/bin/mxr`.
- Published archive checksums: Linux `672be24f34996cfb90d97e3f77571323e595b47b42743a20eed1b7b68fa022d8`; macOS `5bf013dd072194094a9d068af6221f6ed6d867bb1cd636b35447e2e2cd21a709`.
- The first v0.6.29 install verification hit `/tmp` quota. The clean detached issue-review worktree used 5.2 GB; removing it freed enough space. Tagged Cargo was rerun with its build target on `/home` and passed.
- Homebrew's first install auto-cleanup removed the unneeded `unbound` formula while leaving its config files. The later upgrade used `HOMEBREW_NO_INSTALL_CLEANUP=1`; `unbound` can be restored with `brew install unbound` if needed.
- Non-blocking release warnings remain explicit: live Gmail credentials were absent, so that smoke was skipped; Apple signing secrets were absent, so the macOS archive is unsigned.
- Tasks 000-002 are accepted. Their implementation, reviews, CI, release, and install-channel checks are all green.

## 2026-08-25 — Issue 216 secondary-concern audit

- The primary Domino literal-recovery acceptance is shipped. Four independent concerns from the issue body were re-audited before closing the issue.
- IMAP folder sync already has a hard cap of four, but no per-account configuration. This remains real and becomes task 015.
- `mxr-async-imap` 0.10.6 logs raw outgoing requests and incoming response buffers at trace level. LOGIN passwords, AUTHENTICATE responses, and message bodies can therefore reach logs. This security task is task 014 and must ship before logging controls expand.
- `accounts repair` is already disk-first with optional keychain mirror/fallback; the claimed runtime behavior is stale. Its help and OpenAPI copy still promise a protected keychain, so copy correction becomes task 016.
- `logging.level` is settable but daemon tracing initialization ignores it. Wiring it becomes task 017 and depends on deployed trace containment from task 014.
- Transferable rule: never broaden dependency trace controls before auditing whether trace events contain protocol bytes or secrets.
