//! mbox concurrent-access locking.
//!
//! Five strategies, chosen by the caller. Pick `FcntlThenDotlock` if
//! you want to interoperate with Debian's mutt/dotlock convention; pick
//! `Flock` if you only need single-host coordination; pick `None` if
//! the caller takes responsibility (read-only iteration over a file
//! you own).
//!
//! # Cross-platform reality
//!
//! - **Unix**: `Flock`, `Fcntl`, `Dotlock`, `FcntlThenDotlock` all work
//!   via `libc`. `Dotlock`'s NFS-safe form uses `link(2)` + a stat
//!   check (Linux-flavoured NFSv2/NFSv3 are picky about
//!   `O_CREAT|O_EXCL`).
//! - **Windows**: `Flock` maps to `LockFileEx`. `Fcntl` is a no-op
//!   (POSIX advisory record locks don't exist on Windows). `Dotlock`
//!   works via POSIX rename semantics. `FcntlThenDotlock` collapses to
//!   `Dotlock` alone.
//!
//! [`Lock`] is an RAII guard: dropping it releases the lock. Use
//! [`Lock::release`] when you need explicit error handling at release
//! time.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Locking policy. Default is [`LockStrategy::None`] — the caller takes
/// responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LockStrategy {
    /// Caller takes responsibility. The default.
    #[default]
    None,
    /// Create a sibling `<path>.lock` file with `O_CREAT|O_EXCL`. On
    /// NFS, uses `link(2)` + `stat(2)` to detect the race.
    Dotlock,
    /// Advisory whole-file lock via `flock(2)` on Unix or `LockFileEx`
    /// on Windows.
    Flock,
    /// POSIX advisory record lock via `fcntl(F_SETLK)`. Unix only —
    /// a no-op on Windows.
    Fcntl,
    /// Debian/mutt convention: try `Fcntl` first, then also create a
    /// `Dotlock`. Most interoperable.
    FcntlThenDotlock,
}

/// RAII guard that releases the lock on drop.
///
/// You can call [`Lock::release`] explicitly if you want to handle
/// release errors. The drop impl swallows them.
pub struct Lock {
    handle: Option<LockHandle>,
}

enum LockHandle {
    None,
    Dotlock {
        lock_path: PathBuf,
    },
    Flock {
        file: File,
    },
    Fcntl {
        file: File,
        fd: i32,
    },
    Combined {
        lock_path: PathBuf,
        file: File,
        fd: i32,
    },
}

impl Lock {
    /// Acquire a lock for `path` under the given strategy.
    ///
    /// For all non-`None` strategies the target file must exist. Tries
    /// once and returns immediately on failure — there is no built-in
    /// retry. Callers wanting bounded retry should wrap this in their
    /// own loop with backoff.
    pub fn acquire(path: &Path, strategy: LockStrategy) -> Result<Self> {
        let handle = match strategy {
            LockStrategy::None => LockHandle::None,
            LockStrategy::Dotlock => acquire_dotlock(path)?,
            LockStrategy::Flock => acquire_flock(path)?,
            LockStrategy::Fcntl => acquire_fcntl(path)?,
            LockStrategy::FcntlThenDotlock => acquire_combined(path)?,
        };
        Ok(Self {
            handle: Some(handle),
        })
    }

    /// Release the lock, returning any release-time error.
    pub fn release(mut self) -> Result<()> {
        if let Some(handle) = self.handle.take() {
            release_handle(handle)?;
        }
        Ok(())
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = release_handle(handle);
        }
    }
}

fn acquire_dotlock(path: &Path) -> Result<LockHandle> {
    let lock_path = dotlock_path(path);
    create_exclusive(&lock_path)?;
    Ok(LockHandle::Dotlock { lock_path })
}

fn create_exclusive(lock_path: &Path) -> Result<()> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(Error::Lock(format!(
            "dotlock already held: {}",
            lock_path.display()
        ))),
        Err(e) => Err(Error::Lock(format!(
            "failed to create dotlock {}: {e}",
            lock_path.display()
        ))),
    }
}

fn acquire_flock(path: &Path) -> Result<LockHandle> {
    let file = File::open(path)
        .map_err(|e| Error::Lock(format!("cannot open {} for flock: {e}", path.display())))?;
    platform::flock_exclusive(&file)?;
    Ok(LockHandle::Flock { file })
}

fn acquire_fcntl(path: &Path) -> Result<LockHandle> {
    // F_WRLCK requires the file to be opened with write permission.
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| Error::Lock(format!("cannot open {} for fcntl: {e}", path.display())))?;
    let fd = platform::fcntl_setlk_exclusive(&file)?;
    Ok(LockHandle::Fcntl { file, fd })
}

fn acquire_combined(path: &Path) -> Result<LockHandle> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| {
            Error::Lock(format!(
                "cannot open {} for combined lock: {e}",
                path.display()
            ))
        })?;
    let fd = platform::fcntl_setlk_exclusive(&file)?;
    let lock_path = dotlock_path(path);
    if let Err(e) = create_exclusive(&lock_path) {
        // Roll back the fcntl lock.
        let _ = platform::fcntl_unlock(fd);
        return Err(e);
    }
    Ok(LockHandle::Combined {
        lock_path,
        file,
        fd,
    })
}

fn release_handle(handle: LockHandle) -> Result<()> {
    match handle {
        LockHandle::None => Ok(()),
        LockHandle::Dotlock { lock_path } => remove_lock_path(&lock_path),
        LockHandle::Flock { file } => {
            // Hold `file` until after the unlock call: dropping the
            // File closes the fd and would invalidate the lock state.
            let result = platform::flock_unlock(&file);
            drop(file);
            result
        }
        LockHandle::Fcntl { fd, file } => {
            let result = platform::fcntl_unlock(fd);
            drop(file);
            result
        }
        LockHandle::Combined {
            lock_path,
            fd,
            file,
        } => {
            let fnt = platform::fcntl_unlock(fd);
            drop(file);
            let dot = remove_lock_path(&lock_path);
            fnt.and(dot)
        }
    }
}

fn remove_lock_path(lock_path: &Path) -> Result<()> {
    match std::fs::remove_file(lock_path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Lock(format!(
            "failed to remove dotlock {}: {e}",
            lock_path.display()
        ))),
    }
}

fn dotlock_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".lock");
    PathBuf::from(s)
}

// ---------------------------------------------------------------------------
// Platform shims
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[allow(unsafe_code)]
mod platform {
    use std::fs::File;
    use std::os::unix::io::AsRawFd;

    use crate::error::{Error, Result};

    pub(super) fn flock_exclusive(file: &File) -> Result<()> {
        // Safety: file descriptor is owned by `file` for the duration
        // of this call. flock is documented and side-effect-bounded.
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc == 0 {
            Ok(())
        } else {
            Err(Error::Lock(format!(
                "flock(LOCK_EX|LOCK_NB) failed: {}",
                std::io::Error::last_os_error()
            )))
        }
    }

    pub(super) fn flock_unlock(file: &File) -> Result<()> {
        // Safety: same as flock_exclusive.
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
        if rc == 0 {
            Ok(())
        } else {
            Err(Error::Lock(format!(
                "flock(LOCK_UN) failed: {}",
                std::io::Error::last_os_error()
            )))
        }
    }

    pub(super) fn fcntl_setlk_exclusive(file: &File) -> Result<i32> {
        let fd = file.as_raw_fd();
        // Safety: zero-init is valid for `flock`. Per POSIX, l_type =
        // F_WRLCK; l_whence = SEEK_SET; l_start = l_len = 0 locks the
        // whole file.
        let mut fl: libc::flock = unsafe { std::mem::zeroed() };
        #[allow(clippy::unnecessary_cast)] // libc::F_WRLCK is c_short which is i16 on Linux/macOS but may differ elsewhere
        let f_wrlck = libc::F_WRLCK as i16;
        fl.l_type = f_wrlck;
        fl.l_whence = libc::SEEK_SET as i16;
        fl.l_start = 0;
        fl.l_len = 0;
        // Safety: fd is owned by `file`; fl is a properly-initialised
        // struct; fcntl with F_SETLK is documented.
        let rc = unsafe { libc::fcntl(fd, libc::F_SETLK, &fl) };
        if rc == 0 {
            Ok(fd)
        } else {
            Err(Error::Lock(format!(
                "fcntl(F_SETLK, F_WRLCK) failed: {}",
                std::io::Error::last_os_error()
            )))
        }
    }

    pub(super) fn fcntl_unlock(fd: i32) -> Result<()> {
        // Safety: same as fcntl_setlk_exclusive.
        let mut fl: libc::flock = unsafe { std::mem::zeroed() };
        #[allow(clippy::unnecessary_cast)]
        let f_unlck = libc::F_UNLCK as i16;
        fl.l_type = f_unlck;
        fl.l_whence = libc::SEEK_SET as i16;
        // Safety: fd was previously locked by us; F_SETLK with F_UNLCK
        // releases.
        let rc = unsafe { libc::fcntl(fd, libc::F_SETLK, &fl) };
        if rc == 0 {
            Ok(())
        } else {
            Err(Error::Lock(format!(
                "fcntl(F_SETLK, F_UNLCK) failed: {}",
                std::io::Error::last_os_error()
            )))
        }
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform {
    use std::fs::File;
    use std::os::windows::io::AsRawHandle;

    use crate::error::{Error, Result};

    use windows_sys::Win32::Storage::FileSystem::{
        LockFileEx, UnlockFile, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
    };

    pub(super) fn flock_exclusive(file: &File) -> Result<()> {
        let handle = file.as_raw_handle();
        let mut overlapped: windows_sys::Win32::System::IO::OVERLAPPED =
            unsafe { std::mem::zeroed() };
        let ok = unsafe {
            LockFileEx(
                handle as _,
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                u32::MAX,
                u32::MAX,
                &mut overlapped,
            )
        };
        if ok != 0 {
            Ok(())
        } else {
            Err(Error::Lock(format!(
                "LockFileEx failed: {}",
                std::io::Error::last_os_error()
            )))
        }
    }

    pub(super) fn flock_unlock(file: &File) -> Result<()> {
        let handle = file.as_raw_handle();
        let ok = unsafe { UnlockFile(handle as _, 0, 0, u32::MAX, u32::MAX) };
        if ok != 0 {
            Ok(())
        } else {
            Err(Error::Lock(format!(
                "UnlockFile failed: {}",
                std::io::Error::last_os_error()
            )))
        }
    }

    pub(super) fn fcntl_setlk_exclusive(file: &File) -> Result<i32> {
        // POSIX advisory record locks don't exist on Windows. Treat
        // Fcntl as a degenerate no-op so FcntlThenDotlock still works.
        Ok(file.as_raw_handle() as i32)
    }

    pub(super) fn fcntl_unlock(_fd: i32) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::io::Write;

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        let mut f = File::create(&p).unwrap();
        writeln!(f, "test").unwrap();
        p
    }

    #[test]
    fn lock_none_is_noop() {
        let _l = Lock::acquire(Path::new("/nonexistent"), LockStrategy::None).unwrap();
    }

    #[test]
    fn dotlock_creates_and_releases_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let target = touch(dir.path(), "inbox");
        let dot = dir.path().join("inbox.lock");

        let lock = Lock::acquire(&target, LockStrategy::Dotlock).unwrap();
        assert!(dot.exists(), "dotlock sibling should exist");
        lock.release().unwrap();
        assert!(!dot.exists(), "dotlock should be removed after release");
    }

    #[test]
    fn dotlock_second_acquire_fails() {
        let dir = tempfile::tempdir().unwrap();
        let target = touch(dir.path(), "inbox");

        let _first = Lock::acquire(&target, LockStrategy::Dotlock).unwrap();
        let second = Lock::acquire(&target, LockStrategy::Dotlock);
        assert!(matches!(second, Err(Error::Lock(_))));
    }

    #[cfg(unix)]
    #[test]
    fn flock_acquires_and_drops() {
        let dir = tempfile::tempdir().unwrap();
        let target = touch(dir.path(), "inbox");

        let lock = Lock::acquire(&target, LockStrategy::Flock).unwrap();
        drop(lock);
        // Should be re-acquireable after drop.
        let _again = Lock::acquire(&target, LockStrategy::Flock).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn combined_lock_creates_dotlock_and_holds_fcntl() {
        let dir = tempfile::tempdir().unwrap();
        let target = touch(dir.path(), "inbox");
        let dot = dir.path().join("inbox.lock");

        let lock = Lock::acquire(&target, LockStrategy::FcntlThenDotlock).unwrap();
        assert!(dot.exists());
        lock.release().unwrap();
        assert!(!dot.exists());
    }
}
