# Lessons from extracting `mail-threading`

Status: active playbook
source extraction: `mail-threading`
use for: every future package in `docs/extractable-crates`

## Companion files

This README is the conceptual playbook (15 lessons + checklists). The numbered files below are the hands-on, gotcha-level companions — use them at the corresponding phase of the next extraction.

- [`01-preflight-checks.md`](./01-preflight-checks.md) — Phase -1 checks before writing the migration plan (name availability, internal coupling, MSRV decision, category slugs, dry-run, branch posture).
- [`02-cargo-and-workspace-mechanics.md`](./02-cargo-and-workspace-mechanics.md) — Manifest inheritance, lockfile collateral, `cargo update -p` failure mode, `Cargo.lock` for libs, docs.rs config, `include` discipline, MSRV in two places.
- [`03-git-mechanics.md`](./03-git-mechanics.md) — Subtree split, signed annotated tags, GPG signing antipattern, branch hygiene, commit boundaries.
- [`04-ci-and-publish-pattern.md`](./04-ci-and-publish-pattern.md) — GitHub Secrets + tag-triggered CI publish. The full two-workflow setup, secret-injection safety, secret setup walkthrough.
- [`05-documentation-and-status-surfaces.md`](./05-documentation-and-status-surfaces.md) — The two/three-doc structure, three status surfaces to update on ship, planning-doc preservation.
- [`06-reusable-artifacts.md`](./06-reusable-artifacts.md) — Verbatim copy-paste templates: standalone `Cargo.toml`, `clippy.toml`, `.gitignore`, both workflows, license files, testdata schema, conformance test skeleton, commit messages.
- [`07-plan-template-refinements.md`](./07-plan-template-refinements.md) — Specific edits to apply to `02-mail-threading-external-repo.md` when copying as the next crate's plan.
- [`08-incidents-and-near-misses.md`](./08-incidents-and-near-misses.md) — Honest log of what broke during `mail-threading` and `list-unsubscribe`. Read before the next extraction.
- [`09-carving-out-of-existing-crates.md`](./09-carving-out-of-existing-crates.md) — Patterns that only show up when the new crate is scaffolded out of an existing one (rather than splitting an already-standalone workspace member). Captured from `list-unsubscribe`.
- [`10-publishing-bar.md`](./10-publishing-bar.md) — The three-test bar a candidate must clear before it's nominated for extraction. Re-read before adding a new file to `docs/extractable-crates/`. Captured 2026-05-16 after almost extracting `format-flowed` (a 4-page RFC, an afternoon's work) as the third crate.
- [`11-build-from-spec-carve-outs.md`](./11-build-from-spec-carve-outs.md) — The case where the mxr seed is thin and Phase 0 is mostly new code anchored to specs. Distinguishes from lesson 09's "carve out of existing crate" pattern (which assumed production-credible code being lifted). Captured 2026-05-17 from `mailbox-formats`.

## The short version

The biggest lesson from `mail-threading` is that extraction is not a file move.
It is a trust-building process.

Before `mail-threading`, "extractable crate" mostly meant:

- clean enough boundary
- useful enough API
- publishable enough manifest

After `mail-threading`, the bar is higher:

- explain why the package should exist
- anchor claims to a spec or stable external behavior
- ship a portable conformance corpus
- map coverage explicitly
- document intentional divergences
- give users enough policy knobs to disagree safely
- wire the crate back into `mxr` before asking anyone else to trust it
- make the standalone repo own the package after publish

Use this file as the checklist before starting the next package.

```bash
rg -n "Tier 1|Tier 2|ship|extract" docs/extractable-crates
```

## Lesson 1: Extract a contract, not code

The useful artifact was not just `src/lib.rs`. The useful artifact became:

```text
README.md
src/lib.rs
tests/conformance.rs
testdata/
  README.md
  schema.json
  conformance/*.json
  rfc5256-coverage.md
```

That shape changed the package from "code `mxr` happens to use" into "a public
contract others can evaluate."

For the next package, start by naming the contract:

```bash
sed -n '1,220p' docs/extractable-crates/01-list-unsubscribe.md
sed -n '1,220p' docs/extractable-crates/04-format-flowed.md
sed -n '1,220p' docs/extractable-crates/03-gmail-query.md
```

Ask:

- What behavior does this package promise?
- What inputs does it accept?
- What outputs does it return?
- Which choices are algorithmic facts?
- Which choices are policy?
- Which choices belong in `mxr`, not the crate?

If those answers are fuzzy, extraction is premature.

## Lesson 2: The README is part of the product

The `mail-threading` README had to do more than show examples. It had to help a
skeptical user decide whether to depend on the crate.

Every extracted package README needs:

- the problem it solves
- why the ecosystem needs another crate
- the spec or standard it follows
- what is covered
- what is out of scope
- intentional divergences
- examples that compile
- feature flags
- conformance test story
- maintenance expectations
- links to the source spec and coverage matrix

For a standards-adjacent crate, "clear API docs" are not enough. Users want to
know whether the implementation is trustworthy.

Good README question:

> If a maintainer of a competing crate reads this, can they see exactly what we
> claim and exactly what we do not claim?

Check this before publish:

```bash
cargo test -p <crate-name> --all-features --doc
cargo publish --dry-run -p <crate-name>
```

## Lesson 3: A spec anchor makes claims safer

`mail-threading` worked because RFC 5256 and JWZ threading gave the package a
stable external anchor.

Future packages should prefer anchors like:

- RFCs
- IETF drafts
- file format specs
- provider query syntax
- de facto behavior with stable examples
- existing test suites used by other implementations

The anchor does not have to mean "we implement everything." It means the crate
can explain:

- implemented
- partially implemented
- out of scope
- intentional divergence

That framing made these `mail-threading` statements safe:

- "This is not a full IMAP THREAD implementation."
- "Subject fallback is opinionated."
- "Same-subject header-backed roots are not force-merged."

Without a spec anchor, those statements would feel like caveats. With one, they
become honest boundaries.

Run this for each candidate:

```bash
rg -n "RFC|spec|standard|grammar|ABNF|conformance|corpus" docs/extractable-crates/<candidate>.md
```

If there is no stable anchor, raise the extraction bar.

## Lesson 4: Conformance is the trust mechanism

The conformance corpus became the most important artifact.

For `mail-threading`, the corpus did four jobs:

- proved behavior against the spec
- documented edge cases better than prose
- caught regressions
- became reusable for the future JS package

The Rust tests were not enough. The JSON fixtures mattered because another
implementation can load the same files.

Future corpus shape:

```text
testdata/
  README.md
  schema.json
  coverage.md
  conformance/
    basic-case.json
    edge-case.json
    regression-case.json
```

Every fixture should include:

- `name`
- `description`
- `spec.source`
- `spec.url`
- `spec.behavior`
- `input`
- `expected`

Run:

```bash
scripts/cargo-test -p <crate-name> --all-features --tests
```

If a future JS package is planned, require that the JSON corpus can be copied or
submoduled without depending on `mxr`.

## Lesson 5: Coverage must be visible

`mail-threading` needed a coverage matrix because "we have fixtures" is not the
same as "we know what those fixtures prove."

A coverage matrix should say:

- covered
- partial
- out of scope
- intentional divergence

This prevents two bad outcomes:

- users over-trust the crate
- maintainers accidentally shrink the contract

Every conformance fixture must appear in the matrix. The test suite should fail
when a fixture is added but not documented.

For the next crate, add a test like:

```rust
#[test]
fn coverage_matrix_mentions_every_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let matrix = std::fs::read_to_string("testdata/coverage.md")?;
    for fixture in fixture_names()? {
        assert!(
            matrix.contains(&format!("`{fixture}`")),
            "coverage matrix does not mention fixture {fixture}"
        );
    }
    Ok(())
}
```

Then run:

```bash
scripts/cargo-test -p <crate-name> --all-features --tests
```

## Lesson 6: Public packages need policy knobs

`mail-threading` taught us that opinionated defaults are fine only when users
can adapt them.

The crate kept defaults that work for `mxr`, but exposed options for:

- subject fallback on/off
- phantom pruning on/off
- custom subject prefixes

That made the package safer for users who want stricter RFC behavior or more
aggressive local-client behavior.

Future extraction rule:

- algorithmic behavior should be stable
- policy behavior should be configurable
- `mxr` preferences should be defaults, not hidden assumptions

Ask this before publishing:

```text
What would a strict/purist user object to?
What would a product/app developer want to tune?
Can they tune it without forking?
```

If the answer is no, either add options or document why the behavior is fixed.

## Lesson 7: Integration comes before extraction

Wiring `mail-threading` into `mxr-sync` before externalizing it was the right
move.

That proved:

- the public API was usable in the real product
- `mxr` did not need hidden internal hooks
- the return types were sufficient
- the crate boundary was real

For the next package, do not publish first and integrate later. Incubate inside
`mxr`, consume it through the same API external users will consume, then move it.

Run:

```bash
cargo tree -p <mxr-consuming-crate> -i <new-crate-name>
scripts/cargo-test -p <mxr-consuming-crate> --tests
```

If integration requires special backdoors, the API is not done.

## Lesson 8: Workspace-ready is not publish-ready

`mail-threading` looked like an independent crate, but the manifest still
inherited workspace metadata:

```toml
edition.workspace = true
license.workspace = true
rust-version.workspace = true
repository.workspace = true
homepage.workspace = true
```

That is fine in `mxr`. It is not fine in a standalone repo.

Before publishing a split crate, make explicit:

- `edition`
- `license`
- `rust-version`
- `repository`
- `homepage`
- dependencies
- dev-dependencies
- lints
- docs.rs metadata
- package `include`
- license files

Run in the standalone repo:

```bash
cargo package --list
cargo publish --dry-run
```

Inspect the package. Do not assume Cargo packed what users need.

## Lesson 9: Published versions are a promise

crates.io versions are effectively immutable. If `0.1.0` is wrong, the fix is
`0.1.1`.

That changes the workflow:

1. dry-run publish
2. inspect packaged files
3. run docs
4. run clippy
5. publish
6. verify crates.io
7. verify docs.rs
8. tag
9. cut `mxr` over

Commands:

```bash
cargo fmt -- --check
cargo test --all-features
cargo test --all-features --doc
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo package --list
cargo publish --dry-run
cargo publish
```

Do not delete the in-tree crate until the registry version exists and `mxr`
builds against it.

## Lesson 10: Community trust comes from honesty

The strongest parts of `mail-threading` were the honest boundaries:

- not a full IMAP THREAD server
- not a parser for raw RFC 5322 messages
- not exposing nested IMAP response trees
- subject fallback is pragmatic
- same-subject header-backed roots are intentionally not force-merged

That kind of honesty prevents community anger.

For every future crate, add a "What this is not" section.

Questions:

- What could a standards purist complain about?
- What could a user reasonably assume from the crate name?
- Where are we deliberately narrower than the spec?
- Where are we deliberately more opinionated than the spec?
- What behavior belongs above this crate?

If those answers are hidden, users will discover them as bugs.

## Lesson 11: The corpus may outlive the implementation

The `mail-threading` JSON corpus is likely more reusable than the Rust crate
itself. It can support:

- Rust package tests
- JS package tests
- bug reports
- third-party comparisons
- community standardization

That lesson applies to the next RFC-shaped packages.

For `list-unsubscribe`, the corpus may become the useful thing people reuse.
For `format-flowed`, corpus cases may matter more than the decoder.
For `gmail-query`, a portable query parser corpus could be the strongest
differentiator.

Design the corpus as if another maintainer will adopt it without adopting our
code.

## Lesson 12: Dual ecosystem work starts at the testdata layer

The future JS package should not reverse-engineer Rust tests. It should load the
same JSON fixtures.

Do this:

```text
Rust implementation -> loads testdata/conformance/*.json
JS implementation   -> loads testdata/conformance/*.json
```

Do not do this:

```text
Rust tests -> hand-written
JS tests   -> manually translated later
```

That second path creates drift.

If a candidate is likely to ship to npm, require a portable corpus before the
Rust crate is published.

## Lesson 13: Extraction creates maintenance ownership

After extraction, `mxr` should not be the source of truth.

The ownership model becomes:

- package repo receives bug fix
- package repo adds or updates fixture
- package repo publishes patch version
- `mxr` bumps dependency

Not:

- patch `mxr`
- copy patch to package later
- maybe remember to publish

For `mail-threading`, that means future changes land in
`planetaryescape/mail-threading` first.

For the next package, decide whether that workflow is acceptable before
publishing. If it feels too annoying, the code should probably stay internal.

## Lesson 14: Do not extract because it is easy

Product discipline still applies.

Extraction is justified when:

- there is a real external user problem
- the ecosystem gap is real
- the package has a defensible contract
- the maintenance cost is acceptable
- `mxr` gets a cleaner boundary

Extraction is not justified when:

- the code is merely reusable
- the package would duplicate healthy libraries
- the API is mostly `mxr` product policy
- the package lacks tests or a spec anchor
- we would resent maintaining it

Use this command to review the current candidate list:

```bash
sed -n '80,150p' docs/extractable-crates/README.md
```

## Lesson 15: The next package should pass the new crate test

Before starting another package, answer yes to most of these:

1. Is there a real ecosystem gap?
2. Is there a stable external contract?
3. Can we build a portable conformance corpus?
4. Can the API stay boring and free of `mxr` concepts?
5. Can disagreements become options?
6. Can `mxr` consume it before publishing?
7. Can we explain what it is not?
8. Can we maintain it outside `mxr`?
9. Can a README make a skeptical user trust it?
10. Can a coverage matrix prove what we cover?

If the answer is mostly no, defer.

## Applying this to current candidates

### Shipped

`list-unsubscribe`:

- Shipped on 2026-05-16 as
  [`list-unsubscribe v0.1.0`](https://crates.io/crates/list-unsubscribe)
  at [`planetaryescape/list-unsubscribe`](https://github.com/planetaryescape/list-unsubscribe).
- See [`09-carving-out-of-existing-crates.md`](./09-carving-out-of-existing-crates.md)
  for the new-shape lessons (this extraction was a carve-out, not a lift).

`mail-query` (née gmail-query):

- Shipped on 2026-05-16 as
  [`mail-query v0.1.0`](https://crates.io/crates/mail-query) at
  [`planetaryescape/mail-query`](https://github.com/planetaryescape/mail-query).
- The largest carve-out so far (~1200 LoC). Lesson 09 grew with three
  new sections: re-export bridges keep internal consumers compiling,
  behaviour-changing carve-outs are real, and `#[non_exhaustive]`
  discipline cascades downstream.
- The runbook is at
  [`../implementation/04-mail-query-external-repo.md`](../implementation/04-mail-query-external-repo.md).

`mailbox-formats`:

- Shipped on 2026-05-17 as
  [`mailbox-formats v0.1.0`](https://crates.io/crates/mailbox-formats)
  at
  [`planetaryescape/mailbox-formats`](https://github.com/planetaryescape/mailbox-formats).
- First **build-from-spec** carve-out — mxr's seed was a single
  195-line writer; the shipped crate is ~1400 LoC of spec-anchored
  new code (mbox reader + writer for 4 variants, Maildir reader +
  writer, full `LockStrategy` enum). Captured as new lesson 11.
- The runbook is at
  [`../implementation/05-mailbox-formats-external-repo.md`](../implementation/05-mailbox-formats-external-repo.md).

### Strong next candidates

`sync-engine`:

- See [07-sync-engine](../../extractable-crates/07-sync-engine.md) —
  marked investigate-later
- Real ecosystem gap; highest impact of any remaining candidate
- 2-3 day discovery before commit; wait for mxr's sync surface to
  stabilise

### High-risk or not now

`sync-engine`:

- real ecosystem gap
- high coupling risk
- hard to define a small public contract
- 2-3 day discovery before commit; wait until mxr's sync surface stabilises

### Won't do

Audited and rejected against the publishing bar
([`10-publishing-bar.md`](./10-publishing-bar.md)) on 2026-05-16.
Listed in [`../../extractable-crates/wont-do/`](../../extractable-crates/wont-do/):
`format-flowed`, `reader-quote-sig`, `outbound`, `rules`, `compose`,
`humanizer`, `llm`, `keychain`.

## Standard extraction artifact checklist

Every future package should have this before publish:

```text
<crate>/
  Cargo.toml
  README.md
  LICENSE-MIT
  LICENSE-APACHE
  clippy.toml
  src/lib.rs
  tests/conformance.rs
  testdata/
    README.md
    schema.json
    coverage.md
    conformance/*.json
  .github/workflows/ci.yml
```

Minimum CI:

```yaml
- run: cargo fmt -- --check
- run: cargo test --all-features
- run: cargo test --all-features --doc
- run: cargo clippy --all-targets --all-features --locked -- -D warnings
- run: cargo publish --dry-run
```

Minimum local publish checks:

```bash
cargo fmt -- --check
cargo test --all-features
cargo test --all-features --doc
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo package --list
cargo publish --dry-run
```

## Standard README checklist

Every extracted package README should answer:

- What does this package do?
- Why does it exist?
- What gap does it fill?
- What spec or behavior does it implement?
- What is the quickstart?
- What are realistic examples?
- What feature flags exist?
- What behavior is configurable?
- What does it deliberately not do?
- What conformance tests exist?
- What coverage is partial?
- Where is the source repo?
- How should users report gaps?

If a README cannot answer those, the package is not ready.

## Standard conformance checklist

Every conformance corpus should:

- use JSON or another language-neutral format
- include descriptions, not just inputs and outputs
- cite spec source and behavior
- include positive cases
- include malformed input cases
- include duplicate/conflict cases
- include ordering cases
- include option/policy cases
- include regressions found during integration
- have a schema
- have a coverage matrix
- be enforced by tests

Run:

```bash
scripts/cargo-test -p <crate-name> --all-features --tests
```

## Standard cutover checklist

When a package moves to its own repo:

```bash
git subtree split --prefix=crates/<crate> -b split/<crate>
gh repo create planetaryescape/<crate> --public
git push git@github.com:planetaryescape/<crate>.git split/<crate>:main
```

In the standalone repo:

```bash
cargo publish --dry-run
cargo publish
git tag v0.1.0
git push origin main v0.1.0
```

Back in `mxr`:

```bash
cargo update -p <crate-name>
cargo tree -p <consumer-crate> -i <crate-name>
scripts/cargo-test -p <consumer-crate> --tests
```

Then remove the in-tree crate only after registry resolution works.

## Mistakes to avoid next time

- Treating a workspace crate as publish-ready before checking manifest
  inheritance.
- Relying on Rust-only tests when a JS package is planned.
- Saying "spec-aligned" without a coverage matrix.
- Hiding opinionated behavior in defaults.
- Publishing before docs.rs and package contents are checked.
- Removing the in-tree crate before the registry dependency is verified.
- Letting `mxr` remain the source of truth after extraction.
- Creating a package just because the code is tidy.
- Creating a README that explains what the API is but not why users should trust
  it.
- Forgetting that a published package is a maintenance promise.

## The new extraction definition

An extracted crate is not:

> a folder moved out of `crates/`.

An extracted crate is:

> a small public contract with an owned repo, a tested API, a conformance
> corpus, a coverage story, honest docs, semver discipline, and a maintenance
> path back into `mxr`.

Do not start the next package until that is the goal.
