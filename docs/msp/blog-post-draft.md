---
title: "Mail needs a DAP"
subtitle: "A proposal for a Mail Sync Protocol that does for email what DAP did for debugging"
status: draft
author: planetaryescape
date_drafted: 2026-05-17
target_audience: Rust/email-tooling community, mail-client builders, protocol-curious folks
estimated_length: ~2000 words
---

# Mail needs a DAP

Every mail client reinvents sync. Yours probably does. Mine
definitely does.

If you've built a mail client recently, you wrote code that
authenticates with Gmail's REST API. You probably also wrote code
that authenticates with IMAP. If you supported Outlook, you wrote
code that talks to Microsoft Graph. Each of these is a different
sync model — Gmail has a `historyId`, IMAP has a `(UIDVALIDITY,
MODSEQ)` matrix, Graph has delta-link cursors. Each needs its own
error handling for cursor expiry. Each needs its own offline-queue
replay logic. Each handles push notifications differently — IMAP
IDLE for one, history.watch for another, webhooks for the third.

This work gets repeated in every mail client that exists. There's a
mature mail client in Rust (himalaya). There's one in Swift (Apple
Mail). There's one in TypeScript (every webmail). There's one in
Python (offlineimap). There's one in C++ (Thunderbird, mutt).
Every one of them has its own copy of "talk to Gmail, talk to
IMAP, talk to Outlook." The collective person-years sunk into
maintaining N copies of the same provider logic is enormous, and
the bugs aren't shared — they're rediscovered independently every
time.

In 2016, Microsoft shipped the Debug Adapter Protocol. Before DAP,
every editor that wanted to support debugging Python had to wrap
pdb. Every editor that wanted to debug C++ had to wrap gdb. The
problem looked exactly like mail's: an N×M matrix of "every editor
× every language" with the same translation work happening
N×M times.

DAP collapsed it. Now each language ships **one debug adapter** —
a binary that speaks DAP on one side and the language's native
debugger on the other. Each editor speaks **DAP**, not pdb-or-gdb-
or-V8-inspector. The N×M problem became N+M.

**Mail is the next domain that needs this treatment.**

## What MSP would look like

I've been drafting a Mail Sync Protocol. The full spec is at
[link]. The shape is:

- **JSON-RPC 2.0 over stdio**, same framing as LSP. Clients launch
  per-provider adapters as subprocesses; they speak JSON over the
  pipe.
- **An abstract model** of Account, Folder, Message, Thread, Flag,
  Mutation, SyncDelta. Every adapter maps its provider into these
  concepts. Clients only see these.
- **Opaque state cursors.** The adapter chooses the format
  (`historyId`, `(UIDVALIDITY, MODSEQ)`, JMAP `state`, whatever);
  the client persists it as bytes and passes it back unchanged.
  Lifted directly from JMAP, which got this right.
- **Namespaced capabilities.** Each request is paired with the
  capability it requires; clients check before calling.
  Unsupported methods return a typed `unsupported` error. Borrowed
  from LSP, minus its dynamic-registration footgun.
- **Push notifications carry no payload.** Notifications fire to
  say "state changed" with an opaque hint; the client reconciles
  via the same sync method it uses for pull. Same shape for IMAP
  IDLE, Gmail history watch, JMAP push, and webhook delivery —
  the adapter handles the provider-specific mechanism internally.

The provider divergences mail clients normally wrestle with —
labels-vs-folders, server-side-vs-client threading, custom
keywords-vs-fixed flags, bulk fetch availability — all live below
the protocol. Each adapter does its translation work once. Every
client benefits.

## Why now

Two reasons.

**First, the gap is real and widening.** I count at least five
Rust mail clients shipped in 2025-2026, each rewriting sync from
scratch. The JavaScript side keeps generating mail clients too;
the Tauri-wrapped wave (Velo, VibeMail, ZenMail, Envoyer) is the
recent crop. Apple Mail clones in Swift. None of them share sync
code. The duplication is visible if you go reading their
repositories.

**Second, JMAP didn't win at the layer that matters.** [JMAP]
(RFC 8620) was supposed to be this. It has opaque state cursors;
it has `created/updated/destroyed` deltas; it has push subscriptions.
The protocol design is excellent. But adoption stalled at the
**server side** — Gmail, Outlook 365, iCloud collectively hold
~95% of consumer mailboxes, and none of them speak JMAP. Their
incentive to add a third protocol on top of IMAP + their
proprietary APIs is exactly zero.

MSP sits one layer down. Where JMAP says "the server speaks this
protocol natively," MSP says "an adapter translates the server's
native API to a protocol the client speaks." A JMAP-aware adapter
can implement MSP by forwarding the methods one-to-one. An IMAP
adapter translates from IMAP commands. A Gmail adapter translates
from Gmail's REST API. The wire surface area is the same; the
implementation strategy differs.

This is the same relationship LSP has to language compilers. C++
has its own native protocols (clangd, ccls); Python has Pyright;
Rust has rust-analyzer. LSP doesn't replace those — it gives
clients one shape to learn, with adapters bridging.

## What mxr offers as a starting point

I run a mail client called [mxr](https://github.com/planetaryescape/mxr).
It's local-first, daemon-backed, written in Rust. Without intending
to, it ended up with an architecture that's very close to what MSP
would look like:

- A daemon that holds the state.
- Provider adapters (`MailSyncProvider` trait) — currently Gmail,
  IMAP, Outlook, and a fake provider for tests.
- Clients (TUI, CLI, web) that talk to the daemon over a length-
  delimited JSON IPC.

I wrote down where mxr's existing shape matches MSP and where it
diverges. The gaps are mostly architectural hygiene improvements
mxr would benefit from regardless of whether MSP ships as a public
protocol — things like making the sync cursor opaque to the daemon,
or namespacing the capability struct, or adding `mutation_id` for
idempotent retries. The full alignment audit is in the proposal
repo at [link].

I'd like mxr to become a working reference implementation of MSP.
That means refactoring toward MSP's abstract model — over several
weeks of normal work, not a big bang. The result: mxr is the first
client that speaks MSP, and the first provider adapters (at least
IMAP, possibly Gmail) are shipped out of the mxr project as
standalone binaries that any mail client can use.

## What I'm asking for

This blog post is a starting point, not a finished standard. Three
asks:

**1. Eyes on the draft.** [Link to the spec draft]. Read it. Tell
me where I'm wrong, where the abstract model leaks provider
specifics, where the capability negotiation will go wrong. The
spec is at v0.1; it should hurt for a few rounds before it
stabilises.

**2. Adopter signals.** If you build a mail client and you'd
consider switching from your in-house sync to an MSP-compliant
adapter — say so. The number of "yes I would" responses
determines whether this is worth pursuing as a public protocol or
keeping as an mxr-internal architecture document. I'm not asking
for commitment, just signal.

**3. Co-leads, especially adapter authors.** I don't have the
credentials or the platform knowledge to write a great Outlook
adapter. Or a Fastmail adapter. Or an Apple Mail adapter. If
you're someone who already knows one of those provider APIs deeply
and you'd be willing to author the adapter for it, I want to talk
to you. The protocol gets better the more adapter authors stress-
test the abstract model.

## What this is NOT

A few things I want to be clear about, so this post doesn't
overpromise:

- **Not a finished standard.** This is a draft, not an RFC. It's
  not blessed by anyone. It hasn't been through a working group.
  It probably has the wrong shape in places I haven't realised yet.
- **Not a competitor to JMAP.** JMAP is great; if anything, MSP
  makes JMAP adapters cleaner (one method-call-translation table
  vs writing yet another full mail client). JMAP-aware adapters
  may be the simplest to write.
- **Not a Rust-only thing.** The protocol is a wire spec. Adapters
  can be written in Go, TypeScript, Swift, Python, anything. The
  mxr-side reference implementation is Rust because that's what
  mxr is written in.
- **Not a working group invitation.** Yet. If there's enough
  signal that this is worth pursuing, a working group is the next
  step. For now: just the spec draft and an invitation to comment.
- **Not promising mxr will lead this forever.** I'd love co-leads.
  I'd happily hand the spec to someone better-placed if they
  appeared. The point is to get the work started, not to own it.

## The realistic next steps

If the response to this post is "yes, please continue":

1. **Reference IMAP adapter.** IMAP is the open protocol; there
   are no commercial credentials to negotiate. A Rust crate that
   speaks MSP on one side and IMAP on the other becomes the
   conformance benchmark.
2. **mxr alignment.** I work through the alignment audit's
   refactor list (Phases A-F: ~10 focused days). mxr becomes
   MSP-shaped.
3. **Second provider adapter.** Probably Gmail. The point is to
   stress-test the abstract model against a wildly different
   provider.
4. **A v0.2 spec round.** Based on what the IMAP and Gmail
   adapters actually need, the spec gets revised.
5. **Working group, if there are co-leads to populate it.**

If the response is "interesting but not for me": MSP becomes mxr's
internal architectural north star and the spec draft sits in the
repo as a working document. That's also a fine outcome — the spec
work is worth it for mxr's own architecture regardless of external
adoption.

If the response is "this is wrong; here's why": I'd love to hear
why. Tell me on [the repo issues page] or [HN] or wherever you
prefer.

## A small note about ambition

I don't think any single person can fix the mail-tooling ecosystem.
It's calcified by 30 years of provider-vendor incentives, an RFC
process that takes years, and a user base that doesn't care what
protocol their mail client speaks.

But I do think we — as a community of people who build mail tools
— can collectively stop reinventing the same sync code. DAP didn't
need a vendor mandate; it just needed someone to write down a
useful spec and ship a couple of reference implementations. Three
years later it was everywhere. Mail is overdue.

This is the proposal. If you build mail tools, please read the
draft. If you're someone who's been frustrated by how every mail
client redoes provider plumbing, please say so. If you have ideas
for what the protocol should look like that I haven't covered,
please open an issue.

The draft spec, the mxr alignment audit, and this post all live
at [planetaryescape/mxr/docs/msp]. The spec is at v0.1.

---

*Coda: this post commits me to a position. If you read it again in
a year and find I've quietly retracted, call me out. The spec is
the spec; if it's wrong I'd rather hear it now than ship a wrong
v1.0.*
