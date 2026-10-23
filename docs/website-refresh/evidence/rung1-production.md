# The runtime opening is deployed on mxr.sh

Verified 23 October 2026. [PR #688](https://github.com/planetaryescape/mxr/pull/688) merged as `cc00d4081bb5bd7875c39da4641ff95888817d80` at 16:40:44 UTC. The squash commit's tree, `d06c902fc47fba841312bed31fb8a5d098dd6310`, exactly matches the reviewed candidate `60abed5f28b30fd5eed2db15092369dda4571e1a`. BK is the sole author. The merge used the admin authority BK had already given; no branch protection or repository setting changed.

The existing Vercel project `mxr-mail` produced deployment `dpl_Hj2ghZ8eX5KjzC4TP1fJs4rzECxt`, [production deployment](https://mxr-mail-b7zfiei4d-planetaryescape.vercel.app). Connected deployment metadata confirms READY, target production, source `cc00d4081bb5bd7875c39da4641ff95888817d80`, and aliases `mxr.sh` and `www.mxr.sh`. The connected tool has no blocking wait operation, so the existing Vercel CLI's scoped `inspect --wait --timeout 45s` completed the bounded wait. No new hosting project, configuration, credential or release tag was created.

## Production serves the revised opening and its next steps

The real [homepage](https://mxr.sh/) was opened in Dia and inspected through T3:

- The approved category and headline are present. Support copy begins "mxr keeps a synced mailbox on your computer" and explains shared use by people, tools and clients.
- The dense subscription illustration is absent. The existing recording is prominent, Recursive loads, and selected recordings reach readyState 4 without a media error. Unselected media can remain unloaded by design.
- Canonical URL is `https://mxr.sh/`. The description explicitly names synced mail on the computer. At the actual 1280px browser viewport, document width is also 1280px.
- The install command is `brew install planetaryescape/mxr/mxr`. Clicking the primary CTA reaches `/getting-started/quick-start/#1-try-the-demo-inbox`, where the anchor exists and the code is `mxr demo`.
- Homepage Markdown, Why mxr HTML and Markdown, `llms.txt`, `llms-full.txt`, quick start, For agents, Your day, email modes and `og.png` return HTTP 200. Updated ownership and daemon wording appears in the applicable Markdown exports.

The six protected Rust checks were skipped under the repository's docs-only scope. Docs Build, Secret Scan and Release Gate Scripts passed separately. This is not a claim that six Rust suites executed. Local build, link, control and static phone/desktop evidence is in [the implementation record](rung1-clarity-implementation.md).

## The website changed; the mail runtime did not

The `crates` tree is `4d879193faf6e4d066405ad80e2b1de9774e7b96` at both pre-change main `f39ae66578e3099e464967ab2f042e24b1ad8ec4` and deployed main `cc00d4081bb5bd7875c39da4641ff95888817d80`. Existing S2 evidence describes the released v0.6.156 runtime, not a new binary release. No real mailbox or account connection was used.

External reader comprehension and conversion remain unmeasured. The shared-draft S3 demonstration is still a prerequisite for the next agent tutorial. Phone layout was checked statically with compiled CSS and loaded font/posters; interactive phone video behaviour remains unverified because T3's requested resize did not change its actual viewport. These limits do not become completed studies or demonstrations through deployment.

Root stopped its owned Astro preview PID 45016 on port 4323 after checking the absolute command path. The listener is gone. Worktrees, npm/dist artifacts, synthetic fixtures and original planning remain retained; no deletion was authorised.

Next: verify a synthetic draft across MCP and a human client, including read-only refusal and prohibited sending, and establish one current agent-host setup path. Only then write the concrete agent tutorial. Rungs 2–5 remain website work under the existing orchestration authority.
