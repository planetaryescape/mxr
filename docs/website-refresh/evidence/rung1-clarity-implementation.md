# Rung 1 opening refinement

Implemented from the proposal at `245da75547e8110c21fabfa85d243b0902481210`, based on freshly fetched HTTPS `origin/main` at `f39ae66578e3099e464967ab2f042e24b1ad8ec4`. The merge base is `f39ae66578e3099e464967ab2f042e24b1ad8ec4`.

The homepage keeps the approved category and headline, uses the supplied support copy and CTA labels, removes the synthetic subscription panel and its styles, and restores the existing CLI/TUI/agent recording to the desktop right column. The agent tab remains selected. The recording and its poster, controls, lazy loading and reduced-motion behavior were not changed. The poster is an existing search-and-jq recording about phishing/scam senders; this change makes no shared-draft or cross-client handoff claim.

Two short paragraphs below the opening explain when a Gmail connector may be sufficient and how clients reuse the daemon. The provider support rows remain. The offline section keeps the initial-sync, provider-connection and attachment-cache limits beside its first local-mail claim. The five modes and Now remain in the homepage content. The Why page repeats the opening benefits, connector choice and daemon reuse once, then keeps its network limits, synthetic demo, provider matrix and existing routes.

## Verification

- `git diff --check` passed.
- The Impeccable detector reported one existing navigation hover transition on `padding-left` in the untouched `.pickpath` rule; it is outside this opening refinement. The inline code-simplifier pass found no new helpers or logic to simplify, and no TypeScript review was needed because no JS/TS contracts changed.
- `npm ci` completed with 529 packages and reported no vulnerabilities.
- `env PATH=/opt/homebrew/opt/node/bin:/usr/bin:/bin npm run build` passed: 10 script tests, 262 OpenAPI paths and 208 built pages. The OpenAPI generation step used the committed `site/public/openapi.json` fallback because Cargo was unavailable; live OpenAPI generation was not verified. Astro reported its existing large-chunk and empty-i18n warnings.
- The built homepage was served from `http://127.0.0.1:4173/` and returned HTTP 200. Dia was opened to that URL. T3's collaborative snapshot timed out once at 15 seconds. A static T3 HTML preview of the built page at 1280px and 390px showed the opening and following copy stacked without visible horizontal overflow. The static capture used compiled CSS; its isolated frame could not resolve the page's font and poster URLs, so typography and media loading were not verified by that capture. Carousel interaction was not tested.
- No runtime, real mailbox, Rust build or new artwork was used.

## Files and generated resources

Changed source files: `site/src/content/docs/index.mdx`, `site/src/styles/custom.css`, `site/src/content/docs/guides/why-mxr.md`, and this evidence note.

Build resources retained: `site/node_modules` (450 MB) and `site/dist` (51 MB), both under the 3 GiB task limit. The preview server was started on port 4173 with PID 12791 and is stopped after inspection. The `agent-work` queue reported task 251 for dependency installation and the site build. No Rust artifacts were generated.
