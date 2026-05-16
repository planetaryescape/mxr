---
candidate: list-unsubscribe
source_doc: ../../extractable-crates/done/01-list-unsubscribe.md
status: complete-extracted-and-published
external_repo: https://github.com/planetaryescape/list-unsubscribe
crates_io: https://crates.io/crates/list-unsubscribe
last_reviewed: 2026-05-16
---

# Move `list-unsubscribe` to its own repo

> **Status: complete.** This document is the executable record of the
> extraction. The crate is published at
> [`list-unsubscribe v0.1.0`](https://crates.io/crates/list-unsubscribe) and
> hosted at
> [`planetaryescape/list-unsubscribe`](https://github.com/planetaryescape/list-unsubscribe).

The playbook applied here is captured in
[`../lessons/`](../lessons/). This file is the per-crate record;
the playbook itself is the source of truth for *how* each step works.

## Goal

Carve a new crate out of `crates/mail-parse/src/lib.rs` (where the RFC 2369
+ RFC 8058 parser used to live), publish it to crates.io as
`list-unsubscribe`, then cut mxr over to consume it from the registry.

The migration is complete only when:

- `planetaryescape/list-unsubscribe` is the source of truth for the crate.
- `list-unsubscribe = "0.1.0"` is published on crates.io.
- docs.rs builds the crate docs.
- `mxr-mail-parse` consumes `list-unsubscribe = "0.1.0"` from crates.io
  with the `mail-parser` feature enabled and converts at the boundary
  into mxr-core's 5-variant `UnsubscribeMethod`.
- `crates/list-unsubscribe/` is removed from `mxr`.
- `mxr-mail-parse` tests pass against the registry dependency.

## Phase -1: Preflight

Per [`../lessons/01-preflight-checks.md`](../lessons/01-preflight-checks.md).
All boxes green at start:

- [x] `cargo search list-unsubscribe --limit 5` — name free on crates.io.
- [x] No `mxr_*` imports inside the new crate (it was scaffolded fresh).
- [x] Categories `["email", "parser-implementations"]` validated against the
      crates.io API. The lessons-cited `/category_slugs` HTML endpoint now
      returns 404; the JSON API at `/api/v1/categories?per_page=100` is the
      working alternative.
- [x] MSRV set to `1.74` (lower than workspace 1.88) — parsing crate; the
      lower bar widens downstream adoption.
- [x] `cargo publish --dry-run -p list-unsubscribe --allow-dirty` from
      `mxr/` passes.
- [x] Branch posture: continued on `release-clean` per user direction (the
      working tree had 60+ unrelated dirty files; the cost was ~10 minutes of
      `git add -p` work at commit time, well-trodden territory per
      lessons/08).

## Phase 0: Carve the crate in-tree

The mail-threading extraction lifted an already-standalone workspace member.
This extraction was different: the parser was a private function buried in
`crates/mail-parse/src/lib.rs` and the `UnsubscribeMethod` enum lived in
`mxr-core` with five variants (including the mxr-specific `BodyLink` from
HTML body scraping). The cleanest path was to **scaffold a new
`crates/list-unsubscribe/` from scratch with the contract-shaped public API**
and only then refactor `mxr-mail-parse` to consume it.

Layout delivered:

```text
crates/list-unsubscribe/
  Cargo.toml          # publish-ready: explicit deps, MSRV, license, include allowlist
  README.md           # full README per lessons/02 checklist
  src/lib.rs          # parse, parse_with_post, optional parse_from_message
  tests/conformance.rs
  testdata/
    README.md
    schema.json
    coverage.md
    conformance/      # 14 JSON fixtures
```

Public API:

```rust
pub enum UnsubscribeMethod {
    OneClick { url: Url },
    HttpLink { url: Url },
    Mailto { address: String, subject: Option<String> },
    None,
}

pub fn parse(header_value: &str) -> UnsubscribeMethod;
pub fn parse_with_post(header_value: &str, post: Option<&str>) -> UnsubscribeMethod;

#[cfg(feature = "mail-parser")]
pub fn parse_from_message(message: &mail_parser::Message<'_>) -> UnsubscribeMethod;
```

Features: `serde` (also activates `url/serde`), `mail-parser` (adds the
optional adapter).

The mxr boundary in `crates/mail-parse/src/lib.rs:407` was replaced with
`convert_unsubscribe(list_unsubscribe::parse_from_message(message))`, where
`convert_unsubscribe` is a private 4-arm match that maps each public variant
to `mxr_core::types::UnsubscribeMethod` (URLs become `String`s via
`Url::into`). `BodyLink` is not produced by this path — the existing
`body_unsubscribe_from_html` fallback (line 83) still owns it.

The old internal `parse_list_unsubscribe` and `parse_mailto_unsubscribe`
were removed. The `url` dep was dropped from `mxr-mail-parse`'s Cargo.toml
since the parser was its only use.

Quality gates from `mxr/`:

```bash
cargo fmt -- --check
scripts/cargo-test -p list-unsubscribe --all-features
scripts/cargo-test -p mxr-mail-parse --tests
cargo clippy -p list-unsubscribe --all-targets --all-features --locked -- -D warnings
cargo publish --dry-run -p list-unsubscribe --allow-dirty
```

All green. Commit `34cac5e — feat: prepare list-unsubscribe for external publish`.

## Phase 1: Subtree split + GitHub repo

```bash
git subtree split --prefix=crates/list-unsubscribe -b split/list-unsubscribe
gh repo create planetaryescape/list-unsubscribe --public \
  --description "Parse RFC 2369 List-Unsubscribe + RFC 8058 one-click headers into a typed action enum."
git push git@github.com:planetaryescape/list-unsubscribe.git split/list-unsubscribe:main
git branch -D split/list-unsubscribe
```

The split produced a single commit (the in-tree prep commit). Repo URL:
<https://github.com/planetaryescape/list-unsubscribe>.

## Phase 2: Standalone-ify

Cloned the new repo as a sibling directory. Added:

- `LICENSE-MIT`, `LICENSE-APACHE` (copied from `mxr/`).
- `.gitignore` (`/target` only — `Cargo.lock` is committed for libraries).
- `clippy.toml` with `msrv = "1.74"` (matches `package.rust-version`).
- `.github/workflows/ci.yml` and `.github/workflows/publish.yml` from
  [`../lessons/06-reusable-artifacts.md`](../lessons/06-reusable-artifacts.md)
  verbatim.

The `Cargo.toml` was already publish-ready from Phase 0 — no workspace
inheritance, explicit dep versions, `include` allowlist, `package.metadata.docs.rs`
block, lints declared. Zero edits needed.

Standalone verification:

```bash
cargo fmt -- --check
cargo test --all-features
cargo test --all-features --doc
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo package --list      # 26 files, including all testdata/**
cargo publish --dry-run   # 60.4 KiB / 18.1 KiB compressed
```

Commit `abceccd — chore: make crate standalone and add CI + publish workflow`.
Pushed to `main`. First CI run green in ~32 s.

## Phase 3: Publish via tag push

`CARGO_REGISTRY_TOKEN` repo secret set by the user (one-time browser flow,
~60 s).

```bash
git tag -a v0.1.0 -m "v0.1.0\n\nInitial release ..."
git push origin v0.1.0
```

The tag push triggered `publish.yml`. The workflow ran fmt/test/doctest/
clippy/publish-dry-run gates, then called `cargo publish` with the secret.
Total wall time: ~38 s.

Verification:

```bash
cargo search list-unsubscribe --limit 3
# list-unsubscribe = "0.1.0"  ← live

curl -sI https://crates.io/api/v1/crates/list-unsubscribe
# HTTP/2 200
```

docs.rs build is async; typically rendered within 1–10 minutes after publish.

## Phase 4: Cut mxr over

Back in `mxr/`:

```toml
# Cargo.toml — workspace.dependencies
- list-unsubscribe = { path = "crates/list-unsubscribe", version = "0.1.0" }
+ list-unsubscribe = "0.1.0"
```

```toml
# Cargo.toml — workspace.members
- "crates/list-unsubscribe",
```

```bash
rm -rf crates/list-unsubscribe
cargo check -p mxr-mail-parse
# Updating crates.io index / Locking 1 package to latest compatible version /
# Adding list-unsubscribe v0.1.0

cargo tree -p mxr-mail-parse -i list-unsubscribe
# list-unsubscribe v0.1.0
# └── mxr-mail-parse v0.5.18

scripts/cargo-test -p mxr-mail-parse --tests
# 13 passed; 0 failed
```

The `Cargo.lock` cutover diff was two clean hunks: the `list-unsubscribe`
entry flipped from path to registry-source-and-checksum, and `url` lost
`serde_derive` (the published crate doesn't activate `url/serde` by
default). An unrelated outbound chrono dev-dep was temporarily stashed
during lock regeneration to keep the diff surgical (lessons/08).

Commit `61235c9 — chore: consume list-unsubscribe from crates.io`.

## Phase 5: Docs and status surfaces

Per [`../lessons/05-documentation-and-status-surfaces.md`](../lessons/05-documentation-and-status-surfaces.md),
three surfaces updated in update-last-but-verify-first order:

1. `docs/extractable-crates/README.md` — row moved from active Candidates
   table into the Done table with crates.io + repo links.
2. `docs/extractable-crates/01-list-unsubscribe.md` moved to
   `done/01-list-unsubscribe.md` and prefixed with a `Status: Shipped`
   banner.
3. Frontmatter updated: `status: published`, `decision: shipped`,
   `external_repo`, `crates_io`, `last_reviewed`.

This document was added as the third surface — the executable migration
record, mirroring `02-mail-threading-external-repo.md`.

## Rollback plan

The cutover commit is `61235c9`. To roll back:

```bash
git revert 61235c9
cargo check -p mxr-mail-parse   # re-resolves to the previous source
```

The crates.io publish is immutable; rollback only affects mxr's consumption.
If a regression is found post-publish, fix in the standalone repo and ship
`v0.1.1`.

## Lessons captured

New lessons from this extraction landed in
[`../lessons/`](../lessons/). The notable ones:

- The `crates.io/category_slugs` HTML endpoint returns 404; use the JSON API
  endpoint `/api/v1/categories?per_page=100`. Captured in
  `01-preflight-checks.md` (or a follow-up file).
- "Pathspec `git stash push -- <file>`" can sweep in more than intended
  when the index is also dirty. Use it carefully or rely on `git add -p`.
- Carving a new crate out of an existing one (rather than splitting an
  already-standalone workspace member) introduces a different shape of
  Phase 0: scaffold-then-rewire, not lift-and-publish. The boundary
  conversion (`From<list_unsubscribe::UnsubscribeMethod>` → mxr-core's
  enum) is the new artifact — captured in `09-carving-out-of-existing-crates.md`.

## Final checklist

- [x] `crates/list-unsubscribe/` scaffolded in-tree with public API
- [x] Conformance corpus + coverage matrix + integrity tests
- [x] `mxr-mail-parse` rewired through the new crate
- [x] Phase 0 quality gates green
- [x] `planetaryescape/list-unsubscribe` exists on GitHub
- [x] Repo standalone-ified (license, gitignore, clippy.toml, CI, publish workflows)
- [x] `CARGO_REGISTRY_TOKEN` secret set in the standalone repo
- [x] `v0.1.0` tag pushed and publish workflow green
- [x] `list-unsubscribe v0.1.0` live on crates.io
- [x] `mxr/Cargo.toml` consumes registry version
- [x] `crates/list-unsubscribe/` removed from `mxr`
- [x] `mxr-mail-parse` tests pass against registry version
- [x] `docs/extractable-crates/` and `docs/extracted-crates/` status surfaces updated
- [x] Lessons captured for future extractions
