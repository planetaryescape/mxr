# 09: The product must mean the same thing before and after someone opens the homepage

Search snippets, social cards, README text, recordings and agent-readable files often supply a visitor's first explanation. Update their source alongside the entry page. (D-WEB-020, D-WEB-022)

## Correct each entry surface at its source

| Source | Change | Verify |
|---|---|---|
| `index.mdx` frontmatter | Literal category, outcome title and accurate description | Browser title, visible hero and page-specific social metadata |
| `astro.config.mjs` | Site description, image alt text, deliberate keyword set and sidebar | No global title/description overrides that mislabel every page |
| `site/scripts/generate-social-images.mjs` | Replace old headline, “every attachment stays local” and unmeasured time in source | Regenerate `og.png`; inspect actual image, not just SVG strings |
| `site/public/llms.txt` | Lead with local mail runtime, supported account types, human/machine surfaces, safety boundary and canonical routes | Curated links resolve; distinguish CLI skill from MCP server |
| `site/scripts/generate-llms-txt.mjs` output | Regenerate `.md` siblings and `llms-full.txt` from current source | Root `index.md`, changed routes and full corpus carry current text |
| `README.md` | Runtime category and first proof; retain install/provider precision and existing modes links | Reader can reach the same trial as website visitors |
| `PRODUCT.md` | Record approved audience order, category and actual current typography | Do not silently treat Q1/Q2 as already approved |
| `DESIGN.md` | Align only site-related directions affected by the approved concept | No redesign of application surfaces implied |
| Existing product blueprint | Add pointer to this feature blueprint; preserve original text and old decisions | Agent can discover this plan without overwriting history |
| Legal/privacy pages | Audit product descriptions and data-flow consistency; revise only claims actually made stale by changed behaviour | No new legal promises or silent terms changes |
| Footer, 404, robots and sitemap | Keep stable recovery and discovery; correct links if their targets change | Unknown URL has useful navigation; no internal plans in public search |

A newer screenshot or card must not assert that every attachment is cached. A still image with a fictional “0.04s” result is not a benchmark. Model names and version numbers belong in setup/reference where they can be maintained, not the core pitch. (D-WEB-019)

## Machine documents need a precise starting summary

Proposed llms.txt opening, to verify and edit during implementation:

> mxr is a local mail runtime for people, scripts and agents. It syncs connected provider accounts into SQLite, searches locally, and exposes daemon-backed mail capabilities through CLI, MCP and the web bridge. Human clients work with the same mailbox through Now and five modes. Native model features are optional; agent and MCP policies, previews and send gates have explicit limits.

Follow with a small task index: understand fit, try demo, connect an agent, use modes, preview and verify changes, inspect exact CLI/MCP/HTTP contracts. Preserve useful reference links. Do not imply the curated llms.txt is generated: the current generator copies/combines other source pages but leaves this file authored.

Generated Markdown currently strips frontmatter and carries source body text. Homepage raw HTML/MDX does not automatically become a clean narrative in those outputs. Inspect the generated files; if they are hard to use, provide the essential textual explanation in source or narrowly improve the existing generator. Do not patch `dist/` or create a second drifting homepage solely for machines.

## Media proves the same current workflow

Inventory recordings and posters in `site/public/`, VideoGallery and the public gallery page. Mark each keep, recaption or re-record after checking its command, current result and UI. Reuse a still-relevant local search video; re-record outdated default TUI or three-interface narratives when they teach a wrong hierarchy or omit the shared-review point.

First recording priorities are the local aggregate and the shared draft/rejected send. Keep account identity and draft identity visible. Synthetic captures must be labelled as such; canned model output must not be presented as live quality evidence. Add or update text transcripts/captions and meaningful poster alt text. Provide media controls and reduced-motion behaviour. Heavy renders use the shared build queue with at most four workers.

No bespoke animation or new recording framework is required. Existing source/tapes and scripts remain the starting point; verify them before reuse. (D-WEB-010, D-WEB-014, D-WEB-019)

## Release verification is distinct from website deployment

The audit baseline is a named main commit, not proof of what a visitor installs. Before each rung, verify the release/tag and artifact version used in examples and compare newly promoted claims. The supplied feature branch must not silently become the public default. When a claim is main-only, wait for release or label it in the appropriate development context; do not fill the public guide with roadmap promises.

Use the existing Vercel project/build configuration for website delivery; do not create a new project. Before public deployment, inspect the existing site's actual production branch and deploy mechanism. If it is protected, source/build evidence does not count as a deployed-page check. The implementer records that limitation. A docs-only change does not require inventing a binary release, changelog entry or manual tag. (D-WEB-018, D-WEB-024)

## Keep the documentation contract maintainable

When behaviour changes, the owner updates the affected canonical page, its examples and linked metadata/media in the same change. Use existing generated reference and docs validation. Add only focused checks for drift the existing checks cannot catch: authored route ownership, curated llms links, or a touched incorrect output envelope. Do not encode the entire marketing paragraph in brittle tests.

The content closeout for each rung records changed claims, unchanged claims, held branch/future claims, demo evidence, site build result and final URLs in this blueprint's HANDOFF.md until a more specific implementation handoff owns them. No new personal-memory directory, public visitor tracking or separate journal is required.

Unresolved questions: Q5 assumes release-aligned docs; Q7 assumes direct comprehension checks. S5 checks generated and deployed discovery surfaces.
