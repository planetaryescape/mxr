---
candidate: mail-threading
status: implemented-in-repo
decision: ship
mxr_source: crates/mail-threading
last_reviewed: 2026-05-15
---

# `mail-threading`

> Complete JWZ / RFC 5256 email threading algorithm. Build threads from a
> stream of messages using `References`, `In-Reply-To`, and subject-based
> fallback merging.

## Decision: **Tier 1 — ship**

This is the single highest-value extraction we can make. The implementation
now lives as an independently publishable in-repo crate at
`crates/mail-threading`, with a shared JSON conformance corpus at
`crates/mail-threading/testdata`. `mxr-sync` consumes that crate instead of
carrying its own internal threading module.

## What mxr has today

**Source:** `crates/mail-threading`

The implementation is a focused, mxr-independent crate for the Jamie
Zawinski/RFC 5256 threading algorithm. Specifically:

- **Step 1 — build the ID table.** Walk every message's `References`
  header, or `In-Reply-To` when `References` is absent, creating placeholder
  ("phantom") containers for referenced `Message-ID`s that have not been seen
  yet. Link adjacent pairs as parent → child, with cycle detection.

- **Step 2 — find the root set.** Containers whose parent is `None` form
  the root set of disjoint threads. Phantoms are pruned from public output by
  default.

- **Step 3 — subject-based merge.** For headerless replies (clients that
  strip `References`), normalise the subject (`Re:`, `Fw:`, `Aw:`, `Sv:`,
  …) and merge messages that share a normalised subject.

The public input is a small, mxr-agnostic struct:

```rust
pub struct Message {
    pub id: String,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub date: DateTime<Utc>,
    pub subject: String,
}
```

The public output is a flat thread membership list, which is what `mxr-sync`
needs:

```rust
pub struct Thread {
    pub root_message_id: String,
    pub messages: Vec<String>,
}
```

There is no `mxr-store`, `mxr-core`, provider type, SQL, or async dependency.

**Conformance coverage** lives in `crates/mail-threading/testdata/conformance`
and is loaded by `crates/mail-threading/tests/conformance.rs`. It covers:

- Basic two-message chain
- Two disjoint threads
- Missing references and multi-level phantom chains
- `In-Reply-To`-only parentage with present and missing parents
- Reply-before-parent arrival
- Cycle in `References` chain (detected and broken)
- Self-reference
- Missing `Message-ID` assignment behavior
- Duplicate `Message-ID` unique reassignment behavior
- Case-sensitive `Message-ID` comparison
- Quoted vs unquoted `Message-ID` normalization
- Invalid `References` fallback to `In-Reply-To`
- Conflicting `References` vs `In-Reply-To`
- Adjacent `References` links preserve existing parents
- Current message reparents to its last reference
- Subject normalisation across localized prefixes
- RFC 5256 whitespace collapse
- RFC 5256 subject blobs/list tags
- RFC 5256 `(fwd)` and `[Fwd: ...]` subject artifacts
- Headerless reply (subject-only fallback)
- Configurable subject fallback disabled
- Custom subject-prefix options
- Phantom-pruning options
- Deterministic ordering and canonical root behavior

## Ecosystem state

There is no maintained, published JWZ threading crate in Rust as of
2026-05-15.

| Candidate | Status |
|---|---|
| [`mailthread-rs`](https://github.com/asayers/mailthread-rs) | 0 stars, 1 commit, never published to crates.io |
| Built-in to `notmuch` / `mu` | C/C++ only, not callable from Rust without FFI scaffolding |
| `mail-parser` (stalwart) | Provides headers but **not** threading |
| `jmap-client` (stalwart) | Relies on server-side `Thread/get`; no client-side fallback |

This means every Rust project that wants to build an email client, an
archive viewer, a forensic mail analyser, an MX log replayer, or a
mailing-list archive search engine has to re-implement JWZ from scratch.
That is the gap we'd fill.

## Why our code is publication-ready

- **Spec posture.** The crate README links RFC 5256, the JWZ reference, and
  RFC 5322, and states exactly what is in and out of scope.
- **No mxr coupling.** The module imports `chrono`, `std::collections`,
  and nothing from `mxr-core`, `mxr-store`, or `mxr-protocol`.
- **Shared conformance corpus.** Rust tests load JSON fixtures from
  `crates/mail-threading/testdata`, and the future JS package must use the same
  corpus.
- **Small surface.** One input struct, one entry-point function, one flat
  output type, and options. No lifetimes, no async, no I/O.

## Public API

```rust
pub fn thread_messages(messages: &[Message]) -> Vec<Thread>;

pub fn thread_messages_with(
    messages: &[Message],
    options: &ThreadingOptions,
) -> Vec<Thread>;

pub struct Message {
    pub id: String,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub date: DateTime<Utc>,
    pub subject: String,
}

pub struct Thread {
    pub root_message_id: String,
    pub messages: Vec<String>,
}

pub struct ThreadingOptions {
    pub subject_merge: bool,
    pub prune_phantoms: bool,
    pub subject_prefixes: Vec<String>,
}
```

Nested tree output and trait-based input can be added later if external users
need them. The first published surface is intentionally flat because it is
small, stable, and matches `mxr-sync`'s real integration point.

## Implementation plan

The implementation plan is tracked at
`docs/extracted-crates/implementation/01-jwz-threading.md`.

Completed locally:

- Created `crates/mail-threading`.
- Added a comprehensive README with spec links, examples, caveats, semver
  policy, and conformance policy.
- Added shared JSON corpus at `crates/mail-threading/testdata`.
- Added Rust conformance tests loading that corpus.
- Replaced `mxr-sync`'s internal module with a dependency on `mail-threading`.

Before crates.io publishing:

- Run `scripts/cargo-test -p mail-threading --tests`.
- Run `cargo test -p mail-threading --doc`.
- Run `cargo publish --dry-run -p mail-threading`.
- Decide whether to add benchmarks before first publish or immediately after.

## Estimated effort

**Roughly half a day, agent-assisted, for the Rust crate alone.**

See [00-publishing-strategy.md](./00-publishing-strategy.md) for the
AI-era effort framework. The pre-agent estimate of "two working days"
assumed human typing; with agents driving the API polish, docs, and
benchmark setup, the lift collapses substantially. The remaining
human work is API design judgment (trait vs struct) and review.

## TS / npm distribution

**Recommended approach: native TS port + shared JSON corpus.**

The npm ecosystem is *worse* than crates.io here. The only JS port
(`conversationThreading-js`, max-mapper) last committed in **March
2013** and was never published to npm. `mail-threading` (2017) has had
no updates in 9 years. This means a maintained, dual-ecosystem JWZ
library is genuinely unfilled — and JS audience (webmail, Electron
clients, archive viewers, support tooling) is larger than the Rust
audience for this problem.

**Why TS port (not WASM):**
- Algorithm is stable (the 1997 spec hasn't moved; bug fixes will be
  rare).
- Surface is small (one entry function, one input type, one output
  tree).
- The drift risk is the lowest of any Tier 1 candidate — a shared
  corpus essentially eliminates it.
- Native TS gives users source-map debugging and a JS-native feel,
  which the algorithm-shaped audience appreciates.

**Corpus shape.** The shared corpus now lives in
`crates/mail-threading/testdata/conformance`. Each JSON file is an
implementation-neutral fixture used by Rust now and by the future TS package:

```json
{
  "name": "phantom-container-missing-root",
  "description": "A reply references a missing root; the missing ancestor is a phantom.",
  "spec": {
    "source": "jwz",
    "url": "https://www.jwz.org/doc/threading.html",
    "behavior": "missing referenced ancestors become phantom containers"
  },
  "options": {
    "subject_merge": true,
    "prune_phantoms": true
  },
  "input": [
    { "id": "reply", "message_id": "<b>", "in_reply_to": "<a>", "references": ["<a>"], "subject": "Re: Lunch?", "date": "2026-05-15T10:00:00Z" }
  ],
  "expected": {
    "threads": [
      { "root": "reply", "messages": ["reply"] }
    ]
  }
}
```

**Ship order.** This is the **second** Tier 1 crate to ship — after
`01-list-unsubscribe` has validated the workflow, JWZ becomes the
*headline*. Maximum credibility per line of code.

**Effort with dual publish.** ~1 day total: Rust crate + TS port +
corpus + CI wiring. The corpus is the heaviest piece (porting the
existing test cases to JSON fixtures) and is still agent-shaped work.

## Risks and unknowns

- **Trait vs struct API choice.** Resolved for v0.1: ship a concrete `Message`
  struct and keep the surface small. A trait-based input can be added later if
  callers need it.

- **Subject prefix corpus.** Covered by `localized-subject-prefixes.json` and
  `subject-prefixes-custom.json`.
  Users can override `ThreadingOptions.subject_prefixes`.

- **Performance on pathological inputs.** A long cycle in `References`
  (which the cycle-detector handles) is the worst case. Benchmarks should
  confirm O(n).

- **Bus factor.** Once published this becomes a thing we're expected to
  maintain. The crate is small enough that bug-fix releases should be rare.

## When to re-evaluate this decision

- If stalwart adds a `jmap-client::Thread::compute_local` or equivalent
  helper that does JWZ, the gap narrows. Re-evaluate but: it's likely such a
  helper would live behind a JMAP feature and not be a clean standalone
  threading crate, so the gap probably stays.
- If a third party publishes a competing JWZ crate first, decide whether to
  contribute there or ship anyway. Most likely we ship — our test corpus
  is good enough to compete on quality.

## Naming

Candidates considered:

- `jwz-threading` — descriptive, but JWZ is in-jargon and the name is
  associated with the man himself
- `email-thread` — clean, generic, possibly trademark-adjacent
- `mail-threading` — clean, parallel to `mail-parser`
- `rfc5256-threading` — most specific, ugliest
- `mboxthread` — too narrow

Recommended: **`mail-threading`**. Pairs naturally with `mail-parser`
and `mail-send` in the ecosystem.
