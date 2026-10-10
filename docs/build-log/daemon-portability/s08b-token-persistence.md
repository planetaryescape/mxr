# Gmail token replacement preserves a complete disk fallback

Status: integrated through [PR 333](https://github.com/planetaryescape/mxr/pull/333) at `origin/main` commit `3d17b1600a9db36b9f062bc4e2ca6997d8a9c562`, with the same tree as the final candidate. Final candidate `8b04cf94117f7931bbfea554222451191faafb7b`, tree `1e523767e55a05a2fa64efc34e99332ede982835`, base and merge base `8e81648a48a726aa22f0c210dd2407b003a9331e`. [Version PR 334](https://github.com/planetaryescape/mxr/pull/334) merged as `2c55a735e2b013910cb7222211fa8d6156d6204c`; [v0.6.63](https://github.com/planetaryescape/mxr/releases/tag/v0.6.63) is published and this stage is done. The [release workflow](https://github.com/planetaryescape/mxr/actions/runs/38018087397) passed, both archive checksums match, and the pinned installer and Homebrew each passed all six isolated client checks. Both installed binaries report `mxr 0.6.63`.

The existing writer truncated the live Gmail OAuth fallback file before rewriting it. The candidate creates a private temporary file beside the destination, writes and syncs it, replaces the destination atomically, then syncs the directory. The storage-instance mutex covers the cache update, serialization and persistence, preventing two calls on that instance from persisting snapshots out of order. The JSON format and keychain-first behavior remain compatible.

The production changes are in `crates/provider-gmail/src/auth_storage.rs`, `token_cache_file.rs`, `lib.rs`, and the provider's `Cargo.toml` at the final candidate above. Provider-specific behavior remains in the Gmail crate. No daemon, protocol or client capability was added.

## Synthetic failures preserve old or new complete JSON

The final focused run passed 82 unit tests and three offline smoke tests. Tests cover failures before and after replacement, reload through the fallback reader, concurrent persistence ordering, private permissions, relative paths, final symlink replacement, and temporary-file cleanup after rename failure. The final test-only commit replaced a timing-based absence assertion with a direct mutex assertion.

`cargo build -p mxr` passed on `2466936c`, whose production code is identical to the final candidate. Formatting and diff checks passed on the final head. Previous provider Clippy, architecture-boundary and dependency checks passed on the same production patch. [Full CI](https://github.com/planetaryescape/mxr/actions/runs/38016846923) passed on `8b04cf94`, including both workspace test variants. The [merge proof](evidence/s08b-pr333-gate.json) records the reviewed tree and actual integrated commit.

Run the scoped suite from the candidate checkout:

```sh
scripts/cargo-test -p mxr-provider-gmail --tests
```

## Claude found no blockers

Claude Opus 5.5/max reviewed the complete provider patch in a fresh read-only OpenCode 1.18.35 session. The [review](evidence/s08b-claude-7d290b1f.md), [model metadata](evidence/s08b-claude-7d290b1f-metadata.json) and [candidate manifest](evidence/s08b-review-manifest-7d290b1f.json) pin reviewed `7d290b1f` / tree `5e6be371cd3ed032c5c78ee9045ebdf80e881e30`.

The [recorded final delta](evidence/s08b-trivial-delta-8b04cf94.diff) contains the S01 version metadata plus deterministic test assertions, rename-failure coverage and a Unix-only test-helper annotation. Parent review found no production change or blocker in that delta; it uses the orchestration skill's trivial-fix exception. Full CI passed on the final head.

| Measured phase | Wall time | Evidence |
| --- | --- | --- |
| Independent review | 13m46.94s | Session `ses_edc7254c1ffeTW4g5H2MjK5rzi`; metadata above |
| Final unit tests | 4.29s | [Pueue job4873](evidence/s08b-tests-8b04cf94-job4873.json); three offline tests also passed |
| Build on identical production code | 51.27s | Worker run at `2466936c` |

The implementation worker used GPT-6-Luna/high, the orchestrator GPT-6-Astra/xhigh and the reviewer Anthropic Opus 5.5/max. No escalation occurred. Review was the largest measured active wait; independent S02 work and S01 publication ran alongside it. Agent-time figures are lower bounds, not human delivery estimates.

## The guarantee stops at one storage instance

Synthetic boundary failures do not simulate hardware power loss. The mutex orders calls on one storage instance; it does not coordinate separate authentication sessions or processes. A crash can leave a private orphan temporary file. The existing memory-versus-disk behavior when persistence returns an error is unchanged.

Atomic replacement requires a writable parent directory. A future headless deployment should mount the token directory rather than one cache file. The separate Outlook writer is recorded as a [source-audit follow-up](../../issues/outlook-token-cache-durability.md); no Outlook runtime corruption or exposure was reproduced.

The [installer proof](evidence/s08b-installed-v0.6.63.json), [Homebrew proof](evidence/s08b-homebrew-v0.6.63.json) and [release workflow record](evidence/s08b-release-workflow-38018087397.json) preserve the final result. The Homebrew binary resolves to `/opt/homebrew/Cellar/mxr/0.6.63/bin/mxr`. S01 publication and both installed smoke checks passed before the v0.6.63 version PR merged. The [version merge proof](evidence/s08b-release-pr334-gate.json) preserves exact-tree equivalence and successful checks. The release-source persistence checks and installed-client smoke are separate evidence. At source `8e81648a`, production OAuth uses hard-coded Google endpoints (`crates/provider-gmail/src/auth.rs`) and the writer attempts keychain persistence before disk. The fake provider does not reach that path. Verification therefore pairs exact-release-source synthetic persistence tests with an isolated installed-binary smoke; it does not claim a live Gmail token refresh.

Next: S08c consumes this released persistence behavior when validating headless operation. No user decision or credential change is required by S08b.
