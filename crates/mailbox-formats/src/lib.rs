//! Read and write classic local-mail storage formats.
//!
//! Covered:
//!
//! - **mbox** ([RFC 4155]) — all four variants: `Mboxo`, `Mboxrd`,
//!   `Mboxcl`, `Mboxcl2`. Streaming reader and writer.
//! - **Maildir** ([DJB spec]) — atomic delivery, flag parsing, basic
//!   `cur/`/`new/`/`tmp/` iteration.
//! - **Locking** for mbox concurrent access: `LockStrategy::{Dotlock,
//!   Flock, Fcntl, FcntlThenDotlock}` plus the explicit `None`. Unix
//!   first-class; Windows degradation is documented.
//!
//! [RFC 4155]: https://datatracker.ietf.org/doc/html/rfc4155
//! [DJB spec]: https://cr.yp.to/proto/maildir.html
//!
//! # Quickstart
//!
//! ```
//! use std::io::Cursor;
//! use mailbox_formats::{MboxReader, MboxVariant, RawMessage};
//!
//! let bytes = b"From alice@example.com Tue Mar 17 09:30:00 2026\r\n\
//!               From: Alice <alice@example.com>\r\n\
//!               Subject: Hello\r\n\r\n\
//!               Hi there.\r\n\r\n";
//! let mut reader = MboxReader::new(Cursor::new(&bytes[..]), MboxVariant::Mboxrd);
//! let msg: RawMessage = reader.next().expect("one message")?;
//! assert_eq!(msg.header("Subject"), Some(&b"Hello"[..]));
//! # Ok::<_, mailbox_formats::Error>(())
//! ```
//!
//! # Design choices
//!
//! - **Byte-preserving**: `RawMessage` holds headers as
//!   `Vec<(String, Vec<u8>)>` and the body as `Vec<u8>`. No MIME
//!   decoding, no charset assumptions. Pair with [`mail-parser`] if you
//!   need decoded headers.
//! - **Streaming**: the mbox reader is `BufRead`-based with bounded
//!   memory; a 10GB mbox streams without loading into memory.
//! - **Honest variants**: `MboxVariant::Auto` sniffs the first few
//!   messages for `Content-Length:` headers and `>From ` escaping. If
//!   it can't tell, it returns
//!   [`Error::UndetectedMboxVariant`][crate::Error] rather than
//!   guessing.
//! - **Locking is explicit**: callers pick a [`LockStrategy`]; the
//!   default is `LockStrategy::None` (caller responsibility).
//!
//! [`mail-parser`]: https://crates.io/crates/mail-parser
//!
//! # Out of scope (v0.1.0)
//!
//! - **MH format**: rare in modern environments; add only if users
//!   file an issue.
//! - **Maildir++ subfolders** (`.foo.bar/`): basic Maildir only.
//! - **MIME parsing or header decoding**: keep
//!   `mailbox-formats` focused on file-format semantics.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unsafe_code, unused_must_use)]

mod error;
mod lock;
mod maildir;
mod mbox;
mod raw_message;

pub use error::{Error, Result};
pub use lock::{Lock, LockStrategy};
pub use maildir::{Maildir, MaildirEntry};
pub use mbox::{MboxReader, MboxVariant, MboxWriter};
pub use raw_message::{Flags, RawMessage};
