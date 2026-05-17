---
candidate: mail-threading
source_doc: ../../extractable-crates/done/02-jwz-threading.md
status: complete-extracted-and-published
external_repo: https://github.com/planetaryescape/mail-threading
crates_io: https://crates.io/crates/mail-threading
last_reviewed: 2026-05-16
---

# `mail-threading` implementation plan

> **Status: complete.** This document captured the in-repo phase. The crate
> was subsequently extracted to its own repository at
> [`planetaryescape/mail-threading`](https://github.com/planetaryescape/mail-threading)
> and published to crates.io. See
> [`02-mail-threading-external-repo.md`](./02-mail-threading-external-repo.md)
> for the extraction phase. Kept here as historical context.

## Decision

Build `mail-threading` as an independently publishable crate inside this
repo, then make `mxr-sync` consume it.

The old `crates/sync/src/threading.rs` module was a good seed, but publication
needed a clearer contract: comprehensive README, spec links, a spec-aligned
conformance suite, and a shared JSON corpus that will also drive the future JS
package.

## Product discipline

This is worth doing because there is a real ecosystem gap:

- Rust has no maintained, focused, published JWZ/RFC 5256 threading crate.
- npm has old or unrelated options, and the unscoped `mail-threading` npm
  name is already taken by a stale package.
- `mxr` already needs this for providers without native thread IDs.

This is not a plan to publish internal `mxr-*` crates. Those remain private
workspace implementation crates unless separately justified.

## Target layout

```text
crates/mail-threading/
  Cargo.toml
  README.md
  src/lib.rs
  tests/conformance.rs

packages/mail-threading/              # later JS package
  package.json
  src/index.ts
  test/conformance.test.ts

crates/mail-threading/testdata/
  README.md
  rfc5256-coverage.md
  schema.json
  conformance/*.json
```

The shared JSON corpus lives inside the Rust crate so it is included in the
published package. Rust loads it directly, and the future JS package should
load the same files from `crates/mail-threading/testdata`. The corpus is the
executable spec.

## Cargo structure

Keep one repo and one Cargo workspace.

Add `crates/mail-threading` as a real workspace member with independent
package metadata. Do not use `version.workspace = true` for this crate unless
we explicitly choose lockstep app/library releases later.

Root `Cargo.toml`:

```toml
[workspace.members]
"crates/mail-threading"

[workspace.dependencies]
mail-threading = { path = "crates/mail-threading", version = "0.1.0" }
```

`crates/sync/Cargo.toml`:

```toml
mail-threading = { workspace = true }
```

The public crate must not depend on unpublished `mxr-*` crates.

References:

- Cargo workspaces: <https://doc.rust-lang.org/cargo/reference/workspaces.html>
- Path plus version dependencies: <https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html>
- Publishing: <https://doc.rust-lang.org/cargo/reference/publishing.html>
- SemVer compatibility: <https://doc.rust-lang.org/cargo/reference/semver.html>

## Spec posture

The crate should state exactly what it implements:

- RFC 5256 `THREAD=REFERENCES`, especially the REFERENCES threading model.
- Jamie Zawinski's original threading algorithm as the historical source.
- RFC 5322 identification headers as input semantics.

Spec links:

- RFC 5256: <https://www.rfc-editor.org/rfc/rfc5256>
- JWZ threading: <https://www.jwz.org/doc/threading.html>
- RFC 5322: <https://www.rfc-editor.org/rfc/rfc5322>

The crate does not parse raw RFC 5322 messages. Callers provide parsed
message IDs, references, dates, and subjects.

## Public contract

Initial API should stay small.

```rust
pub fn thread_messages(messages: &[Message]) -> Vec<Thread>;

pub fn thread_messages_with(
    messages: &[Message],
    options: &ThreadingOptions,
) -> Vec<Thread>;
```

Input type:

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

Output type for `mxr` compatibility:

```rust
pub struct Thread {
    pub root_message_id: String,
    pub messages: Vec<String>,
}
```

Options:

```rust
pub struct ThreadingOptions {
    pub subject_merge: bool,
    pub prune_phantoms: bool,
    pub subject_prefixes: Vec<String>,
}
```

Defaults:

- `subject_merge = true`
- `prune_phantoms = true`
- localized prefixes include `re`, `fw`, `fwd`, `aw`, `sv`, `antw`, `rv`,
  `odp`, `tr`, `wg`

Defer nested tree output unless the corpus or external users need it. `mxr`
currently needs flat thread membership.

## README requirements

The crate README is a release gate. It must include:

1. Problem statement
   - local mail clients, archives, support tools, and migration utilities need
     client-side thread reconstruction.

2. Ecosystem gap
   - Rust gap: no maintained focused crate.
   - npm gap: old or unrelated packages; JS package will use same corpus.

3. Spec claim
   - which parts of RFC 5256 and JWZ are implemented.
   - what behavior is extension/configuration rather than strict spec.

4. Examples
   - basic reply chain.
   - missing parent/phantom container.
   - subject fallback.
   - subject fallback disabled.

5. Conformance promise
   - all fixtures in `crates/mail-threading/testdata/conformance` pass.
   - bug fixes start as corpus fixtures.

6. Caveats
   - raw email parsing is out of scope.
   - provider-native thread IDs should be preferred when available.
   - subject fallback can over-merge by nature.
   - duplicate `Message-ID` behavior is explicitly defined.

7. Operational details
   - complexity and expected performance.
   - MSRV.
   - semver policy.
   - feature flags, including optional serde support if added.

## Shared JSON corpus

The JSON corpus is mandatory because the future Rust and JS packages must
prove the same behavior.

Fixture shape:

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
    {
      "id": "reply",
      "message_id": "<reply@example>",
      "in_reply_to": "<root@example>",
      "references": ["<root@example>"],
      "subject": "Re: Lunch",
      "date": "2026-05-15T10:00:00Z"
    }
  ],
  "expected": {
    "threads": [
      {
        "root": "reply",
        "messages": ["reply"]
      }
    ]
  }
}
```

`crates/mail-threading/testdata/schema.json` defines this shape. Both implementations
must validate/load the same fixtures.

`crates/mail-threading/testdata/README.md` must define:

- what the corpus covers.
- what passing the corpus means.
- what is intentionally not covered.
- how to add fixtures.
- fixture naming conventions.

## Required conformance fixtures

Seeded from the old `crates/sync/src/threading.rs` tests:

- empty input.
- single message.
- basic reply chain.
- two independent threads.
- missing top reference.
- no replies.
- headerless subject fallback.
- headerless reply attaches to header-backed thread.
- same subject does not merge two header-backed threads.

Added before publishing:

- `In-Reply-To` only, parent present.
- `In-Reply-To` only, parent missing.
- missing `Message-ID`.
- missing `Message-ID` with a present parent.
- reply arrives before parent.
- multi-level missing phantom chain.
- cycle in `References`.
- self-reference.
- duplicate `Message-ID`.
- case-sensitive `Message-ID` comparisons.
- quoted vs unquoted `Message-ID` normalization.
- invalid `References` falling back to `In-Reply-To`.
- conflicting `References` and `In-Reply-To`.
- adjacent `References` links do not replace an existing parent.
- current message reparents to its last reference.
- stable ordering by date.
- canonical root under unusual dates.
- `subject_merge = false`.
- `prune_phantoms = false`.
- custom `subject_prefixes`.
- RFC 5256 whitespace collapse.
- RFC 5256 subject blobs/list tags.
- RFC 5256 `(fwd)` subject trailer.
- RFC 5256 `[Fwd: ...]` subject wrapper.
- prefixes: `Re[2]`, `Fwd`, `AW`, `SV`, `Antw`, `RV`, `Odp`, `TR`, `WG`.

Each fixture must name the spec/JWZ behavior it exercises.

## Implementation gaps fixed in the Rust crate

Current source: `crates/mail-threading/src/lib.rs`.

Fixed:

- Materialize missing `In-Reply-To` parents as phantom containers.
- Preserve reply-before-parent behavior.
- Add explicit RFC-aligned missing and duplicate `Message-ID` policy.
- Add case-sensitive `Message-ID` comparison coverage.
- Add quoted/unquoted `Message-ID` normalization.
- Add invalid `References` fallback to `In-Reply-To`.
- Add fixtures for RFC 5256's parent-preservation and current-message
  reparenting rules.
- Add RFC 5256 whitespace-collapse subject handling.
- Add RFC 5256 `(fwd)` and `[Fwd: ...]` subject artifact handling.
- Add RFC 5256 subject blob/list-tag handling.
- Verify and test cycle prevention.
- Verify and test self-reference handling.
- Keep output stable across input order where dates tie.
- Decided flat output is enough for v0.1. Nested tree output can be added later
  if external users need it.

The source audit doc `docs/extractable-crates/done/02-jwz-threading.md` has been
updated to point at the in-repo crate and shared corpus.

## mxr integration

`mxr-sync` should call the new crate from `rethread_account`.

Rules:

- Preserve native-thread split.
- Providers with native thread IDs keep using provider-native IDs.
- JWZ threading applies only when provider capabilities say native thread IDs
  are unavailable.
- Do not leak provider-specific concepts into `mail-threading`.

Likely affected files:

- `Cargo.toml`
- `Cargo.lock`
- `crates/sync/Cargo.toml`
- `crates/sync/src/engine.rs`
- `crates/sync/src/lib.rs`
- `crates/sync/src/threading.rs` (removed)
- `docs/extractable-crates/done/02-jwz-threading.md`

## JS package plan

Defer JS implementation until the Rust crate and corpus are stable.

Expected package:

- npm name: `@planetaryescape/mail-threading`
- implementation: native TypeScript port
- test source: same `crates/mail-threading/testdata/conformance/*.json`

Do not shell out to the `mxr` binary. This must be a library API.

The JS package is not allowed to fork behavior. New bugs or edge cases must
land in the shared corpus first, then both implementations are fixed.

## CI gates

Before publish:

```sh
scripts/cargo-test -p mail-threading --tests
cargo test -p mail-threading --doc
cargo publish --dry-run -p mail-threading
```

Recommended after first publish:

```sh
cargo semver-checks check-release -p mail-threading
```

If JS package exists, CI must also run its conformance test against the same
JSON corpus.

## Release plan

Completed locally:

1. Land `crates/mail-threading` and corpus without publishing.
2. Wire `mxr-sync` to consume it locally.
3. Run `mxr` sync tests that exercise non-native provider threading.
4. Run `cargo publish --dry-run -p mail-threading`.

External release steps, only after an explicit publish decision:

1. Publish `mail-threading` to crates.io.
2. Tag as `mail-threading-v0.1.0` or equivalent package-specific tag.
3. Later publish `@planetaryescape/mail-threading` to npm after TS port passes
   the same corpus.

Do not publish any internal `mxr-*` workspace crates as part of this.

## Done definition

The Rust crate is ready when:

- README explains why the crate exists, the ecosystem gap, the implemented
  spec, examples, caveats, and conformance policy.
- `crates/mail-threading/testdata` has schema, README, and spec-aligned fixtures.
- `crates/mail-threading/testdata/rfc5256-coverage.md` maps every fixture to
  covered, partial, out-of-scope, or intentional-divergence RFC behavior.
- Rust conformance tests load the shared JSON corpus.
- Known implementation gaps are fixed.
- `mxr-sync` consumes `mail-threading`.
- Native provider thread IDs still bypass JWZ.
- `scripts/cargo-test -p mail-threading --tests` passes.
- `cargo publish --dry-run -p mail-threading` passes.

The dual-ecosystem plan is ready when:

- JS package uses the same corpus.
- Rust and JS outputs match for every fixture.
- New behavior changes start as shared corpus changes.
