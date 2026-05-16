# mailbox-formats

[![Crates.io](https://img.shields.io/crates/v/mailbox-formats.svg)](https://crates.io/crates/mailbox-formats)
[![Documentation](https://docs.rs/mailbox-formats/badge.svg)](https://docs.rs/mailbox-formats)
[![License](https://img.shields.io/crates/l/mailbox-formats.svg)](#license)

Read and write classic local-mail storage formats: **mbox** (all four
variants — mboxo, mboxrd, mboxcl, mboxcl2) and **Maildir** (basic, with
atomic tmp→new delivery). Streaming reader, byte-preserving message
representation, configurable concurrent-access locking.

```rust
use std::io::Cursor;
use mailbox_formats::{MboxReader, MboxVariant};

let bytes = b"From alice@example.com Mon Mar 17 09:36:40 2025\r\n\
              From: Alice <alice@example.com>\r\n\
              Subject: Hello\r\n\r\n\
              Hi there.\r\n\r\n";
let mut reader = MboxReader::new(Cursor::new(&bytes[..]), MboxVariant::Mboxrd);
let msg = reader.next().expect("one message")?;
assert_eq!(msg.header("Subject"), Some(&b"Hello"[..]));
# Ok::<_, mailbox_formats::Error>(())
```

## Why this crate exists

The Rust ecosystem's mailbox-format coverage was weak across the board:

- [`maildir`](https://crates.io/crates/maildir) hasn't released since
  2023; Maildir-only.
- [`mbox-reader`](https://crates.io/crates/mbox-reader) hasn't released
  since 2019; mbox-read-only.
- [`melib`](https://crates.io/crates/melib) is actively maintained but
  EUPL/GPL-licensed and a heavyweight mail-client framework.

`mailbox-formats` is the focused, MIT/Apache-licensed, unified crate the
ecosystem didn't yet have. One dependency (`bitflags`) plus `thiserror`
for errors and `libc`/`windows-sys` for locking.

## What it does

- **mbox reader** — streaming `BufRead`-based iterator. A 10GB mbox
  streams without loading into memory.
- **mbox writer** — append-mode `Write` wrapper.
- **All four mbox variants**: `Mboxo` (no escape), `Mboxrd` (`>From `
  escape), `Mboxcl` and `Mboxcl2` (Content-Length framed).
- **`MboxVariant::Auto`** — sniffs the first few messages for
  `Content-Length:` headers and `>From ` escaping signals.
- **Maildir reader** — iterate `cur/` and `new/`, parse flag suffix
  (`:2,SRF`).
- **Maildir writer** — atomic `tmp/` → `new/` delivery with DJB-style
  unique filenames (`<seconds>.M<micros>P<pid>Q<counter>.<hostname>`).
- **Concurrent-access locking** — five strategies covering the common
  mbox lock conventions: `None`, `Dotlock`, `Flock`, `Fcntl`, and the
  Debian-default `FcntlThenDotlock`.

## What it does not do

- **No MIME parsing or header decoding.** Headers stay as
  `Vec<(String, Vec<u8>)>` and the body as `Vec<u8>`. Pair with
  [`mail-parser`](https://crates.io/crates/mail-parser) or
  [`mailparse`](https://crates.io/crates/mailparse) if you want
  decoded values.
- **No MH format.** Rare in modern environments; would add only if
  users file an issue.
- **No Maildir++ subfolders** (`.foo.bar/`). Basic Maildir only in
  v0.1.0.

## Spec anchors

- mbox: [RFC 4155](https://datatracker.ietf.org/doc/html/rfc4155)
  (Eric Allman's documentation of the format). Variants come from the
  original sendmail/Berkeley mail tradition and are documented at
  [Wikipedia](https://en.wikipedia.org/wiki/Mbox).
- Maildir: [D. J. Bernstein's spec](https://cr.yp.to/proto/maildir.html).
  Flag suffix and Maildir++ extensions documented at
  [Courier](https://www.courier-mta.org/maildir.html).
- mbox locking conventions follow
  [Dovecot's documentation](https://doc.dovecot.org/2.3/configuration_manual/mail_location/mbox/mboxlocking/).

## mbox variant guide

| Variant   | When to use                                          | Escape rule                                   |
|-----------|------------------------------------------------------|-----------------------------------------------|
| `Mboxrd`  | **Default for interchange.** Safe and unambiguous.   | `>*From ` body lines get an extra `>`.        |
| `Mboxo`   | Reading legacy files. Don't write new files in this. | None. `From ` in body is heuristically guessed.|
| `Mboxcl`  | Interoperating with old Unix utilities that use it.  | None. `Content-Length:` header frames bodies. |
| `Mboxcl2` | Rare; like `Cl` but never escapes.                   | Same as `Cl`.                                 |
| `Auto`    | Reading a file of unknown provenance.                | Sniffs the first ~64 KiB.                     |

When writing, `Auto` falls back to `Mboxrd` (the safest interchange
default). When reading, `Auto` returns `UndetectedMboxVariant` only if
the stream is empty.

## Locking guide

| Strategy             | Where it works    | When to use                                       |
|----------------------|-------------------|---------------------------------------------------|
| `None` (default)     | Everywhere        | Read-only iteration; caller takes responsibility. |
| `Dotlock`            | Unix + Windows    | Interoperate with mutt/procmail's `<path>.lock`.  |
| `Flock`              | Unix + Windows    | Single-host coordination only (advisory).         |
| `Fcntl`              | Unix only         | NFS-aware POSIX record locking.                   |
| `FcntlThenDotlock`   | Unix (Win: dotlock) | **Debian default.** Most interoperable.         |

On Windows, `Fcntl` is a no-op (POSIX advisory record locks don't
exist), and `FcntlThenDotlock` collapses to `Dotlock` alone. `Flock`
maps to `LockFileEx`.

[`Lock`] is an RAII guard — drop releases. Call `Lock::release` if you
want to handle release-time errors explicitly.

## Forward compatibility

Every public enum is `#[non_exhaustive]`. Adding new variants (a new
mbox dialect, a new lock strategy) will be non-breaking. Pattern-
matching callers must include a `_ => …` arm.

## Feature flags

- `serde` — adds `Serialize`/`Deserialize` derives to all public types.

## Companion direction

A future version may add:

- Maildir++ subfolder support (`.foo.bar/` paths).
- Dovecot keyword extension support (keep `a-z` flag characters in a
  `keywords: Vec<String>` field).
- A `mailbox-cli` binary for converting between formats (mbox ↔
  Maildir).
- A WASM build for `npm` distribution if a real consumer appears.

Out of scope for v0.1.0. Open an issue if you need them.

## Maintenance

- File bug reports at
  <https://github.com/planetaryescape/mailbox-formats/issues>.
- Patches that change behaviour should ship with a test case (unit
  test for parsing logic, integration test in `tests/maildir_fs.rs`
  for filesystem ops).

## License

MIT OR Apache-2.0. See [LICENSE-MIT](./LICENSE-MIT) and
[LICENSE-APACHE](./LICENSE-APACHE).
