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
