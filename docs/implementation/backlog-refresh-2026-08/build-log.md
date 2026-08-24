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
