# 01: Keep the current site and change how readers reach its evidence

The website remains an Astro/Starlight site in `site/`. There is no new backend or content service. The existing homepage is `site/src/content/docs/index.mdx`; it is not a separate `src/pages/index.astro`. (D-WEB-010, D-WEB-011)

## One entry fans into three reader jobs

```text
Homepage: result + local mail runtime + connector answer
  ├── Try it: install → demo → inspect local result
  │     ├── Work directly: web / TUI / CLI → Now → five modes
  │     └── Use my agent: CLI skill or MCP → read → draft → review
  ├── Understand: Why mxr → email modes → architecture / trust
  └── Build or look up: recipes → automation contract → reference

All paths return to the same nouns, provider accounts and draft IDs.
The site describes the runtime; it does not connect to a real mailbox.
```

Keep a short first-time path visible ahead of deep feature groups. The docs shell keeps search, theme control, breadcrumbs and on-page navigation; do not replace these with a custom navigation application. [07](07-guides-and-navigation.md) defines the exact group membership. (D-WEB-022)

## Existing sources keep clear ownership

| Source | Owns | Required handling |
|---|---|---|
| `site/src/content/docs/index.mdx` | Homepage content and hero frontmatter | Rewrite argument and task proof; reuse working code-copy/media behaviours |
| `site/astro.config.mjs` | Sidebar, site description, head metadata and Astro redirects | Update as part of the same rung as route moves |
| `site/src/styles/custom.css` | Shared site visual language | Reuse tokens and selectors; add only styles needed by changed surfaces |
| `site/src/components/VideoGallery.astro` and `Footer.astro` | Media presentation and footer links | Update labels/links only where the journey needs them |
| `site/src/content/docs/` | Authored guides and reference narrative | Keep one canonical owner per subject |
| `site/scripts/generate-cli-reference.mjs` | CLI reference generation | Change examples/source inputs, regenerate; do not hand-edit generated commands |
| `site/scripts/dump-openapi-spec.sh` | OpenAPI artifact | Use live source generation/check for contract changes; no manual spec patches |
| `site/public/llms.txt` | Curated machine entry | Edit by hand; it is not generated |
| `site/scripts/generate-llms-txt.mjs` | Markdown siblings and full corpus | Verify generated output after build |
| `site/scripts/generate-social-images.mjs` | Social image source | Correct copy in source, regenerate PNG |
| `README.md`, `PRODUCT.md`, `DESIGN.md` | Repo introduction and current site direction | Align deliberately; do not erase historical blueprint decisions |

## Static evidence must stay distinct from a live runtime

The mailbox diagram and task walkthrough use synthetic fixture data. Any static command transcript must say whether it is captured output or an illustrative sequence. A proposed interaction is labelled as a comp. The site must never display simulated mail as a visitor's real mailbox, execute mail commands, or ask for credentials. A live demo server is outside this scope. (D-WEB-014, D-WEB-019, D-WEB-023)

## Preserve paths and update navigation atomically

Move sidebar entries, not files, wherever possible. Add the new agent tutorial at `/getting-started/agent-quick-start/`; add an outcome recipe at `/guides/invoice-workflow/` only after the fixture proves its full path. Existing Why mxr and For agents routes remain the owners of their subjects.

If a route genuinely must move, put its redirect in `site/astro.config.mjs`, then check HTML, Markdown sibling, fragment links and llms.txt. Astro HTML redirects alone do not prove that an old `.md` endpoint works. Prefer keeping the source route for this refresh. Preserve the existing desk and desktop redirects. (D-WEB-011)

Unresolved questions: Q4 assumes the existing stack and routes; S5 verifies built outputs and redirects.
