# GitHub-hosted macOS jobs are cancelled after 15 minutes without a runner

Status: open. Seen 2026-11-04.

## What happens

Jobs that run on GitHub's `macos-15` runners sometimes never get a runner. They stay queued, and after exactly 15 minutes they end as `cancelled` with no steps and no log. Both jobs below have an empty `runner_name`:

| Job | Workflow | Created | Completed |
|---|---|---|---|
| 126731336039, Desktop `build` on PR #844 | `desktop.yml` | 2026-11-04 17:09:50Z | 17:24:52Z |
| 126752848782, `Homebrew Formula and Cask` for v0.6.204 | `release.yml` | 2026-11-04 17:50:13Z | 18:05:15Z |

A rerun of the Homebrew job succeeded at once. Until it did, v0.6.204 was published but Homebrew still had 0.6.203.

## What isn't known

- The 15-minute cancellation doesn't match `desktop.yml`'s `timeout-minutes: 45`. GitHub's own limit for a queued job is 24 hours.
- The org's Actions billing could show whether a macOS minutes or spending limit is involved. Reading it needs a token with the `admin:org` scope (`gh auth refresh -h github.com -s admin:org`). The old `orgs/<org>/settings/billing/actions` endpoint now returns 410.

## What to do when it happens

Rerun only the cancelled job (`gh run rerun <run> --failed`). A release left at that point is already published; only Homebrew lags.

## Options

- Check the billing page for a macOS spending limit.
- Split the Homebrew job: update the tap from Linux, and verify the install separately on macOS. Today one macOS job does both: it renders and pushes the formula and cask, then runs `brew install` and `brew install --cask` to check them (`release.yml`, job `homebrew`). The tap update doesn't need macOS; the install check does.
- Have the ship script rerun a job that was cancelled before it got a runner, once.
