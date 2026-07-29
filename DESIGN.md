# DESIGN.md

Visual system documentation, derived from `site/src/styles/custom.css`
and used by impeccable for on-brand output. Format follows the
[Google Stitch DESIGN.md schema](https://stitch.withgoogle.com/docs/design-md/format/).

## Theme

**Deep-ocean, editorial.** Dark-default with a navy ink surface
(`#071522`, never `#000`), bright cyan signal (`#57d5ff`), and a
small yellow cue (`#ffd166`). Light theme becomes pale blue paper
with deep navy text and a darker cyan accent. The vocabulary is
publishing-meets-shell: column rules, section folios (`§ 01`),
shell prompts, monospace as texture, no cards, no grain, no
gradients.

The scene sentence: *a developer at 11pm with the TUI in one tmux
pane and the marketing site open in their browser at 70% screen
width, deciding whether to install.* Both panes should feel like
they belong in the same project. The site is the documentation of
the tool, in prose form.

## Color

OKLCH-style relationships, expressed as hex tokens in CSS. Never
use `#000` or `#fff`; the surface always carries a little blue.

### Tokens (dark theme — default)

| Role | Token | Value | Notes |
|---|---|---|---|
| Surface | `--ink` | `#071522` | Page background, deep navy |
| Surface raised | `--ink-soft` | `#0d2234` | Code blocks, install command background |
| Surface fold | `--ink-fold` | `#132c42` | Inline code, kbd elements |
| Text 1 | `--paper` | `#f4f9fc` | Body, headings, primary text |
| Text 2 | `--paper-soft` | `#c7d8e5` | Tagline, prose |
| Text 3 | `--paper-mute` | `#8fa8bc` | Captions, masthead, copy button label |
| Text 4 | `--paper-faint` | `#587088` | Decoration and tertiary rules only |
| Rule | `--rule` | `#294963` | Section dividers, panel borders |
| Rule soft | `--rule-soft` | `#18354d` | Inner row dividers |
| Brand accent | `--signal` | `#57d5ff` | Cyan; primary CTA, prompts, hover |
| Brand soft | `--signal-soft` | `#0b3a4b` | Selection background, copy-success state |
| Brand deep | `--signal-deep` | `#bfefff` | Inline code color, accent-high |
| Quiet | `--quiet` | `#ffd166` | Yellow; small status and window cues |

### Tokens (light theme)

| Token | Value | Notes |
|---|---|---|
| `--ink` | `#f4f9fc` | Pale blue paper surface |
| `--ink-soft` | `#e5f0f6` | Raised surface, code blocks |
| `--ink-fold` | `#d7e7f0` | Inline code |
| `--paper` | `#10263b` | Primary text |
| `--paper-soft` | `#344f66` | Body |
| `--paper-mute` | `#536e84` | Captions |
| `--paper-faint` | `#8aa0b2` | Decoration only |
| `--rule` | `#b9cedc` | Dividers |
| `--rule-soft` | `#d7e5ed` | Inner rows |
| `--signal` | `#006f93` | Cyan on pale paper |
| `--signal-soft` | `#d5eff7` | |
| `--signal-deep` | `#004f6a` | |
| `--quiet` | `#7a5400` | |

### Color strategy

**Restrained.** Navy and pale blue carry most of the surface. Cyan
`--signal` appears on shell prompts, primary CTA, hover states,
the hero accent, and selected controls. Yellow `--quiet` is a
small status cue, under 3% of any one screen.

This is a deliberate departure from the GitHub-green +
saturated-everything template common to dev tools. mxr's surface
is calm; the accents earn their place.

## Typography

**One typeface.** [Recursive](https://www.recursive.design/) variable
font, self-hosted through Fontsource. Variation axes (`wght`, `CASL`,
`MONO`, `slnt`) generate every voice the page needs without
loading a second family.

### Stack

```css
--font-display:  'Recursive Variable', ui-sans-serif, system-ui, sans-serif;
--font-body:     'Recursive Variable', ui-sans-serif, system-ui, sans-serif;
--font-mono:     'Recursive Variable', ui-monospace, monospace;
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
| Tabular numbers | `--rec-mono` + `font-variant-numeric: tabular-nums` | Search dates, terminal timestamps |

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
italics get the cyan accent and `slnt -10`. No shell-prompt
prefix on h2 (folios cover that role).

### Hero

On wide screens the hero is split by a 1px column rule. The promise,
tagline, actions, provider support, and install command sit on the
left. A real CLI, TUI, or agent recording runs on the right.
The mono eyebrow names Gmail, Outlook, and IMAP so provider breadth
remains visible on short viewports before the detailed ledger.
The headline uses `--rec-display`; its second line uses `--signal`
and `--rec-display-italic`. On narrow screens the pieces stack.

### Hero install row

Single mono line directly under the hero actions:
`$ brew install planetaryescape/mxr/mxr [copy]`. `--ink-soft`
background, 1px `--rule` left border (the only "stripe" allowed
under DESIGN guardrails), copy button toggles to `--signal`
on success. A trailing micro-meta line links to the full install
grid in §07.

### Provider ledger

Three compact, unboxed rows in the hero show sync, send, and adapter
support. Gmail, Outlook / Microsoft 365, IMAP, and SMTP are visible
before the install command. The adapter row links to the documented
provider interface. No logo wall, marquee, or feature tags.

### Install grid (§07)

Three methods inline (Homebrew, Cargo, binaries) under a single
masthead. A run-line below shows the three first-run commands:
`mxr demo` → `mxr accounts add gmail` → `mxr`. No card chrome.

### Search section (§02)

Query renders as a typographic event: `$` prompt in `--signal`,
command in `--paper`, query string in `--signal-em`. Results
render as a long ruled list with no card. Date in `--quiet`
tabular-nums, sender in `--paper` mono-em, subject in `--paper`
sans, attachment glyph in `--paper-mute`. No marketing latency
display in the header.

### Agent transcripts (§03)

Italic `--rec-body-italic` prompt with a 2px `--paper` left
border (quote-block convention, not decorative accent). Mono
command runs as marginal notes prefixed by `$`. Plain-language
result with `--signal` success span. The closing JSON peek
(`◈ what your agent sees`) shows real schema from
`crates/daemon/src/commands/search.rs`: `message_id`, flat
`from` string, `date` RFC 3339, `read`, `starred`, `score`.

### Two-way sync (§04)

Three-column comparison: actions in another mail client, the provider
protocols in the centre, and the resulting state in mxr. Gmail,
Outlook / Microsoft 365, and generic IMAP remain visible.

### Client surfaces (§05)

Ruled rows explain the TUI, CLI, HTTP bridge, and MCP / agent
surfaces. Each row names the interface, its role, and one concrete
command or request. The section follows sync so the shared daemon
model answers how all interfaces see the same mailbox.

### Daily mail tools (§06)

A compact scan grid holds the useful features that do not need a full
section: editor compose, reader mode, follow-up queues, analytics,
mail rules, and saved searches.

## Motion

Ease-out curves, never bounce or elastic. The hero recording cycles
between agent, CLI, and TUI, pauses while hovered or focused, and has
a visible pause control. Subtle hover transitions remain on links,
action arrows, and copy buttons. Honors `prefers-reduced-motion`.

Forbidden: layout-animating properties, parallax scrolling,
scroll-jacked transitions, marquee text, animated background
shaders.

## Backgrounds

Flat navy ink, no gradient overlays, no noise, no scanlines, no
glassmorphism. The page is a publication on deep blue paper. Texture
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
