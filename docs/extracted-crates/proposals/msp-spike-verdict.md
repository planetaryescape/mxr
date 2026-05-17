---
title: MSP spike — verdict
status: complete
date: 2026-05-17
companions:
  - msp-spec-draft.md
  - msp-mxr-alignment.md
  - msp-blog-post-draft.md
---

# MSP spike — verdict

After Phase 1 (read DAP/LSP/JMAP), Phase 2 (draft spec), Phase 3
(mxr alignment audit), Phase 4 (draft blog post), the verdict.

## Verdict: Spec + blog + reference adapter

Of the four shapes the plan defined, this falls between **Spec +
blog** (shape 2) and **Spec + blog + reference adapter** (shape 3),
landing closer to shape 3. The reasoning:

1. **The spec held up under writing.** Drafting the nine sections
   surfaced no fatal contradictions. The abstract model maps
   cleanly to mxr's existing types; the wire shape leans on
   established prior art (LSP framing + JMAP state cursors); the
   capability negotiation has a clean rule against dynamic
   registration. The spec feels honest, not forced.

2. **mxr's alignment cost is reasonable.** ~10 focused days across
   Phases A-F to align mxr (excluding the optional lazy-fetch
   Phase G). Every refactor is hygiene mxr would benefit from
   regardless. Not a sunk cost.

3. **A reference IMAP adapter is the smallest credible "next
   step" beyond the spec draft.** IMAP is open (no commercial
   credentials), well-specified (RFCs), and the existing
   `provider-imap` crate is most of the implementation. Writing
   the MSP/IMAP shim on top is days, not months.

4. **The blog post can earn its place by surfacing two specific
   signals** before we commit further: (a) co-leads who'd author
   adapters for providers we can't reach (Outlook, Fastmail,
   iCloud), and (b) explicit adopter signals from at least 2-3
   mail-client projects.

## What this commits us to

In order, with explicit gates between each step:

### Step 1 (next session): land the spike artifacts in mxr

- Commit `msp-spec-draft.md`, `msp-mxr-alignment.md`,
  `msp-blog-post-draft.md`, and this verdict to mxr's repo.
- Update `docs/extractable-crates/07-sync-engine.md` frontmatter to
  reflect the protocol reframing: status `pursue-as-protocol`,
  point to these proposal docs.
- **Gate before Step 2:** internal review / sleep on it / re-read
  the blog draft 24h later. If it still feels honest, proceed.

### Step 2 (1-2 days): kick off mxr alignment Phase A

The "cheap wins" from the alignment audit (~1 day total):

- Namespaced `SyncCapabilities` restructure.
- Typed `SyncCursorExpired` error variant.
- `Role` enum on Folder/Label.

These are pure clean-ups. mxr is strictly better afterwards. No
external dependency, no blog-post commitment needed yet.

**Gate before Step 3:** Phase A landed; mxr's tests green.

### Step 3 (~2 days): mxr alignment Phase B — opaque SyncCursor

The biggest single architectural win. Decouples the daemon from
provider-specific cursor shapes. Strict improvement for mxr.

**Gate before Step 4:** Phase B landed; CLI smoke test on the
fake provider passes (per the new agent workflow); Gmail + IMAP
adapters re-test against real accounts if convenient.

### Step 4 (decision point): publish or hold the blog post

By this stage we've done ~3 days of alignment work and mxr is
visibly closer to the MSP shape. The spec draft has had at least
one internal-feedback pass.

Decision: publish the blog post (and the spec) to a Rust forum or
HN, OR hold and continue refactoring without external surface.

The publish-vs-hold decision is informed by:

- Does the spec still feel right after living with it for a few
  days?
- Do we have at least one concrete external person we'd want to
  reach with the post (a known mail-client builder, an existing
  Rust mail crate maintainer)?
- Is there bandwidth to handle inbound feedback over the next 2-3
  weeks?

If yes: publish. If no: hold; revisit in a month.

### Step 5 (~1-2 weeks): reference IMAP adapter as a separate crate

If the blog post is published and gets non-negative response:

- New crate `planetaryescape/msp-imap` (or wherever it lands).
- Implements MSP server-side (stdio JSON-RPC).
- Talks IMAP via the existing `provider-imap` code (now packaged
  for standalone use).
- mxr's daemon learns to subprocess `msp-imap` as one of its
  adapter options.

This is the first hard external surface. After this step, an
external mail client could in principle use `msp-imap` without
knowing about mxr. That's the protocol's first real validation.

### Step 6 (open-ended): more adapters, working group, v0.2 spec

Beyond Step 5, the path forks based on what response the post
generated and what gaps the IMAP adapter exposed. Not pre-decided
here.

## What this does NOT commit us to

- **Not committing to ship the protocol as a public standard.**
  Steps 1-3 are pure mxr improvement. Step 4 is the
  publish-or-hold decision; even if we hold, the spec stays as an
  internal architecture document.
- **Not committing to be the working-group leader.** If a co-lead
  appears who's better placed (more time, more political capital,
  more existing reach), we hand them the spec.
- **Not committing to lazy body fetch.** Alignment Phase G is
  optional. mxr can stay eager-fetch indefinitely and still be
  MSP-compatible by negotiating that capability.

## Risk register

- **Risk: the spec is wrong in ways the alignment audit didn't
  surface.** Mitigation: the IMAP adapter (Step 5) is the first
  hard test. If it forces awkward protocol contortions, the spec
  changes — that's what v0.1 → v0.2 is for.

- **Risk: no external adopter signals after the blog post.** That
  doesn't sink the project; mxr alignment is valuable regardless.
  But it does mean we shouldn't escalate to working-group / full
  protocol push.

- **Risk: a competing protocol or library appears.** Already
  partially the case (pimalaya's `email-lib` is one possible
  competing direction). Mitigation: position MSP as the
  orchestration layer above per-op backend traits, not a
  replacement.

- **Risk: scope creep — adding calendar/contacts/send to v0.1.**
  Mitigation: section 8 of the spec lists these as explicit punts.
  Keep saying no.

## What the spike taught us

Two things worth flagging back to lesson 10 (publishing bar):

- **The publishing bar applies asymmetrically to specs vs
  implementations.** Writing a spec draft and an alignment audit
  costs ~1 day and earns immediate architectural value for mxr.
  Shipping a reference adapter as a public crate costs weeks and
  triggers full publishing-bar review (real gap, non-trivial,
  credible seed). Don't conflate the two costs.

- **"Protocol-first design" is itself a forcing function** even
  when no protocol gets published. Articulating the contract from
  the outside (what would a non-mxr client need to call?) surfaces
  internal coupling that's invisible from inside. Worth capturing
  as a lesson 12 if we end up running this pattern again.

## File map

- Spec: `docs/extracted-crates/proposals/msp-spec-draft.md`
- mxr alignment: `docs/extracted-crates/proposals/msp-mxr-alignment.md`
- Blog post draft: `docs/extracted-crates/proposals/msp-blog-post-draft.md`
- This verdict: `docs/extracted-crates/proposals/msp-spike-verdict.md`
- Existing candidate doc (to be reframed):
  `docs/extractable-crates/07-sync-engine.md`
