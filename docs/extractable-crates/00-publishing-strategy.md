---
type: cross-cutting-strategy
applies_to: all candidates
last_reviewed: 2026-05-28
---

# Publishing strategy — ports, WASM, drift, and AI-era timings

> Cross-cutting decisions that apply to every candidate in this directory.
> Read this *before* picking up any per-crate doc. The per-crate docs
> assume the framework laid out here.

## Current status

This document was written before the first extraction wave. Its framework still
holds, but the concrete Rust publishing order is now historical:

- `list-unsubscribe` shipped to crates.io and is consumed by
  `mxr-mail-parse`.
- `mail-threading` shipped to crates.io and is consumed by `mxr-sync`.
- `mail-query` shipped to crates.io and is consumed by `mxr-search`.
- `mailbox-formats` shipped to crates.io and is consumed by `mxr-export`.

The npm/TS work remains future work. The remaining active candidate in this
directory is `sync-engine`, and that is still investigation-first because the
public contract is much harder to make small.

## Why this doc exists

The per-crate docs were written with two assumptions that need correcting:

1. **They estimated effort in a pre-agent baseline.** "Two days" /
   "a week" estimates assumed a human typing every line. With agents
   driving the mechanical work, the real numbers are hours, not days.
2. **They focused on crates.io alone.** The npm ecosystem has the same
   gaps as the Rust ecosystem, often worse. Restricting our extraction
   thinking to one registry leaves audience on the table.

This doc reframes both. The per-crate docs now reference back here for
effort calibration and distribution strategy.

## The two-ecosystem question

For every Tier 1 / Tier 2 candidate the real question is no longer "do
we publish to crates.io" but "**do we publish to crates.io and npm, and
if so how**". The audience on npm is substantially larger than on
crates.io for most email-shaped problems (webmail, Electron clients,
admin tooling, automation scripts).

The npm side of the gap is real. Empirically verified for JWZ
threading: the only JS port (`conversationThreading-js`, max-mapper)
last committed in **March 2013**, never published to npm. The 2017
`mail-threading` package has had no updates in 9 years and no users.
The other gaps mxr could fill (Gmail-style query parsing, RFC 8058
one-click unsubscribe, format=flowed decoding) have similar or worse
coverage on npm.

So the default posture is **dual publish: crates.io + npm**, with the
shape of the npm distribution decided per-candidate.

## Three ways to ship to npm

There's also a fake fourth option — shelling out to a CLI binary —
discussed at the end. For library use it's not really an option.

### Option A — Native TS port (agent-driven)

Translate the Rust source to TypeScript. Ship as a normal npm package.

**Strengths.** Native TS debugging, source maps to actual TS, no
WebAssembly machinery in the consumer's bundle, feels like a JS-native
library. Works in every JS runtime (Node, Deno, Bun, browsers, edge
runtimes) without special handling. Small footprint.

**Tradeoffs.** Two implementations to keep in sync — drift is the
recurring cost (see below). Cannot share runtime behavior;
implementations can subtly diverge on edge cases (integer overflow,
iterator laziness, allocator panics, regex engine differences).

**Effort with AI agents.** For an algorithm-shaped, well-tested crate:
hours, not days. Agent translates the test suite first, makes it pass,
done. Pre-agent estimates of "a week" collapse to "an afternoon".

### Option B — WASM via `wasm-pack` / `wasm-bindgen`

Compile the Rust crate to WebAssembly. Ship the `.wasm` plus a thin
TypeScript wrapper.

**Strengths.** **No second implementation, so no drift.** The Rust
source is canonical; bug fixes happen once. Same runtime behavior
guaranteed by construction. One artifact runs in Node, Deno, Bun, and
browsers.

**Tradeoffs.** WASM startup cost (instantiation, on the order of
milliseconds). String/struct marshalling across the JS↔WASM boundary
has overhead — fine for batch operations (thread 10k messages in one
call), worse for chatty per-item APIs. Bundle size is bigger
(200KB–500KB typical for focused crates) — irrelevant on Node, mildly
fat in browsers. No native TS debugging; stack traces stop at the WASM
boundary.

**Effort with AI agents.** Comparable to a TS port — `wasm-pack build`
plus a TypeScript wrapper plus build glue is agent-shaped work.
Hours, not days.

### Option C — `napi-rs` native Node addon

Compile to a platform-specific native Node addon. Used by turbopack,
swc, Prisma, Parcel, Rspack.

**Strengths.** Native performance, no WASM overhead.

**Tradeoffs.** Per-platform prebuilds (linux-x64-gnu, linux-x64-musl,
linux-arm64, darwin-x64, darwin-arm64, win32-x64, possibly more) — 6–8
artifacts per release, fatter CI. Doesn't work in browsers. Postinstall
download UX.

**When to use.** Almost never for our candidates. Threading and query
parsing aren't perf-bottlenecked. Only reach for napi-rs if a specific
heavy user appears and asks.

### Option D (the fake one) — Wrap the mxr binary

Ship the compiled mxr CLI in an npm package, postinstall-fetch a
prebuilt binary per platform. The pattern used by `esbuild`, `swc`,
`tailwindcss`, `biome`.

**When this answers a question.** When you want `npx mxr` to work, or
to be in someone's `package.json` devDependencies as a tool.

**When it doesn't.** When a JS developer wants `import { threadMessages }
from 'mail-threading'`. Shelling out to a CLI is not a library API.

This is a **separate decision** from library distribution. Worth doing
for CLI discoverability, but not relevant to the per-candidate questions
in this directory. Track it elsewhere.

## The drift problem (and how to actually solve it)

If we ship a TS port (Option A), drift is the ongoing cost. Initial
port = cheap (agent). Ongoing parity = real cognitive tax, per change,
forever. Every Rust bug fix needs to land in TS; every TS bug report
may apply to Rust; every new operator added on one side must propagate
to the other.

Four patterns for managing it. Ranked by how well they actually work.

### 1. Shared JSON test corpus (recommended)

Move the source of truth out of either implementation. Both repos
become slaves to a shared corpus of input/expected fixtures.

```
/mail-threading-corpus
  /tests
    basic-chain.json
    phantom-container.json
    cycle-detection.json
    subject-merge-german.json
    ...
```

Each fixture: `{ "name": "...", "description": "...", "input": [...],
"expected": [...] }`. Both crates carry a small test harness that loads
the corpus and runs every fixture. CI in both repos pulls the corpus
(git submodule, or a published `mail-threading-corpus` package).

**The drift-killing property.** When a bug arrives:
1. Add a failing fixture to the corpus.
2. Both repos' CI goes red.
3. Fix both. (Agent-assisted in either direction.)
4. Both go green. Merge.

You cannot silently ship a Rust fix that doesn't land in TS, because
CI yells. The corpus *is* the spec, in executable form.

This is the pattern `serde`/`serde_json` use, what TOML uses
(`toml-test`), what WebAssembly uses (`testsuite`), what SQL uses
(`sqlogictest`). Battle-tested.

For algorithm-shaped crates (threading, query parsing, format-flowed,
list-unsubscribe) the corpus is finite, small, and stable. Drift cost
collapses to ~zero.

### 2. WASM as the implementation (no second codebase)

Pick Option B above. There's no second source to drift. Bug fix in
Rust → recompile → re-publish. Done.

Use when:
- The crate ships frequent updates
- You don't have headroom to babysit two repos
- Performance and bundle-size tradeoffs are acceptable

### 3. AI-mediated port sync

Treat Rust as canonical. After every Rust PR run an agent: "port this
diff to TS". Human reviews the agent's PR. Merge on the TS side.

**Workable, but has a sharper edge than it looks.**

Cuts because:
- Agents introduce subtle structural drift over many PRs (variable
  naming, control flow shape, idiom choices). After 50 ports the two
  codebases are behaviorally similar but structurally divergent enough
  that the next auto-port is harder.
- The discipline lives in *you* remembering to run the sync. Skip it
  three times during a deadline and TS becomes a stale fork.
- Agents can miss implicit behaviors (panic semantics, integer
  overflow, iterator laziness, regex differences). Subtle correctness
  drift is real.

**Only use combined with #1.** The corpus catches what the agent
misses.

### 4. Specification document (skip)

Write a written spec. Both implementations cite it. Changes start as
spec PRs.

Doesn't work in indie OSS. Spec rots, nobody reads it, the
implementations become the truth anyway.

## Per-shape recommendation framework

The choice between TS port and WASM depends less on porting cost (now
cheap either way) and more on the *shape* of the crate:

| Shape | Recommendation | Why |
|---|---|---|
| Stable algorithm, finite spec, small surface | **TS port + shared corpus** | Drift is solvable, native feel is worth it, audience values JS-native debugging |
| Evolving surface, growing operator set, frequent edge-case fixes | **WASM** | Eliminates drift by construction, accepts perf/bundle tradeoffs in exchange |
| Heavy compute, latency-sensitive | **napi-rs** | Only if a specific user needs it; default to WASM otherwise |

Apply per candidate.

## Historical Tier 1 recommendations

These rows explain the thinking that led to the first extraction wave. Treat
them as historical context for future npm/TS decisions, not as a live Rust ship
queue.

| Crate | Rust status | npm/TS recommendation |
|---|---|---|
| **list-unsubscribe** | Shipped as `list-unsubscribe v0.1.0` | **TS port + shared corpus.** Tiny surface, finite test cases. |
| **mail-threading** | Shipped as `mail-threading v0.1.0` | **TS port + shared corpus.** Stable algorithm; the corpus is the portable artifact. |
| **gmail-query** | Shipped as `mail-query v0.1.0` | **WASM** is still plausible because the operator surface can grow. Native TS is fine only with the shared corpus as gate. |

For Tier 2:

| Crate | Rust status | npm/TS recommendation |
|---|---|
| **format-flowed** | Won't do for Rust package extraction | Do not start unless a real user appears; the Rust package failed the publishing bar. |
| **mailbox-formats** | Shipped as `mailbox-formats v0.1.0` | TS port + corpus eventually. Byte-streaming mbox might be a WASM candidate for perf, but the audience is small enough that either works. |

## Historical ship order

The order we originally expected was close, but reality changed after the
publishing bar was tightened:

1. **`mail-threading`** — shipped first as the headline spec-backed crate.
2. **`list-unsubscribe`** — shipped next as the smallest carve-out from
   existing mxr code.
3. **`mail-query`** — shipped after the parser boundary was made public and
   mxr kept execution policy local.
4. **`mailbox-formats`** — shipped after we learned to distinguish "lift
   existing code" from "build a real package from a thin seed and a spec."

The next package is not a queue item. `sync-engine` needs discovery before any
commitment.

## AI-era effort estimates (replaces the per-doc numbers)

Old "Estimated effort" sections in the per-crate docs were written
pre-agent. Updated reality:

| Activity | Pre-agent | Agent-driven |
|---|---|---|
| Lift Rust code to standalone repo, polish API | 1–2 days | 2–4 hours |
| TS port of an algorithm-shaped, tested crate | 3–5 days | 2–6 hours |
| WASM build setup + TS wrapper | 1–2 days | 2–4 hours |
| Shared JSON corpus + dual-repo CI wiring | 1 day | 2–3 hours |
| Documentation (README + rustdoc + JSDoc) | 1 day | 1–3 hours |
| Total for a Tier 1 candidate, both ecosystems, shared corpus | ~2 weeks | **~1 day** |

Caveats:

- Open API design questions still cost human thought time. The
  agent-driven numbers assume the API is mostly settled (true for
  threading and list-unsubscribe; less so for gmail-query, where
  `Display` round-trip and custom-filter extensibility need real
  decisions).
- Each "agent-driven" task still needs human review. Hours are agent
  output, but the review tail is real.
- First-time-publishing-to-npm overhead is real (account, scope
  creation, dist-tag policy) but a one-time cost.

## What changes per-candidate

Every per-candidate doc in this directory should be read with this
framework in mind. Specifically:

- Historical **Estimated effort** sections should be treated as rough order of
  magnitude only. The shipped runbooks under `docs/extracted-crates/` are more
  useful than the original estimates.
- A future **TS/npm port** should start at the shared corpus, not by translating
  Rust tests by hand.
- The **Skip** and **Defer** decisions are still mostly unchanged — AI tooling
  doesn't make a crate audience materialise that doesn't exist. The
  audience question still gates everything.

## When to re-evaluate this whole doc

- If a credible competitor publishes a maintained npm JWZ library, the
  list-unsubscribe-first ordering changes — they may want mail-threading
  shipped *before* a competitor cements.
- If WASM tooling regresses meaningfully (unlikely — `wasm-pack` and
  `wasm-bindgen` are stable Foundation projects), revisit Option B.
- If `napi-rs` adoption explodes and the per-platform prebuild fatigue
  reduces, raise C's ranking.
- If we get a real user request for native TS perf, raise Option C
  case-by-case.

Reviewed quarterly while any extraction is in flight; annually
otherwise.
