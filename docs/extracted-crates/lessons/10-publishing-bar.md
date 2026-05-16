# The publishing bar

Captured 2026-05-16 after a misstep: I almost recommended `format-flowed`
as the third extraction. The user pushed back — RFC 3676 is a 4-page spec,
the decoder is ~30 lines, anyone can implement it from the spec in an
afternoon. Shipping it as a standalone crate would have been npm-flavored
noise.

This file records the bar that should apply to every future candidate.
Re-read before nominating a new one.

## crates.io is not npm

The npm ecosystem normalised micro-packages: `is-odd`, `left-pad`, every
RFC implemented as its own three-line module. Some of that is cultural
inertia, some is structural (npm's flat dependency tree, JavaScript's
weak standard library). Whatever the cause, the result is a registry where
discoverability is poor and supply-chain risk compounds.

The Rust ecosystem norm is the opposite: a smaller number of
better-maintained crates, each justifying its existence with a real
contract. The standard library is fuller. Tooling like cargo-audit
penalises sprawl. A new crate is a maintenance promise.

The implication: a crate that would be unremarkable in npm-land can fail
the bar on crates.io. Specifically, anything described accurately by
"could be a Stack Overflow answer" should stay private.

## The three bar tests

A candidate ships only if it clears all three.

### 1. Real ecosystem gap

Either no crate exists, or the existing crates are abandoned, broken, or
narrowly scoped to a single use case the candidate wouldn't be locked
into. Re-validate this within ~6 months of acting on it — libraries
appear and die.

`list-unsubscribe`: no focused crate; `mail-parser` exposes the raw header
but no typed action.

`mail-threading`: only port (JS, 2013) is abandoned; nothing on
crates.io.

`gmail-query` (planned next): no comparable AST library on either
crates.io or npm.

`format-flowed` (won't-do): the gap is real but cosmetic — anyone who
wants this can write it.

### 2. Non-trivial work

"Implement in an afternoon from the spec" must be FALSE. The work might
be non-trivial because:

- The algorithm has subtle correctness traps (JWZ threading: phantom
  parents, subject-fallback ordering, same-subject root merging).
- The spec is large and interacts with other specs (`list-unsubscribe`:
  RFC 2369 + RFC 8058 + RFC 6068 + Gmail/Yahoo bulk-sender rules).
- The contract requires careful policy decisions (`gmail-query`: 20+
  operators, precedence, quoting rules, timezone semantics for date
  operators).
- The corpus to make the implementation credible is large
  (`reader-quote-sig`: needs multilingual quote markers, Outlook variants,
  classifier-grade signature detection — ~1-2 weeks of corpus work).

If the implementation can be derived from the spec by a competent person
in an afternoon, it does not justify a standalone crate. The right home
is `mxr-*` internal, where the maintenance cost is bundled with mxr's
own evolution.

### 3. Production-credible seed

The mxr code being extracted must already be production-credible. A
v0.1.0 placeholder that users would outgrow in their first month is worse
than no crate at all — it consumes the name, sets a low quality
expectation, and forces a v0.2.0 reshape that breaks every consumer.

`reader-quote-sig` failed this test even though it has both ecosystem gap
and non-trivial work potential. mxr's current quote/signature stripping
is English-only heuristics, fine for mxr's English-speaking userbase, not
fine for a public crate users would assume handles French quoted
replies, Outlook variants, or non-Latin signatures.

`outbound` failed all three tests but most decisively this one: the gap
is modest, the effort small, and the existing code is mxr's draft
conventions (Markdown front-matter, mxr's calendar attendee format)
masquerading as a general MIME composer.

## What stays internal

A perfectly good `mxr-*` crate can fail the publishing bar without that
being a defect. Internal seams exist for organisational clarity inside the
workspace; they do not need to be public APIs.

The bar is asymmetric: it's much cheaper to keep something internal and
extract later (if the world changes) than to publish prematurely and live
with a bad v0.1.0 forever.

## What this means for the audit

The original audit (`docs/extractable-crates/`) was a generous casting
call. It identified everything that could *plausibly* be extracted. The
bar applied here is stricter: ship only what *should* be.

Outcome of the 2026-05-16 audit:

- **Ship next:** `gmail-query`.
- **Ship after gmail-query:** `mailbox-formats`.
- **Investigate later, after sync stabilises:** `sync-engine`.
- **Won't-do (moved to `wont-do/`):** `format-flowed`, `reader-quote-sig`,
  `outbound`, `rules`. Plus the original "skip" set (`compose`,
  `humanizer`, `llm`, `keychain`).

Two shipped (`list-unsubscribe`, `mail-threading`), one next, one after,
one to investigate, eight won't-do. That ratio is healthier than the
original "everything is at least Tier 3" framing.

## Bar test for revisiting won't-do

The `wont-do/` directory is not a graveyard, it's a holding pen with
explicit revisit conditions in each candidate's frontmatter. Move a
candidate back into the active set when:

- An ecosystem change flips the bar (a key dependency adopts the
  candidate; a new requirement makes the work non-trivial).
- mxr's own development hardens the code to production-credible.
- A user file in the standalone repo asks for it.

Do NOT revisit because of:

- "We had it shipped half-done, might as well finish."
- "Someone wrote a competing crate, we should ship ours."
- Sunk cost on the planning doc.

The audit re-runs every ~6 months (or sooner if a key assumption flips).

## How to nominate a new candidate

If a new function emerges inside mxr that feels extractable:

1. Write a one-page candidate doc under `docs/extractable-crates/`.
2. Answer all three bar tests in the doc, before doing any work.
3. If any test fails, add the candidate to `wont-do/` with the failure
   reason in frontmatter.
4. Only if all three pass, propose extraction.

The bar exists to prevent reflexive yes. Use it.
