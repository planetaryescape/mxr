//! Finds `mxr` and opens its selected local or paired remote web app.
//!
//! The shell owns no daemon logic: `mxr web --session-json --no-open` starts or
//! reuses the managed bridge or issues a one-time remote browser code.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
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

/// The code is held only until the selected page can exchange it for its cookie.
#[derive(PartialEq, Eq)]
pub struct LaunchTarget {
    pub url: Url,
    pub browser_code: Option<String>,
}

impl std::fmt::Debug for LaunchTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchTarget")
            .field("url", &self.url)
            .field("has_browser_code", &self.browser_code.is_some())
            .finish()
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchResponse {
    url: String,
    #[serde(default)]
    browser_code: Option<String>,
}

pub fn parse_launch_target(stdout: &str) -> Option<LaunchTarget> {
    let line = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let response: LaunchResponse = serde_json::from_str(line).ok()?;
    let url = Url::parse(&response.url).ok()?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let host = url.host_str()?;
    let loopback = host == "localhost"
        || host.ends_with(".localhost")
        || host == "127.0.0.1"
        || host == "[::1]";
    let valid = match (url.scheme(), response.browser_code.as_deref()) {
        ("http", None) => loopback,
        ("https", Some(code)) => valid_browser_code(code),
        _ => false,
    };
    valid.then_some(LaunchTarget {
        url,
        browser_code: response.browser_code,
    })
}

pub(super) fn valid_browser_code(code: &str) -> bool {
    code.len() == 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn launch(timeout: Duration) -> Result<LaunchTarget, LaunchError> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dirs = candidate_dirs(std::env::var_os("PATH"), home.as_deref());
    let mxr = find_mxr(&dirs).ok_or(LaunchError::Missing)?;
    run_web(&mxr, timeout)
}

fn run_web(mxr: &Path, timeout: Duration) -> Result<LaunchTarget, LaunchError> {
    let mut child = Command::new(mxr)
        .args(["web", "--session-json", "--no-open"])
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
        let mut reader = BufReader::new(stdout.take(4097));
        // The URL is a single line; stop at the first so a lingering child
        // holding the pipe open cannot stall us.
        let _ = reader.read_line(&mut out);
        let _ = tx.send((out.len() <= 4096).then_some(out));
    });

    let deadline = Instant::now() + timeout;
    let remaining = |now: Instant| deadline.saturating_duration_since(now);
    let Ok(out) = rx.recv_timeout(remaining(Instant::now())) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(LaunchError::TimedOut);
    };

    let Some(out) = out else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(LaunchError::Failed);
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
    parse_launch_target(&out).ok_or(LaunchError::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parses_the_default_bridge_url() {
        let target = parse_launch_target(r#"{"url":"http://mxr.localhost:42829"}"#).unwrap();
        let url = target.url;
        assert_eq!(url.host_str(), Some("mxr.localhost"));
        assert_eq!(url.port(), Some(42829));
    }

    #[test]
    fn takes_the_first_non_empty_line() {
        let url = parse_launch_target("\n  {\"url\":\"http://127.0.0.1:5000\"}  \nnoise\n")
            .unwrap()
            .url;
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
            let output = serde_json::json!({"url": bad}).to_string();
            assert!(parse_launch_target(&output).is_none(), "{bad}");
        }
    }

    #[test]
    fn accepts_an_https_host_only_with_a_browser_code_and_redacts_debug() {
        let code = "b".repeat(64);
        let output =
            serde_json::json!({"url":"https://mail.example.test", "browser_code":code}).to_string();
        let target = parse_launch_target(&output).unwrap();
        assert_eq!(target.url.as_str(), "https://mail.example.test/");
        assert_eq!(target.browser_code.as_deref(), Some(code.as_str()));
        assert!(!format!("{target:?}").contains(&code));
    }

    #[test]
    fn refuses_launch_credentials_in_urls_and_insecure_remote_codes() {
        let code = "c".repeat(64);
        for url in [
            "http://mail.example.test/",
            "http://mxr.localhost:42829/",
            "https://user:pass@mail.example.test/",
            "https://mail.example.test/#code=secret",
            "https://mail.example.test/?code=secret",
            "https://mail.example.test/elsewhere",
            "file:///tmp/mxr",
        ] {
            let output = serde_json::json!({"url":url, "browser_code":code}).to_string();
            assert!(parse_launch_target(&output).is_none(), "{url}");
        }
        for code in ["c".repeat(63), "C".repeat(64), "g".repeat(64)] {
            let output =
                serde_json::json!({"url":"https://mail.example.test/", "browser_code":code})
                    .to_string();
            assert!(parse_launch_target(&output).is_none());
        }
        assert!(parse_launch_target(r#"{"url":"https://mail.example.test/"}"#).is_none());
        assert!(
            parse_launch_target(r#"{"url":"http://mxr.localhost:42829/","unexpected":true}"#)
                .is_none()
        );
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
            "#!/bin/sh\nprintf '%s\\n' '{\"url\":\"http://mxr.localhost:4242\"}'\n",
            0o755,
        );
        let url = run_web(&mxr, Duration::from_secs(5)).unwrap();
        assert_eq!(url.url.port(), Some(4242));
    }

    #[test]
    fn run_web_requests_the_session_contract_and_keeps_the_code_out_of_the_url() {
        let tmp = tempdir("remote");
        let code = "e".repeat(64);
        let response =
            serde_json::json!({"url":"https://mail.example.test/","browser_code":code}).to_string();
        let mxr = script(&tmp, "mxr", &format!("#!/bin/sh\n[ \"$*\" = \"web --session-json --no-open\" ] || exit 2\nprintf '%s\\n' '{response}'\n"), 0o755);
        let target = run_web(&mxr, Duration::from_secs(5)).unwrap();
        assert_eq!(target.url.as_str(), "https://mail.example.test/");
        assert_eq!(target.browser_code.as_deref(), Some(code.as_str()));
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
