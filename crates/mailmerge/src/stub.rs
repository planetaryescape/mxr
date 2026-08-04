//! A stand-in for the `mxr` binary, for tests.
//!
//! The whole tool is defined by what it asks `mxr` to do, so the tests drive it
//! against a real subprocess that records its argv and the body files it was
//! handed. That is what makes "never sends", "never drafts twice", and "one
//! record's token never reaches another record's body" testable claims rather
//! than hopes.

#![expect(clippy::unwrap_used, reason = "test scaffolding fails loudly")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// How the stub answers one subcommand.
pub enum Reply {
    /// Succeed, returning `{"draft_id":"draft-N"}` for the Nth compose.
    Draft,
    /// Succeed, printing exactly this on stdout.
    Literal(String),
    /// Fail, printing this on stderr.
    Fail(String),
}

pub struct Stub {
    dir: TempDir,
}

impl Stub {
    /// A stub that drafts and sends successfully.
    pub fn new() -> Self {
        Self::with(true, Reply::Draft, Reply::Literal(String::new()))
    }

    /// A stub whose every subcommand fails, `--version` included.
    pub fn failing(stderr: &str) -> Self {
        Self::with(
            false,
            Reply::Fail(stderr.to_string()),
            Reply::Fail(stderr.to_string()),
        )
    }

    /// A stub whose compose succeeds but prints this instead of a draft id.
    pub fn replying(stdout: &str) -> Self {
        Self::with(
            true,
            Reply::Literal(stdout.to_string()),
            Reply::Literal(String::new()),
        )
    }

    /// A stub that is installed and healthy but refuses to compose.
    pub fn compose_fails(stderr: &str) -> Self {
        Self::with(
            true,
            Reply::Fail(stderr.to_string()),
            Reply::Literal(String::new()),
        )
    }

    /// A stub that drafts happily and then refuses to send.
    pub fn send_fails(stderr: &str) -> Self {
        Self::with(true, Reply::Draft, Reply::Fail(stderr.to_string()))
    }

    pub fn with(version_ok: bool, compose: Reply, send: Reply) -> Self {
        let dir = tempfile::tempdir().expect("stub temp dir");
        std::fs::create_dir_all(dir.path().join("bodies")).expect("stub bodies dir");
        std::fs::create_dir_all(dir.path().join("workdirs")).expect("stub workdirs dir");

        let script = format!(
            r#"#!/bin/sh
LOG='{log}'
if [ "$1" = "--version" ]; then
{version}
fi
for arg in "$@"; do printf '%s\n<<ARG>>\n' "$arg" >> "$LOG"; done
printf '<<CALL>>\n' >> "$LOG"
case "$1" in
  compose)
    n=$(cat '{seq}' 2>/dev/null || echo 0)
    n=$((n+1))
    printf '%s\n' "$n" > '{seq}'
    prev=""
    for arg in "$@"; do
      case "$prev" in
        --html-file)
          cp "$arg" '{bodies}'/"body-$n.html" 2>/dev/null || true
          ls "$(dirname "$arg")" > '{workdirs}'/"$n" 2>/dev/null || true
          ;;
        --text-file) cp "$arg" '{bodies}'/"body-$n.txt" 2>/dev/null || true ;;
      esac
      prev="$arg"
    done
{compose}
    ;;
  send)
{send}
    ;;
  send-time)
    printf '%s\n' '{{"proposed_at":"2026-08-05T08:00:00Z"}}'; exit 0
    ;;
esac
echo "stub got an unexpected command: $1" >&2
exit 1
"#,
            log = dir.path().join("calls.log").display(),
            seq = dir.path().join("seq").display(),
            bodies = dir.path().join("bodies").display(),
            workdirs = dir.path().join("workdirs").display(),
            version = if version_ok {
                "  echo 'mxr 0.0.0-stub'; exit 0".to_string()
            } else {
                "  echo 'stub is broken' >&2; exit 1".to_string()
            },
            compose = reply_script(&compose, true),
            send = reply_script(&send, false),
        );

        let binary = dir.path().join("mxr-stub");
        std::fs::write(&binary, script).expect("writing stub");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("stub permissions");
        Self { dir }
    }

    pub fn binary(&self) -> String {
        self.dir.path().join("mxr-stub").display().to_string()
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// Every invocation's argv, in order, `--version` excluded.
    pub fn calls(&self) -> Vec<Vec<String>> {
        let Ok(log) = std::fs::read_to_string(self.dir.path().join("calls.log")) else {
            return Vec::new();
        };
        log.split("<<CALL>>\n")
            .filter(|call| !call.is_empty())
            .map(|call| {
                call.split("\n<<ARG>>\n")
                    .filter(|arg| !arg.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .collect()
    }

    pub fn calls_to(&self, subcommand: &str) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|argv| argv.first().is_some_and(|first| first == subcommand))
            .collect()
    }

    pub fn value_after<'a>(&self, argv: &'a [String], flag: &str) -> Option<&'a str> {
        let position = argv.iter().position(|arg| arg == flag)?;
        argv.get(position + 1).map(String::as_str)
    }

    /// What was sitting in the directory of rendered bodies at the moment of
    /// each compose, in call order. Lets a test see how long a personalised
    /// body stays on disk, which nothing observable after the run can show.
    pub fn workdir_at_each_compose(&self) -> Vec<Vec<String>> {
        let Ok(entries) = std::fs::read_dir(self.dir.path().join("workdirs")) else {
            return Vec::new();
        };
        let mut listings: Vec<(usize, Vec<String>)> = entries
            .map(|entry| {
                let path = entry.unwrap().path();
                let call: usize = path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .parse()
                    .expect("stub names each listing after its call number");
                let files = std::fs::read_to_string(&path)
                    .unwrap_or_default()
                    .lines()
                    .map(str::to_string)
                    .collect();
                (call, files)
            })
            .collect();
        listings.sort_by_key(|(call, _)| *call);
        listings.into_iter().map(|(_, files)| files).collect()
    }

    /// The rendered bodies mxr was handed, as the stub saw them.
    pub fn captured_bodies(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.dir.path().join("bodies")) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries.map(|entry| entry.unwrap().path()).collect();
        paths.sort();
        paths
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap_or_default())
            .collect()
    }
}

fn reply_script(reply: &Reply, draft_allowed: bool) -> String {
    match reply {
        Reply::Draft if draft_allowed => {
            r#"    printf '{"draft_id":"draft-%s"}\n' "$n"; exit 0"#.to_string()
        }
        Reply::Draft => "    exit 0".to_string(),
        Reply::Literal(text) => format!("    printf '%s' {}; exit 0", shell_quote(text)),
        Reply::Fail(stderr) => format!("    printf '%s\\n' {} >&2; exit 1", shell_quote(stderr)),
    }
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

impl Default for Stub {
    fn default() -> Self {
        Self::new()
    }
}
