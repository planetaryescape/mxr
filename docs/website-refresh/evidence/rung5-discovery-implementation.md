# R5 discovery implementation

## Source and scope

This slice started from parent commit `36db26ab80ccc68e6cb7a4a4bc82787b409c2c81` on branch `t3/discovery-consistency`. Before final source claims, HTTPS `main` was fetched from `https://github.com/planetaryescape/mxr.git`; it resolved to `cd08cfe5eb72b0959bc623ad15439bbf09cbe2c3`. The merge base is the same SHA. The approved product category is “A local mail runtime for you and your agents.” The homepage headline remains “Put your whole mail history to work.”

The slice owns these files:

- `PRODUCT.md`, `DESIGN.md`, `README.md`
- `docs/research/website-page-audit-2026-10-23.md`
- `site/public/llms.txt` was reviewed; its curated category and links already match and it did not need edits.
- `site/scripts/generate-llms-txt.mjs` (comment only) and `site/scripts/generate-cli-reference.mjs` (existing subscription use text only)
- `site/src/content/docs/getting-started/first-sync.md`
- `site/src/content/docs/guides/assistant.md`, `linked-drafts.md`, `mailbox.md`, and `web-app.md`
- `site/src/content/docs/reference/cli/concepts.md`, `site/src/content/docs/troubleshooting.md`, and `site/src/pages/privacy.md`

No video or media files, homepage implementation, CSS, navigation, routes, or runtime code changed. The existing CLI reference generator remains the source of generated reference output.

## Verified source facts

All code references below were checked after the HTTPS fetch to `main` at `cd08cfe5eb72b0959bc623ad15439bbf09cbe2c3`.

- `crates/daemon/src/handler/diagnostics/mod.rs:428-441` resolves an omitted subscription account to the configured default when present and applies the requested limit before returning rows. `crates/daemon/src/commands/subscriptions.rs:95-123` sorts that returned set locally when `--rank` is set. The help copy now describes the metric as a local READ-state proxy, not a distinct-open count, and states the query bound and account scope.
- `site/src/content/docs/guides/analytics.md:113-151` documents the existing READ-state-based subscription rank and response fields. Rust help and generated output were not hand-edited.
- `crates/daemon/src/handler/reading/article.rs:1-7,19-63` describes request-only linked-article fetching and the demo exception. `crates/daemon/src/handler/reading/marginalia.rs:250-365` sends a named item only on request and fetches an unsaved digest article first. `site/src/content/docs/guides/security-and-privacy.md:6-21` already describes those article and Marginalia network paths. The privacy page adds those verified paths without changing its terms or making a no-network promise.
- `site/src/styles/custom.css:10-13,32-34` imports self-hosted Recursive and assigns it to display, body, and code. Product and design guidance no longer refers to IBM Plex as the site font.
- The current TUI guide at `site/src/content/docs/reference/tui.md` is the canonical route for Now, the five modes, screens, and bindings. The first-sync guide now links to it rather than preserving an outdated screen/key list.

The R5 audit source and root correction are recorded in parent commit `36db26ab80ccc68e6cb7a4a4bc82787b409c2c81:docs/website-refresh/evidence/rung5-discovery-media-map.md`. The page audit adds scoped dispositions and explicitly does not claim an exhaustive runtime replay.

`gh release view` reported latest published release `v0.6.159` (published 2026-10-23T20:33:30Z; tag commit `e98f499`).

## Checks

`agent-work run -- npm ci` completed as queue task 257 and installed 528 packages. npm reported one high-severity audit finding; this slice made no dependency changes or audit remediation.

An initial site build was queue task 263. After removing the remaining unmeasured timing example, the final site build was queue task 251: `agent-work run -- env PATH=/opt/homebrew/opt/node/bin:/usr/bin:/bin npm run build`. It completed generation, all 10 site-script tests, docs validation (`265 OpenAPI paths`), Astro generation (`209 pages`), and llms export generation after the final copy edit. `dump-openapi-spec.sh` reported `cargo not found` and used the committed `site/public/openapi.json`; this is not live conformance evidence. The build emitted a Vite large-chunk warning and an empty `i18n` collection warning.

Generated-output checks passed: `site/dist/index.md`, `llms.txt`, `llms-full.txt`, `reference/cli/subscriptions.md`, and the agent/first-sync/Now/Your-day entry files exist. The built homepage export contains the category and human workflow links, the curated llms file contains the existing-agent and Now/Your-day entries, the generated subscription reference contains the bounded READ-state/account explanation, and the privacy HTML contains both network disclosures. The homepage export has HTML-bearing content; no consumer failure requiring a conversion pipeline was demonstrated.

Generated resources retained: `site/node_modules` was 450 MB and `site/dist` was 47 MB. Install queue task 257 and final build queue task 251 completed. The install's transient wrapper PIDs 29774 and 91628 exited with task 257. No preview, server, or listener was started; no owned process remained at closeout.

Inline review used writing-docs clarity, medium humanizer, mxr-development source guidance, documentation-refiner closeout, and anti-slop-check on the two touched generator files. Code-simplifier was applied inline across reuse, simplification, efficiency, and layering: the `.mjs` edits add no code path, parsing behavior, dependency, or repeated computation. TypeScript reviewer is not applicable because no JS/TS logic or type contracts changed.

The Unicode format/control and soft-hyphen scan and `git diff --check` passed after the evidence and build checks.

## Limits and handoff

This work checks authored copy, source claims, current links, and generated site output. It does not replay every page or mail workflow and makes no claim about visitor comprehension or live-host behavior. No preview was started, so there is no Dia page, listener, or preview PID to hand off. No deployment was run because root owns integration and delivery. Generated dependencies, build output, caches, assets, and unfinished source are retained.

The scoped knowledge delta is that the homepage Markdown export exists at the site root; its HTML-bearing content alone has not demonstrated a consumer failure that justifies a conversion pipeline. The subscription clarification is confined to the existing website reference source. Sync progress examples no longer imply a measured `3,000/50,000` run; the synthetic 50,000-message demo remains documented.
