# Rung 3 modes and navigation implementation

Implementation candidate checked on 2026-10-23. The checkout began at
`cad1cb853ba16bfa399916f0d5ea56cadc61a774`; an HTTPS fetch of
`https://github.com/planetaryescape/mxr.git` returned `main` at
`fe8ea0f1ba4253df34d586c82eb2a58e1a240695`. `HEAD` and fetched main share
that merge base. No branch switch, reset or remote write was performed.

The implementation keeps the existing root homepage category and visual
sections. Its existing mode section links to the human route; that route now
starts at Now and points into each mode. The guide describes the five shipped
jobs over one daemon-backed mailbox and keeps each mode's object and completion
meaning distinct. It preserves the incumbent page and generated CLI reference
hierarchy.

## Source and release boundary

The source map was prepared at `4874fbd31d785e9e37d64080b2ba1db696e689dd`.
This pass retained the intervening guide additions at `748044ab4` (Messages
identity and audience details) and `fe8ea0f1` (shared stored classifier
assessment); neither was reverted. The latest published release check returned
`v0.6.157`, published at `2026-10-23T17:36:05Z`, tag
`549e10e6d928d6293c93ab3d0fef57c7f0b0e4f6`. GitHub reports the `v0.6.158`
release as a draft with no `publishedAt`; its tag points to
`26919135bbfdfb50305d63807c8a2dcad1d33a04`, which predates `main` at
`fe8ea0f1`. The `v0.6.157` tag is an ancestor of `main`; the `main` commit is
not an ancestor of `v0.6.158`.

The touched Messages identity/audience and shared-assessment explanations name
the current `main` SHA and say that published `v0.6.157` predates them. They
do not imply that the `v0.6.158` tag contains those changes. No release status
is inferred from a merge or tag alone.

The default `modes.archive_on_last_done` is `true` at the verified main SHA
(`crates/config/src/types.rs#L116-L120`). The shared mode-done handler rejects
Archive completion (`crates/daemon/src/handler/mode_done.rs#L121-L128`), and
its plan applies provider archive only when no inbox-holding mode remains and
the setting is enabled (`#L280-L413`). This supports the guide's previewed,
conditional provider effect. It does not equate Archive records with provider
archive or promise universal undo.

## Routes and navigation

At `fe8ea0f1`, `site/src/content/docs/` contains 81 authored Markdown/MDX
files including the home page, so 80 non-home authored routes. Each has one
primary sidebar home. The existing
`getting-started/agent-quick-start.md` route was omitted from the original
page audit at `f39ae66578e3099e464967ab2f042e24b1ad8ec4`; its row is now added
to that audit without claiming a full new audit. The conditional invoice
workflow remains absent. CLI command pages remain generated under the single
`reference/cli` autogenerate branch, with authored CLI concepts hand-listed.

Starlight accepts `collapsed` on sidebar groups, confirmed in its [official
configuration documentation](https://starlight.astro.build/uk/reference/configuration/).
Start here is expanded by default; all other top-level groups are collapsed.
On an active page, Starlight expands its own group and leaves other collapsed
groups closed. Existing redirects, legal and footer routes, API explorer,
generated OpenAPI route, 404 page and technical reference hierarchy remain
configured.

## Verification

- `git fetch https://github.com/planetaryescape/mxr.git main`: fetched
  `fe8ea0f1ba4253df34d586c82eb2a58e1a240695`; merge base matched.
- A source route comparison found 81 authored Markdown/MDX files, all 80
  non-home routes assigned, and no configured slug without a target. The CLI
  subtree remains covered by autogeneration.
- `agent-work run -- npm ci` completed successfully; 528 packages installed,
  with zero npm audit findings.
- `agent-work run -- env PATH=/opt/homebrew/opt/node/bin:/usr/bin:/bin
  CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 NEXTEST_TEST_THREADS=4
  RAYON_NUM_THREADS=4 npm run build` completed: 10 script tests passed,
  `validate-docs` passed with 265 OpenAPI paths, and Astro built 209 pages.
  The PATH excluded Cargo, so OpenAPI generation used the committed
  `site/public/openapi.json`; this is not live API conformance evidence.
  Astro reported the existing empty `i18n` collection and a chunk-size warning.
- The built preview opened in Dia at `http://127.0.0.1:4321`. T3 reported
  a 1280-pixel CSS viewport. Three preview snapshots timed out, so no screenshot,
  mobile layout or dark/light visual pass is claimed. DOM inspection confirmed
  the human home section's `Work through your day` link reaches that guide;
  the guide links to Now and the mode guides. On `Your day`, the active
  “Work with your mail” group was expanded, Start here stayed expanded, and
  the other six inactive groups were collapsed. The active page was in its
  group. The Search control's click timed out, but DOM inspection immediately
  afterward found its search input open and focused. The final build's
  keyboard disclosure interaction could not be verified because preview
  automation timed out; no keyboard pass is claimed. The mode overview has
  same-site links to all five
  guides, each with its source or follow-up path in the rendered page.
- This is local build and DOM evidence only. No real mailbox, runtime, live
  model or public deployment was exercised. Production deployment remains
  root-owned.

## Scope

Changed files are `site/astro.config.mjs`, the assigned mode and compatibility
guides, `site/public/llms.txt`, the page-audit addendum and this evidence
record. No dependency, homepage layout/hero/media, generator, application UI,
runtime behavior or root-owned ledger changed. No test fixture or milestone
spec was added. Before handoff, PID `73627` was verified as this checkout's
Astro preview listening on `127.0.0.1:4321`, stopped with `kill 73627`, and
`lsof -nP -iTCP:4321 -sTCP:LISTEN` returned no listener.


## Root checked the integrated candidate

Root refreshed main at `fe8ea0f1ba4253df34d586c82eb2a58e1a240695`, read the complete candidate diff and integrated it into the control worktree. GitHub now reports v0.6.158 published at `2026-10-23T20:03:09Z`, at `26919135bbfdfb50305d63807c8a2dcad1d33a04`. It still predates the classifier and Messages changes. Root replaced the worker's now-stale “still a draft” statements with concise, version-bounded guidance: the specific newer details require changes after v0.6.158. Full commit provenance stays in this evidence record rather than repeated in operational prose. Root also corrected the audit addendum: the agent tutorial was added after the original audit, rather than present-but-omitted in that original tree.

The final root build passed 10 script tests, validation of 265 OpenAPI paths, and 209 Astro pages, with 205 Markdown siblings. It used the committed OpenAPI fallback, not live conformance. The first root build accidentally ran before the planned wording edits because the edit command used a site-relative working directory; it passed, and the build was rerun after the edits. Both jobs completed; no job was cancelled or left active.

Root independently counted 81 authored routes, with all 80 non-home routes assigned in the sidebar. A focused compiled-HTML check validated 1,844 internal route/anchor occurrences across the homepage and eight changed guides, including their shared sidebar, with no failures. Whitespace and Astro config syntax checks passed. The source-map's Updates selection-token detail was checked against the CLI: the CLI previews and supplies the token internally; readers are not instructed to pass a nonexistent flag.

The real built Your day page opened in Dia at port 4323. T3 DOM checks found Recursive loaded, document width equal to the measured 1280px viewport, the active Work with your mail group expanded and inactive top-level groups collapsed. Root focused Search, understand and automate and pressed Enter through the browser tool; its disclosure opened. Root focused Search and pressed Enter; the search dialog opened with its input focused, then Escape closed it. The mode comparison rendered all five distinct completion meanings. The actual browser did not resize to 390px, so this is a desktop interaction check.

Full compiled-CSS static captures at 390px in dark and light show the readable Your day opening and expanded navigation state with no horizontal or code-block overflow; the matching Recursive font loaded. The menu state was set to the same attributes used by Starlight, and viewport-height units were mapped to an 844px capture viewport. These captures do not test phone menu interaction. Missing favicon/touch-icon requests belong to the extracted capture; the production assets were not changed. No model, runtime mail or real user profile was used.

Inline simplifier review found no new helper, abstraction or executable logic: navigation strings and focused prose use existing patterns. There is no TypeScript contract change. Existing worker node_modules/dist and root build artifacts remain retained. The root-owned Astro preview is PID 1737 on port 4323, retained until the online handoff. No independent review was run, as BK requested.
