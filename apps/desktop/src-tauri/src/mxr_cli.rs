//! Finds the `mxr` binary and asks it for the local web app URL.
//!
//! The shell owns no daemon logic: `mxr web --print-url --no-open` starts or
//! reuses the managed bridge and prints its URL, and that URL is all we keep.

use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tauri::Url;

/// How long `mxr web` gets to print a URL. A cold start spawns the bridge, so
/// this is generous; the CLI's own start timeout is 5s.
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Why the web app could not be opened. Shown on the down page by name.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LaunchError {
    Missing,
    Failed,
    TimedOut,
}

impl LaunchError {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Failed => "failed",
            Self::TimedOut => "timeout",
        }
    }
}

/// Where to look for `mxr`. A GUI app launched from Finder gets a minimal
/// PATH without Homebrew, so the usual install locations are appended after
/// whatever PATH the process did get (an explicit PATH entry wins).
pub fn candidate_dirs(path_env: Option<OsString>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path_env
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    if let Some(home) = home {
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join(".local/bin"));
    }
    dirs
}

pub fn find_mxr(dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter()
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join("mxr"))
        .find(|path| is_executable_file(path))
}

fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Accepts the first stdout line that is an http URL on a loopback host.
/// The bridge only binds loopback, so anything else is not our bridge and
/// must never become a place the window may navigate to.
pub fn parse_bridge_url(stdout: &str) -> Option<Url> {
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let url = Url::parse(line).ok()?;
    if url.scheme() != "http" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host_str()?;
    let loopback = host == "localhost"
        || host.ends_with(".localhost")
        || host == "127.0.0.1"
        || host == "[::1]";
    loopback.then_some(url)
}

pub fn launch(timeout: Duration) -> Result<Url, LaunchError> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dirs = candidate_dirs(std::env::var_os("PATH"), home.as_deref());
    let mxr = find_mxr(&dirs).ok_or(LaunchError::Missing)?;
    run_web(&mxr, timeout)
}

fn run_web(mxr: &Path, timeout: Duration) -> Result<Url, LaunchError> {
    let mut child = Command::new(mxr)
        .args(["web", "--print-url", "--no-open"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        // stderr can carry the URL too; it is dropped, never logged.
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| LaunchError::Failed)?;
    let stdout = child.stdout.take().ok_or(LaunchError::Failed)?;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut out = String::new();
        let mut reader = BufReader::new(stdout);
        // The URL is a single line; stop at the first so a lingering child
        // holding the pipe open cannot stall us.
        let _ = reader.read_line(&mut out);
        let _ = tx.send(out);
    });

    let deadline = Instant::now() + timeout;
    let remaining = |now: Instant| deadline.saturating_duration_since(now);
    let Ok(out) = rx.recv_timeout(remaining(Instant::now())) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(LaunchError::TimedOut);
    };

    // Let the command finish so a non-zero exit is reported as a failure.
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return Err(LaunchError::Failed),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(LaunchError::TimedOut);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
        }
    }
    parse_bridge_url(&out).ok_or(LaunchError::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parses_the_default_bridge_url() {
        let url = parse_bridge_url("http://mxr.localhost:42829\n").unwrap();
        assert_eq!(url.host_str(), Some("mxr.localhost"));
        assert_eq!(url.port(), Some(42829));
    }

    #[test]
    fn takes_the_first_non_empty_line() {
        let url = parse_bridge_url("\n  http://127.0.0.1:5000  \nnoise\n").unwrap();
        assert_eq!(url.port(), Some(5000));
    }

    #[test]
    fn rejects_urls_that_are_not_a_local_bridge() {
        for bad in [
            "",
            "not a url",
            "https://mxr.localhost:1",
            "http://example.com:42829",
            "http://localhost.evil.com",
            "http://user:pw@127.0.0.1:1",
            "file:///etc/passwd",
            "javascript:alert(1)",
        ] {
            assert!(parse_bridge_url(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn path_entries_come_before_fallbacks() {
        let dirs = candidate_dirs(
            Some(OsString::from("/custom/bin:/usr/bin")),
            Some(Path::new("/Users/me")),
        );
        assert_eq!(dirs[0], PathBuf::from("/custom/bin"));
        assert_eq!(dirs[1], PathBuf::from("/usr/bin"));
        for expected in [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/Users/me/.cargo/bin",
            "/Users/me/.local/bin",
        ] {
            assert!(dirs.contains(&PathBuf::from(expected)), "{expected}");
        }
    }

    #[test]
    fn works_without_a_path_or_home() {
        let dirs = candidate_dirs(None, None);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin")
            ]
        );
    }

    fn script(dir: &Path, name: &str, body: &str, mode: u32) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        path
    }

    #[test]
    fn find_mxr_skips_non_executable_and_relative_entries() {
        let tmp = tempdir("find");
        let plain = tmp.join("plain");
        let good = tmp.join("good");
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::create_dir_all(&good).unwrap();
        script(&plain, "mxr", "", 0o644);
        let exe = script(&good, "mxr", "", 0o755);
        let dirs = vec![PathBuf::from("relative"), plain, good];
        assert_eq!(find_mxr(&dirs), Some(exe));
        assert_eq!(find_mxr(&[]), None);
    }

    #[test]
    fn run_web_returns_the_printed_url() {
        let tmp = tempdir("ok");
        let mxr = script(
            &tmp,
            "mxr",
            "#!/bin/sh\necho http://mxr.localhost:4242\n",
            0o755,
        );
        let url = run_web(&mxr, Duration::from_secs(5)).unwrap();
        assert_eq!(url.port(), Some(4242));
    }

    #[test]
    fn run_web_reports_failure_and_bad_output() {
        let tmp = tempdir("fail");
        let fails = script(
            &tmp,
            "fails",
            "#!/bin/sh\necho http://localhost:1\nexit 3\n",
            0o755,
        );
        assert_eq!(
            run_web(&fails, Duration::from_secs(5)),
            Err(LaunchError::Failed)
        );
        let junk = script(&tmp, "junk", "#!/bin/sh\necho hello\n", 0o755);
        assert_eq!(
            run_web(&junk, Duration::from_secs(5)),
            Err(LaunchError::Failed)
        );
    }

    #[test]
    fn run_web_times_out_and_kills_the_child() {
        let tmp = tempdir("slow");
        let slow = script(&tmp, "slow", "#!/bin/sh\nsleep 30\n", 0o755);
        let started = Instant::now();
        assert_eq!(
            run_web(&slow, Duration::from_millis(300)),
            Err(LaunchError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mxr-desktop-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
