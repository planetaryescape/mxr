# mail-query

[![Crates.io](https://img.shields.io/crates/v/mail-query.svg)](https://crates.io/crates/mail-query)
[![Documentation](https://docs.rs/mail-query/badge.svg)](https://docs.rs/mail-query)
[![License](https://img.shields.io/crates/l/mail-query.svg)](#license)

Parser and typed AST for Gmail-style email search queries.
Backend-agnostic — produces a portable [`QueryNode`], you pick the
search engine.

```rust
use mail_query::{parse, FilterKind, QueryField, QueryNode};

let ast = parse("from:alice subject:\"deploy\" is:unread after:2026-01-01")?;
// AST is now a Boxed tree of And/Or/Not + leaves. Walk it with the
// Visitor trait to translate to tantivy / meilisearch / SQL FTS / IMAP
// SEARCH / whatever backend you prefer.
# Ok::<_, mail_query::ParseError>(())
```

## Why this crate exists

Every Rust email project re-implements this parser. The closest
alternatives:

- `query-parser`, `search-query-parser` — generic, toy syntax, no email
  operators.
- `tantivy-query-grammar::UserInputAst` — Lucene vocabulary, not Gmail.
  Pulls heavy deps. Not `#[non_exhaustive]`, so any spec addition is a
  breaking change for downstream pin-on-major users.

`mail-query` is the focused Gmail-vocabulary parser the ecosystem
doesn't yet have.

## What it does

- Parses Gmail's documented operator surface from
  <https://support.google.com/mail/answer/7190>:
  - Address fields: `from:`, `to:`, `cc:`, `bcc:`, `deliveredto:`,
    `rfc822msgid:`, `list:`
  - Content fields: `subject:`, `body:`, `filename:`
  - `is:` and `has:` filters
  - `label:` and `category:`
  - `size:`, `larger:`, `smaller:` with unit suffixes (`5M`, `200K`)
  - `after:`, `before:`, `date:`, `older:`, `newer:`, `older_than:`,
    `newer_than:` with both specific dates and relative durations
    (`older_than:5d`)
  - `AND` / `OR` / `NOT` / `-` / parentheses / brace groups
  - `AROUND<n>` for word proximity
- Recognises `+word` as an exact-match (no-stemming) hint, mirroring
  Gmail's syntax.
- Round-trips: `parse(node.to_string())? == node` (structural equality,
  not byte identity).
- Walks the AST via a [`Visitor`] trait so backend authors can translate
  to their own query language.
- Exposes extension points for caller-specific filters: register names
  via [`ParserOptions::register_custom_filter`] and they route through
  [`FilterKind::Custom`].

## What it does not do

- It does **not** execute queries. The output is a portable AST; you
  pick the backend.
- It does **not** resolve `older_than:5d` to a concrete date at parse
  time. The AST carries `DateValue::Relative { amount, unit }`;
  backends call `ParserOptions::now_provider` at execution time. This
  is what lets a saved query mean the same thing tomorrow as today and
  lets the AST round-trip without embedding a date.
- It does **not** parse IMAP SEARCH grammar (RFC 3501 §6.4.4) — that's a
  separate, future crate. The vocabularies overlap but the grammars do
  not.

## Intentional divergences

These are decisions where the crate is narrower or more opinionated
than the Gmail surface.

- **`older_than:5d` is `Relative`, not a resolved `NaiveDate`.** See
  above.
- **`+word` is a distinct AST variant `Exact`, not `Text`.** The no-
  stemming hint is preserved so backends can act on it.
- **OR has lower precedence than AND.** `a b OR c` parses as
  `(a AND b) OR c`. Matches Gmail's documented behaviour and Lucene
  convention.
- **Unknown filters error by default.** A bare `is:my-app-flag` returns
  [`ParseError::UnknownFilter`] unless the caller has registered it via
  [`ParserOptions::register_custom_filter`]. This is the
  default-strict posture; opt in to widen.

## Extensibility

Filter names Gmail adds over time, color-star variants beyond the
common set, or your application's own `is:owed-reply` — register them
once at construction time:

```rust
use mail_query::{parse_with, FilterKind, ParserOptions, QueryNode};

let mut options = ParserOptions::new();
options.register_custom_filters(["owed-reply", "reply-later"]);

let ast = parse_with("is:owed-reply", &options)?;
assert_eq!(
    ast,
    QueryNode::Filter(FilterKind::Custom("owed-reply".into()))
);
# Ok::<_, mail_query::ParseError>(())
```

The crate canonicalises names to lowercase + hyphenated form, so
`is:owed_reply` and `is:Owed-Reply` both resolve to
`Custom("owed-reply")`.

## Visitor

```rust
use mail_query::{parse, FilterKind, Visitor};

#[derive(Default)]
struct CountFilters(usize);
impl Visitor for CountFilters {
    fn visit_filter(&mut self, _: &FilterKind) {
        self.0 += 1;
    }
}

let ast = parse("from:alice is:unread OR has:attachment")?;
let mut counter = CountFilters::default();
counter.walk(&ast);
assert_eq!(counter.0, 2);
# Ok::<_, mail_query::ParseError>(())
```

The default `walk` implementation recurses into `And` / `Or` / `Not`
and dispatches to typed `visit_*` hooks for leaves. Override only what
you need.

## Forward compatibility

Every public enum is `#[non_exhaustive]`. New variants (for new Gmail
operators) are non-breaking additions. Pattern-matching callers must
include a `_ => …` arm.

## Conformance

The full coverage matrix lives in
[`testdata/coverage.md`](./testdata/coverage.md). Each fixture is a
language-neutral JSON file under [`testdata/conformance/`](./testdata/conformance/)
so a future port to another language can adopt the same corpus.

Three tests enforce the integrity of the corpus:

1. Every fixture file is referenced in `coverage.md`.
2. Every contract-critical fixture exists on disk.
3. The actual parser output matches `expected_ast` (or
   `expected_error`) for every fixture.

```bash
cargo test --all-features
```

## Feature flags

- `serde` — adds `Serialize`/`Deserialize` derives to every AST type,
  with `chrono/serde` enabled for `NaiveDate`. Default off.

The crate has two required dependencies (`chrono` with `clock` only and
`thiserror`) and no transitive runtime cost beyond that.

## Companion direction

Future work (out of scope for v0.1.0):

- Tantivy interop: `From<tantivy_query_grammar::UserInputAst>` and
  back. Behind a feature flag so the heavy deps stay opt-in.
- IMAP SEARCH grammar (RFC 3501 §6.4.4) parser to the same AST as a
  normalisation layer.
- WASM build for an `npm` package consuming the same conformance
  corpus.

If you want them, open an issue.

## Maintenance

- File bug reports at
  <https://github.com/planetaryescape/mail-query/issues>.
- Patches that change behaviour must add or update a fixture in
  `testdata/conformance/` and a row in `testdata/coverage.md`.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](./LICENSE-MIT) and
[LICENSE-APACHE](./LICENSE-APACHE).
