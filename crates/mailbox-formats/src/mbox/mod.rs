//! mbox file format ([RFC 4155]).
//!
//! Four variants are recognised, differing in how the body's `From `
//! lines are disambiguated from message separators:
//!
//! | Variant   | Escape rule                                                 |
//! |-----------|-------------------------------------------------------------|
//! | `Mboxo`   | No escaping. `From ` in body is ambiguous; readers heuristic. |
//! | `Mboxrd`  | `From ` lines prefixed with `>` get an extra `>`.            |
//! | `Mboxcl`  | `Content-Length:` header drives body length; no body escape. |
//! | `Mboxcl2` | Like `Cl` but always omits escaping.                         |
//!
//! In practice almost all real-world mbox files are `Mboxo` or `Mboxrd`.
//! `MboxVariant::Auto` sniffs the first few messages to decide.
//!
//! [RFC 4155]: https://datatracker.ietf.org/doc/html/rfc4155

mod reader;
mod variant;
mod writer;

pub use reader::MboxReader;
pub use writer::MboxWriter;

/// Which mbox dialect the reader/writer should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MboxVariant {
    /// Auto-detect from the first few messages. Sniffs for
    /// `Content-Length:` headers (`Mboxcl`/`Mboxcl2`) and `>From `
    /// escaping (`Mboxrd`). Falls back to `Mboxo` if no signals are
    /// found. Returns [`crate::Error::UndetectedMboxVariant`] only if
    /// the stream is empty.
    Auto,
    /// "Original" mbox. No escaping; `From ` at line start delimits.
    Mboxo,
    /// "Rd". Lines matching `>*From ` are escaped with an extra `>`.
    Mboxrd,
    /// "Cl". `Content-Length:` header is authoritative for body length;
    /// the body itself is unmodified.
    Mboxcl,
    /// "Cl2". Like `Cl` but the writer never escapes (rarely seen in
    /// the wild).
    Mboxcl2,
}
