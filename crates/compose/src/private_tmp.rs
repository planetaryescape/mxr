/// Per-user private scratch directory for mail-content temp files.
///
/// On Unix: uses `$XDG_RUNTIME_DIR/mxr` when set (already 0700 by spec),
/// otherwise creates `$TMPDIR/mxr-<uid>` with mode 0700. Never writes
/// mail-content files into the shared temp root directly.
///
/// On non-Unix: falls back to `temp_dir()/mxr` (plain create, no mode enforcement).
use std::path::{Path, PathBuf};

pub fn private_scratch_dir() -> std::io::Result<PathBuf> {
    #[cfg(unix)]
    {
        private_scratch_dir_unix()
    }
    #[cfg(not(unix))]
    {
        let dir = std::env::temp_dir().join("mxr");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

#[cfg(unix)]
fn private_scratch_dir_unix() -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;

    // Prefer $XDG_RUNTIME_DIR/mxr — already 0700 by the spec.
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        let dir = PathBuf::from(runtime).join("mxr");
        std::fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&dir)?;
        return Ok(dir);
    }

    // Fall back to $TMPDIR/mxr-<uid> created 0700.
    // We use the uid of a newly created file in the temp dir as a
    // portable way to get the effective uid without libc.
    let uid = effective_uid()?;
    let dir = std::env::temp_dir().join(format!("mxr-{uid}"));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(&dir)?;

    // Verify the directory has the expected permissions and owner —
    // guards against a squatter: another user pre-creating the path
    // (with 0700 owned by THEM they could rename/delete entries and
    // pre-place paths under a directory our mail-content files use).
    let meta = std::fs::metadata(&dir)?;
    if !meta.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{} exists but is not a directory", dir.display()),
        ));
    }
    use std::os::unix::fs::MetadataExt;
    if meta.uid() != uid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "private scratch dir {} is owned by uid {}, expected {uid}",
                dir.display(),
                meta.uid()
            ),
        ));
    }
    use std::os::unix::fs::PermissionsExt;
    let mode = meta.permissions().mode() & 0o777;
    if mode != 0o700 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "private scratch dir {} has mode {mode:04o}, expected 0700",
                dir.display()
            ),
        ));
    }

    Ok(dir)
}

/// Determine effective UID by stat-ing a freshly created file.
/// This avoids a libc dependency while remaining correct.
#[cfg(unix)]
fn effective_uid() -> std::io::Result<u32> {
    use std::os::unix::fs::MetadataExt;
    use uuid::Uuid;
    // Use a UUID-based name so concurrent calls (e.g. tests) never collide.
    let tmp = std::env::temp_dir().join(format!(".mxr-uid-probe-{}", Uuid::now_v7()));
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
    }
    let uid = std::fs::metadata(&tmp)?.uid();
    let _ = std::fs::remove_file(&tmp);
    Ok(uid)
}

/// Create `path` with 0600 permissions (O_EXCL) and write `content`.
/// On non-Unix, falls back to a plain write.
pub fn write_private(path: &Path, content: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(content)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
    }
}

/// Async variant of [`write_private`].
pub async fn write_private_async(path: &Path, content: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .await?;
        file.write_all(content).await?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        tokio::fs::write(path, content).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_scratch_dir_is_created() {
        let dir = private_scratch_dir().expect("private_scratch_dir should succeed");
        assert!(dir.is_dir(), "scratch dir should be a directory");
    }

    #[cfg(unix)]
    #[test]
    fn private_scratch_dir_has_0700_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_scratch_dir().expect("private_scratch_dir should succeed");
        let meta = std::fs::metadata(&dir).expect("metadata");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "dir mode should be 0700, got {mode:04o}");
    }

    /// The fallback dir ($TMPDIR/mxr-<uid>) must be owned by the current
    /// user — a squatter-owned dir is rejected. A true negative test would
    /// need root to chown, so assert the positive: a dir we own passes the
    /// ownership check and is in fact owned by our effective uid.
    #[cfg(unix)]
    #[test]
    fn private_scratch_dir_verifies_ownership() {
        use std::os::unix::fs::MetadataExt;
        // Force the $TMPDIR/mxr-<uid> fallback branch (the one with the
        // ownership assertion) even on hosts where XDG_RUNTIME_DIR is set.
        let dir = temp_env::with_var("XDG_RUNTIME_DIR", None::<&str>, || {
            private_scratch_dir().expect("scratch dir owned by us should pass")
        });
        let our_uid = super::effective_uid().expect("effective uid");
        let meta = std::fs::metadata(&dir).expect("metadata");
        assert_eq!(
            meta.uid(),
            our_uid,
            "scratch dir should be owned by the current user"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_private_creates_0600_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_scratch_dir().expect("scratch dir");
        let path = dir.join(format!("test-write-private-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path); // clean up any leftover
        write_private(&path, b"hello").expect("write_private");
        let meta = std::fs::metadata(&path).expect("metadata");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "file mode should be 0600, got {mode:04o}");
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn write_private_refuses_overwrite() {
        let dir = private_scratch_dir().expect("scratch dir");
        let path = dir.join(format!("test-excl-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        write_private(&path, b"first").expect("first write");
        let err = write_private(&path, b"second");
        assert!(err.is_err(), "second write should fail (O_EXCL)");
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn write_private_async_creates_0600_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = private_scratch_dir().expect("scratch dir");
        let path = dir.join(format!("test-write-private-async-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        write_private_async(&path, b"async hello").await.expect("write_private_async");
        let meta = std::fs::metadata(&path).expect("metadata");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "file mode should be 0600, got {mode:04o}");
        let _ = std::fs::remove_file(&path);
    }
}
