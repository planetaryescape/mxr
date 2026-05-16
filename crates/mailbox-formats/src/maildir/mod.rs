//! Maildir file format ([DJB's spec]).
//!
//! Three required subdirectories: `tmp/`, `new/`, `cur/`. Delivery is
//! write-to-`tmp/` then rename-to-`new/` (same-filesystem atomic).
//! Reading the message moves it to `cur/` with optional flag suffix
//! `:2,<chars>` (Maildir++ extension).
//!
//! [DJB's spec]: https://cr.yp.to/proto/maildir.html

mod flags;
mod reader;
mod writer;

pub use reader::MaildirEntry;

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Handle to a Maildir directory.
///
/// On Unix the flag separator in filenames is `:`. On Windows that
/// character is illegal in filenames, so the default is `-` and
/// callers can override via [`Maildir::with_separator`]. Pick the same
/// separator the consumer/producer of your Maildir uses (Dovecot's
/// `maildir_separator` setting, for example).
pub struct Maildir {
    pub(super) root: PathBuf,
    pub(super) separator: char,
}

impl Maildir {
    /// Open an existing Maildir. Validates that `cur/`, `new/`, `tmp/`
    /// all exist; returns `MaildirMissingSubdir` if any is missing.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let root = path.as_ref().to_path_buf();
        for sub in ["cur", "new", "tmp"] {
            let p = root.join(sub);
            if !p.is_dir() {
                return Err(Error::MaildirMissingSubdir(sub.to_string()));
            }
        }
        Ok(Self {
            root,
            separator: default_separator(),
        })
    }

    /// Create a Maildir at `path`. Idempotent — succeeds if the tree
    /// already exists. Creates `cur/`, `new/`, `tmp/` with mode 0700
    /// on Unix.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let root = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        for sub in ["cur", "new", "tmp"] {
            std::fs::create_dir_all(root.join(sub))?;
        }
        Ok(Self {
            root,
            separator: default_separator(),
        })
    }

    /// Override the flag separator character.
    pub fn with_separator(mut self, sep: char) -> Self {
        self.separator = sep;
        self
    }

    /// The root directory.
    pub fn path(&self) -> &Path {
        &self.root
    }
}

fn default_separator() -> char {
    if cfg!(windows) {
        '-'
    } else {
        ':'
    }
}
