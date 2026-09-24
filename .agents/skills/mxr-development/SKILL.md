---
name: mxr-development
description: "Use when changing mxr source, architecture, docs, tests, IPC, daemon handlers, store/search/sync/semantic behavior, provider adapters, TUI/web clients, release flow, or repo-level development process."
---

# mxr Development

This skill holds the context that used to bloat always-on agent files. Load it only for source/docs/process work.

## Product shape

- Local-first: SQLite is canonical, search indexes are rebuildable, and core mail works offline.
- Daemon-backed: TUI, CLI, web, and scripts are clients over Unix-socket IPC.
- CLI-first: new capabilities land in CLI at the same time as daemon support, with stable JSON/JSONL.
- Client parity: a user-facing daemon capability is incomplete until CLI,
  TUI, web, and MCP expose it. Interaction design may differ; capability may
  not silently disappear in one client.
- Provider-agnostic core: Gmail/IMAP/SMTP/Outlook behavior maps into the internal model in adapters.
- Compose uses `$EDITOR` with YAML frontmatter plus markdown body.
- Reader mode is plain text first; no inline images in the terminal rendering path.

## Build and verify loop

For feature work:

1. Implement in the narrowest crate surface.
2. Run `scripts/cargo-test -p <crate> --tests`.
3. Run `cargo build -p mxr`.
4. Restart stale daemons: `pkill -f 'mxr daemon' 2>/dev/null`, then `cargo run --bin mxr -- daemon --foreground`.
5. Drive the feature through the CLI, preferably with `--format json`.
6. Inspect daemon state with `mxr events`, `mxr logs`, `mxr doctor`, and `mxr activity`.
7. Query persisted state back through the CLI, such as `mxr search ... --format json`.

If the daemon will not start, check `cargo build -p mxr`, `pgrep -af 'mxr|cargo'`, `mxr daemon --foreground`, and stale socket files.

## Client and mutation contract

- TUI and CLI must use the same daemon request; avoid client-only capabilities.
- Every reusable daemon capability needs a CLI surface. TUI/web support should layer on it.
- Destructive or batch mutations need a dry-run/preview path before commit.
- Dry-run selection/query logic must match the real mutation path.
- Read/list/status/search/export surfaces must keep structured output pipeable.

## Crate boundaries

- `core` depends on no internal crates.
- `protocol` depends only on `core`.
- Provider crates depend on `core` plus shared mail utility crates such as `mail-parse` and `outbound`.
- `store` and `search` depend only on `core`.
- `semantic` owns embeddings/dense retrieval and must not depend on daemon, TUI, or provider crates.
- `llm` owns provider clients and prompt/result DTOs; higher layers pass plain data in.
- `relationship` may depend on `core`, `store`, `reader`, and `llm`; not protocol/daemon/client/sync/search/semantic/provider crates.
- `safety` may depend on `core`, `reader`, and `relationship`; not store/protocol/daemon/clients/sync/search/semantic/provider crates.
- `sync` depends on `core`, `store`, and `search`.
- `daemon` is the integration point, but talks to providers only through `MailSyncProvider` and `MailSendProvider`.
- `tui` and `web` are clients; they must not depend on daemon, store, search, sync, semantic, or provider crates.
- Use Cargo dependencies for architecture seams, never `#[path]`.

See `docs/blueprint/01-architecture.md` for longer rationale.

## Domain invariants

- Semantic search is optional platform behavior layered above core mail runtime.
- Sync stores envelopes/bodies and commits lexical search before a sync batch finishes.
- Semantic chunks may be persisted during sync, but embeddings/ANN refresh only happen when semantic is enabled or explicitly reindexed.
- Hybrid search keeps BM25 exactness and adds dense recall with RRF.
- Activity log rows are local-only personal data. Never sync or transmit them, never store credentials/full bodies/attachment bytes, and write only through `state.activity.record(...)`.
- `INSERT OR REPLACE` can trigger `ON DELETE CASCADE`; prefer `INSERT ... ON CONFLICT UPDATE` for parent rows with dependents.
- `mxr reset --hard` / `mxr burn` wipe runtime state only by default; preserve config and credentials unless explicitly requested.

## Local gates vs CI

CI is the workspace gate. Every PR runs Check, Clippy, Rustdoc, Test (nextest, whole workspace), SQLx Offline, cargo deny and the build as required checks on 4-vCPU runners. Do not run `--workspace` clippy, `cargo doc --workspace`, `cargo nextest run --workspace` or `cargo build --workspace --release` on the developer machine: it repeats CI on the laptop, pegs every core for minutes, and proves nothing CI will not prove. Locally run only what is scoped to the change:

- `scripts/pre-pr-rust-gate` (fmt, clippy for touched crates only, boundary script, cargo deny). `--list` shows the crates it will check; `--full` is for reproducing a CI failure you cannot narrow to one crate.
- `scripts/cargo-test -p <crate> --tests` for the crates you touched.

Then push and read the PR checks. If one is red, reproduce that one failure with `-p <crate>`, fix, push again.

## Release shorthand

Releases go through release-please. `ship it` means run the whole chain below, including merging the release PR, so the manifest, `Cargo.toml`, `CHANGELOG.md` and tags never drift. Never create, push or move a `v*` tag by hand; never overwrite a tag or GitHub release. The flow is built so the laptop never rebuilds what CI already built.

0. Preflight. `git fetch origin --tags`, then compare `.release-please-manifest.json` on `origin/main` with `git tag -l 'v*' --sort=-v:refname | head -1`. If they disagree, land a PR that sets the manifest to the latest tag's version first. Also check `gh pr list --head release-please--branches--main`: an open release PR whose version is already tagged is stale; close it and let release-please regenerate it.
1. Commit release-ready changes on a branch with conventional commits. Run the scoped local gates above, nothing wider. Two separate gates decide whether a release PR appears. First, `release-please.yml` runs its job only when `scripts/release_change_scope.sh` finds an artifact-affecting change since the last tag (`crates/`, `apps/web/`, real Cargo dependency changes, release workflows/scripts), or when the push is a merged release PR; otherwise the job is skipped. Second, release-please itself only opens a PR for releasable commits: `feat:` or `fix:` (pre-1.0 both bump the patch) or a breaking change (`feat!:` or a `BREAKING CHANGE:` footer, a minor bump pre-1.0). `chore:`/`docs:`/`ci:` alone do not release.
2. Push, open the PR, and wait on it: `gh pr checks <N> --watch`.
3. Merge with `gh pr merge <N> --squash --admin` (self-approval is blocked, `--admin` bypasses only the review).
4. Wait for the `release-please.yml` run on the merge commit (`gh run list --workflow=release-please.yml --commit <sha>`), then for `release-please--branches--main` to show the new version. If the job was skipped, step 1's conditions were not met. If it failed on "Require release token" or a 401, `RELEASE_PLEASE_TOKEN` is missing or expired: stop and ask BK to rotate the PAT. Never set that secret from `gh auth token`. Once the release PR is current and green, merge it with `gh pr merge <N> --squash --admin`. That merge makes release-please push `v{version}` with the PAT, which starts the tag-driven release workflow.
5. Wait for the release workflow on the tag, not for a `main` run: `gh run list --workflow=release.yml --json databaseId,headBranch,status,conclusion` and pick the row whose `headBranch` is `v{version}`, then `gh run watch <id>`. If no run exists for the tag, the tag was created without the PAT; re-run on the existing tag with `gh workflow run release.yml --ref v{version}` rather than re-tagging. It builds the macOS and Linux binaries (`cargo build --release --locked -p mxr` from a clean checkout), creates the GitHub release, and pushes the Homebrew formula.
6. Verify on this machine with downloads only; no step here compiles anything:
   - Homebrew (the real install): `brew update && brew upgrade mxr`; `mxr version` must report the new release.
   - install.sh: `D="$(mktemp -d)"; MXR_INSTALL_DIR="$D/bin" ./install.sh v{version}; "$D/bin/mxr" version; rm -rf "$D"`. It downloads the release tarball and checks the sha256.
   - cargo channel: covered by step 5. The release workflow's build-binaries job compiled `-p mxr --release --locked` on this exact tag from a clean checkout, which is the same compile `cargo install --git` would do. Do not run `cargo install --git ...` locally: it is a cold release build of the whole workspace and takes every core for 10+ minutes to re-prove a passed CI job.
7. Converge this machine on Homebrew only. The sole `mxr` on PATH must be Homebrew's: remove any `~/.cargo/bin/mxr` (`cargo uninstall mxr`) and any `~/.local/bin/mxr` left by past install.sh runs, then confirm with `which -a mxr`. Multiple mxr binaries on PATH carry different build ids, and every invocation restarts the daemon to "match the current binary", ping-ponging the daemon between versions.
8. Report: the version, tag, release URL, release workflow run URL, and each install check's `mxr version` output. Confirm the manifest on `main` now equals the new tag.

Source of truth: `docs/blueprint/17-release-pipeline.md` and the checked-in GitHub workflows.

## Useful docs

- `docs/blueprint/` - requirements, architecture, decisions.
- `docs/blueprint/15-decision-log.md` - settled decisions.
- `docs/activity-log.md` - activity table lifecycle and privacy rules.
- `docs/implementation-journey.md` - historical context and superseded plans.
