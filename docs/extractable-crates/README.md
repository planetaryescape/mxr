# Extractable Crates — Open Source Opportunities from mxr

This directory inventories internal mxr workspace crates (and sub-modules)
that could plausibly be extracted, polished, and published as standalone
libraries — to **crates.io** for the Rust ecosystem and **npm** for the
JS/TS ecosystem.

Each candidate has its own document. Read this README first to understand
the methodology and ordering, then read **[00-publishing-strategy.md](./00-publishing-strategy.md)**
before acting on any candidate — it covers cross-cutting decisions (port
vs WASM, drift management, shared test corpus, AI-era effort estimates)
that apply to every candidate.

## Where to start

The filename prefix tells you the order. The numbering is roughly
*"start at the top, work down"*:

- **`00-…`** — Cross-cutting strategy. Read first.
- **`01-…` to `03-…`** — Tier 1: ship these.
- **`04-…` to `05-…`** — Tier 2: ship after Tier 1.
- **`06-…` to `07-…`** — Tier 3: defer or investigate.
- **`08-…` to `09-…`** — Defer indefinitely.
- **`10-…` to `13-…`** — Skip.

Within Tier 1, the order is "smallest first" — not because the smallest is
the most important, but because the unknowns at the start are about the
*publishing workflow* (dual-repo, shared corpus, dual-publish to crates.io
+ npm, semver discipline across registries), not about the code. Get the
workflow right on `01-list-unsubscribe` (tiny surface) before stress-testing
it on `02-jwz-threading` / `mail-threading` (the headline) and
`03-gmail-query` (the most
ambitious surface).

## Why bother

Two observations motivated this audit:

1. **mxr already solves several problems the Rust and npm email ecosystems
   have not solved well.** Building an opinionated, agent-native email
   client forced us to write production-quality code for RFC-anchored
   problems that have either no published library or only an abandoned
   one. That work is currently buried inside `mxr-*` workspace crates
   marked `publish = false`.

2. **Extracting the well-bounded pieces benefits both sides.** Outside
   users get a maintained library. We get a smaller mxr codebase, sharper
   API boundaries, free design feedback from external contributors, and a
   credibility signal for the project. The cost is modest: most candidates
   are small, already decoupled, have test coverage, and — with
   agent-assisted porting and a shared JSON test corpus — can ship to both
   Rust and JS ecosystems without doubling the maintenance load.

This is **not** a plan to publish the whole workspace. mxr's stance —
stated in the root `Cargo.toml` — is that workspace crates are
organisational seams, not library APIs. That stance is correct for the
bulk of the workspace. This audit is about the exceptions.

## Methodology

The audit ran in three passes:

1. **Codebase map.** Every crate under `crates/` scored on purpose,
   public surface, internal coupling, dependencies, generalisability
   (1–5), whether it implements an open standard, and notable extractable
   sub-modules.

2. **Ecosystem scan.** For each functional area mxr touches, we checked
   **both** crates.io and npm (lib.rs, GitHub, npm registry direct
   queries) to identify gaps. The npm side is often worse than the Rust
   side — e.g. for JWZ threading the only JS port last committed in 2013
   and was never published.

3. **Verification.** For the highest-value intersections (mxr quality ×
   real ecosystem gap) we read the actual code to confirm the
   implementation is publication-ready rather than a thin shim.

## Decision framework

Each candidate is classified into one of four buckets:

- **Tier 1 — Ship.** mxr already has high-quality, decoupled code and the
  ecosystem has a real, well-defined gap. Small extraction effort.
  Publishing is a net win.

- **Tier 2 — Worth doing later.** Either the mxr seed is solid but needs
  surrounding library work, or the ecosystem gap is real but smaller.
  Do these after Tier 1.

- **Tier 3 — Defer / investigate.** Real ecosystem gap, but our
  implementation isn't yet library-quality, or the code is too entangled
  with mxr internals to extract cheaply.

- **Defer / Skip.** Either covered by an existing library, or our
  implementation isn't differentiated enough to credibly anchor a new
  one.

Classifications are not permanent. Each per-candidate doc states the
assumptions behind its decision so you can re-evaluate when those
assumptions change.

## Candidates

| # | Candidate | Decision | One-line rationale |
|---|---|---|---|
| 00 | [Publishing strategy](./00-publishing-strategy.md) | **Read first** | Cross-cutting framework: port vs WASM, drift, shared corpus, effort estimates |
| 01 | [list-unsubscribe](./01-list-unsubscribe.md) | **Tier 1 — ship** | RFC 2369 + RFC 8058 one-click; deliverability-mandated, no focused crate. Smallest Tier 1 — start here. |
| 02 | [mail-threading](./02-jwz-threading.md) | **Implemented in repo** | Full RFC 5256 / JWZ impl; now an independently publishable workspace crate with shared JSON conformance corpus. |
| 03 | [gmail-query](./03-gmail-query.md) | **Tier 1 — ship** | Comprehensive Gmail-style operator parser with typed AST; no comparable library on either registry. |
| 04 | [format-flowed](./04-format-flowed.md) | **Tier 2 — later** | RFC 3676 decoder; useful, niche, easy weekend extraction |
| 05 | [mailbox-formats](./05-mailbox-formats.md) | **Tier 2 — later** | mbox writer is solid seed; needs maildir writer to be a complete library |
| 06 | [reader-quote-sig](./06-reader-quote-sig.md) | **Tier 3 — defer** | Real ecosystem gap (`email_reply_parser` is dead) but our impl is too lightweight |
| 07 | [sync-engine](./07-sync-engine.md) | **Tier 3 — investigate** | Biggest ecosystem gap by impact; extraction risk is high; needs dedicated investigation |
| 08 | [outbound](./08-outbound.md) | **Defer** | Complements lettre but small audience; bundle later |
| 09 | [rules](./09-rules.md) | **Defer** | Generic rule engines exist; ours has email-shaped verbs baked in (Sieve is the real prize) |
| 10 | [compose](./10-compose.md) | **Skip** | Thin `$EDITOR` wrapper; many similar utilities exist |
| 11 | [humanizer](./11-humanizer.md) | **Skip** | No demand signal, hard to brand, too niche |
| 12 | [llm](./12-llm.md) | **Skip** | Many existing options (`async-openai`, `genai`, `rig`) |
| 13 | [keychain](./13-keychain.md) | **Skip** | `keyring` crate covers this fully |

## How to use these docs

When you want to act on a candidate:

1. Read **[00-publishing-strategy.md](./00-publishing-strategy.md)** if
   you haven't yet — it sets effort expectations and distribution
   recommendations.
2. Open the per-candidate doc.
3. Re-read the **Assumptions / When to re-evaluate** section. Are the
   ecosystem assumptions still true? (Libraries appear and die; a "Skip"
   can flip to "Ship" if the alternative becomes unmaintained.)
4. Follow the **Extraction plan** section. It lists the files to lift,
   the API surface to expose, the gaps to fill before publishing.
5. For Tier 1 candidates, follow the **TS/npm distribution** section to
   decide port vs WASM and set up the shared corpus.
6. If you decide to ship, update this README's table and the candidate's
   frontmatter to reflect the new status.

When new candidates emerge (a new mxr crate is added, a new ecosystem gap
opens), add a new doc to this directory and a row to the table above.

## What this audit is not

- **Not a commitment.** Publishing any of these is a deliberate, owned
  decision. The audit just makes the choice visible and informed.
- **Not a refactor plan.** Internal mxr code can keep its `mxr-*` naming
  and `publish = false` posture indefinitely. Extraction means "give the
  reusable piece its own public package name, version, README, conformance
  corpus, and semver policy", not "publish every internal crate".
- **Not a maintenance commitment for free.** Each extracted crate adds a
  small but real maintenance surface. The shared-corpus pattern reduces
  this dramatically for dual-ecosystem ships, but doesn't eliminate it.

## Date of audit

2026-05-15. Re-validate ecosystem assumptions before acting if more than
~6 months have passed.
