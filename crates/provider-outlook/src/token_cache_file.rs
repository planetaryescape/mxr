use std::io::{self, Write};
use std::path::Path;

#[derive(Clone, Copy)]
pub(crate) enum ReplaceStage {
    Created,
    BeforeRename,
    AfterRename,
}

pub(crate) fn write(path: &Path, contents: &str) -> io::Result<()> {
    write_with_hook(path, contents, |_| Ok(()))
}

/// Replaces `path` with `contents` atomically. The temporary file is created
/// private (0600) at open, so the token is never readable more broadly, and it
/// gets a unique name so concurrent writers cannot clobber each other's
/// temporary file. Any error drops the temporary file, which removes it.
pub(crate) fn write_with_hook(
    path: &Path,
    contents: &str,
    mut hook: impl FnMut(ReplaceStage) -> io::Result<()>,
) -> io::Result<()> {
    let parent = parent_directory(path);
    std::fs::create_dir_all(parent)?;

    let mut builder = tempfile::Builder::new();
    builder.prefix(".mxr-token-cache-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o600));
    }
    let mut temporary = builder.tempfile_in(parent)?;
    hook(ReplaceStage::Created)?;
    temporary.write_all(contents.as_bytes())?;
    temporary.as_file().sync_all()?;

    hook(ReplaceStage::BeforeRename)?;
    temporary.persist(path).map_err(|error| error.error)?;
    hook(ReplaceStage::AfterRename)?;

    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;

    Ok(())
}

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn mode(path: &Path) -> io::Result<u32> {
        Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
    }

    fn temporary_files(directory: &Path) -> Result<Vec<std::ffi::OsString>, io::Error> {
        Ok(std::fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|name| name.to_string_lossy().starts_with(".mxr-token-cache-"))
            .collect())
    }

    #[test]
    fn bare_filename_uses_current_directory() {
        assert_eq!(parent_directory(Path::new("tokens.json")), Path::new("."));
    }

    #[test]
    fn temporary_file_is_private_from_creation_before_any_content_is_written() -> TestResult {
        let directory = tempdir()?;
        let path = directory.path().join("tokens.json");

        write_with_hook(&path, "secret", |stage| {
            if let ReplaceStage::Created = stage {
                let temporary = temporary_files(directory.path())?;
                assert_eq!(temporary.len(), 1);
                let temporary_path = directory.path().join(&temporary[0]);
                assert_eq!(mode(&temporary_path)?, 0o600);
                assert_eq!(std::fs::read(&temporary_path)?, Vec::<u8>::new());
            }
            Ok(())
        })?;

        assert_eq!(mode(&path)?, 0o600);
        Ok(())
    }

    #[test]
    fn replacement_restricts_existing_permissions_and_replaces_symlink() -> TestResult {
        let directory = tempdir()?;
        let target = directory.path().join("target.json");
        let path = directory.path().join("tokens.json");
        std::fs::write(&target, "target")?;
        std::fs::write(&path, "old")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))?;
        write(&path, "new")?;
        assert_eq!(mode(&path)?, 0o600);

        std::fs::remove_file(&path)?;
        std::os::unix::fs::symlink(&target, &path)?;
        write(&path, "cache")?;
        assert_eq!(std::fs::read_to_string(&path)?, "cache");
        assert_eq!(std::fs::read_to_string(&target)?, "target");
        Ok(())
    }

    #[test]
    fn interrupted_write_leaves_no_temporary_file_and_keeps_old_contents() -> TestResult {
        let directory = tempdir()?;
        let path = directory.path().join("tokens.json");
        std::fs::write(&path, "old")?;

        let result = write_with_hook(&path, "new", |stage| match stage {
            ReplaceStage::BeforeRename => Err(io::Error::other("injected interruption")),
            _ => Ok(()),
        });

        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path)?, "old");
        assert!(temporary_files(directory.path())?.is_empty());
        Ok(())
    }

    #[test]
    fn nested_parent_is_created_and_no_temporary_file_is_left_on_error() -> TestResult {
        let directory = tempdir()?;
        let path = directory.path().join("nested").join("tokens.json");
        let result = write_with_hook(&path, "new", |stage| match stage {
            ReplaceStage::Created => Err(io::Error::other("injected failure")),
            _ => Ok(()),
        });

        assert!(result.is_err());
        assert!(temporary_files(&directory.path().join("nested"))?.is_empty());
        Ok(())
    }

    #[test]
    fn rename_failure_preserves_destination_and_cleans_up_temporary_file() -> TestResult {
        let directory = tempdir()?;
        let path = directory.path().join("tokens.json");
        std::fs::create_dir(&path)?;

        assert!(write(&path, "new").is_err());
        assert!(path.is_dir());
        assert!(temporary_files(directory.path())?.is_empty());
        Ok(())
    }

    #[test]
    fn concurrent_writers_each_use_their_own_temporary_file() -> TestResult {
        let directory = tempdir()?;
        let path = directory.path().join("tokens.json");

        // Writer one is paused with its temporary file on disk while writer two
        // runs to completion. A shared temporary name would make writer two
        // either fail or clobber writer one's file.
        write_with_hook(&path, "first", |stage| {
            if let ReplaceStage::BeforeRename = stage {
                assert_eq!(temporary_files(directory.path())?.len(), 1);
                write(&path, "second")?;
            }
            Ok(())
        })?;

        assert!(temporary_files(directory.path())?.is_empty());
        assert_eq!(std::fs::read_to_string(&path)?, "first");
        Ok(())
    }

    #[test]
    fn concurrent_threads_never_leave_temporary_files_behind() -> TestResult {
        let directory = tempdir()?;
        let path = directory.path().join("tokens.json");
        let handles = (0..8)
            .map(|index| {
                let path = path.clone();
                std::thread::spawn(move || write(&path, &format!("writer-{index}")))
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().map_err(|_| "writer thread panicked")??;
        }

        assert!(temporary_files(directory.path())?.is_empty());
        assert!(std::fs::read_to_string(&path)?.starts_with("writer-"));
        assert_eq!(mode(&path)?, 0o600);
        Ok(())
    }
}
