//! The HTML helper: untrusted mail HTML is parsed in a child process.
//!
//! The helper is this same executable started as `mxr __html-worker`. Every
//! process that parses mail HTML (the daemon, the TUI, the CLI) installs a
//! supervisor in `main`; the child starts on the first parse and is restarted
//! if it times out or dies. See `mxr_html_worker` for the mechanics and
//! blueprint chapter 7 for why.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use mxr_html_worker::{AnyJob, Config};

/// The first argument that puts the executable in helper mode. It is checked
/// before argument parsing, a runtime or logging, and is not a documented
/// command.
pub const HELPER_ARG: &str = "__html-worker";

/// Every parse any crate runs in the helper, which is what its registry holds.
pub fn jobs() -> Vec<&'static dyn AnyJob> {
    let mut jobs = mxr_reader::html_jobs();
    jobs.extend(mxr_reading::html_jobs());
    jobs.extend(mxr_safety::gaps::html_jobs());
    jobs.extend(mxr_outbound::html::html_jobs());
    jobs.extend(mxr_tui::html_jobs());
    jobs.extend(crate::handler::html_jobs());
    jobs
}

/// Runs this process as a helper until its input closes.
pub fn run_helper() -> ExitCode {
    mxr_html_worker::run_helper(&jobs())
}

/// Makes this process parse mail HTML in a helper. Nothing starts until the
/// first parse. If the executable cannot be found the process keeps parsing
/// in-process, as before.
pub fn install() {
    let program = match helper_program() {
        Ok(program) => program,
        Err(error) => {
            tracing::warn!(%error, "HTML helper not installed; mail HTML is parsed in-process");
            return;
        }
    };
    mxr_html_worker::install(Config::new(program, vec![OsString::from(HELPER_ARG)]));
}

fn helper_program() -> std::io::Result<PathBuf> {
    // A daemon that outlives an upgrade must start the binary it is running,
    // not the one now on disk: the two could speak different frame layouts.
    // On Linux `/proc/self/exe` is the running image even after its file is
    // replaced; elsewhere the path is the best there is.
    #[cfg(target_os = "linux")]
    if std::path::Path::new("/proc/self/exe").exists() {
        return Ok(PathBuf::from("/proc/self/exe"));
    }
    std::env::current_exe()
}

/// Counts for `mxr doctor` and the diagnostics page.
pub fn stats() -> mxr_protocol::HtmlHelperStats {
    let stats = mxr_html_worker::stats();
    mxr_protocol::HtmlHelperStats {
        installed: stats.installed,
        requests: stats.requests,
        restarts: stats.restarts,
        timeouts: stats.timeouts,
        deaths: stats.deaths,
        job_errors: stats.job_errors,
        fallbacks: stats.fallbacks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_registered_job_has_its_own_kind() {
        let kinds: Vec<&str> = jobs().iter().map(|job| job.kind()).collect();
        let unique: HashSet<&str> = kinds.iter().copied().collect();
        assert_eq!(kinds.len(), unique.len(), "duplicate kinds in {kinds:?}");
    }

    #[test]
    fn every_parse_the_workspace_isolates_is_registered() {
        let kinds: HashSet<&str> = jobs().iter().map(|job| job.kind()).collect();
        for expected in [
            "reader.render",
            "reader.split_quote",
            "reading.walk_html",
            "reading.article",
            "reading.issue_reader_html",
            "safety.html_views",
            "outbound.validate_html",
            "outbound.text_alternative",
            "tui.rich_lines",
            "daemon.cid_references",
            "daemon.image_sources",
        ] {
            assert!(kinds.contains(expected), "{expected} is not registered");
        }
    }
}
