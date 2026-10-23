# PRODUCT.md

Strategic context for design work. Answers who, what, why.

## Register

**brand** — the landing page, marketing site, splash content, GitHub
README, install pages. Design IS the product here; the site has to
do positioning work, not serve a logged-in workflow. The human clients themselves
use a separate **product** register, but it's not the focus of design
work right now.

## Users

The primary audience is people who already use an agent and want it to
work with a mailbox they own locally. The website should explain that
path first, then make the human and developer surfaces clear. Three
profiles, in rough order of priority:

1. Existing-agent users want an agent to search and work with synced
   mail, then review shared work in a human client.
2. People who want a local mail runtime want provider mail on their
   computer and a human interface alongside scripts or agents.
3. Tool builders want to reuse the daemon's sync, search, mail
   operations and applicable policy when they add another client.

Some users prefer the terminal; the product and website do not assume
it is everyone's primary interface.

## Purpose

mxr is a local mail runtime for you and your agents. It keeps a local
mailbox of synced provider mail while the provider continues to sync
and receive supported changes. A daemon exposes shared mail operations
to the CLI, TUI, web app, MCP server and scripts; each client keeps its
own authority. Search and cached mail can be used locally. How much
history and which attachments are available depends on sync completion
and what has been cached.

## Outcomes the page must produce

In priority order:

1. A visitor with an existing agent understands that agent and human
   clients can use the same daemon-backed mailbox, each with its own
   authority.
2. A visitor sees a task over synced mail and understands the limits
   of incomplete sync and uncached content.
3. A visitor can try the synthetic demo and existing-agent walkthrough
   before connecting a personal account.
4. A visitor can choose a client and find install and reference details.

## Brand personality

- **Editorial, with technical detail in its place.** The website
  presents the runtime in plain language, then uses command examples
  and references where readers need them. Readers can choose a client.
- **Restrained, then sharply punctuated.** Deep blue and pale blue
  surfaces with cyan and small yellow accents. Color earns its place.
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
- **Generic developer-tool fonts.** The site uses its self-hosted
  Recursive variable font across display, body and code roles.
- **Comparison matrices that pit mxr against named competitors.**
  Adversarial framing tests as bad positioning. Lineage section
  replaces this.
- **"All-in-one"** as a claim. Table stakes for any MUA, fails the
  trade-off test.
- **Em dashes** in copy. The user reviews for these.

## Strategic principles

From CLAUDE.md and confirmed in this session.

1. **Lead with the runtime's user value.** Explain local ownership,
   synced history, common mail operations and human review. The daemon,
   search index and protocol are the mechanism; introduce them after
   the visitor understands the task.
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
6. **Local ownership and agent access lead together.** A synced local
   mailbox gives people, scripts and agents shared mail state. State
   these benefits without measured speed or development-cost promises.

## Accessibility & inclusivity needs

- **Keyboard access.** Keep the website usable from the keyboard, and
  present the TUI alongside other product surfaces.
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

## Current direction

The approved category is “A local mail runtime for you and your
agents.” The homepage headline is “Put your whole mail history to
work.” Existing-agent visitors come first. Website design guidance
applies to the site and does not imply a redesign of the application.
