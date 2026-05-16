# list-unsubscribe

[![Crates.io](https://img.shields.io/crates/v/list-unsubscribe.svg)](https://crates.io/crates/list-unsubscribe)
[![Documentation](https://docs.rs/list-unsubscribe/badge.svg)](https://docs.rs/list-unsubscribe)
[![License](https://img.shields.io/crates/l/list-unsubscribe.svg)](#license)

Parse `List-Unsubscribe` (RFC 2369) and `List-Unsubscribe-Post` (RFC 8058)
email headers into a typed action enum.

```rust
use list_unsubscribe::{parse_with_post, UnsubscribeMethod};

let header = "<mailto:u@example.com>, <https://example.com/unsub?u=abc>";
let post = Some("List-Unsubscribe=One-Click");

match parse_with_post(header, post) {
    UnsubscribeMethod::OneClick { url } => {
        // POST to `url` with body `List-Unsubscribe=One-Click`
    }
    UnsubscribeMethod::Mailto { address, subject } => {
        // Open mail composer to `address` with `subject`
    }
    UnsubscribeMethod::HttpLink { url } => {
        // Open `url` in a browser
    }
    UnsubscribeMethod::None => {
        // No header offered, or every candidate was unparseable
    }
}
```

## Why this crate exists

In February 2024 Gmail and Yahoo introduced
[bulk-sender deliverability requirements](https://support.google.com/mail/answer/81126).
One of them is mandatory RFC 8058 one-click unsubscribe for senders above
5,000 messages/day. This promoted `List-Unsubscribe-Post` from "obscure RFC"
to "required for inbox placement", and elevated the audience for clients
that honour it.

Every Rust project that wants to act on `List-Unsubscribe` — mail readers,
mailing-list compliance tools, deliverability auditors, spam filters using
header presence as a positive signal — re-implements the parsing logic.
`mail-parser` and `mailparse` expose the raw header but no typed action
enum, no RFC 8058 awareness, no `mailto:` query parsing.

This crate fills that gap, and nothing more.

## What it does

- Parses RFC 2369 multi-method headers like
  `<mailto:list@x>, <https://x/u>`.
- Distinguishes RFC 8058 one-click (POST endpoint) from a plain web link
  via the accompanying `List-Unsubscribe-Post` header.
- Captures the `?subject=` parameter from `mailto:` URIs.
- Skips unparseable URIs silently and falls through to the next candidate.

## What it does not do

- It does **not** POST to the one-click endpoint. The caller picks an
  HTTP client (`reqwest`, `ureq`, whatever) and executes the action.
- It does **not** send the unsubscribe mail. The caller hands the
  `Mailto` variant to a mail composer.
- It does **not** scrape unsubscribe links from the message body. That
  is a policy decision that belongs above the crate.
- It does **not** capture `?body=` from `mailto:` URIs. See
  "Intentional divergences" below.
- It does **not** verify the unsubscribe endpoint actually works. The
  contract is "parse the header, classify the method".

## Spec anchors

- [RFC 2369](https://www.rfc-editor.org/rfc/rfc2369) — the
  `List-Unsubscribe` header.
- [RFC 8058](https://www.rfc-editor.org/rfc/rfc8058) — one-click
  unsubscribe with `List-Unsubscribe-Post`.
- [RFC 6068](https://www.rfc-editor.org/rfc/rfc6068) — the `mailto:`
  URI scheme.
- [Google sender rules](https://support.google.com/mail/answer/81126) —
  the deliverability backstory.

## Conformance

The full coverage matrix lives in
[`testdata/coverage.md`](./testdata/coverage.md). Each fixture is a
language-neutral JSON file under
[`testdata/conformance/`](./testdata/conformance/) so a future
TypeScript or other-language port can load the same corpus.

Three tests enforce the integrity of the corpus:

1. Every fixture file is referenced in `coverage.md`.
2. Every contract-critical fixture exists on disk.
3. The actual parser output matches `expected` for every fixture.

Run them with:

```bash
cargo test --all-features
```

## Intentional divergences

These are decisions where this crate is narrower or more opinionated
than the spec.

- **Mailto preferred over http** when both are present and no one-click
  Post header. Mailto unsubscribe does not require a browser session and
  tends to be faster for power users; clients that want the opposite
  preference can pattern-match on the returned enum.
- **`?body=` dropped from `mailto:` URIs.** Including it would let
  clients silently send pre-canned text on the user's behalf, which is
  a UX and safety footgun.
- **Multiple URLs of the same scheme: first wins.** RFC 2369 does not
  specify ordering; this gives callers a deterministic single choice.

## Feature flags

- `serde` — derives `Serialize` + `Deserialize` for `UnsubscribeMethod`
  (internally tagged with `kind`), and pulls in `url/serde`.
- `mail-parser` — adds `parse_from_message(&mail_parser::Message<'_>)`
  for callers that already use the `mail-parser` crate to parse RFC 5322
  messages.

The default feature set is empty. The crate has one required dependency
(`url`) and no transitive runtime cost beyond that.

## Companion direction

A future v2 could add executor features (`reqwest`-backed
`oneclick.unsubscribe()`, `lettre`-backed `mailto.send()`) but v1 is
deliberately parse-only. If you want them, open an issue.

## Maintenance

- File bug reports at
  <https://github.com/planetaryescape/list-unsubscribe/issues>.
- Patches that change behaviour must add or update a fixture in
  `testdata/conformance/` and a row in `testdata/coverage.md`.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](./LICENSE-MIT) and
[LICENSE-APACHE](./LICENSE-APACHE).
