//! Byte-preserving message representation.
//!
//! `mailbox-formats` deliberately does not parse MIME or decode header
//! values — that would couple this crate to a specific parser's
//! evolution and licensing. Headers and bodies pass through as bytes;
//! callers who want decoded values pair the output with
//! [`mail-parser`](https://crates.io/crates/mail-parser) or
//! [`mailparse`](https://crates.io/crates/mailparse).

use std::time::SystemTime;

use bitflags::bitflags;

/// One message as it lives in an mbox file or a Maildir folder.
///
/// Fields are kept byte-oriented so MIME-encoded values pass through
/// unmangled. The crate writes them out verbatim on the wire (with
/// per-variant escaping for mbox, naming for Maildir).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RawMessage {
    /// Headers as a list of `(name, value)` pairs. Name is ASCII-7
    /// (per RFC 5322 §2.2). Value is bytes so MIME-encoded forms
    /// (`=?UTF-8?B?...?=`) round-trip exactly.
    pub headers: Vec<(String, Vec<u8>)>,

    /// Message body. CRLF line endings expected for wire output;
    /// readers preserve whatever the source uses.
    pub body: Vec<u8>,

    /// `From ` line address. For mbox, this becomes the envelope-sender
    /// in the `From <addr> <date>` separator. For Maildir, this field
    /// is informational only — set to `None` if unknown.
    pub envelope_from: Option<String>,

    /// Timestamp for the `From ` line (mbox) or the unique filename
    /// (Maildir). Use `SystemTime::now()` if you don't have a real one.
    pub timestamp: SystemTime,

    /// Flags carried over from the source file. mbox formats don't
    /// natively carry flags except via the `Status:` and `X-Status:`
    /// headers (some readers do, but we don't); Maildir carries them
    /// in the filename suffix.
    pub flags: Flags,
}

impl RawMessage {
    /// Construct a `RawMessage` with empty flags, no envelope-from,
    /// and `SystemTime::now()` as the timestamp. Builder methods
    /// ([`Self::with_envelope_from`], [`Self::with_timestamp`],
    /// [`Self::with_flags`]) override the defaults.
    pub fn new(headers: Vec<(String, Vec<u8>)>, body: Vec<u8>) -> Self {
        Self {
            headers,
            body,
            envelope_from: None,
            timestamp: SystemTime::now(),
            flags: Flags::empty(),
        }
    }

    /// Set the envelope-from address (used as the `From ` line for mbox).
    pub fn with_envelope_from(mut self, from: impl Into<String>) -> Self {
        self.envelope_from = Some(from.into());
        self
    }

    /// Set the timestamp (used in the mbox `From ` date and the
    /// Maildir unique filename).
    pub fn with_timestamp(mut self, ts: SystemTime) -> Self {
        self.timestamp = ts;
        self
    }

    /// Set the message flags.
    pub fn with_flags(mut self, flags: Flags) -> Self {
        self.flags = flags;
        self
    }

    /// Find the first header with the given name (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_slice())
    }
}

bitflags! {
    /// IMAP-style flags shared across mbox (when reading `Status:` /
    /// `X-Status:` headers) and Maildir (filename suffix encoding).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Flags: u8 {
        /// Message has been read.
        const SEEN     = 0b0000_0001;
        /// Message has been replied to.
        const REPLIED  = 0b0000_0010;
        /// Message is flagged as important.
        const FLAGGED  = 0b0000_0100;
        /// Message is a draft.
        const DRAFT    = 0b0000_1000;
        /// Message is deleted (Maildir `T`, IMAP `\Deleted`).
        const DELETED  = 0b0001_0000;
        /// Message has been forwarded (Maildir `P`, "passed").
        const PASSED   = 0b0010_0000;
        /// Message is in the trash (Maildir `T` is sometimes used
        /// distinctly from `\Deleted`; we treat them as the same bit
        /// for compatibility with most MDAs).
        const TRASHED  = 0b0100_0000;
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Flags {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        self.bits().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Flags {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let bits = u8::deserialize(deserializer)?;
        Flags::from_bits(bits).ok_or_else(|| serde::de::Error::custom("invalid Flags bits"))
    }
}
