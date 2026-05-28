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

The original filename prefixes preserve the order from the audit, but the
first wave has now shipped. Read the directory by status, not by old tier:

- **`00-publishing-strategy.md`** — Cross-cutting strategy. Still useful, but
  now partly historical because the first Rust crates are published.
- **`done/`** — Candidates that became standalone packages and are now consumed
  by mxr from crates.io.
- **`wont-do/`** — Candidates rejected against the publishing bar. Revisit only
  if the ecosystem or mxr's implementation materially changes.
- **Active root-level candidate docs** — Work still worth investigating. Today
  that is only `07-sync-engine.md`.

Before touching another package, read the extraction playbook:
[`../extracted-crates/lessons/README.md`](../extracted-crates/lessons/README.md).
The bar is no longer "can we publish this code?" It is "can we own a small
public contract with a README, conformance corpus, coverage story, semver
policy, and a clean path back into mxr?"

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
| 07 | [sync-engine](./07-sync-engine.md) | **Investigate later** | Highest ecosystem impact but highest extraction risk; 2-3 day discovery before commit; do not start until mxr's sync surface stabilises |

> **Bar test for new candidates:** see
> [`docs/extracted-crates/lessons/10-publishing-bar.md`](../extracted-crates/lessons/10-publishing-bar.md).
> Three rules established 2026-05-16: (1) crates.io is not npm — micro-packages
> hurt the ecosystem; (2) "afternoon RFC implementation" fails the bar; (3) the
> mxr seed must be production-credible, not a v0.1.0 placeholder users would
> outgrow.

## Done

| # | Candidate | Result | One-line rationale |
|---|---|---|---|
| 01 | [list-unsubscribe](./done/01-list-unsubscribe.md) | **Shipped** | RFC 2369 + RFC 8058 one-click parser. Published as [`list-unsubscribe`](https://crates.io/crates/list-unsubscribe); source at [planetaryescape/list-unsubscribe](https://github.com/planetaryescape/list-unsubscribe). |
| 02 | [mail-threading](./done/02-jwz-threading.md) | **Shipped** | Full RFC 5256 / JWZ impl. Published to crates.io as [`mail-threading`](https://crates.io/crates/mail-threading); source at [planetaryescape/mail-threading](https://github.com/planetaryescape/mail-threading). |
| 03 | [gmail-query](./done/03-gmail-query.md) | **Shipped (as `mail-query`)** | Parser + typed AST for Gmail-style email search queries. Published as [`mail-query`](https://crates.io/crates/mail-query); source at [planetaryescape/mail-query](https://github.com/planetaryescape/mail-query). |
| 05 | [mailbox-formats](./done/05-mailbox-formats.md) | **Shipped** | mbox (all 4 variants) + Maildir reader/writer + LockStrategy. Published as [`mailbox-formats`](https://crates.io/crates/mailbox-formats); source at [planetaryescape/mailbox-formats](https://github.com/planetaryescape/mailbox-formats). |

## Won't do

Candidates that failed the publishing bar
([`lessons/10-publishing-bar.md`](../extracted-crates/lessons/10-publishing-bar.md))
on the 2026-05-16 audit. Each frontmatter records why and what would have to
change to revisit.

| # | Candidate | Reason |
|---|---|---|
| 04 | [format-flowed](./wont-do/04-format-flowed.md) | RFC 3676 is a 4-page spec; encoder mechanical, decoder ~30 lines. Afternoon-from-spec. Audience too narrow. |
| 06 | [reader-quote-sig](./wont-do/06-reader-quote-sig.md) | Real ecosystem gap, but mxr's English-only heuristics need ~1-2 weeks of corpus work to be credible. Shipping the current code would mislead users. |
| 08 | [outbound](./wont-do/08-outbound.md) | Real-but-modest gap, small effort (3-5 days mostly polish), narrow audience (only senders). Stays internal. |
| 09 | [rules](./wont-do/09-rules.md) | The natural rival is Sieve (RFC 5228); a custom DSL competes confusingly. mxr's verbs (`Snooze`, `ReplyLater`) are product-shaped. |
| 10 | [compose](./wont-do/10-compose.md) | Thin `$EDITOR` wrapper; the `edit` crate covers this. |
| 11 | [humanizer](./wont-do/11-humanizer.md) | No demand signal, no clear brand, too niche. |
| 12 | [llm](./wont-do/12-llm.md) | Crowded space (`async-openai`, `genai`, `rig`). |
| 13 | [keychain](./wont-do/13-keychain.md) | `keyring` crate covers this fully. |

## How to use these docs

When you want to act on a candidate:

1. Read **[00-publishing-strategy.md](./00-publishing-strategy.md)** if
   you haven't yet — it sets effort expectations and distribution
   recommendations.
2. Read the lessons from the extraction wave:
   **[../extracted-crates/lessons/README.md](../extracted-crates/lessons/README.md)**.
   That file captures the higher bar created by `mail-threading`,
   `list-unsubscribe`, `mail-query`, and `mailbox-formats`: conformance corpus,
   coverage matrix, honest divergences, standalone ownership, registry cutover
   discipline, and the "do not publish just because the code is easy" rule.
3. Open the per-candidate doc.
4. Re-read the **Assumptions / When to re-evaluate** section. Are the
   ecosystem assumptions still true? (Libraries appear and die; a "Skip"
   can flip to "Ship" if the alternative becomes unmaintained.)
5. Follow the **Extraction plan** section. It lists the files to lift,
   the API surface to expose, the gaps to fill before publishing.
6. If a JS/TS package is still desirable, follow the **TS/npm distribution**
   section to decide port vs WASM and set up the shared corpus.
7. If you decide to ship, update this README's active/done/won't-do tables and
   the candidate's frontmatter to reflect the new status.

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
