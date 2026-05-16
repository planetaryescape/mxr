---
candidate: mail-query
source_doc: ../../extractable-crates/done/03-gmail-query.md
status: complete-extracted-and-published
external_repo: https://github.com/planetaryescape/mail-query
crates_io: https://crates.io/crates/mail-query
last_reviewed: 2026-05-16
---

# Move `mail-query` (née `gmail-query`) to its own repo

> **Status: complete.** Published as
> [`mail-query v0.1.0`](https://crates.io/crates/mail-query) at
> [`planetaryescape/mail-query`](https://github.com/planetaryescape/mail-query).

The playbook applied here is captured in
[`../lessons/`](../lessons/). This file is the per-crate record.

## Goal

Carve mxr's hand-written Gmail-style query parser (1054 lines of
recursive descent in `crates/search/src/parser.rs` + 110 lines of AST in
`ast.rs`) out of `mxr-search` into a publishable, backend-agnostic Rust
crate. Publish to crates.io as `mail-query`. Cut mxr over to consume it
from the registry.

The migration is complete only when:

- `planetaryescape/mail-query` is the source of truth for the crate.
- `mail-query = "0.1.0"` is published on crates.io.
- docs.rs builds the crate docs.
- `mxr-search` consumes `mail-query = "0.1.0"` from crates.io and
  re-exports it so existing `mxr_search::QueryNode` paths keep working.
- The mxr-specific `is:owed-reply` and `is:reply-later` filters route
  through `FilterKind::Custom(_)` and are correctly handled at the
  `query_builder.rs` / `search_filter.rs` boundary.
- `crates/mail-query/` is removed from mxr.
- `mxr-search` and `mxr-daemon` tests pass against the registry
  dependency.

## Phase -1: Preflight

Per [`../lessons/01-preflight-checks.md`](../lessons/01-preflight-checks.md).
All boxes green at start:

- [x] `cargo search mail-query --limit 5` returns no match; `curl
      crates.io/api/v1/crates/mail-query` returns 404. Name free.
- [x] `rg "use mxr_" crates/search/src/parser.rs crates/search/src/ast.rs`
      empty. Parser is already standalone.
- [x] Categories `["email", "parser-implementations"]` validated.
- [x] MSRV set to `1.74` (matches list-unsubscribe; widens downstream
      adoption).
- [x] `cargo publish --dry-run -p mail-query --allow-dirty` from
      `mxr/` passes after Phase 0 lands.
- [x] Branch posture: continued on `release-clean` per established
      practice. Accepting the `git add -p` cost on commit.

## Phase 0: Carve `mail-query` in-tree (lessons/09)

This is a **carve-out** — lesson 09 applies. mxr-search holds a
production-credible parser; the contract design is the hoist.

### Layout

```text
crates/mail-query/
  Cargo.toml             # publish-ready: explicit deps, MSRV, license, include
  README.md              # full README per lessons/02 checklist
  src/
    lib.rs               # re-exports + crate-level docs
    parser.rs            # ported + behaviour-fixed
    ast.rs               # refactored (Custom/Exact/Relative, #[non_exhaustive])
    display.rs           # new: Display impl with property-tested round-trip
    visitor.rs           # new: Visitor trait + default walk
    error.rs             # extracted ParseError
    options.rs           # new: ParserOptions
  tests/
    conformance.rs       # corpus runner with 3 integrity tests
    unit_tests.rs        # 51 ported + new unit tests
  testdata/
    schema.json
    coverage.md
    conformance/         # 14 JSON fixtures
```

### Public API decisions (confirmed before Phase 0)

- **Closed enums + `FilterKind::Custom(String)` escape hatch.** Future
  Gmail additions (`has:reaction`, color stars beyond the common set)
  and application-specific filters (`is:owed-reply`) land in `Custom`.
- **`#[non_exhaustive]` on every public enum.** Tantivy's regret per
  web research; we apply the discipline from day one.
- **`+word` as `QueryNode::Exact(String)`.** Gmail's no-stemming hint
  becomes a distinct AST variant.
- **`older_than:5d` as `DateValue::Relative { amount, unit }`.** The
  parser does *not* resolve to a `NaiveDate`. Backends call
  `ParserOptions::now_provider` at query-execution time. Lets the AST
  round-trip cleanly through Display and lets saved queries not drift.
- **Visitor trait with default walk** for backend authors.

### Behavioural changes (largest scope risk)

Three behaviour changes landed alongside the code move:

1. **OR precedence fix.** mxr's parser had `a b OR c` produce
   `And(a, Or(b, c))` — opposite of Gmail and Lucene. Fixed to
   `Or(And(a, b), c)` by restructuring `parse_expression` to call
   `parse_or` which calls `parse_and`. No existing mxr test exercised
   the broken combination so nothing regressed.

2. **Brace-group regression caught and fixed.** The precedence fix
   made `parse_or` greedy via `parse_and`, which broke
   `parse_brace_group`'s expectation that `parse_or` returns a single
   atom. Fixed by having `parse_brace_group` call `parse_unary`
   directly. Caught by mxr-search's `e2e_search_gmail_or_braces_and_field_groups`
   test.

3. **`older_than:5d` AST shape change.** Old: `DateValue::Specific(NaiveDate)`
   resolved at parse time. New: `DateValue::Relative { amount, unit }`.
   mxr's `resolve_date` and `matches_date` consumers updated to handle
   the new variant.

### mxr-side boundary

Old `mxr_search::ast::QueryNode` paths kept working via:

```rust
// crates/search/src/lib.rs
pub mod ast {
    pub use mail_query::{
        DateBound, DateValue, FilterKind, ParseError, ParserOptions,
        QueryField, QueryNode, RelativeUnit, SizeOp, Visitor,
    };
}
pub use mail_query::{...};  // top-level re-exports too

pub const FILTER_OWED_REPLY: &str = "owed-reply";
pub const FILTER_REPLY_LATER: &str = "reply-later";

pub fn parse_query(input: &str) -> Result<QueryNode, ParseError> {
    let mut options = ParserOptions::new();
    options.register_custom_filters([FILTER_OWED_REPLY, FILTER_REPLY_LATER]);
    mail_query::parse_with(input, &options)
}
```

That re-export trick meant every `mxr_search::QueryNode` consumer in
the daemon kept compiling with zero churn. Only the four files that
pattern-match on `FilterKind` needed `Custom(name) if name == "owed-reply"`
arms.

Quality gates from `mxr/`:

```bash
cargo fmt -- --check
scripts/cargo-test -p mail-query --all-features
scripts/cargo-test -p mxr-search --tests
scripts/cargo-test -p mxr --lib
cargo clippy -p mail-query --all-targets --all-features --locked -- -D warnings
cargo publish --dry-run -p mail-query --allow-dirty
```

Commit `8e524ef — feat: prepare mail-query for external publish`.

## Phase 1: Subtree split + GitHub repo

```bash
git subtree split --prefix=crates/mail-query -b split/mail-query
gh repo create planetaryescape/mail-query --public \
  --description "Parser and typed AST for Gmail-style email search queries. Backend-agnostic."
git push git@github.com:planetaryescape/mail-query.git split/mail-query:main
git branch -D split/mail-query
cd .. && git clone git@github.com:planetaryescape/mail-query.git && cd mail-query
```

Split scanned 451 commits, retained 1. Wall clock ~30s.

## Phase 2: Standalone-ify

In the cloned sibling repo:

- `LICENSE-MIT`, `LICENSE-APACHE` copied from `mxr/`.
- `.gitignore` = `/target` only. `Cargo.lock` committed.
- `clippy.toml` with `msrv = "1.74"`.
- `.github/workflows/{ci.yml, publish.yml}` from lessons/06 verbatim.
- `cargo fmt` cleanups + a `#[allow(dead_code)]` on the conformance
  `Fixture` struct (serde-read fields the test doesn't use directly).

Verification:

```bash
cargo fmt -- --check
cargo test --all-features        # 58 tests pass (51 unit + 3 conformance + 4 doctest)
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo package --list             # 34 files including all testdata
cargo publish --dry-run          # 123 KiB / 33.5 KiB compressed
```

Commit `c39575d — chore: make crate standalone and add CI + publish workflow`.

## Phase 3: Publish via tag push

`CARGO_REGISTRY_TOKEN` repo secret set by the user (one-time browser
flow, ~60s).

```bash
git tag -a v0.1.0 -m "v0.1.0\n\nInitial release of mail-query. ..."
git push origin v0.1.0
```

Publish workflow ran fmt/test/doctest/clippy/dry-run gates then
`cargo publish` with the secret. Wall clock ~45s.

Verification:

```bash
cargo search mail-query --limit 3
curl -sI https://crates.io/api/v1/crates/mail-query    # HTTP/2 200
curl -sI https://docs.rs/mail-query/0.1.0              # 302 within ~10 min
```

## Phase 4: Cut mxr over

```toml
# mxr/Cargo.toml — workspace.dependencies
- mail-query = { path = "crates/mail-query", version = "0.1.0" }
+ mail-query = "0.1.0"
```

```toml
# mxr/Cargo.toml — workspace.members
- "crates/mail-query",
```

```bash
rm -rf crates/mail-query
cargo check -p mxr-search       # re-resolves to registry
cargo tree -p mxr-search -i mail-query
# mail-query v0.1.0
# └── mxr-search v0.5.18
scripts/cargo-test -p mxr-search --tests      # 36 pass
```

The lockfile cutover was a clean single-hunk diff (path → registry +
checksum). The outbound chrono dev-dep was temporarily stashed during
lock regeneration to keep the diff surgical (lessons/08).

Commit `4c2feff — chore: consume mail-query from crates.io`.

## Phase 5: Docs and status surfaces (lessons/05)

Three surfaces updated, table-first per lesson 05:

1. `docs/extractable-crates/README.md` — flipped row 03 from "Tier 1 —
   ship next" into the Done table, noting the crate-name change
   (`gmail-query` → `mail-query`). Bumped "ship next" pointer to
   `05-mailbox-formats`.
2. `docs/extractable-crates/03-gmail-query.md` moved to
   `done/03-gmail-query.md` with `Status: Shipped` banner and updated
   frontmatter (`crate_name: mail-query`).
3. Added this file as the executable migration record.

## Rollback plan

The cutover commit is `4c2feff`. To roll back:

```bash
git revert 4c2feff
cargo check -p mxr-search
```

The crates.io publish is immutable; rollback only affects mxr's
consumption. Behaviour changes from Phase 0 (precedence fix,
`DateValue::Relative` shape) would also need to revert if a regression
turns up.

## Lessons captured

New lessons from this extraction landed in
[`../lessons/`](../lessons/). The notable ones:

- **`pub use` re-export preserves consumer churn.** mxr-search adds
  `pub use mail_query::*` plus a thin `ast` submodule re-export, and
  ~all existing `mxr_search::QueryNode` paths keep compiling. This
  pattern works particularly well in carve-outs where you want zero
  churn on internal consumers.
- **Behaviour-changing carve-outs.** Three behaviour changes landed
  in Phase 0 (`older_than:5d` shape, OR precedence, brace group
  parsing). Each was caught by a different mxr-side test. Lesson 09
  gets an addendum on "carve-outs are not always pure code moves —
  expect to fix latent bugs in the original."
- **`#[non_exhaustive]` discipline from day one.** Tantivy's regret
  per the web research. Lesson 02 gets a small append.
- **Re-using the same `CARGO_REGISTRY_TOKEN` across publishes.** The
  token is per-account; new repos just need the secret copied in. The
  user flagged this as a recurring annoyance worth automating later.

## Final checklist

- [x] `crates/mail-query/` scaffolded in-tree with full public API
- [x] Conformance corpus + coverage matrix + 3 integrity tests
- [x] Display impl with property-tested round-trip
- [x] Visitor trait with default-walk dispatch
- [x] ParserOptions with custom-filter extension
- [x] `+word` Exact + `older_than:5d` Relative + OR precedence fix
- [x] `mxr-search` rewired via `pub use mail_query::*`
- [x] Phase 0 quality gates green
- [x] `planetaryescape/mail-query` exists on GitHub
- [x] Standalone-ified (license, gitignore, clippy.toml, CI, publish)
- [x] `CARGO_REGISTRY_TOKEN` secret set
- [x] `v0.1.0` tag pushed; publish workflow green
- [x] `mail-query v0.1.0` live on crates.io
- [x] mxr cuts over to registry version
- [x] `crates/mail-query/` removed from mxr
- [x] mxr-search + daemon tests pass against registry version
- [x] `docs/extractable-crates/` and `done/` surfaces updated
- [x] Lessons captured for future extractions
