# mxr — Addendum: Documentation Site Infrastructure

> Amendment A010. This document defines the documentation site strategy, framework choice, and content structure.

---

## A010: Documentation Site Infrastructure

**Affects**: 13-open-source.md, 14-roadmap.md

**What was missing**: The blueprint mentions documentation but does not specify how docs are built, hosted, or structured for end users.

---

## Framework: Astro Starlight

**Chosen**: [Astro Starlight](https://starlight.astro.build/)

**Why Starlight**:

- Static site generation (SSG) — no runtime, no server, fast.
- Markdown-native — docs are plain `.md` files. No proprietary format.
- Built for documentation — sidebar navigation, search, versioning, dark mode out of the box.
- Astro ecosystem — components when needed, zero JS by default.
- Active maintenance and growing adoption (Astro is well-funded).

**Considered and rejected**:

- **mdBook**: Rust-native but limited. No sidebar customization, no component support, no built-in search worth using.
- **Docusaurus**: React-based, heavy. Overkill for a CLI tool's docs.
- **MkDocs / Material**: Python dependency. Good but Starlight is better for this use case.
- **VitePress**: Vue-based. Good but Starlight's doc-specific features are stronger.

---

## Current site structure

The site lives in the `site/` directory at the repo root.

```
site/
  astro.config.mjs
  package.json
  scripts/
    generate-cli-reference.mjs
    dump-openapi-spec.sh
    validate-docs.mjs
    generate-llms-txt.mjs
  src/
    content/
      docs/
        getting-started/
          install.md
          quick-start.md
          gmail-setup.md
          imap-smtp-setup.md
          first-sync.md
        guides/
          mailbox.md
          triage-flow.md
          compose.md
          search.md
          rules.md
          web-app.md
          security-and-privacy.md
          for-agents.md
          ...
        reference/
          cli/
            *.md
          bridge.md
          config.md
          keybindings.md
          json-output.md
          tui.md
          ...
    pages/
      reference/api-explorer.astro
      privacy.md
      terms.md
```

### Section purposes

| Section | Audience | Content |
|---|---|---|
| Getting Started | New users | Install, authenticate, first sync - zero to reading email in 5 minutes |
| Daily Use | Active users | Mailbox, triage, compose, search, labels, web app, recipes |
| Power Features | Power users | Follow-ups, rules, LLM, semantic search, analytics, activity log |
| Concepts | Evaluators and contributors | Why mxr, architecture, security, glossary, agent contract |
| Building on mxr | Integrators | Adapter development, agent skill, public Rust crates |
| Reference | Power users and agents | Generated CLI pages, config, JSON output, TUI, keybindings, bridge, API explorer |

---

## Deployment

**Host**: Vercel

**Why Vercel**:

- Free tier covers open source projects.
- Automatic preview deployments on PRs.
- Edge CDN. Fast globally.
- Zero config for Astro — first-class adapter support.

**Build**:

```bash
cd site && npm run build
```

**Deploy trigger**: Push to `main` branch, changes in `site/` directory.

**Public URL**: `https://mxr-mail.vercel.app` (configured in
`site/astro.config.mjs`).

Local checks:

```bash
npm --prefix site run generate
npm --prefix site run validate
npm --prefix site run build
```

---

## Privacy & Terms pages

The `site/src/pages/privacy.md` and `site/src/pages/terms.md` pages are the public legal pages for Google OAuth verification and users evaluating mxr.

The root `PRIVACY.md` and `TERMS.md` files remain the repository-facing copies.
The site pages carry Astro frontmatter and use `site/src/layouts/legal.astro`, so
they are maintained as parallel Markdown pages rather than copied by a build
script. When policy text changes, update both the root file and the matching
site page in the same PR.

Local check:

```bash
cd site
npm run build
```

---

## Content Strategy

### Docs alongside implementation

Documentation is written as features are implemented, not after. Each implementation PR that adds user-facing functionality should include corresponding doc updates.

### Generated references

The CLI reference is generated from clap help output. The API reference
is generated from the bridge OpenAPI example. The docs build runs both
before validation.

```bash
npm --prefix site run generate
```

`generate-cli-reference.mjs` owns the human examples added to generated
CLI pages. Do not hand-edit generated command pages for wording drift;
change the generator input instead.

### Writing guidelines

The full writing principles — voice, page shape, recipe pattern, anti-
patterns, reviewer checklist — live in [`docs/guides/writing-docs.md`](../guides/writing-docs.md).
The headline rules are:

- Use second person ("you") not third person ("the user").
- Show the command first, explain after. Terminal users want to copy-paste.
- Every section ends with at least one runnable `mxr` invocation.
- Every CLI command, every flag, every config key gets a page.
- Recipes are mandatory where composition is possible.
- One Diátaxis quadrant per page (tutorial / how-to / reference / explanation).
