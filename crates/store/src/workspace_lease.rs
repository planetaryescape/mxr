//! Process-lifetime leases cover raw SQLx connections detached from their pool.
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static WRITERS: OnceLock<Mutex<HashMap<PathBuf, File>>> = OnceLock::new();

fn lock_path(workspace: &Path) -> io::Result<PathBuf> {
    let absolute = if workspace.is_absolute() {
        workspace.to_path_buf()
    } else {
        std::env::current_dir()?.join(workspace)
    };
    let absolute = if absolute.exists() {
        absolute.canonicalize()?
    } else {
        absolute
    };
    let name = absolute
        .file_name()
        .ok_or_else(|| io::Error::other("Workspace must have a directory name"))?;
    let parent = absolute
        .parent()
        .ok_or_else(|| io::Error::other("Workspace must have a parent"))?
        .canonicalize()?;
    Ok(parent.join(format!(".{}.mxr-workspace.lock", name.to_string_lossy())))
}

fn open(path: &Path) -> io::Result<File> {
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(io::Error::other("Workspace lease must not be a symlink"));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

pub(crate) fn retain_shared(db_path: &Path) -> io::Result<()> {
    let workspace = db_path
        .parent()
        .ok_or_else(|| io::Error::other("Database needs a workspace directory"))?;
    let path = lock_path(workspace)?;
    let mut registry = WRITERS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| io::Error::other("Workspace lease registry poisoned"))?;
    if let Some(held) = registry.get(&path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let original = held.metadata()?;
            let current = path.symlink_metadata()?;
            if current.file_type().is_symlink()
                || (original.dev(), original.ino()) != (current.dev(), current.ino())
            {
                return Err(io::Error::other(
                    "Workspace lease identity changed; exit this process before reopening",
                ));
            }
        }
        return Ok(());
    }
    let file = open(&path)?;
    file.try_lock_shared()
        .map_err(|e| io::Error::other(format!("Workspace is held by offline maintenance: {e}")))?;
    registry.insert(path, file);
    Ok(())
}

/// All participating Store processes must exit before offline maintenance.
pub fn exclusive(workspace: &Path) -> io::Result<File> {
    let file = open(&lock_path(workspace)?)?;
    file.try_lock().map_err(|e| {
        io::Error::other(format!(
            "Workspace is in use; stop all participating processes before offline maintenance: {e}"
        ))
    })?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lease_child() {
        let Ok(root) = std::env::var("MXR_LEASE_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let store = crate::Store::new(&root.join("mxr.db")).await.unwrap();
            let pool = store.writer().clone();
            let mode = std::env::var("MXR_LEASE_TEST_MODE").unwrap();
            let mut checked = Some(pool.acquire().await.unwrap());
            let mut raw = match mode.as_str() {
                "detach" => Some(checked.take().unwrap().detach()),
                "leak" => Some(checked.take().unwrap().leak()),
                _ => None,
            };
            drop(store);
            if mode == "pool" {
                drop(checked.take());
            }
            if let Some(raw) = &mut raw {
                sqlx::query("CREATE TABLE escaped_connection_proof(id INTEGER)")
                    .execute(&mut *raw)
                    .await
                    .unwrap();
            } else if let Some(checked) = &mut checked {
                sqlx::query("CREATE TABLE escaped_connection_proof(id INTEGER)")
                    .execute(&mut **checked)
                    .await
                    .unwrap();
            } else {
                sqlx::query("CREATE TABLE escaped_connection_proof(id INTEGER)")
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            eprintln!("lease child {mode}: ready");
            std::fs::write(root.join("ready"), b"ready").unwrap();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            eprintln!("lease child {mode}: stdin released");
            use sqlx::Connection;
            if let Some(raw) = raw {
                raw.close().await.unwrap();
            }
            eprintln!("lease child {mode}: raw closed");
            drop(checked);
            // leak() permanently consumes a pool permit even after the raw connection closes.
            if mode != "leak" {
                tokio::time::timeout(std::time::Duration::from_secs(3), pool.close())
                    .await
                    .unwrap();
            }
            drop(pool);
            eprintln!("lease child {mode}: runtime shutdown");
        });
        drop(runtime);
        eprintln!("lease child: exiting");
    }
    #[test]
    fn offline_lease_waits_for_detached_connection_process_exit() {
        use std::process::{Command, Stdio};
        for mode in ["pool", "checked", "detach", "leak"] {
            let root = tempfile::tempdir().unwrap();
            let alias_parent = tempfile::tempdir().unwrap();
            #[cfg(unix)]
            let alias = {
                let alias = alias_parent.path().join("alias");
                std::os::unix::fs::symlink(root.path(), &alias).unwrap();
                alias
            };
            #[cfg(not(unix))]
            let alias = root.path().to_path_buf();
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "workspace_lease::tests::lease_child",
                    "--nocapture",
                ])
                .env("MXR_LEASE_TEST_ROOT", &alias)
                .env("MXR_LEASE_TEST_MODE", mode)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while !root.path().join("ready").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "lease child failed to become ready"
                );
                assert!(child.try_wait().unwrap().is_none());
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(exclusive(root.path()).is_err());
            assert!(exclusive(&alias).is_err());
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"exit\n").unwrap();
            let exit_deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            loop {
                if let Some(exit) = child.try_wait().unwrap() {
                    assert!(exit.success());
                    break;
                }
                if std::time::Instant::now() >= exit_deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("lease child {mode} did not exit after stdin release");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(exclusive(root.path()).is_ok());
        }
    }
    #[cfg(unix)]
    #[test]
    fn replacement_lock_inode_is_rejected_instead_of_reusing_registry_entry() {
        let root = tempfile::tempdir().unwrap();
        retain_shared(&root.path().join("mxr.db")).unwrap();
        let path = lock_path(root.path()).unwrap();
        std::fs::rename(&path, path.with_extension("old")).unwrap();
        open(&path).unwrap();
        assert!(retain_shared(&root.path().join("mxr.db")).is_err());
    }
}
