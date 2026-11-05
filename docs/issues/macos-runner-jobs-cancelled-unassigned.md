# Check the org billing page for a macOS spend limit

Status: open, needs someone with the `admin:org` scope. Seen 2026-11-04.

GitHub-hosted `macos-15` jobs can be cancelled after 15 minutes without ever getting a runner (desktop run 126731336039 on PR #844; Homebrew job 126752848782 for v0.6.204). The 15-minute cancellation does not match the `timeout-minutes: 45` in `desktop.yml`.

The Homebrew part is fixed. The tap update runs on Linux (`homebrew` in `.github/workflows/release.yml`), so a cancelled macOS job no longer leaves Homebrew a release behind. Only the `brew install` check runs on macOS (`homebrew-verify`).

## Still open

Whether a macOS minutes or spending limit is involved. The org Actions billing data would show it. The old `orgs/<org>/settings/billing/actions` endpoint returns 410, and reading the new one needs `gh auth refresh -h github.com -s admin:org`. Someone with that scope needs to check it and record the answer here.

If the cancellations keep recurring, the next option is to have the ship script rerun a job that was cancelled before it got a runner, once. That needs a decision first.
