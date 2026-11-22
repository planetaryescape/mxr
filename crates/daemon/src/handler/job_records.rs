use mxr_protocol::JobData;

pub(super) const MAX_JOB_TEXT_BYTES: usize = 512;

pub(super) const MAX_ACCOUNT_NAME_BYTES: usize = 128;

pub(super) const MAX_JOB_FAILURES: usize = 50;

const ELLIPSIS: &str = "…";

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

pub(super) fn encode_job_record(job: &JobData) -> Result<String, String> {
    let mut job = job.clone();
    bound_record(&mut job);
    serde_json::to_string(&job).map_err(|error| error.to_string())
}

pub(super) fn job_detail(mut job: JobData) -> JobData {
    bound_record(&mut job);
    job.undo_id_count = job.undo_ids.len() as u32;
    job.failure_count = job.failures.len() as u32;
    job
}

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
    fn control_characters_become_spaces() {
        assert_eq!(bounded_text("a\u{0}b\nc", 512), "a b c");
        let long = "\u{1}".repeat(2_000);
        let cut = bounded_text(&long, 512);
        assert!(cut.len() <= 512);
        assert!(!cut.chars().any(char::is_control));
    }
}
