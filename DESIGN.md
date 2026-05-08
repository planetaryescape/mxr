# DESIGN.md

Visual system documentation, derived from `site/src/styles/custom.css`
and used by impeccable for on-brand output. Format follows the
[Google Stitch DESIGN.md schema](https://stitch.withgoogle.com/docs/design-md/format/).

## Theme

**Warm-terminal, editorial.** Dark-default with a warm not-black
surface (`#0d0d0c`, never `#000`), single saturated sodium accent
(`#ff7a3d`), tabular cyan (`#5eead4`) for time/metadata. Light
theme inverts the warmth: paper-cream surface, ink text, deeper
ember accent. The vocabulary is publishing-meets-shell: column
rules, section folios (`§ 01`), shell prompts, monospace as
texture, no cards, no panels, no grain, no gradients.

The scene sentence: *a developer at 11pm with the TUI in one tmux
pane and the marketing site open in their browser at 70% screen
width, deciding whether to install.* Both panes should feel like
they belong in the same project. The site is the documentation of
the tool, in prose form.

## Color

OKLCH-style relationships, expressed as hex tokens in CSS. Never
use `#000` or `#fff`; the surface always has a slight warm
desaturation toward the brand hue.

### Tokens (dark theme — default)

| Role | Token | Value | Notes |
|---|---|---|---|
| Surface | `--ink` | `#0d0d0c` | Page background, warm not-black |
| Surface raised | `--ink-soft` | `#15140e` | Code blocks, install command background |
| Surface fold | `--ink-fold` | `#1a1813` | Inline code, kbd elements |
| Text 1 | `--paper` | `#f3eee0` | Body, headings, primary text |
| Text 2 | `--paper-soft` | `#d6cebc` | Tagline, prose |
| Text 3 | `--paper-mute` | `#8a8273` | Captions, masthead, copy button label |
| Text 4 | `--paper-faint` | `#524e44` | Folio numbers, tertiary metadata |
| Rule | `--rule` | `#2c2920` | Section dividers, panel borders |
| Rule soft | `--rule-soft` | `#1f1d16` | Inner row dividers |
| Brand accent | `--signal` | `#ff7a3d` | Sodium orange; primary CTA, prompts, hover |
| Brand soft | `--signal-soft` | `#2b1c10` | Selection background, copy-success state |
| Brand deep | `--signal-deep` | `#ffb487` | Inline code color, accent-high |
| Quiet | `--quiet` | `#5eead4` | Cyan; tabular nums (years, dates), agent labels |

### Tokens (light theme)

| Token | Value | Notes |
|---|---|---|
| `--ink` | `#f5f0e2` | Paper-cream surface |
| `--ink-soft` | `#ebe3cf` | Raised surface, code blocks |
| `--ink-fold` | `#e1d8c2` | Inline code |
| `--paper` | `#1a1814` | Primary text |
| `--paper-soft` | `#4d4740` | Body |
| `--paper-mute` | `#7a7064` | Captions |
| `--paper-faint` | `#a59c8b` | Folios |
| `--rule` | `#c8bda7` | Dividers |
| `--rule-soft` | `#ddd2bb` | Inner rows |
| `--signal` | `#c2410c` | Deeper ember on warm-paper background |
| `--signal-soft` | `#f1dac8` | |
| `--signal-deep` | `#7c2a08` | |
| `--quiet` | `#1e5e6e` | |

### Color strategy

**Restrained.** Warm neutrals carry 90%+ of the surface. The
sodium orange `--signal` appears on shell prompts, primary CTA,
hover states, hero accent, lineage `HEAD`, install commands, and
`+` deltas in the lineage section. Cyan `--quiet` is a spot
accent for tabular-nums (years, dates) and agent labels — under
3% of any one screen.

This is a deliberate departure from the GitHub-green +
saturated-everything template common to dev tools. mxr's surface
is calm; the accents earn their place.

## Typography

**One typeface.** [Recursive](https://www.recursive.design/) variable
font, loaded from Google Fonts. Variation axes (`wght`, `CASL`,
`MONO`, `slnt`) generate every voice the page needs without
loading a second family.

### Stack

```css
--font-display:  'Recursive', ui-sans-serif, system-ui, sans-serif;
--font-body:     'Recursive', ui-sans-serif, system-ui, sans-serif;
--font-mono:     'Recursive', ui-monospace, monospace;
```

### Variation axis presets

| Preset | Variation | Use |
|---|---|---|
| `--rec-display` | `wght 800, CASL 0, MONO 0, slnt 0` | Hero h1, section h2 |
| `--rec-display-italic` | `wght 800, CASL 0, MONO 0, slnt -10` | Hero accent span, section em |
| `--rec-headline` | `wght 600, CASL 0, MONO 0, slnt 0` | h3, principle headings |
| `--rec-body` | `wght 400, CASL 0.6, MONO 0, slnt 0` | Prose body |
| `--rec-body-em` | `wght 600, CASL 0.6, MONO 0, slnt 0` | strong, emphasis |
| `--rec-body-italic` | `wght 400, CASL 0.6, MONO 0, slnt -10` | Italic prose |
| `--rec-meta` | `wght 500, CASL 0, MONO 1, slnt 0` | Mastheads, folios, labels (all-mono) |
| `--rec-mono` | `wght 400, CASL 0, MONO 1, slnt 0` | Code, CLI examples |
| `--rec-mono-em` | `wght 600, CASL 0, MONO 1, slnt 0` | Bold mono accents |

### Roles

| Role | Variation | Notes |
|---|---|---|
| Hero headline | `--rec-display` | clamp(2.8rem, 10vw, 8.5rem); line-height 0.94; letter-spacing -0.05em |
| Section header | `--rec-display` | clamp(1.6rem, 3vw, 2.4rem); ruled top border in `--paper` |
| Body | `--rec-body` | line-height 1.65; max-width 38rem |
| Mono / code | `--rec-mono` | font-feature-settings 'ss01', 'ss02', 'ss03' globally |
| Tabular numbers | `--rec-mono` + `font-variant-numeric: tabular-nums` | Lineage years, eulogy dates |

### Anti-rules

- **No Inter, Roboto, Space Grotesk, IBM Plex, system-ui** as
  primary display. Recursive is the chosen distinctive voice; one
  variable font covers display, body, and mono.
- **Hierarchy through scale + weight + variation axes.** No
  `background-clip: text` gradients. No three-color gradient text.
- **Em dashes are banned** in copy. Use commas, colons, periods,
  parentheses. Also not `--`.

## Spacing & rhythm

8px base, but rhythm comes from variation, not from a flat 8/16/24
grid. Section padding goes `clamp(3rem, 7vw, 6rem)` block;
inner spacing ranges 0.4rem to 1.5rem to create breathing.

Sections sit `max-width: 84rem` centered with
`padding-inline: clamp(1.25rem, 4vw, 4rem)`. Body prose caps at
`max-width: 38rem`. Hero h1 caps at `max-width: 16ch`. Long-form
prose stops at 65 to 75ch.

## Components & motifs

### Section folios

Every `.landing-section[data-folio]` gets a `§ 01`-style folio
number rendered via `::before` with the `data-folio` attribute,
in mono-meta variation, lowercase, 0.72rem. This is the editorial
taxonomy move (Linear-inspired).

### Section headers

Each `<h2>` in `.landing-section` has a 1px top border in
`--paper` and 0.75rem padding-top. Display variation, weight 800,
italics get the sodium accent and `slnt -10`. No shell-prompt
prefix on h2 (folios cover that role).

### Hero

Large display headline (clamp 2.8rem to 8.5rem) on warm ink,
second line in `--signal` italicised via `--rec-display-italic`.
Above the headline: a small mono masthead
(`mxr · 0.4.72 · ◈ a notebook for your inbox`) on a 1px
`--paper` top rule. Below: the tagline, then mono action links
with `↗` and `→` arrows that translate on hover.

### Hero install row

Single mono line directly under the hero actions:
`$ brew install planetaryescape/mxr/mxr [copy]`. `--ink-soft`
background, 1px `--rule` left border (the only "stripe" allowed
under DESIGN guardrails), copy button toggles to `--signal`
on success. A trailing micro-meta line links to the full install
grid in §01.

### Provider line

Single typographic statement replacing logo-wall conventions:
`works with Gmail, any IMAP server, any SMTP relay ◈ tested
with Fastmail, Migadu, Proton Bridge`. Sits in the same horizontal
rhythm as the section folios. No marquee, no animation, no
gradient overlay.

### Install grid (§01)

Three methods inline (Homebrew, Cargo, binaries) under a single
masthead. A run-line below shows the three first-run commands:
`mxr accounts add` → `mxr sync` → `mxr`. No card chrome.

### Search section (§03)

Query renders as a typographic event: `$` prompt in `--signal`,
command in `--paper`, query string in `--signal-em`. Results
render as a long ruled list with no card. Date in `--quiet`
tabular-nums, sender in `--paper` mono-em, subject in `--paper`
sans, attachment glyph in `--paper-mute`. No marketing latency
display in the header.

### Agent transcripts (§04)

Italic `--rec-body-italic` prompt with a 2px `--paper` left
border (quote-block convention, not decorative accent). Mono
command runs as marginal notes prefixed by `$`. Plain-language
result with `--signal` success span. The closing JSON peek
(`◈ what your agent sees`) shows real schema from
`crates/daemon/src/commands/search.rs`: `message_id`, flat
`from` string, `date` RFC 3339, `read`, `starred`, `score`.

### Eulogy (§05)

Two-column: ledger on the left with real ink-strikethrough lines
on each killed Google product, a pending `?` row, prose on the
right. No card. The strikethrough is a CSS pseudo-element, not a
text decoration, so it persists across line wraps.

### Lineage / git-log (§09)

Specialised component class set (`.lineage-log`, `.lineage-entry`,
`.lineage-graph`, `.lineage-decoration`, `.lineage-tool`,
`.lineage-year`, `.lineage-tagline`, `.lineage-deltas`,
`.lineage-inherits`) renders the section as a literal `git log
--graph` view. Vertical `--rule` connector line, commit dots
(●○◌) in `--signal` / `--paper-mute`, year stamps in `--quiet`
tabular-nums, italic taglines, `+` (signal) for mxr's additions
vs. `→` (cyan) for inherited features. Staggered fade-in
animation 60ms apart, honors `prefers-reduced-motion`.

## Motion

CSS-only, ease-out curves, never bounce or elastic. One
high-impact orchestrated moment (lineage section staggered
fade-in). Subtle hover transitions on links, action arrows, and
copy buttons. Honors `prefers-reduced-motion`.

Forbidden: layout-animating properties, parallax scrolling,
scroll-jacked transitions, marquee text, animated background
shaders.

## Backgrounds

Flat warm ink, no gradient overlays, no noise, no scanlines, no
glassmorphism. The page is a publication on dark paper. Texture
comes from typography, ruled lines, and column rhythm.

## Forbidden defaults (the AI-slop guardrails)

- Side-stripe `border-left` greater than 1px on cards as a
  decorative accent. The 2px border on `.agent-prompt` is a
  blockquote convention in `--paper` body color, not an accent
  stripe.
- `background-clip: text` gradient text.
- Glassmorphism / `backdrop-filter: blur(...)` as default.
- Hero metric template (big number + small caption + three stats).
- Identical card grids stamped out for "features."
- Modals where progressive disclosure would do.
- Marquees / scrolling text strips that act as logo walls in
  disguise.
- Em dashes in copy.
