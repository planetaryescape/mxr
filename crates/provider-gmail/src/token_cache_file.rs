use std::io::{self, Write};
use std::path::Path;

#[derive(Clone, Copy)]
pub(super) enum ReplaceStage {
    BeforeRename,
    AfterRename,
}

pub(super) fn write(path: &Path, contents: &str) -> io::Result<()> {
    write_with_hook(path, contents, |_| Ok(()))
}

pub(super) fn write_with_hook(
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
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn bare_filename_uses_current_directory() {
        assert_eq!(parent_directory(Path::new("tokens.json")), Path::new("."));
    }

    #[test]
    fn replacement_restricts_existing_permissions_and_replaces_symlink() {
        let directory = tempdir().unwrap();
        let target = directory.path().join("target.json");
        let path = directory.path().join("tokens.json");
        std::fs::write(&target, "target").unwrap();
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write(&path, "new").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        write(&path, "cache").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "cache");
        assert_eq!(std::fs::read_to_string(target).unwrap(), "target");
    }

    #[test]
    fn nested_parent_is_created_and_stale_temporary_files_are_not_left_on_error() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("nested").join("tokens.json");
        let result = write_with_hook(&path, "new", |stage| match stage {
            ReplaceStage::BeforeRename => Err(io::Error::other("injected interruption")),
            ReplaceStage::AfterRename => Ok(()),
        });

        assert!(result.is_err());
        let parent = path.parent().unwrap();
        assert_eq!(std::fs::read_dir(parent).unwrap().count(), 0);
    }
}
