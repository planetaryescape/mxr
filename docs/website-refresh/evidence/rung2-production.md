# The agent quick start is live on mxr.sh

Verified 23 October 2026. [PR #691](https://github.com/planetaryescape/mxr/pull/691) merged at 19:43:22 UTC as `4874fbd31d785e9e37d64080b2ba1db696e689dd`. Its tree, `5c60daf9b642f779dbce676077ecc8bbac0314e3`, matches the combined tree computed before merge from reviewed head `3ae20749191f6db6f70ba1239d17fba651d7cd07` and fresh main `26919135bbfdfb50305d63807c8a2dcad1d33a04`. Incoming release work changed only the OpenAPI version string in the site. BK is the sole author; the merge used previously authorised admin privileges without changing protection settings.

The existing Vercel project `mxr-mail` produced READY production deployment `dpl_7pXWraQfuzZPKHmpfMVJG5fSRufQ`, [deployment](https://mxr-mail-44kzz323d-planetaryescape.vercel.app). Connected metadata confirms source `4874fbd31d785e9e37d64080b2ba1db696e689dd` and aliases `mxr.sh` and `www.mxr.sh`. It first queued behind an earlier release deployment. No replacement project, redeploy, billing change or new credential was needed.

## The public entry reaches the shared-draft tutorial

Root opened [mxr.sh](https://mxr.sh/) through T3 and clicked `Use it with your agent`. It reached [the agent quick start](https://mxr.sh/getting-started/agent-quick-start/). The rendered page contains the read-only profile, transition to draft-only, MCP-versus-shell boundary and same-draft review instructions. Recursive loads; the actual viewport and document width both measure 1280px. The real tutorial was also opened in Dia.

The tutorial Markdown, shared quick-start Markdown, For agents Markdown, MCP Markdown and curated `llms.txt` all returned HTTP 200. The deployed shared quick start places its local search before `mxr demo stop` and that exit before own-account setup. The agent CTA resolves to the new route.

The final ready-event Docs Build and Secret Scan passed; Vercel passed. Six protected Rust checks were skipped under docs-only scope. The earlier draft-event Docs Build was cancelled by CI concurrency and superseded by the passing ready-event run. CodeRabbit reported an OSS review limit with no findings; BK had waived second-opinion reviews.

## Evidence remains bounded to what ran

The [implementation evidence](rung2-onboarding-implementation.md) records the local 10-test, 264-path, 209-page build and link/static-layout checks. The [S3 proof](s3-shared-draft-proof.md) observed the synthetic MCP-to-web draft, read-only refusal and draft-only send refusal on v0.6.156. It was a deterministic protocol replay, not a Codex/model reasoning session. Public deployment verifies the documentation journey; it does not turn the default demo setup, a real Codex host run or model quality into observed results.

The current source-map report was checked at the rung-2 merge SHA. Main subsequently gained Messages identity/audience and stored classifier assessment changes at `748044ab4` and `fe8ea0f1ba4253df34d586c82eb2a58e1a240695`. Root refreshed before rung 3 and retained those upstream changes. GitHub's latest published release at that check was v0.6.157; tags or a main merge alone do not establish those newer capabilities in the published binary. Rung 3 must recheck this boundary before promoting new classifier wording.

The root-owned Astro preview PID 59298 on port 4323 was stopped after verifying its absolute worktree command. Existing worktrees, fixtures, caches and author backups remain retained; no files were deleted. Rung 3 can now implement mode and navigation documentation against fresh main.
