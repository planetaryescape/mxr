use thiserror::Error;

/// Errors returned by the mailbox-formats crate.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The mbox stream is structurally malformed (e.g. missing `From `
    /// separator, premature EOF inside headers, invalid `Content-Length:`
    /// when reading mboxcl/mboxcl2).
    #[error("malformed mbox: {0}")]
    MalformedMbox(String),

    /// A Maildir filename did not follow the `unique:2,flags` (or
    /// configured separator) convention. Returned by the reader's
    /// flag-parsing step; the entry is skipped.
    #[error("malformed maildir filename: {0}")]
    MalformedMaildirFilename(String),

    /// `MboxVariant::Auto` couldn't determine the variant from the
    /// first few messages — typically because the input is empty or
    /// contains neither `Content-Length:` headers nor `>From ` escaping
    /// signals.
    #[error("could not auto-detect mbox variant")]
    UndetectedMboxVariant,

    /// Lock acquisition failed (already held, permission denied, NFS
    /// link race, etc.). The carried string is the underlying reason.
    #[error("lock acquisition failed: {0}")]
    Lock(String),

    /// A Maildir directory was opened but is missing one of the
    /// required `cur/`, `new/`, `tmp/` subdirectories.
    #[error("maildir directory missing required subdir: {0}")]
    MaildirMissingSubdir(String),
}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, Error>;
