# DESIGN.md

Visual system documentation, derived from the existing
`site/src/styles/custom.css` and used by impeccable for on-brand
output. Format follows the [Google Stitch DESIGN.md
schema](https://stitch.withgoogle.com/docs/design-md/format/).

## Theme

**Terminal-utilitarian.** GitHub Dark base with green-amber-cyan
accents drawn from terminal palettes (think tmux status line, vim
ricers, Catppuccin / Tokyo Night sympathies but more restrained).
Light theme available; dark is the default the design is tuned
for.

The scene sentence: *a developer at 11pm with the TUI in one tmux
pane and the marketing site open in their browser at 70% screen
width, deciding whether to install.* Both panes should feel like
they belong in the same project.

## Color

OKLCH-style relationships, expressed as hex tokens in CSS. Never
use `#000` or `#fff`; the surface always has a slight green-tinted
desaturation toward the brand hue.

### Tokens (dark theme — default)

| Role | Token | Value | Notes |
|---|---|---|---|
| Brand accent | `--mxr-green` | `#39d353` | GitHub-green; used on hover glows, primary CTA, lineage marks |
| Brand dim | `--mxr-green-dim` | `#1a6b29` | Lineage connector lines, subtle accents |
| Warning | `--mxr-amber` | `#e3b341` | Decoration tags, "HEAD →" pill, warning copy |
| Info | `--mxr-cyan` | `#58a6ff` | Years, tabular nums, secondary highlights |
| Error | `--mxr-red` | `#f85149` | Error copy only |
| Surface | `--mxr-surface` | `#0d1117` | Page background |
| Surface raised | `--mxr-surface-raised` | `#161b22` | Section panels, install card |
| Border | `--mxr-border` | `#30363d` | Panel borders |
| Text 1 | `--mxr-text-primary` | `#e6edf3` | Body and headings |
| Text 2 | `--mxr-text-secondary` | `#8b949e` | Captions, taglines |
| Text 3 | `--mxr-text-tertiary` | `#484f58` | Watermarks, root commit |

### Tokens (light theme overrides)

| Token | Value | Notes |
|---|---|---|
| `--mxr-green` | `#1a7f37` | Darker for contrast on white |
| `--mxr-green-dim` | `#dafbe1` | Lineage connector with stronger contrast |
| `--mxr-surface` | `#ffffff` | (close to white but tinted in practice) |
| `--mxr-surface-raised` | `#f6f8fa` | |
| `--mxr-border` | `#d0d7de` | |
| `--mxr-text-primary` | `#1f2328` | |
| `--mxr-text-secondary` | `#656d76` | |
| `--mxr-text-tertiary` | `#b1bac4` | |

### Color strategy

**Restrained.** Tinted neutrals carry 90%+ of the surface; green
accent appears on hover glows, primary CTA, lineage commit dots,
and the `> ` prompt prefix on section headers. Amber and cyan are
spot accents under 5% of any one screen.

This is a deliberate departure from the over-saturated brand pages
common in dev tools — mxr's surface is calm, the accents earn
their place.

## Typography

Two typefaces, both already loaded from Google Fonts.

### Stack

```css
--sl-font: 'IBM Plex Sans', system-ui, sans-serif;
--sl-font-mono: 'IBM Plex Mono', 'Cascadia Code', ui-monospace, monospace;
```

### Roles

| Role | Family | Weight | Notes |
|---|---|---|---|
| Hero headline | IBM Plex Mono | 700 | clamp(2.2rem, 5.5vw, 3.5rem); tight letter-spacing |
| Section header | IBM Plex Mono | 600 | clamp(1.3rem, 3vw, 1.7rem); preceded by green `> ` prompt |
| Body | IBM Plex Sans | 400 | line-height 1.7; max-width 60ch |
| Caption / tagline | IBM Plex Sans | 400 italic | secondary text color |
| Code / metadata | IBM Plex Mono | 400-500 | inline numbers, install commands, lineage entries |
| Tabular numbers | IBM Plex Mono | 500 | `font-variant-numeric: tabular-nums` for years |

### Anti-rules

- **No Inter, Roboto, Space Grotesk, system-ui** as primary
  display. Plex is the chosen distinctive voice.
- **Hierarchy through scale + weight.** No three-color gradients
  on text. No `background-clip: text`.

## Spacing & rhythm

8px base. Section padding goes 2.5rem block; inner spacing is
freer (0.35rem to 1.5rem) to create rhythm rather than a flat
8/16/24/32 grid.

Sections sit max-width 64rem centered, with breathing room before
the next section header (`> `) starts.

## Components & motifs

### Section panels

Some sections sit in a bordered raised panel
(`background: var(--mxr-surface-raised); border: 1px solid var(--mxr-border)`).
A scanline texture (1.2% green over 4px stripe) overlays the panel.
Top-right corner often has a small uppercase watermark
(`INSTALL`, `LINEAGE`, `git log --graph --decorate genre/...`)
that reinforces the terminal idiom.

### Section headers

Every `<h2>` in `.landing-section` is preceded by a green `> `
shell-prompt prefix (`::before` content). Mono, weight 600.

### Hero

Large mono headline, second line in `--mxr-green` with a soft
text-shadow glow. Subtle scanline backdrop. Primary CTA is a
solid green button with black text; secondary CTA is minimal-
border.

### Lineage / git-log treatment

Specialized component class set (`.lineage-log`,
`.lineage-entry`, `.lineage-graph`, etc.) that renders the
lineage section as a literal `git log --graph` view. Vertical
green-dim connector line, commit dots in green, year stamps in
cyan tabular-nums, italic taglines, additions vs inheritance
distinguished by `+` (green) vs `→` (cyan). Staggered fade-in
animation honors prefers-reduced-motion.

### Cards (Starlight `Card` / `CardGrid`)

Bordered panels matching the section style. Hover state uses a
green glow rather than elevation (terminal idiom — terminals
don't have shadows).

### Install grid

Three-column layout (collapses to 1 on mobile) showing each
install method as a `.install-method` block. Code block beneath.
Below the grid: a 3-line first-run sequence (`accounts add` →
`sync` → `mxr`).

## Motion

CSS-only, ease-out-quart curves, never bounce or elastic. One
high-impact orchestrated moment (lineage section fade-in
staggered 60ms apart). Subtle hover glows on interactive
elements. Honors `prefers-reduced-motion`.

Forbidden: layout-animating properties, scattered micro-
interactions on every element, parallax scrolling, scroll-jacked
section transitions.

## Backgrounds

The page background is the dark surface tint, not flat black.
Hero has a faint repeating scanline overlay (2-4px stripes,
green at 1.2-1.5% alpha). Section panels repeat the scanline.
No noise textures, no abstract gradients, no animated background
shaders.

## Forbidden defaults (the AI-slop guardrails)

- Side-stripe `border-left` greater than 1px on cards as accent.
- `background-clip: text` gradient text.
- Glassmorphism / `backdrop-filter: blur(...)` as default.
- Hero metric template (big number + small caption + three stats).
- Identical card grids stamped out for "features."
- Modals where progressive disclosure would do.
- Em dashes in copy.
