//! The local text-log namespace and its cross-process rotation lock.
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use tracing_subscriber::fmt::MakeWriter;

pub(crate) const MAX_RECORD_BYTES: usize = 64 * 1024;
const LOCK_NAME: &str = ".mxr-log.lock";

#[derive(Clone, Copy)]
struct Limits {
    bytes: u64,
    files: usize,
}

impl Limits {
    fn from_config(config: &mxr_config::LoggingConfig) -> io::Result<Self> {
        let bytes = u64::from(config.max_size_mb) * 1024 * 1024;
        if bytes == 0
            || config.max_files == 0
            || bytes.checked_mul(u64::from(config.max_files)).is_none()
        {
            return Err(io::Error::other("logging.max_size_mb and max_files must be positive with a representable total byte cap"));
        }
        Ok(Self {
            bytes,
            files: config.max_files as usize,
        })
    }
}

fn regular_open(path: &Path, create: bool, append: bool) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(create || append)
        .create(create)
        .append(append)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other("log path must be a regular file"));
    }
    Ok(file)
}

/// Readers never create a directory/lock. Mutation and writers use the same lock.
pub(crate) fn lock(dir: &Path, create: bool) -> io::Result<Option<File>> {
    let file = match regular_open(&dir.join(LOCK_NAME), create, false) {
        Ok(file) => file,
        Err(e) if !create && e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    file.lock()?;
    Ok(Some(file))
}

pub(crate) fn archives(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|s| s.strip_prefix("mxr."))
            .and_then(|s| s.strip_suffix(".log"))
        else {
            continue;
        };
        if id.len() == 20
            && id.bytes().all(|b| b.is_ascii_digit())
            && id.parse::<u64>().is_ok()
            && entry.file_type()?.is_file()
        {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

pub(crate) fn files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = archives(dir)?;
    let active = dir.join("mxr.log");
    match active.symlink_metadata() {
        Ok(meta) if meta.is_file() => files.push(active),
        Ok(_) => {
            return Err(io::Error::other(
                "active log must be a regular file, not a link",
            ))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    Ok(files)
}

pub(crate) fn open_read(path: &Path) -> io::Result<File> {
    regular_open(path, false, false)
}

fn diagnostic(message: &str) -> Vec<u8> {
    let mut bytes = serde_json::json!({"timestamp":chrono::Utc::now().to_rfc3339(),"level":"WARN","target":"mxr::logging","fields":{"message":message}}).to_string().into_bytes();
    bytes.push(b'\n');
    bytes
}

#[derive(Clone)]
pub(crate) struct LogWriter {
    dir: PathBuf,
    config_path: PathBuf,
    fallback: mxr_config::LoggingConfig,
    mirror_allowed: bool,
}

impl LogWriter {
    pub(crate) fn new(
        dir: PathBuf,
        config: mxr_config::LoggingConfig,
        mirror_allowed: bool,
    ) -> io::Result<Self> {
        Limits::from_config(&config)?;
        std::fs::create_dir_all(&dir)?;
        let writer = Self {
            dir,
            config_path: mxr_config::config_file_path(),
            fallback: config,
            mirror_allowed,
        };
        writer.commit(&[], false)?;
        Ok(writer)
    }

    fn config(&self) -> io::Result<mxr_config::LoggingConfig> {
        // Do not call load_config here: its missing-file DEBUG event would
        // recursively enter this writer. Parsing itself emits no tracing.
        match std::fs::read_to_string(&self.config_path) {
            Ok(text) => Ok(mxr_config::load_config_from_str(&text)
                .map_or_else(|_| self.fallback.clone(), |c| c.logging)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(self.fallback.clone()),
            Err(e) => Err(e),
        }
    }

    fn commit(&self, bytes: &[u8], oversized: bool) -> io::Result<()> {
        let _lock = lock(&self.dir, true)?;
        let config = self.config()?;
        let limits = Limits::from_config(&config)?;
        let omitted;
        let mut record = bytes;
        if oversized || bytes.len() as u64 > limits.bytes {
            omitted = diagnostic("log record omitted: exceeds bounded record/file size");
            record = &omitted;
        }
        let active = self.dir.join("mxr.log");
        let mut file = regular_open(&active, true, true)?;
        let len = file.metadata()?.len();
        let startup_notice;
        if len > limits.bytes && record.is_empty() {
            startup_notice =
                diagnostic("oversized existing log discarded to enforce configured cap");
            record = &startup_notice;
        }
        if record.len() as u64 > limits.bytes {
            return Err(io::Error::other("log diagnostic exceeds size cap"));
        }
        if len > limits.bytes || len.saturating_add(record.len() as u64) > limits.bytes {
            // Allocate under the shared lock; order does not depend on clocks
            // or per-process randomness during concurrent rollovers.
            let last = archives(&self.dir)?.last().and_then(|p| {
                p.file_name()?
                    .to_str()?
                    .strip_prefix("mxr.")?
                    .strip_suffix(".log")?
                    .parse::<u64>()
                    .ok()
            });
            let next = match last {
                Some(last) => last
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("log archive sequence exhausted"))?,
                None => 0,
            };
            let archive = self.dir.join(format!("mxr.{next:020}.log"));
            if archive.symlink_metadata().is_ok() {
                return Err(io::Error::other("archive path already exists"));
            }
            std::fs::rename(&active, &archive)?;
            file = match regular_open(&active, true, true) {
                Ok(file) => file,
                Err(error) => {
                    if active.symlink_metadata().is_err() {
                        let _ = std::fs::rename(&archive, &active);
                    }
                    return Err(error);
                }
            };
        }
        let mut retained = archives(&self.dir)?;
        // A reduced cap applies to old archives too. Unknown paths and symlinks
        // never enter this namespace selection.
        for path in &retained {
            if path.symlink_metadata()?.len() > limits.bytes {
                std::fs::remove_file(path)?;
            }
        }
        retained.retain(|p| p.exists());
        let excess = retained.len().saturating_sub(limits.files - 1);
        for path in retained.into_iter().take(excess) {
            std::fs::remove_file(path)?;
        }
        let start = file.metadata()?.len();
        if let Err(error) = file.write_all(record) {
            let _ = file.set_len(start);
            return Err(error);
        }
        if self.mirror_allowed && config.stderr && !record.is_empty() {
            std::io::stderr().lock().write_all(record)?;
        }
        Ok(())
    }
}

pub(crate) struct Record<'a> {
    writer: &'a LogWriter,
    bytes: Vec<u8>,
    oversized: bool,
}
impl Write for Record<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !self.oversized && self.bytes.len().saturating_add(bytes.len()) <= MAX_RECORD_BYTES {
            self.bytes.extend_from_slice(bytes);
        } else {
            self.bytes.clear();
            self.oversized = true;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Drop for Record<'_> {
    fn drop(&mut self) {
        if self.writer.commit(&self.bytes, self.oversized).is_err() && self.writer.mirror_allowed {
            // No tracing, event contents, or protocol/TUI output on failure.
            if self.writer.config().is_ok_and(|c| c.stderr) {
                let _ = std::io::stderr().write_all(b"mxr: local log write failed\n");
            }
        }
    }
}
impl<'a> MakeWriter<'a> for LogWriter {
    type Writer = Record<'a>;
    fn make_writer(&'a self) -> Self::Writer {
        Record {
            writer: self,
            bytes: Vec::new(),
            oversized: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_writer(dir: &Path, files: u32) -> LogWriter {
        LogWriter {
            dir: dir.into(),
            config_path: dir.join("absent-config"),
            fallback: mxr_config::LoggingConfig {
                max_size_mb: 1,
                max_files: files,
                stderr: false,
                ..Default::default()
            },
            mirror_allowed: false,
        }
    }
    #[test]
    fn exact_threshold_then_rollover_preserves_valid_records() {
        let dir = tempfile::tempdir().unwrap();
        let writer = test_writer(dir.path(), 2);
        let mut line = b"{\"n\":1}".to_vec();
        line.resize(31, b' ');
        line.push(b'\n');
        std::fs::write(
            dir.path().join("mxr.log"),
            line.repeat(1024 * 1024 / 32 - 1),
        )
        .unwrap();
        writer.commit(&line, false).unwrap();
        assert!(archives(dir.path()).unwrap().is_empty());
        assert_eq!(
            std::fs::metadata(dir.path().join("mxr.log")).unwrap().len(),
            1024 * 1024
        );
        writer.commit(&line, false).unwrap();
        let retained = archives(dir.path()).unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(std::fs::metadata(&retained[0]).unwrap().len(), 1024 * 1024);
        assert_eq!(std::fs::read(dir.path().join("mxr.log")).unwrap(), line);
    }
    #[test]
    fn startup_and_smaller_count_converge_without_touching_unknown_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mxr.log"), vec![b'x'; 2 * 1024 * 1024]).unwrap();
        for index in 0..4 {
            std::fs::write(dir.path().join(format!("mxr.{index:020}.log")), "{}\n").unwrap();
        }
        let unknown = dir.path().join("unrelated.log");
        std::fs::write(&unknown, "preserve").unwrap();
        let writer = test_writer(dir.path(), 2);
        writer.commit(&[], false).unwrap();
        assert_eq!(archives(dir.path()).unwrap().len(), 1);
        let notice: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("mxr.log")).unwrap()).unwrap();
        assert!(notice["fields"]["message"]
            .as_str()
            .unwrap()
            .contains("oversized existing"));
        assert_eq!(std::fs::read_to_string(unknown).unwrap(), "preserve");
        std::fs::write(
            &writer.config_path,
            "[logging]\nmax_size_mb=1\nmax_files=1\nstderr=false\n",
        )
        .unwrap();
        writer.commit(b"{}\n", false).unwrap();
        assert!(archives(dir.path()).unwrap().is_empty());
    }
    #[test]
    fn external_rename_and_removal_do_not_strand_a_writer_handle() {
        let dir = tempfile::tempdir().unwrap();
        let writer = test_writer(dir.path(), 2);
        let active = dir.path().join("mxr.log");
        writer.commit(b"{\"n\":1}\n", false).unwrap();
        std::fs::rename(&active, dir.path().join("external")).unwrap();
        writer.commit(b"{\"n\":2}\n", false).unwrap();
        assert_eq!(std::fs::read_to_string(&active).unwrap(), "{\"n\":2}\n");
        std::fs::remove_file(&active).unwrap();
        writer.commit(b"{\"n\":3}\n", false).unwrap();
        assert_eq!(std::fs::read_to_string(&active).unwrap(), "{\"n\":3}\n");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("external")).unwrap(),
            "{\"n\":1}\n"
        );
    }
    #[test]
    fn invalid_caps_and_conflicting_paths_fail_without_deleting_history() {
        for (size, count) in [(0, 1), (1, 0), (u32::MAX, u32::MAX)] {
            assert!(Limits::from_config(&mxr_config::LoggingConfig {
                max_size_mb: size,
                max_files: count,
                ..Default::default()
            })
            .is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("mxr.00000000000000000000.log");
        std::fs::write(&archive, "history").unwrap();
        std::fs::create_dir(dir.path().join("mxr.log")).unwrap();
        assert!(test_writer(dir.path(), 1).commit(b"{}\n", false).is_err());
        assert_eq!(std::fs::read_to_string(archive).unwrap(), "history");
    }

    #[test]
    fn malformed_toml_keeps_startup_policy_but_read_errors_propagate() {
        let dir = tempfile::tempdir().unwrap();
        let writer = test_writer(dir.path(), 2);
        std::fs::write(&writer.config_path, "[logging\ninvalid=???").unwrap();
        let config = writer.config().unwrap();
        assert_eq!(config.max_size_mb, 1);
        assert_eq!(config.max_files, 2);
        assert!(!config.stderr);
        std::fs::remove_file(&writer.config_path).unwrap();
        std::fs::create_dir(&writer.config_path).unwrap();
        assert!(
            writer.config().is_err(),
            "a config read I/O error is not a TOML parse fallback"
        );
        assert!(writer.commit(b"{}\n", false).is_err());
    }
}
