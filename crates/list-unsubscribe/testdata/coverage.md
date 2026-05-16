# Coverage matrix

This crate parses RFC 2369 `List-Unsubscribe` and RFC 8058
`List-Unsubscribe-Post` headers. Each row below maps a fixture in
[`conformance/`](./conformance/) to the behavior it pins.

Each fixture is enforced by the test suite — a fixture file without a row
here will fail `coverage_matrix_mentions_every_fixture`, and a row here
without a matching fixture will fail
`conformance_corpus_contains_required_behavior_fixtures`.

| Fixture | Status | What it pins |
| --- | --- | --- |
| `rfc2369-mailto-only` | covered | Bare `<mailto:...>` returns `Mailto` with `subject = None` |
| `rfc2369-https-only` | covered | Bare `<https://...>` returns `HttpLink` |
| `rfc2369-both-prefer-mailto` | covered | Both schemes present, no Post header → `Mailto` preferred (intentional divergence; see README) |
| `rfc8058-one-click-basic` | covered | Both headers + https URL → `OneClick` |
| `rfc8058-one-click-case-insensitive` | covered | Post header in uppercase still parses |
| `rfc8058-post-without-https-falls-back` | covered | Post header without https URL falls back to standard preference |
| `mailto-with-subject` | covered | `?subject=` is captured |
| `mailto-with-subject-and-body-drops-body` | covered | `?body=` is intentionally dropped |
| `multiple-https-returns-first` | covered | First https URL wins (intentional divergence) |
| `malformed-url-returns-none` | covered | Unparseable URL is skipped; sole candidate → `None` |
| `empty-header` | covered | Empty header → `None` |
| `angle-bracket-whitespace-quirks` | covered | Inner padding is tolerated |
| `http-scheme-case-insensitive` | covered | `HTTPS://` recognised; `url::Url` normalises |
| `http-fallback-when-no-mailto` | covered | Plain `http://` URI is accepted |

## Not covered

- Verifying that a one-click POST actually unsubscribes. The crate's
  contract is classify-the-method only; the caller executes the POST.
- Parsing `List-Unsubscribe` from raw RFC 5322 messages. Use the
  optional `mail-parser` feature, or pass the header value directly.
- Body-scraping for unsubscribe links when the headers are absent. This
  is a policy decision and belongs above the crate.
- Decoding unusual MIME-encoded header values. `mail-parser` (when used)
  handles decoding; bare `parse_with_post` assumes the caller has
  already decoded.

## Intentional divergences

These are decisions where we are narrower or more opinionated than the
spec. Each is documented in the README.

- **Mailto preferred over http when both are present and no one-click
  Post header.** Mailto unsubscribe does not require a browser session
  and tends to be faster for power users.
- **`?body=` dropped from `mailto:` URIs.** Including it would let
  clients silently send pre-canned text on the user's behalf, which is
  a UX/safety footgun.
- **Multiple http URLs of the same scheme: first wins.** RFC 2369 does
  not specify; this gives callers a deterministic single choice.
