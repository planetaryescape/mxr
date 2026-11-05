//! A named piece of HTML parsing that can run here or in a helper process.

use serde::{de::DeserializeOwned, Serialize};

use crate::supervisor::{self, Outcome, Supervisor};

/// One parse of untrusted HTML: a function from the document and some
/// parameters to a result, identified by `kind` on the wire.
///
/// The same function runs in both places. With no helper installed (library
/// use, unit tests) [`Job::call`] runs it on the calling thread. With one
/// installed it runs in the helper, under that process's memory and CPU limits,
/// and the caller gets `None` if the helper timed out, died or was unavailable.
///
/// A `None` means "use your fallback", the one every caller already has for a
/// document over the HTML budget. Jobs therefore never return the fallback
/// themselves for resource failures; they only decide what an over-budget
/// document means.
pub struct Job<P, R> {
    kind: &'static str,
    run: fn(&str, &P) -> R,
    max_html_bytes: usize,
}

/// Mail HTML above this is never parsed anywhere in mxr (the reader's HTML
/// budget), so a job does not ship more to a helper unless it says otherwise.
pub const DEFAULT_MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

impl<P, R> Job<P, R> {
    /// `kind` must be unique across every job linked into the binary; a
    /// registry that sees a duplicate refuses to start.
    pub const fn new(kind: &'static str, run: fn(&str, &P) -> R) -> Self {
        Self {
            kind,
            run,
            max_html_bytes: DEFAULT_MAX_HTML_BYTES,
        }
    }

    /// Documents above this many bytes get the fallback without reaching a
    /// helper. For jobs whose input has its own, larger, cap (a fetched web
    /// page).
    pub const fn max_html_bytes(mut self, max_html_bytes: usize) -> Self {
        self.max_html_bytes = max_html_bytes;
        self
    }

    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// Runs the job here, with no helper and no limits.
    pub fn run_here(&self, html: &str, params: &P) -> R {
        (self.run)(html, params)
    }
}

impl<P: Serialize, R: DeserializeOwned> Job<P, R> {
    /// Runs the job in the installed helper, or here when none is installed.
    pub fn call(&self, html: &str, params: &P) -> Option<R> {
        match supervisor::current() {
            Some(supervisor) => self.call_on(&supervisor, html, params),
            None => Some(self.run_here(html, params)),
        }
    }

    /// Runs the job in `supervisor`'s helper.
    pub fn call_on(&self, supervisor: &Supervisor, html: &str, params: &P) -> Option<R> {
        let params = serde_json::to_vec(params).ok()?;
        match supervisor.run(self.kind, html.as_bytes(), &params, self.max_html_bytes) {
            Outcome::Done(bytes) => match serde_json::from_slice(&bytes) {
                Ok(result) => Some(result),
                Err(_) => {
                    supervisor.note_undecodable_result();
                    None
                }
            },
            Outcome::Fallback(_) => None,
        }
    }
}

/// A job with its types erased, which is what a helper's registry holds.
pub trait AnyJob: Sync {
    fn kind(&self) -> &'static str;

    /// Decodes `params`, runs the job on `html` and encodes the result. The
    /// error is a short message that names no content.
    fn run_erased(&self, html: &str, params: &[u8]) -> Result<Vec<u8>, String>;
}

impl<P: DeserializeOwned, R: Serialize> AnyJob for Job<P, R> {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn run_erased(&self, html: &str, params: &[u8]) -> Result<Vec<u8>, String> {
        let params: P = serde_json::from_slice(params)
            .map_err(|error| format!("job parameters did not decode: {error}"))?;
        let result = (self.run)(html, &params);
        serde_json::to_vec(&result).map_err(|error| format!("job result did not encode: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SHOUT: Job<usize, String> = Job::new("test.shout", |html, repeat| html.repeat(*repeat));

    #[test]
    fn with_no_helper_installed_the_job_runs_on_the_calling_thread() {
        assert_eq!(SHOUT.call("ab", &2).as_deref(), Some("abab"));
    }

    #[test]
    fn an_erased_job_decodes_params_runs_and_encodes_the_result() {
        let bytes = SHOUT.run_erased("ab", b"3").expect("runs");
        assert_eq!(bytes, b"\"ababab\"");
    }

    #[test]
    fn an_erased_job_reports_bad_params_without_running() {
        let error = SHOUT
            .run_erased("ab", b"\"not a number\"")
            .expect_err("bad params");
        assert!(error.contains("did not decode"), "{error}");
    }
}
