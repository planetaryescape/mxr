//! How big a job record can get, stored and on the wire.
//!
//! A job row holds text that comes from outside the daemon: a provider's error
//! (Gmail keeps the whole HTTP body), an account name. Unbounded, 50 failures
//! a job times 100 retained jobs passed the 16 MiB frame limit. So every
//! free-text field is cut to [`MAX_JOB_TEXT_BYTES`] (an account name to 128)
//! when the row is written
//! ([`encode_job_record`] is the only way a job row is serialised), and again
//! when it is read, for rows an older daemon wrote. Lists are bounded on the
//! way out: `GetJob` carries at most 50 failures; `ListJobs` carries no
//! failures and no undo ids, only their counts.

use mxr_protocol::JobData;

/// The most bytes a free-text field of a job keeps.
pub(super) const MAX_JOB_TEXT_BYTES: usize = 512;

/// The most bytes an account's name keeps in a job's per-account rows: a
/// display name, repeated for every account a job touches.
pub(super) const MAX_ACCOUNT_NAME_BYTES: usize = 128;

/// Failure records a job keeps (the exact total is `progress.failed`).
pub(super) const MAX_JOB_FAILURES: usize = 50;

const ELLIPSIS: &str = "…";

/// `text` cut to at most `max_bytes` bytes on a character boundary, ending in
/// an ellipsis when cut. Control characters become spaces: they are noise in
/// an error line, and JSON writes each as six bytes, which would let a short
/// string outgrow its bound on the wire.
pub(super) fn bounded_text(text: &str, max_bytes: usize) -> String {
    let clean = |c: char| if c.is_control() { ' ' } else { c };
    if text.len() <= max_bytes {
        return text.chars().map(clean).collect();
    }
    let mut end = max_bytes.saturating_sub(ELLIPSIS.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut out: String = text[..end].chars().map(clean).collect();
    out.push_str(ELLIPSIS);
    out
}

fn bound_field(field: &mut String, max_bytes: usize) {
    if field.len() > max_bytes || field.chars().any(char::is_control) {
        *field = bounded_text(field, max_bytes);
    }
}

/// Cut every free-text field and the failure list to their bounds.
fn bound_record(job: &mut JobData) {
    if let Some(error) = job.error.as_mut() {
        bound_field(error, MAX_JOB_TEXT_BYTES);
    }
    job.failures.truncate(MAX_JOB_FAILURES);
    for failure in &mut job.failures {
        bound_field(&mut failure.error, MAX_JOB_TEXT_BYTES);
    }
    if let Some(result) = job.result.as_mut() {
        for account in &mut result.accounts {
            bound_field(&mut account.account_name, MAX_ACCOUNT_NAME_BYTES);
            if let Some(error) = account.error.as_mut() {
                bound_field(error, MAX_JOB_TEXT_BYTES);
            }
        }
    }
}

/// The row a job is stored as, with its text bounded.
pub(super) fn encode_job_record(job: &JobData) -> Result<String, String> {
    let mut job = job.clone();
    bound_record(&mut job);
    serde_json::to_string(&job).map_err(|error| error.to_string())
}

/// A job as `GetJob` (and a job start) sends it: the kept failures and every
/// undo id (clients undo a job's chunks from that list; a selection job has
/// none), with how many of each the job holds.
pub(super) fn job_detail(mut job: JobData) -> JobData {
    bound_record(&mut job);
    job.undo_id_count = job.undo_ids.len() as u32;
    job.failure_count = job.failures.len() as u32;
    job
}

/// A job as `ListJobs` sends it: status, progress, counts, selection, action
/// and the bounded error, but no undo ids and no failure records. Their
/// counts say how many `GetJob` would show.
pub(super) fn job_summary(job: JobData) -> JobData {
    let mut job = job_detail(job);
    job.undo_ids.clear();
    job.failures.clear();
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_kept_as_it_is() {
        assert_eq!(bounded_text("rate limited", 512), "rate limited");
    }

    #[test]
    fn long_text_is_cut_on_a_character_boundary_with_an_ellipsis() {
        // Three-byte characters, so a cut at a fixed byte would split one.
        let text = "€".repeat(400);
        let cut = bounded_text(&text, 512);
        assert!(cut.len() <= 512, "{}", cut.len());
        assert!(cut.ends_with(ELLIPSIS));
        assert!(cut.trim_end_matches(ELLIPSIS).chars().all(|c| c == '€'));
    }

    #[test]
    fn control_characters_become_spaces() {
        assert_eq!(bounded_text("a\u{0}b\nc", 512), "a b c");
        let long = "\u{1}".repeat(2_000);
        let cut = bounded_text(&long, 512);
        assert!(cut.len() <= 512);
        assert!(!cut.chars().any(char::is_control));
    }
}
