# Coverage matrix

`mail-query` parses Gmail-style email search queries (see
[README](../README.md)) into a typed AST. Each fixture below in
[`conformance/`](./conformance/) pins a behaviour of the public
contract.

The test suite enforces three invariants. A fixture file without a row
here fails `coverage_matrix_mentions_every_fixture`, and a row without a
fixture file fails `required_fixtures_exist`. The actual parser output
must match `expected_ast` (or `expected_error`) per
`conformance_fixtures_match_expected_outputs`.

| Fixture | Status | What it pins |
| --- | --- | --- |
| `text-bare-word` | covered | A bare word parses as `Text`. |
| `exact-plus-word` | covered | `+word` parses as `Exact` (no-stemming hint). |
| `phrase-quoted` | covered | `"..."` parses as `Phrase`. |
| `phrase-escaped-inner-quote` | covered | `\"` inside a phrase becomes a literal `"`. |
| `field-from` | covered | `field:value` parses with typed `QueryField`. |
| `filter-is-unread` | covered | Built-in `is:` filter parses as typed `FilterKind`. |
| `filter-custom-via-options` | covered | `ParserOptions::register_custom_filter` widens vocabulary. |
| `filter-unknown-without-registration-errors` | covered | Default-strict: unknown filter → `UnknownFilter`. |
| `label-bare` | covered | `label:value` parses as `Label`. |
| `date-specific-after` | covered | `after:YYYY-MM-DD` parses as `DateRange{After, Specific}`. |
| `date-relative-not-resolved-at-parse-time` | covered | `older:5d` is `Relative`, not a resolved date (intentional divergence). |
| `size-greater-than-megabytes` | covered | `size:>5mb` parses with op + bytes; 1024-based units. |
| `boolean-precedence-implicit-and-binds-tighter-than-or` | covered | OR is lower precedence than adjacency. |
| `parens-override-precedence` | covered | Parens override default precedence. |
| `negation-minus-and-not-keyword-equivalent` | covered | `-X` and `NOT X` both produce `Not(...)`. |

## Not covered

- IMAP SEARCH grammar (RFC 3501 §6.4.4). Vocabulary overlaps but the
  grammar does not. A separate future crate could parse IMAP SEARCH to
  the same AST as a normalisation layer.
- Backend execution. The crate emits an AST; tantivy, meilisearch,
  SQL FTS, etc. are out of scope.
- Validation against semantics. We do not check whether
  `from:alice OR is:unread` is meaningful — we just parse it.
- Operator drift across providers (Outlook KQL, Fastmail, Proton).
  Gmail-default vocabulary with `ParserOptions::custom_filters` for
  aliasing.

## Intentional divergences

- **`older:5d` parses as `Relative`, not a resolved date.** The AST is
  `now`-pure so saved queries don't drift. Backends resolve with
  `ParserOptions::now_provider`.
- **`+word` is a distinct variant (`Exact`), not `Text`.** The
  no-stemming hint is preserved in the AST so backends can act on it.
- **The AST does not preserve operator-source provenance.** `is:unread`
  and `is:Unread` both yield `FilterKind::Unread`. Display renders the
  canonical lower-case form.
- **`FilterKind::Custom(_)` is the escape hatch.** Filters not in the
  closed set require caller opt-in via `ParserOptions::register_custom_filter`.
