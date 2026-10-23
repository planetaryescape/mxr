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

## Root integration verifies fonts, media and controls

Root integrated the scoped change as `e7ed62c781240b6cda9b710b93bfc5605b5ac492` on the control branch. Its four implementation paths match worker commit `42de3ac587ab95665e375ee47a393ac6aecb8e27`; only the root's preceding handoff/waiver docs differ between those trees. Fresh main and merge base remain `f39ae66578e3099e464967ab2f042e24b1ad8ec4`. The hosting build passed again: 10 tests, 262 paths, 208 pages, committed OpenAPI fallback. No tracked generator output changed.

The real Astro preview at port 4323 was opened in Dia and inspected through T3. Its actual viewport stayed 1280 by 800 despite a retained 390px setting; no interactive phone-width pass is claimed. At 1280px, the headline ends at y=322, install at y=685 and recording at y=708. Document and body widths both equal 1280. Recursive loaded; all three recordings reached readyState 4 without media errors. Pausing and choosing the CLI tab showed the CLI pane and a play label. Install-copy feedback was observed changing to "copied" and back through a DOM observer.

Root inspected four static captures at 390/1440px in both themes using full compiled CSS and the matching public Recursive asset. Existing recording posters were embedded as images for this layout check; scripts were removed and viewport-height units mapped to the stated capture height. Font and all posters loaded, document/body widths matched the target, and no console errors occurred. A favicon URL remained unresolved only in the extracted capture. This checks static typography/layout, not phone video interaction. The capture files are retained under the durable ledger's `review-rung1-clarity/` directory.

All 26 retained homepage local links and anchors resolve. The Markdown generator intentionally emits title/description and body, omitting nested hero fields; the homepage description now states local ownership, while connector and daemon explanations remain in the body. No generator behaviour was changed. A full-source copy lint flagged inherited procedural colon labels and CSS class names; these are valid documentation/source constructs and were not rewritten as marketing prose. The new opening copy is audited separately. No reader comprehension or conversion study, new S2 run or S3 handoff is claimed.
