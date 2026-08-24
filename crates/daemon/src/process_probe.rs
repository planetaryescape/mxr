//! What another process on this machine actually is: when it started, what it
//! was exec'd as, and which mxr profile it resolves.
//!
//! The daemon lifecycle has to decide whether a pid may be adopted — and then
//! signalled — on behalf of this profile. A pid alone is not identity: pids are
//! recycled, every mxr checkout builds a binary called `mxr`, and `mxr demo`
//! pins one instance name across every profile. Guessing wrong means killing
//! another checkout's (or another profile's) daemon, so each probe here answers
//! one narrow question and says nothing when it cannot answer.

use std::path::Path;

/// `ps` is asked for machine-comparable output, not human output: `lstart`
/// renders through `LC_TIME`, so a daemon started under one locale and a CLI
/// run under another would disagree about the very same process.
fn ps(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("ps")
        .env("LC_ALL", "C")
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// When `pid` started, as an opaque string that stays the same for the life of
/// that process and differs from whatever pid recycling hands out next.
pub(crate) fn start_time(pid: u32) -> Option<String> {
    let raw = ps(&["-o", "lstart=", "-p", &pid.to_string()])?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The command line of `pid`, split on whitespace.
///
/// Only ever used to ask structural questions ("is argv[1] `daemon`?"), never
/// to recover a value that might contain a space.
///
/// `-ww` because procps truncates to the terminal width — 80 columns when
/// stdout is a pipe, which it always is here. A deep exe path (a worktree, a
/// long `$HOME`) pushes the flags off the end, and a command line cut mid-flag
/// reads as a daemon that is not ours.
pub(crate) fn command_words(pid: u32) -> Option<Vec<String>> {
    let raw = ps(&["-ww", "-o", "command=", "-p", &pid.to_string()])?;
    let words: Vec<String> = raw.split_whitespace().map(ToString::to_string).collect();
    (!words.is_empty()).then_some(words)
}

/// Every `<pid> <command line>` pair `ps` reports.
///
/// `-ww` for the same reason as [`command_words`]: a truncated command line
/// loses the flags the scan matches on.
pub(crate) fn all_command_lines() -> Option<Vec<(u32, String)>> {
    let raw = ps(&["-eww", "-o", "pid=,command="])?;
    Some(parse_pid_command_lines(&raw))
}

fn parse_pid_command_lines(ps_output: &str) -> Vec<(u32, String)> {
    ps_output
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.trim().split_once(char::is_whitespace)?;
            Some((pid.parse().ok()?, command.trim().to_string()))
        })
        .collect()
}

/// The environment of `pid` as `KEY=VALUE` entries, or `None` when this
/// platform will not say.
#[cfg(target_os = "linux")]
pub(crate) fn environment(pid: u32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    Some(
        raw.split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
            .map(|entry| String::from_utf8_lossy(entry).into_owned())
            .collect(),
    )
}

/// macOS has no procfs, and the kernel interface carrying the environment
/// (`KERN_PROCARGS2`) is only reachable through `unsafe`, which this workspace
/// denies. `ps -E` exposes the same data as text, joined with spaces — so the
/// entries must be split on where the next `KEY=` starts rather than on
/// whitespace, or every value containing a space (`~/Library/Application
/// Support`, i.e. every macOS mxr profile) comes back truncated.
#[cfg(not(target_os = "linux"))]
pub(crate) fn environment(pid: u32) -> Option<Vec<String>> {
    let raw = ps(&["-Eww", "-o", "command=", "-p", &pid.to_string()])?;
    Some(split_environment_entries(&raw))
}

/// Split `ps -E` text into `KEY=VALUE` entries.
///
/// A new entry starts at a whitespace-delimited word that looks like an
/// environment assignment (`IDENT=`); everything up to the next such word —
/// spaces included — belongs to the previous value. The command line that
/// precedes the environment falls away under the same rule, since its words
/// are not assignments.
///
/// A value that itself contains ` IDENT=` is cut short there. That direction
/// is safe: a truncated value compares unequal, so the caller declines to
/// adopt rather than adopting something it has not really identified.
///
/// Only the non-procfs `environment` above calls this, so on Linux it is dead
/// code — except under `cfg(test)`, where the tests below still exercise it.
/// Hence the expectation is conditioned on both: `expect` fails CI when it is
/// *unfulfilled*, so claiming "dead on Linux" in the test build would trade one
/// red job for another. The function stays compiled everywhere on purpose —
/// Linux CI is what runs `-D warnings`, and this parser is the part of the file
/// that macOS would otherwise be alone in keeping honest.
#[cfg_attr(
    all(target_os = "linux", not(test)),
    expect(dead_code, reason = "used by the non-procfs environment probe")
)]
fn split_environment_entries(text: &str) -> Vec<String> {
    let mut entries: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        if starts_environment_entry(word) {
            entries.push(word.to_string());
        } else if let Some(current) = entries.last_mut() {
            current.push(' ');
            current.push_str(word);
        }
    }
    entries
}

fn starts_environment_entry(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The value of `key` in a `KEY=VALUE` list.
pub(crate) fn environment_value<'a>(environment: &'a [String], key: &str) -> Option<&'a str> {
    environment.iter().find_map(|entry| {
        entry
            .split_once('=')
            .filter(|(name, _)| *name == key)
            .map(|(_, value)| value)
    })
}

/// Whether `exe`, as `ps` reported it, names the same file as `expected`.
///
/// A process started through a `PATH` lookup can carry a bare `argv[0]` that
/// cannot be resolved from here; such a process is skipped rather than guessed
/// at. Canonicalizing also rejects a binary that has since been replaced or
/// deleted, which is the normal state of affairs in a rebuild loop.
pub(crate) fn exe_is(exe: &str, expected: &Path) -> bool {
    std::fs::canonicalize(exe).is_ok_and(|path| path == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_entries_keep_values_that_contain_spaces() {
        // The shape `ps -Eww -o command=` produces: command line first, then
        // the environment. A macOS demo profile's directories always contain
        // "Application Support".
        let text = "/opt/mxr daemon --instance mxr-demo \
                    HOME=/Users/bk \
                    MXR_DATA_DIR=/Users/bk/Library/Application Support/mxr-demo \
                    SHELL=/bin/zsh";

        let entries = split_environment_entries(text);

        assert_eq!(
            environment_value(&entries, "MXR_DATA_DIR"),
            Some("/Users/bk/Library/Application Support/mxr-demo"),
            "a value with spaces must survive; got {entries:?}"
        );
        assert_eq!(environment_value(&entries, "HOME"), Some("/Users/bk"));
        assert_eq!(environment_value(&entries, "SHELL"), Some("/bin/zsh"));
    }

    #[test]
    fn environment_entries_drop_the_command_line_that_precedes_them() {
        let entries = split_environment_entries("/opt/mxr daemon --instance mxr HOME=/Users/bk");

        assert_eq!(entries, vec!["HOME=/Users/bk".to_string()]);
    }

    #[test]
    fn a_value_containing_an_assignment_is_cut_short_rather_than_swallowing_the_rest() {
        // Documented failure direction: the caller sees a value that does not
        // match and declines to adopt, which is the safe answer.
        let entries = split_environment_entries("MXR_DATA_DIR=/tmp/a OTHER=x HOME=/Users/bk");

        assert_eq!(environment_value(&entries, "MXR_DATA_DIR"), Some("/tmp/a"));
        assert_eq!(environment_value(&entries, "HOME"), Some("/Users/bk"));
    }

    #[test]
    fn pid_command_lines_keep_the_whole_command() {
        let parsed = parse_pid_command_lines(
            "  100 /opt/mxr daemon --instance mxr-dev\n  bad line\n  200 /bin/zsh\n",
        );

        assert_eq!(
            parsed,
            vec![
                (100, "/opt/mxr daemon --instance mxr-dev".to_string()),
                (200, "/bin/zsh".to_string()),
            ]
        );
    }

    #[test]
    fn this_process_reports_a_start_time_and_a_command_line() {
        let pid = std::process::id();

        assert!(start_time(pid).is_some(), "ps must report our own lstart");
        assert!(command_words(pid).is_some_and(|words| !words.is_empty()));
    }

    #[test]
    fn this_process_reports_its_own_environment() {
        let environment =
            environment(std::process::id()).expect("our own environment must be readable");

        assert_eq!(
            environment_value(&environment, "PATH"),
            std::env::var("PATH").ok().as_deref(),
            "the probe must recover PATH exactly, separators and all"
        );
    }
}
