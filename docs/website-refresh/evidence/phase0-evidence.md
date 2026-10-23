# Phase 0 website evidence

Checked 2026-10-23 against fetched `origin/main` at `f39ae66578e3099e464967ab2f042e24b1ad8ec4`. The worktree HEAD and merge base with `origin/main` are the same SHA. The worktree was clean before this evidence file was created.

## Released install and main agree

`git show origin/main:.release-please-manifest.json` reports `0.6.156`. GitHub marks `v0.6.156` as the latest published release, published at `2026-10-23T05:02:49Z`; `git rev-parse v0.6.156^{commit}` resolves to `f39ae66578e3099e464967ab2f042e24b1ad8ec4`. The release is not a draft or prerelease. GitHub lists uploaded `mxr-v0.6.156-macos-aarch64.tar.gz` (80,722,947 bytes), `mxr-v0.6.156-linux-x86_64.tar.gz` (88,213,173 bytes), and `mxr-desktop-0.6.156-aarch64.dmg` (1,962,306 bytes). The installed binary at `/opt/homebrew/bin/mxr` reports `mxr 0.6.155`; only `mxr version` and `mxr --help` were run. This installed binary is one patch behind the published release and main.

## Production is deployed from the same main SHA

At the fetched baseline, `site/astro.config.mjs` sets the canonical site to `https://mxr.sh`. `site/vercel.json` declares the existing build as `npm run build`, Astro output `dist`, and framework `astro`. The GitHub deployment record created `2026-10-23T04:41:15Z` identifies environment `Production`, source SHA `f39ae66578e3099e464967ab2f042e24b1ad8ec4`, and a successful status at `https://mxr-mail-l72323e11-planetaryescape.vercel.app`. This is the observed deployment alias; the custom site domain is `mxr.sh` from the Astro config. The repository has no `site/.vercel/project.json`, so the Vercel project ID/settings were not independently inspectable. A newer successful preview record points to `73cb4ef84233d6f9376f68d0f9e38ed1f9c7668c`; another successful preview points to `2053f18ed406b981ad56639b64a29703b4fd9a7a`. These preview records do not change the production source result. No deploy or site build was run.

## Subscription ranking has a bounded, explainable result

At this SHA, `crates/daemon/src/cli/mod.rs:1063-1074` defines `mxr subscriptions` with a default `--limit 200`, optional `--account`, and `--rank`. `crates/daemon/src/commands/subscriptions.rs:101-123` sends the limit/account to `ListSubscriptions`, receives that set, then sorts the returned rows by `opened_count / message_count` ascending. `crates/store/src/message.rs:1093-1153` aggregates by account and lowercased sender, includes only messages with an unsubscribe method, excludes spam and trash, and applies SQL `LIMIT` after choosing one latest message per account/sender bucket. The query calculates `opened_count` by checking the message READ flag at lines 1112-1114 and binds that flag at lines 1155-1156. It is a count of currently READ messages in each eligible bucket, not distinct opens or open-tracking telemetry. `--rank` sorts only the limited returned set; it does not rank every newsletter in the mailbox.

A safe proof should use a disposable synthetic profile, then run `mxr subscriptions --limit 200 --rank --format json` and report the two account scopes, candidate count, eligible buckets returned, and READ-flag counts. The repository demo's CLI path cannot be shown safe under this round's isolation rule: `crates/daemon/src/commands/demo.rs:65-83` derives config/data and the sticky marker from the process config/data home; `:195-235` seeds the mailbox, writes the sticky marker and prewarms surfaces; `:1665-1675` derives the profile paths from `dirs::config_dir()` and `dirs::data_dir()`. The code can seed two fake accounts, but those writes are not confined to this worktree by an MXR path override. Running it would require redirecting the home/XDG environment or changing demo path support, both outside this phase's constraints. No non-meta CLI command was run, no daemon was started, and no synthetic JSON or runtime counts were observed. S2 therefore remains source-only; do not present the query as a measured demo or claim complete newsletter coverage or latency.

## Verification and limits

- `git -c url.https://github.com/.insteadOf=git@github.com: fetch origin main`: completed; `origin/main`, HEAD and merge base all resolve to `f39ae66578e3099e464967ab2f042e24b1ad8ec4`.
- `git show origin/main:.release-please-manifest.json`, GitHub release/tag metadata, release assets and GitHub deployment/status metadata: read-only checks completed as summarized above.
- `/opt/homebrew/bin/mxr version`: `mxr 0.6.155`. `/opt/homebrew/bin/mxr --help`: CLI help returned; neither command accessed mail state.
- No Rust build/tests, site dependency install/build, browser check, or deployment was run. This is phase 0; deployment execution is N/A.
- The evidence is source/release/deployment-state discovery, not a user comprehension study or runtime demonstration. S4 remains pending. No TypeScript/JavaScript or product/site source was changed.
- Code-simplifier implementation pass and TypeScript review are N/A because no implementation or JS/TS files changed. Anti-slop diff checks were skipped for the same reason. Documentation review followed the repository writing guidance. The untracked-file `git diff --no-index --check /dev/null <report>` check and a direct trailing-whitespace scan found no issues.

## Resource packet

Before writing this report, the canonical worktree `/Users/bhekanik/.t3/worktrees/mxr/t3-b3d8050b` measured 105 MiB; the planning snapshot at `/Users/bhekanik/code/planetaryescape/.orchestrate/mxr-website-runtime-20261023/planning-snapshot` measured 176 KiB. The evidence destination did not exist before writing. The worktree Cargo `target/` and `site/node_modules/` paths were absent. After writing, `docs/website-refresh/evidence/` measures 16 KiB. No build cache or generated synthetic output was created. This worker started no process, listener, or queue job.
