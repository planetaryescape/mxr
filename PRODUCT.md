# PRODUCT.md

Strategic context for design work. Answers who, what, why.

## Register

**brand** — the landing page, marketing site, splash content, GitHub
README, install pages. Design IS the product here; the site has to
do positioning work, not serve a logged-in workflow. The TUI itself
is a separate **product** register, but it's not the focus of design
work right now.

## Users

Technical users who already use the terminal as a primary work
surface. Three profiles, in rough order of priority:

1. **Power-user developers** — work in vim/neovim, tmux, fish/zsh,
   know what BM25 is, build on top of CLI tools, keep email open
   all day. They've tried mutt or aerc; some have moved on, some
   still use them. They distrust SaaS that holds their data.
2. **AI agent operators** — running coding agents (Claude, Cursor,
   Aider) and want their agent to do email properly. Will install
   a tool because their agent will run it, not because they will.
3. **Self-hosters and offline-first folks** — value data sovereignty,
   plane-friendly tools, no telemetry. Adjacent to the homelab
   community.

What they share: command line is home, JSON is the lingua franca,
every tool earns its keep, "blazing fast" is an insult.

## Purpose

mxr gives technical users their entire email locally — searchable,
scriptable, pipeable, agent-operable, offline-capable — without
forcing a switch away from Gmail or whatever IMAP server they
already use. It's a client that keeps a live two-way sync, plus a
daemon that exposes that mailbox to the TUI, the CLI, an HTTP
bridge, and any other surface the user wants to build.

## Outcomes the page must produce

In priority order:

1. **Visitor installs mxr.** Install is reachable in two scrolls;
   the three install methods (Homebrew, Cargo, prebuilt) are visible
   without leaving the page; the first-run command sequence is
   one block away.
2. **Visitor understands the four superpowers** — offline-capable
   archive, local-fast search and analytics, agent-operable, no
   lock-in either way. Each one resonates with a concrete scenario,
   not a feature list.
3. **Visitor sees the lineage.** mxr is the next step after mutt /
   neomutt / aerc / himalaya, not a replacement for them. We honor
   the work each contributed.
4. **Visitor with deeper questions has somewhere to go.** Technical
   detail (daemon architecture, search engine, provider adapters,
   pipeable JSON) lives in deeper sections for readers who care.

## Brand personality

- **Terminal-native, deliberately so.** Monospace as accent
  typography, shell-prompt motifs (`> ` headers, `$` prompts),
  scanline texture. The visual vocabulary nods to old hardware
  without playing dress-up.
- **Restrained, then sharply punctuated.** Mostly neutral surfaces
  with green accents. Color earns its place. Glow is intentional.
- **Honest, not performative.** "Show, don't compare." Concrete
  scenarios over adjective lists. No "blazing fast." Cite numbers
  or stay quiet.
- **Gracious about prior art.** mutt, neomutt, aerc, himalaya,
  notmuch are named, credited, and given specific attribution for
  what mxr inherits. Not adversarial.

## Anti-references

Strict avoidance list. If the design starts to look like any of
these, restart that section.

- **SaaS-cream landing pages.** White / off-white background,
  Inter / Roboto / system-ui body, purple-to-pink gradient hero,
  centered headline + two CTAs, identical feature card grid, three
  testimonial quotes in cards, small footer with company columns.
- **Agent-tool sludge.** "AI-native" / "agent-first" as a tagline.
  Robot icons. Chat-bubble illustrations. Hot pink + black + space
  imagery.
- **Hero-metric template.** Big number, small caption, three
  supporting stats below, gradient accent. The SaaS cliché.
- **Generic developer-tool fonts.** Inter, Space Grotesk, Roboto
  Mono everywhere. Use IBM Plex (already loaded) or earn a
  different distinctive choice.
- **Comparison matrices that pit mxr against named competitors.**
  Adversarial framing tests as bad positioning. Lineage section
  replaces this.
- **"All-in-one"** as a claim. Table stakes for any MUA, fails the
  trade-off test.
- **Em dashes** in copy. The user reviews for these.

## Strategic principles

From CLAUDE.md and confirmed in this session.

1. **Sell the fireball, not the flower.** Lead with user
   superpowers (offline archive, instant search, agent-operable,
   survives Gmail outages). The daemon, Tantivy, OpenAPI etc. are
   how — not what. Technical detail goes deeper, not at the top.
2. **Trade-off test on every claim.** Could a credible competitor
   claim the opposite? "Local-first" passes (cloud-first is a
   real choice). "Fast" fails (no one says "we're slow"). Fail
   the test → rewrite.
3. **Lineage is a real section, not a footnote.** Each predecessor
   gets specific attribution for what mxr inherits. mxr's own
   contributions render as a different visual class so they're
   clearly the new thing, not a re-pitch.
4. **Install lives near the top.** A reader who already trusts
   the project should be able to install in two scrolls.
5. **No lock-in framing in both directions.** "Gmail can die
   without you" + "you can keep using Gmail" form a pair. Both
   are about user control of their data.
6. **The agent angle is a use case, not a position.** Agent
   operability is a real superpower, but the page leads with
   offline / local / sovereignty. Agent is one of four sections,
   not the lead.

## Accessibility & inclusivity needs

- **Keyboard-first.** The TUI is the product; the site can match
  in spirit. No mouse-only interactions on the landing page.
- **prefers-reduced-motion respected.** The lineage section's
  staggered reveal already honors it; any new motion must too.
- **Light + dark themes.** The site already supports both via
  `:root[data-theme='light']` overrides. New design must work in
  both.
- **WCAG AA contrast minimum** for all body text against any
  surface (light or dark theme).
- **Mobile readable.** The landing page should work on a phone
  even though the product itself runs on a laptop. Mobile users
  read the marketing site too.

## Open questions for confirmation

These were inferred from project context. Confirm or correct.

1. **Anti-reference visual examples** — the list above is implicit
   in CLAUDE.md but never stated as a blocked aesthetic family.
   Add or remove entries.
2. **Aesthetic ambition for v0.5 landing** — the existing site is
   already terminal-utilitarian and distinctive. Do we want to
   *amplify* that direction (more retro, more CRT, more ASCII art),
   *refine* it (same vocabulary, sharper execution), or
   *contrast against it* (introduce a new feel for marketing while
   keeping product UI as is)? Default assumption: **refine.**
3. **Mascot / wordmark** — does mxr have a logo, mark, or
   typographic treatment? Not present in repo. Not required for
   this pass.
