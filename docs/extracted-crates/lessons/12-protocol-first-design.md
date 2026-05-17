# Protocol-first design as an architectural forcing function

Captured 2026-05-17 during the Mail Sync Protocol (MSP) spike.
Companion to lessons 09 (carve-outs) and 11 (build-from-spec).

## The pattern

When the obvious framing is "extract this code as a library," ask
the next question: **could it be a wire protocol instead?**

A library locks adoption to one language. A protocol opens it to
any language. The cost looks higher but the leverage is bigger.

More importantly: **the spec-drafting exercise has value even if
no protocol gets published.** Articulating the contract from
outside the system surfaces internal coupling that's invisible
from inside.

## When this applies

- The "library" candidate has wide potential adoption (multiple
  language ecosystems would use it).
- The provider/runtime variants are messy enough that a library
  would have to either pick a winning shape or expose all variants.
- mxr (or whatever the seed system is) already has *something
  like* the right architecture but the boundaries were emergent,
  not designed.
- You can name a precedent: DAP for debugging, LSP for code
  intelligence, JMAP for mail, MCP for AI tooling.

## When it doesn't apply

- The seed is a focused, narrow contract (RFC 2369 unsubscribe
  parser, JWZ threading algorithm). Wire protocols are overkill;
  a Rust library is the right shape. Lessons 09 and 11 cover this
  case.
- The seed has only one obvious consumer profile. If only mxr-
  shaped clients would adopt it, the protocol overhead is a tax.
- The behaviour is fundamentally local-only (no remote state
  reconciliation). Protocols pay off when there's a wire to cross.

## How to run a protocol-first design exercise

Five phases. The full version of the MSP spike took ~1 focused day
agent-assisted.

### Phase 1 — Read the precedents

Pick 2-3 protocols in adjacent domains. For mail-sync, we read DAP,
LSP, and JMAP. For each: framing, lifecycle, capabilities,
error model, what's awkward.

The synthesis matters more than the reading. After 1 hour, write
down **5-7 specific design choices** the new protocol should make,
borrowing what works and avoiding what doesn't. These are
opinionated, not descriptive.

### Phase 2 — Draft the spec

~3-4 hours. Sections: motivation, abstract model, wire shape,
capabilities, resumability, push (if applicable), conformance,
open questions, prior-art comparison. 5-10 pages.

The abstract model is the most important part. **It should be
small enough that every adapter can map its provider into it, and
every client only sees it.** mxr-shaped (or seed-system-shaped)
concepts that don't generalise must live above the protocol.

The biggest temptation here is to over-spec. v0.1 should explicitly
punt on hard adjacent problems (auth, calendar, encryption,
…) and list them as Open Questions.

### Phase 3 — Audit the seed system against the spec

~2 hours. For each section of the spec, where does the seed
already match? Where doesn't it? What's the refactor cost
(cheap/medium/expensive)?

This is the section that produces **immediate architectural value
even if the protocol never ships.** Every gap is a place where the
seed's emergent boundaries don't match what clean would look like.
Most gaps are hygiene improvements the seed would benefit from
regardless.

Use the CLI to drive part of this audit (per the agent workflow):
run the seed's actual sync/process flow and trace the events.
Where the spec's expected events don't match the seed's actual
events, that's a real gap, not a paper gap.

### Phase 4 — Draft a blog post

~2 hours. Aim for ~2000 words. Hook → precedent analogy → the
proposal → what you're offering → what you're asking for → what
this is not.

The blog post is **forcing function for clarity**. Writing for an
external audience exposes hand-waving in the spec. If you can't
explain the value proposition in a paragraph, the proposal needs
more work.

Don't publish yet. Sit on it.

### Phase 5 — Write the verdict

~1 hour. One of four shapes:

1. **Spec only, internal.** The spec is a useful internal
   architecture document; refactor toward it; don't publish.
2. **Spec + blog, no implementation push.** Publish to surface
   feedback; refactor seed toward alignment; don't commit to
   reference adapters yet.
3. **Spec + blog + reference adapter.** Ship a reference
   implementation alongside the spec.
4. **Full ecosystem push.** Working group, conformance suite,
   adapter zoo. Multi-quarter. Needs co-leads.

The verdict is informed by what the spec, audit, and blog actually
look like after writing them. Don't pre-decide.

## Rules

**Rule 1: The spec exercise pays off whether or not you ship the
protocol.** The alignment audit alone is worth the work.

**Rule 2: Specs and implementations have asymmetric publishing
bars.** A spec draft is cheap and risk-free. A published reference
adapter is expensive and triggers full publishing-bar review
(lesson 10). Don't conflate the costs.

**Rule 3: The seed's existing emergent architecture is a hypothesis
about the right shape.** Articulating it as a spec lets you test
the hypothesis. Sometimes the emergent shape is wrong; that's
useful information. More often the emergent shape is mostly right
but with rough edges; the spec exercise lets you sand them.

**Rule 4: A blog post is the cheapest external signal.** Before
committing to multi-month implementation work, publish a spec
draft + blog post and see who responds. The publishing-bar
discipline still applies; the spec just lets you publish a smaller
thing first.

**Rule 5: Don't lead a protocol effort alone.** If the verdict is
shapes 3 or 4, find co-leads before escalating. Solo protocol
maintenance burns out.

## What the MSP spike taught us

Specifically, three things worth carrying forward:

1. **mxr's daemon was already 80% MSP-shaped.** We didn't know
   that until we wrote the spec down. The spike paid for itself in
   "okay we know what 'clean' looks like, here's a 10-day refactor
   list."

2. **Opaque cursors win.** mxr's `SyncCursor` enum baked
   provider-specific variants into the daemon. JMAP's opaque
   `state` token shows how to invert the dependency — adapter
   chooses the format, client persists bytes. The fix is a 2-day
   refactor in mxr; the architectural payoff is decoupling the
   daemon from "which providers exist."

3. **Flat boolean capability soup ages badly.** DAP did this; we
   carried it through into `SyncCapabilities` without thinking.
   LSP's namespaced tree is strictly better and the rename is
   cheap. Worth doing pre-emptively in any protocol-shaped Rust
   trait we ship.

## Companion lessons

- **Lesson 09** — carve-outs from existing crates. Library framing.
- **Lesson 10** — the publishing bar. Applies asymmetrically to
  specs vs implementations per Rule 2 above.
- **Lesson 11** — build-from-spec carve-outs. When the seed is
  thin. The MSP spike is a generalisation: when the seed is one
  system that should become a protocol.

Together, these four lessons cover the four corners of the
extraction matrix:

|                          | Code-lift seed   | Build-from-spec seed |
|--------------------------|------------------|----------------------|
| **Single-language**      | Lesson 09        | Lesson 11            |
| **Multi-language wire**  | (no equivalent)  | Lesson 12 (this)     |

The bottom-left cell (multi-language wire protocol from existing
code) is unusual — DAP/LSP/JMAP all came from spec-first work,
not from extracting one implementation. If you find yourself in
that cell, you're probably actually in the bottom-right cell with
some salvageable internals; treat the existing code as one input
among many.
