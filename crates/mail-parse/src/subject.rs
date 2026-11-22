/// Prefix a subject for a reply without stacking `Re:` on an existing one.
///
/// A thread that has been replied to repeatedly must not accumulate
/// `Re: Re: Re:`. Existing reply and forward markers are stripped first —
/// including mixed and repeated ones, since a forwarded reply arrives as
/// `Fwd: Re: ...` — and exactly one canonical prefix is applied.
///
/// Case-insensitive because clients differ (`RE:`, `re:`, `Fw:`).
pub fn prefix_subject(marker: SubjectMarker, subject: &str) -> String {
    let bare = strip_reply_forward_prefixes(subject);
    match marker {
        SubjectMarker::Reply => format!("Re: {bare}"),
        SubjectMarker::Forward => format!("Fwd: {bare}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectMarker {
    Reply,
    Forward,
}

pub fn strip_reply_forward_prefixes(subject: &str) -> String {
    let mut current = subject.trim();
    loop {
        let lower = current.to_ascii_lowercase();
        let Some(rest) = lower
            .strip_prefix("re:")
            .or_else(|| lower.strip_prefix("fwd:"))
            .or_else(|| lower.strip_prefix("fw:"))
        else {
            break;
        };
        current = current[current.len() - rest.len()..].trim_start();
    }
    current.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_subject_gains_one_marker() {
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Deploy plan"),
            "Re: Deploy plan"
        );
        assert_eq!(
            prefix_subject(SubjectMarker::Forward, "Deploy plan"),
            "Fwd: Deploy plan"
        );
    }

    #[test]
    fn replying_to_a_reply_does_not_stack_markers() {
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Re: Deploy plan"),
            "Re: Deploy plan"
        );
    }

    #[test]
    fn a_subject_that_merely_starts_with_those_letters_is_not_stripped() {
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Rebuild the index"),
            "Re: Rebuild the index"
        );
        assert_eq!(
            prefix_subject(SubjectMarker::Reply, "Fwd checklist"),
            "Re: Fwd checklist"
        );
    }
}
