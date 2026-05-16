//! Maildir writer: atomic `tmp/` → `new/` delivery and flag updates.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};
use crate::raw_message::{Flags, RawMessage};

use super::flags::format_flag_chars;
use super::reader::MaildirEntry;
use super::Maildir;

static DELIVERY_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Maildir {
    /// Deliver `msg` atomically: write to `tmp/<unique>`, fsync, rename
    /// to `new/<unique>`. The `tmp/` and `new/` directories must be on
    /// the same filesystem (POSIX `rename` is atomic only there).
    pub fn deliver(&self, msg: &RawMessage) -> Result<MaildirEntry> {
        self.deliver_into("new", msg, Flags::empty())
    }

    /// Deliver directly to `cur/` with the given flags (skips the
    /// new→cur dance). Use this when the caller knows the message
    /// should be considered already-seen.
    pub fn deliver_with_flags(&self, msg: &RawMessage, flags: Flags) -> Result<MaildirEntry> {
        self.deliver_into("cur", msg, flags)
    }

    /// Move an entry from `new/` to `cur/`, setting the flag suffix.
    pub fn mark_with_flags(&self, entry: &MaildirEntry, flags: Flags) -> Result<MaildirEntry> {
        let new_name = match flags.is_empty() {
            true => entry.unique_id.clone(),
            false => format!(
                "{}{}2,{}",
                entry.unique_id,
                self.separator,
                format_flag_chars(flags)
            ),
        };
        let dest = self.root.join("cur").join(&new_name);
        fs::rename(&entry.path, &dest)?;
        Ok(MaildirEntry {
            path: dest,
            unique_id: entry.unique_id.clone(),
            flags,
        })
    }

    fn deliver_into(
        &self,
        target_subdir: &str,
        msg: &RawMessage,
        flags: Flags,
    ) -> Result<MaildirEntry> {
        let unique = unique_filename(msg.timestamp);
        let tmp = self.root.join("tmp").join(&unique);
        let suffix = if flags.is_empty() {
            String::new()
        } else {
            format!("{}2,{}", self.separator, format_flag_chars(flags))
        };
        let dest_name = format!("{unique}{suffix}");
        let dest = self.root.join(target_subdir).join(&dest_name);

        write_message_to(&tmp, msg)?;

        match fs::rename(&tmp, &dest) {
            Ok(()) => Ok(MaildirEntry {
                path: dest,
                unique_id: unique,
                flags,
            }),
            Err(e) => {
                // Best-effort cleanup of the tmp/ artifact.
                let _ = fs::remove_file(&tmp);
                Err(Error::Io(e))
            }
        }
    }
}

fn write_message_to(path: &PathBuf, msg: &RawMessage) -> Result<()> {
    let mut file: File = OpenOptions::new().write(true).create_new(true).open(path)?;

    for (name, value) in &msg.headers {
        file.write_all(name.as_bytes())?;
        file.write_all(b": ")?;
        file.write_all(value)?;
        file.write_all(b"\r\n")?;
    }
    file.write_all(b"\r\n")?;
    file.write_all(&msg.body)?;
    // Ensure the body ends with a newline. Some readers depend on it.
    if !msg.body.ends_with(b"\n") {
        file.write_all(b"\r\n")?;
    }
    file.sync_all()?;
    Ok(())
}

/// Generate a DJB-style unique filename: `<seconds>.M<microseconds>P<pid>R<random>.<hostname>`.
/// The random suffix handles clock-tick collisions within a single
/// process; an atomic counter handles tighter races.
fn unique_filename(ts: SystemTime) -> String {
    let dur = ts.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs();
    let micros = dur.subsec_micros();
    let pid = std::process::id();
    let counter = DELIVERY_COUNTER.fetch_add(1, Ordering::Relaxed);
    let hostname = hostname_or("localhost");
    format!("{secs}.M{micros}P{pid}Q{counter}.{hostname}")
}

fn hostname_or(default: &str) -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| default.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    fn msg() -> RawMessage {
        RawMessage {
            headers: vec![
                ("From".to_string(), b"alice@example.com".to_vec()),
                ("Subject".to_string(), b"hi".to_vec()),
            ],
            body: b"Hello.".to_vec(),
            envelope_from: None,
            timestamp: UNIX_EPOCH + Duration::from_secs(1742204200),
            flags: Flags::empty(),
        }
    }

    #[test]
    fn deliver_writes_to_new_directory() {
        let dir = tempdir().unwrap();
        let md = Maildir::create(dir.path()).unwrap();
        let entry = md.deliver(&msg()).unwrap();
        assert!(entry.path.starts_with(dir.path().join("new")));
        assert!(entry.path.exists());
        // The tmp/ artifact should be gone after rename.
        let tmp = dir.path().join("tmp").join(&entry.unique_id);
        assert!(!tmp.exists());
    }

    #[test]
    fn deliver_with_flags_writes_to_cur_with_suffix() {
        let dir = tempdir().unwrap();
        let md = Maildir::create(dir.path()).unwrap();
        let flags = Flags::SEEN | Flags::REPLIED;
        let entry = md.deliver_with_flags(&msg(), flags).unwrap();
        let name = entry.path.file_name().unwrap().to_str().unwrap();
        assert!(name.ends_with(":2,RS") || name.ends_with("-2,RS"));
        assert!(entry.path.starts_with(dir.path().join("cur")));
    }

    #[test]
    fn deliver_then_iterate_returns_message() {
        let dir = tempdir().unwrap();
        let md = Maildir::create(dir.path()).unwrap();
        md.deliver(&msg()).unwrap();
        let entries: Vec<_> = md.iter().collect::<Result<Vec<_>>>().unwrap();
        assert_eq!(entries.len(), 1);
        let parsed = entries[0].read().unwrap();
        assert_eq!(parsed.header("Subject"), Some(&b"hi"[..]));
    }

    #[test]
    fn mark_with_flags_moves_new_to_cur() {
        let dir = tempdir().unwrap();
        let md = Maildir::create(dir.path()).unwrap();
        let entry = md.deliver(&msg()).unwrap();
        let updated = md.mark_with_flags(&entry, Flags::SEEN).unwrap();
        assert!(updated.path.starts_with(dir.path().join("cur")));
        let name = updated.path.file_name().unwrap().to_str().unwrap();
        assert!(name.ends_with(":2,S") || name.ends_with("-2,S"));
        // Original new/ path should be gone.
        assert!(!entry.path.exists());
    }

    #[test]
    fn open_validates_subdirs() {
        let dir = tempdir().unwrap();
        // Missing tmp/.
        std::fs::create_dir(dir.path().join("cur")).unwrap();
        std::fs::create_dir(dir.path().join("new")).unwrap();
        let result = Maildir::open(dir.path());
        assert!(matches!(result, Err(Error::MaildirMissingSubdir(_))));
    }
}
