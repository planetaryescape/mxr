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

## Site Structure

The site lives in the `site/` directory at the repo root.

```
site/
  astro.config.mjs
  package.json
  src/
    content/
      docs/
        getting-started/
          install.md
          gmail-setup.md
          first-sync.md
        guides/
          search-syntax.md
          keybindings.md
          compose.md
          rules.md
          scripting.md
          byoc-oauth.md
        reference/
          cli.md
          config.md
          keybindings.md
          ipc-protocol.md
        contributing/
          index.md
          architecture.md
          development.md
    pages/
      privacy.md
      terms.md
```

### Section purposes

| Section | Audience | Content |
|---|---|---|
| Getting Started | New users | Install, authenticate, first sync — zero to reading email in 5 minutes |
| Guides | Active users | Task-oriented: how to search, how to script, how to write rules |
| Reference | Power users | Exhaustive: every CLI flag, every config key, every keybinding |
| Contributing | Developers | Architecture overview, dev setup, how to add a provider |

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

**Custom domain**: `mxr.dev` or `docs.mxr.dev` (TBD based on domain availability).

---

## Privacy & Terms Pages

The `site/src/pages/privacy.md` and `site/src/pages/terms.md` pages mirror the root `PRIVACY.md` and `TERMS.md` files. These serve as the publicly accessible URLs required for Google OAuth verification.

**Sync strategy**: The site build copies content from root `PRIVACY.md` and `TERMS.md` into the site pages with appropriate frontmatter. Single source of truth is the root files.

---

## Content Strategy

### Docs alongside implementation

Documentation is written as features are implemented, not after. Each implementation PR that adds user-facing functionality should include corresponding doc updates.

### CLI reference auto-generation

The CLI reference page is auto-generated from clap's help output. A build script extracts help text for every subcommand and formats it as Markdown. This ensures the reference is always in sync with the actual CLI.

```bash
# Generate CLI reference during site build
cargo run -- --help-all > site/src/content/docs/reference/cli-raw.txt
# Build script converts to structured Markdown
node site/scripts/generate-cli-reference.mjs
```

### Writing guidelines

- Use second person ("you") not third person ("the user").
- Show the command first, explain after. Terminal users want to copy-paste.
- Every guide should have a "what you'll need" section upfront.
- Keep pages focused. One task per guide. Link to reference for exhaustive details.
